//! Running a recipe: the phase order, the rollback policy, and the report.
//!
//! Execution model in `docs/architecture.md` section 5. The rule this module
//! exists to obey is the first one in `CLAUDE.md`: it never learns a mechanism.
//! There is no branch on `mechanism` here, for any reason, and it never calls
//! into `crate::perms`.

pub mod check;
pub mod env;
pub mod hook;
pub mod rollback;

use std::process::Command;

use crate::catalog::Node;
use crate::catalog::model::{Hook, ItemDecl, Privilege, Rollback};
use crate::cli::Options;
use crate::error::Error;
use crate::exec::check::Status;
use crate::render::progress::{Outcome, Progress};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Pre,
    Install,
    Post,
    Uninstall,
    RollPre,
    RollInstall,
    RollPost,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Pre => "pre",
            Phase::Install => "install",
            Phase::Post => "post",
            Phase::Uninstall => "uninstall",
            Phase::RollPre => "roll-pre",
            Phase::RollInstall => "roll-install",
            Phase::RollPost => "roll-post",
        }
    }
}

pub fn item(node: &Node, opts: &Options) -> Result<(), Error> {
    let item = node
        .item
        .as_ref()
        .expect("only an item reaches the executor");
    let progress = Progress::new(opts.json, node);

    // The engine is supposed to record this into the profile registry, and the
    // registry is not written yet. Saying so is the only honest option: a
    // recipe whose profile silently did not land is a sandbox the user believes
    // is there.
    if item.permissions.is_some() {
        eprintln!(
            "shiro: `{}` declares [item.permissions], and recording profiles is not implemented \
             yet; nothing was written",
            node.path
        );
    }

    // Decided once, before anything runs. A password prompt in the middle of a
    // transaction arrives when the user has looked away, and a prompt that
    // times out turns into a rollback of work that had already succeeded.
    let elevated = item.privilege == Privilege::System && !hook::is_root();

    if opts.dry_run {
        return dry_run(node, item, opts, elevated, &progress);
    }

    gate(node, opts)?;
    if elevated {
        authenticate()?;
    }

    if opts.uninstall {
        remove(node, item, elevated, &progress)
    } else {
        install(node, item, opts, elevated, &progress)
    }
}

/// `check` decides whether the recipe should run at all. Recipes are
/// hand-written shell, idempotence can be claimed but not validated, and
/// re-running one by accident is a real way to break a working system.
fn gate(node: &Node, opts: &Options) -> Result<(), Error> {
    if opts.force {
        return Ok(());
    }

    match (opts.uninstall, check::single(node)) {
        (false, Status::Installed) => Err(Error::Refused(format!(
            "`{}` is already installed; --force runs the recipe anyway",
            node.path
        ))),
        (true, Status::Absent) => Err(Error::Refused(format!(
            "`{}` is not installed; --force runs the removal anyway",
            node.path
        ))),
        _ => Ok(()),
    }
}

/// One prompt, up front, before any hook runs.
fn authenticate() -> Result<(), Error> {
    let elevator = hook::elevator();
    let (program, args) = elevator.split_first().expect("the elevator is never empty");

    let status = Command::new(program)
        .args(args)
        .arg("true")
        .status()
        .map_err(|err| Error::Failed(format!("cannot run `{program}`: {err}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(Error::Failed(format!(
            "this item needs root, and `{program}` did not authenticate"
        )))
    }
}

fn install(
    node: &Node,
    item: &ItemDecl,
    opts: &Options,
    elevated: bool,
    progress: &Progress<'_>,
) -> Result<(), Error> {
    let phases = [
        (Phase::Pre, &item.hooks.pre),
        (Phase::Install, &item.hooks.install),
        (Phase::Post, &item.hooks.post),
    ];

    let mut completed: Vec<Phase> = Vec::new();

    for (phase, declared) in phases {
        let Some(declared) = declared else {
            continue;
        };

        match run_phase(node, declared, phase, elevated, progress)? {
            0 => completed.push(phase),
            code => {
                let policy = if opts.keep_partial {
                    Rollback::Phase
                } else {
                    item.rollback
                };
                return undo(
                    node, item, &completed, phase, code, policy, elevated, progress,
                );
            }
        }
    }

    progress.result(Outcome::Installed, 0);
    Ok(())
}

