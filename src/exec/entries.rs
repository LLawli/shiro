//! The entries of a `[[list]]` node: what only exists at run time.
//!
//! A generator is a `check` that answers with a list instead of a yes. The
//! three rules of section 4 hold for it word for word: it has no side effects,
//! it is never elevated, and it runs under a timeout, because it is invoked
//! unbidden while a menu is drawing.
//!
//! What it prints is identities, never commands. The hook that runs for the
//! chosen entry is declared by the catalog, in the layer an administrator
//! controls, and the entry reaches it as `SHIRO_ENTRY`. A generator printing
//! nodes with hooks of their own would move the declaration of what runs out
//! of the catalog, and a user-layer generator could then declare `privilege =
//! "system"` for itself.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::catalog::load::segment_problem;
use crate::catalog::model::{ActionDecl, ActionHooks, Hook, ListDecl};
use crate::catalog::{Body, Node};
use crate::error::Error;
use crate::exec::env;

/// The phase a generator runs as, which is what it reads in `SHIRO_PHASE`.
const PHASE: &str = "entries";

/// How long a generator may take before it is killed. Overridable with
/// `SHIRO_LIST_TIMEOUT`, in whole seconds. Longer than a `check`'s three
/// seconds, because a generator is one process asked to enumerate something
/// rather than one of two hundred asked a yes-or-no question.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// How much a generator may print. A menu cannot draw a megabyte of entries,
/// and reading without a bound turns a runaway generator into a runaway shiro.
const MAX_OUTPUT: u64 = 1 << 20;

/// One entry, as the generator prints it.
///
/// `id` is what the user types, so it is a path segment and obeys the same
/// rule as any other. `value` is what the hook receives, for the case where
/// the two cannot be the same: a wallpaper is chosen as `foto-2024` and set by
/// its absolute path, and neither string can do the other's job.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub value: Option<String>,
}

impl Entry {
    /// What the hook is given. The `id` when the generator declared no
    /// `value`, so that the simple case declares one string and not two.
    pub fn value(&self) -> &str {
        self.value.as_deref().unwrap_or(&self.id)
    }
}

/// The entries of a list node, from the cache when one is valid and from the
/// generator otherwise.
pub fn of(node: &Node, list: &ListDecl) -> Result<Vec<Entry>, Error> {
    if let Some(raw) = cached(node, list)
        && let Ok(entries) = parse(node, &raw)
    {
        return Ok(entries);
    }

    let raw = generate(node, list)?;
    let entries = parse(node, &raw)?;
    // Written only once the answer has been read and accepted, so that a
    // generator having a bad day does not get its bad day cached.
    store(node, list, &raw);
    Ok(entries)
}

/// An entry as a node, so that everything downstream of here (listing,
/// checking, running, reporting) works on the one shape it already knows.
///
/// It materializes as an action, which is what it is: something that runs and
/// has no sense in which it is installed. What the list declared about running
/// is carried by every entry, because one hook runs them all.
pub fn materialize(parent: &Node, list: &ListDecl, entry: &Entry) -> Node {
    let declared = ActionDecl {
        path: format!("{}.{}", parent.path, entry.id),
        title: entry.title.clone(),
        description: entry.description.clone(),
        order: None,
        hidden: false,
        mechanism: list.mechanism.clone(),
        privilege: list.privilege,
        icon: entry.icon.clone(),
        keywords: entry.keywords.clone(),
        confirm: list.confirm.clone(),
        destructive: list.destructive,
        interactive: list.interactive,
        keep_open: list.keep_open,
        hooks: ActionHooks {
            run: list.hooks.run.clone(),
        },
    };

    Node {
        path: declared.path.clone(),
        title: declared.title.clone(),
        description: declared.description.clone(),
        order: None,
        hidden: false,
        icon: declared.icon.clone(),
        keywords: declared.keywords.clone(),
        body: Body::Action(Box::new(declared)),
        entry: Some(entry.value().to_owned()),
        source: parent.source.clone(),
        shadowed: Vec::new(),
    }
}

/// What the generator printed, checked against the contract before anything is
/// built from it. A generator that prints something else fails loudly here
/// rather than producing a menu of nonsense.
fn parse(node: &Node, raw: &[u8]) -> Result<Vec<Entry>, Error> {
    let refuse = |reason: String| {
        Error::Catalog(format!(
            "the generator for `{}` {reason}",
            node.path.replace('.', " ")
        ))
    };

    let entries: Vec<Entry> = serde_json::from_slice(raw)
        .map_err(|err| refuse(format!("did not print a list of entries: {err}")))?;

    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for entry in &entries {
        if let Some(problem) = segment_problem(&entry.id) {
            return Err(refuse(format!(
                "printed the id `{}`, which is not a segment a user can type: {problem}",
                entry.id
            )));
        }
        if entry.title.trim().is_empty() {
            return Err(refuse(format!(
                "printed the entry `{}` with an empty title",
                entry.id
            )));
        }
        if entry.keywords.iter().any(|word| word.trim().is_empty()) {
            return Err(refuse(format!(
                "printed the entry `{}` with an empty keyword, which matches every search",
                entry.id
            )));
        }
        if !seen.insert(entry.id.as_str()) {
            return Err(refuse(format!("printed the id `{}` twice", entry.id)));
        }
    }

    Ok(entries)
}

