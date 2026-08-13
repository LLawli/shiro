//! The `check` hook, which is the only one invoked in bulk and unbidden.
//!
//! Three hard rules, from `docs/architecture.md` section 4: no side effects,
//! never elevated, always bounded. Batch invocation runs the children of a menu
//! in parallel, and that is contract rather than optimization: a menu that
//! spawns 200 checks serially is a menu nobody opens.
//!
//! The four answers are `installed`, `absent`, `unknown` and `timeout`. The
//! last two are honest answers, not failures: an item whose presence can only
//! be seen as root declares no `check` and reports `unknown`, which costs a
//! menu nothing.

use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::catalog::model::Hook;
use crate::catalog::{Kind, Node};
use crate::exec::env;

/// How long a single `check` may take before it is killed and reported as
/// `timeout`. Overridable with `SHIRO_CHECK_TIMEOUT`, in whole seconds.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

/// How many checks run at once. A menu with hundreds of children should not
/// fork hundreds of processes at once, and the work is process-bound rather
/// than CPU-bound, so the cap sits above the core count.
const MAX_CONCURRENCY: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Installed,
    Absent,
    Unknown,
    Timeout,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Installed => "installed",
            Status::Absent => "absent",
            Status::Unknown => "unknown",
            Status::Timeout => "timeout",
        }
    }
}

/// The status of every node given, in the same order. Menus have no status and
/// yield `None`.
pub fn batch(nodes: &[&Node]) -> Vec<Option<Status>> {
    let mut statuses: Vec<Option<Status>> = nodes
        .iter()
        .map(|node| match node.kind {
            Kind::Menu => None,
            // No `check` declared is not a failure to answer, it is the answer:
            // this item does not say how to tell.
            Kind::Item => Some(Status::Unknown),
        })
        .collect();

    let jobs: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.item
                .as_ref()
                .is_some_and(|item| item.hooks.check.is_some())
        })
        .map(|(index, _)| index)
        .collect();

    if jobs.is_empty() {
        return statuses;
    }

    let timeout = timeout();
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(jobs.len()));
    let workers = jobs.len().min(MAX_CONCURRENCY);

    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let slot = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&index) = jobs.get(slot) else {
                        break;
                    };
                    let status = run(nodes[index], timeout);
                    results
                        .lock()
                        .expect("no thread panics here")
                        .push((index, status));
                }
            });
        }
    });

    for (index, status) in results.into_inner().expect("no thread panics here") {
        statuses[index] = Some(status);
    }
    statuses
}

fn timeout() -> Duration {
    std::env::var("SHIRO_CHECK_TIMEOUT")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_TIMEOUT)
}

/// One item's `check`, for the gate in front of a recipe. Same rules as the
/// batch: bounded, never elevated, no side effects it is allowed to have.
pub fn single(node: &Node) -> Status {
    match node.item.as_ref().map(|item| &item.hooks.check) {
        Some(Some(_)) => run(node, timeout()),
        _ => Status::Unknown,
    }
}

fn run(node: &Node, timeout: Duration) -> Status {
    let Some(hook) = node
        .item
        .as_ref()
        .and_then(|item| item.hooks.check.as_ref())
    else {
        return Status::Unknown;
    };

    let mut command = match hook {
        Hook::Shell(script) => {
            let mut command = Command::new("sh");
            command.arg("-c").arg(script);
            command
        }
        Hook::Script(file) => {
            // A catalog the validator accepted has no script hook it cannot
            // locate, so this only fires on a built-in layer node, which has no
            // directory on disk.
            let Some(dir) = node.source.dir.as_ref() else {
                return Status::Unknown;
            };
            Command::new(dir.join(file))
        }
    };

    if let Some(dir) = node.source.dir.as_ref() {
        command.current_dir(dir);
    }
    env::apply(&mut command, node, "check");

    // A check answers a question. Whatever it prints is noise to a menu, and
    // stdin is closed so that a hook that waits for input times out instead of
    // hanging a front end forever.
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let Ok(mut child) = command.spawn() else {
        return Status::Unknown;
    };

    let deadline = Instant::now() + timeout;
    let mut interval = Duration::from_millis(1);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Status::Installed
                } else {
                    Status::Absent
                };
            }
            Ok(None) => {}
            Err(_) => return Status::Unknown,
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let _ = child.kill();
            let _ = child.wait();
            return Status::Timeout;
        }

        // Short waits first, so a check that answers in a millisecond is not
        // held back by the polling interval, then longer ones so that a slow
        // check does not spin a core for three seconds.
        thread::sleep(remaining.min(interval));
        interval = (interval * 2).min(Duration::from_millis(20));
    }
}