fn remove(
    node: &Node,
    item: &ItemDecl,
    elevated: bool,
    progress: &Progress<'_>,
) -> Result<(), Error> {
    let steps = rollback::removal(&item.hooks);
    if steps.is_empty() {
        return Err(Error::Refused(format!(
            "`{}` declares no `uninstall` and no rollback hooks, so there is nothing to remove",
            node.path
        )));
    }

    for (phase, declared) in steps {
        let code = run_phase(node, declared, phase, elevated, progress)?;
        if code != 0 {
            // A removal has no undo. Stopping at the first failure leaves the
            // rest of the item in place, which is the state the report has to
            // describe rather than paper over.
            progress.result(Outcome::Failed, 1);
            return Err(Error::Failed(format!(
                "`{}` failed while removing `{}` (exit {code})",
                node.path,
                phase.as_str()
            )));
        }
    }

    progress.result(Outcome::Removed, 0);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn undo(
    node: &Node,
    item: &ItemDecl,
    completed: &[Phase],
    failed: Phase,
    code: i32,
    policy: Rollback,
    elevated: bool,
    progress: &Progress<'_>,
) -> Result<(), Error> {
    let failure = format!(
        "`{}` failed in `{}` (exit {code})",
        node.path,
        failed.as_str()
    );

    for phase in rollback::plan(policy, completed, failed) {
        let Some((undo_phase, declared)) = rollback::undo(&item.hooks, phase) else {
            continue;
        };

        let undo_code = run_phase(node, declared, undo_phase, elevated, progress)?;
        if undo_code != 0 {
            progress.result(Outcome::RollbackFailed, 4);
            return Err(Error::RollbackFailed(format!(
                "{failure}, and `{}` failed too (exit {undo_code}); the system is in a state \
                 neither you nor the recipe intended",
                undo_phase.as_str()
            )));
        }
    }

    let outcome = match policy {
        Rollback::Atomic => Outcome::RolledBack,
        _ => Outcome::Partial,
    };
    progress.result(outcome, 1);
    Err(Error::Failed(failure))
}

fn run_phase(
    node: &Node,
    declared: &Hook,
    phase: Phase,
    elevated: bool,
    progress: &Progress<'_>,
) -> Result<i32, Error> {
    let invocation = hook::build(node, declared, phase.as_str(), elevated, false);

    progress.phase_start(phase);
    let status = invocation
        .run(progress.hook_output())
        .map_err(|err| Error::Failed(format!("cannot run `{}`: {err}", phase.as_str())))?;

    // A hook killed by a signal has no exit code, and reporting it as success
    // would be the worst possible reading of it.
    let code = status.code().unwrap_or(128);
    progress.phase_end(phase, code);
    Ok(code)
}

fn dry_run(
    node: &Node,
    item: &ItemDecl,
    opts: &Options,
    elevated: bool,
    progress: &Progress<'_>,
) -> Result<(), Error> {
    let steps: Vec<(Phase, &Hook)> = if opts.uninstall {
        rollback::removal(&item.hooks)
    } else {
        [
            (Phase::Pre, &item.hooks.pre),
            (Phase::Install, &item.hooks.install),
            (Phase::Post, &item.hooks.post),
        ]
        .into_iter()
        .filter_map(|(phase, hook)| hook.as_ref().map(|hook| (phase, hook)))
        .collect()
    };

    for (phase, declared) in steps {
        let invocation = hook::build(node, declared, phase.as_str(), elevated, true);
        progress.dry_run(phase, &invocation.describe(), invocation.elevated());
    }

    progress.result(Outcome::DryRun, 0);
    Ok(())
}