fn generate(node: &Node, list: &ListDecl) -> Result<Vec<u8>, Error> {
    let failed = |reason: String| {
        Error::Failed(format!(
            "the generator for `{}` {reason}",
            node.path.replace('.', " ")
        ))
    };

    let mut command = match &list.entries.command {
        Hook::Shell(script) => {
            let mut command = Command::new("sh");
            command.arg("-c").arg(script);
            command
        }
        Hook::Script(file) => {
            // A catalog the validator accepted has no script hook it cannot
            // locate, so this only fires on a built-in layer node, which has
            // no directory on disk.
            let Some(dir) = node.source.dir.as_ref() else {
                return Err(failed(format!(
                    "points at the script `{file}`, and the built-in layer has no directory"
                )));
            };
            Command::new(dir.join(file))
        }
    };

    if let Some(dir) = node.source.dir.as_ref() {
        command.current_dir(dir);
    }
    env::apply(&mut command, node, PHASE);

    // Never elevated, like a `check`, and given no stdin, so that a generator
    // stopping to ask a question times out instead of hanging the menu that
    // called it. Its stderr is inherited: when it fails, what it says is the
    // only explanation anybody gets.
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = command
        .spawn()
        .map_err(|err| failed(format!("could not run: {err}")))?;
    let mut stdout = child.stdout.take().expect("stdout is a pipe");

    // Read on a thread of its own, because a generator printing more than a
    // pipe holds would otherwise block forever waiting for a reader that is
    // busy waiting for it. The thread is deliberately not joined on timeout: a
    // grandchild holding the pipe open must not turn a bounded wait into an
    // unbounded one.
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut buffer = Vec::new();
        let read = stdout
            .by_ref()
            .take(MAX_OUTPUT + 1)
            .read_to_end(&mut buffer);
        let _ = sender.send(read.map(|_| buffer));
    });

    let timeout = timeout();
    let deadline = Instant::now() + timeout;
    let mut interval = Duration::from_millis(1);

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(err) => return Err(failed(format!("could not be waited for: {err}"))),
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(failed(format!(
                "did not answer within {} seconds",
                timeout.as_secs()
            )));
        }
        thread::sleep(remaining.min(interval));
        interval = (interval * 2).min(Duration::from_millis(20));
    };

    let output = match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => return Err(failed(format!("could not be read: {err}"))),
        // The generator exited and something it left behind still holds the
        // pipe. That is the same failure as never answering.
        Err(_) => {
            return Err(failed(format!(
                "did not answer within {} seconds",
                timeout.as_secs()
            )));
        }
    };

    if !status.success() {
        return Err(failed(format!(
            "failed (exit {})",
            status.code().unwrap_or(128)
        )));
    }
    if output.len() as u64 > MAX_OUTPUT {
        return Err(failed(format!(
            "printed more than {} bytes; a menu cannot draw that many entries",
            MAX_OUTPUT
        )));
    }

    Ok(output)
}

fn timeout() -> Duration {
    std::env::var("SHIRO_LIST_TIMEOUT")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_TIMEOUT)
}

/// Whether an answer may be reused at all. `SHIRO_LIST_CACHE=0` turns it off
/// wholesale, for debugging a generator and for the front end that would
/// rather pay the spawn. Exactly `0`, and nothing else: a variable that means
/// one thing is spelled one way.
fn caches() -> bool {
    !matches!(std::env::var("SHIRO_LIST_CACHE").as_deref(), Ok("0"))
}

/// Where this node's answer is kept. Keyed by what would produce it, so that
/// editing the generator in a catalog invalidates nothing and collides with
/// nothing: it is simply a different question with a different answer.
fn cache_file(node: &Node, list: &ListDecl) -> Option<PathBuf> {
    let mut hasher = Sha256::new();
    for field in [
        node.path.as_str(),
        node.source.layer.as_str(),
        match &list.entries.command {
            Hook::Shell(script) => script.as_str(),
            Hook::Script(file) => file.as_str(),
        },
    ] {
        hasher.update(field.as_bytes());
        hasher.update([0x1f]);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect();

    Some(
        crate::layers::user_cache_dir()?
            .join("shiro/entries")
            .join(format!("{digest}.json")),
    )
}

fn cached(node: &Node, list: &ListDecl) -> Option<Vec<u8>> {
    let ttl = list.entries.ttl?;
    if !caches() {
        return None;
    }

    let file = cache_file(node, list)?;
    let age = fs::metadata(&file)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|written| SystemTime::now().duration_since(written).ok())?;

    (age < ttl.0).then(|| fs::read(&file).ok())?
}

/// Best effort, and silent when it fails. A cache that cannot be written is a
/// generator that runs every time, which is the behaviour of every list that
/// declares no `ttl` anyway.
fn store(node: &Node, list: &ListDecl, raw: &[u8]) {
    if list.entries.ttl.is_none() || !caches() {
        return;
    }
    let Some(file) = cache_file(node, list) else {
        return;
    };
    let Some(dir) = file.parent() else {
        return;
    };
    if fs::create_dir_all(dir).is_err() {
        return;
    }

    // Written aside and renamed, so that a reader never sees half an answer.
    let temporary = file.with_extension(format!("tmp{}", std::process::id()));
    if fs::write(&temporary, raw).is_ok() && fs::rename(&temporary, &file).is_err() {
        let _ = fs::remove_file(&temporary);
    }
}
