//! Running a recipe: the phase order, the rollback policy, and the report.
//!
//! Execution model in `docs/architecture.md` section 5. The rule this module
//! exists to obey is the first one in `CLAUDE.md`: it never learns a mechanism.
//! There is no branch on `mechanism` here, for any reason, and it never calls
//! into `crate::perms`.

pub mod check;
pub mod entries;
pub mod env;
pub mod hook;
pub mod rollback;

use std::io::{Write, stderr, stdin};
use std::process::Command;

use crate::catalog::model::{ActionDecl, Hook, ItemDecl, Privilege, Rollback};
use crate::catalog::{Body, Node};
use crate::cli::Options;
use crate::error::Error;
use crate::exec::check::Status;
use crate::perms::registry;
use crate::render::progress::{Outcome, Progress};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The single phase of an action. Not a step in a transaction: there is
    /// nothing before it and nothing to undo after it.
    Run,
    Pre,
    Install,
    Post,
    Uninstall,
    /// Recording an `[item.permissions]` declaration into the profile registry.
    /// Not a hook: it is the engine's own write, and the engine undoes it.
    Permissions,
    RollPre,
    RollInstall,
    RollPost,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Run => "run",
            Phase::Pre => "pre",
            Phase::Install => "install",
            Phase::Post => "post",
            Phase::Uninstall => "uninstall",
            Phase::Permissions => "permissions",
            Phase::RollPre => "roll-pre",
            Phase::RollInstall => "roll-install",
            Phase::RollPost => "roll-post",
        }
    }
}

/// What naming a node that runs means. A list never arrives here: it lists
/// itself, and the entry chosen out of it arrives as the action it is.
pub fn run(node: &Node, opts: &Options) -> Result<(), Error> {
    match &node.body {
        Body::Item(item) => recipe(node, item, opts),
        Body::Action(declared) => action(node, declared, opts),
        Body::Menu | Body::List(_) => {
            unreachable!("a menu lists itself and never reaches the executor")
        }
    }
}

/// An action: one hook, no gate, no rollback, no removal.
///
/// The flags that belong to a transaction are refused rather than ignored. An
/// action has no `check` for `--force` to skip and nothing for `--uninstall` or
/// `--keep-partial` to act on, and a flag that is silently dropped is a flag
/// whose user believes it did something.
fn action(node: &Node, declared: &ActionDecl, opts: &Options) -> Result<(), Error> {
    let refused = [
        ("--uninstall", opts.uninstall),
        ("--force", opts.force),
        ("--keep-partial", opts.keep_partial),
    ]
    .into_iter()
    .find_map(|(flag, passed)| passed.then_some(flag));

    if let Some(flag) = refused {
        return Err(Error::Usage(format!(
            "`{flag}` acts on an installation, and `{}` is an action: it has no `check` to \
             skip and nothing to undo",
            node.path
        )));
    }

    let progress = Progress::new(opts.json, node);
    let elevated = declared.privilege == Privilege::System && !hook::is_root();

    if opts.dry_run {
        let invocation = hook::build(
            node,
            &declared.hooks.run,
            Phase::Run.as_str(),
            elevated,
            true,
        );
        progress.dry_run(Phase::Run, &invocation.describe(), invocation.elevated());
        progress.result(Outcome::DryRun, 0);
        return Ok(());
    }

    confirm(node, opts)?;
    if elevated && hook::probes() {
        authenticate()?;
    }

    match run_phase(node, &declared.hooks.run, Phase::Run, elevated, &progress)? {
        0 => {
            progress.result(Outcome::Ok, 0);
            Ok(())
        }
        code => {
            progress.result(Outcome::Failed, 1);
            Err(Error::Failed(format!(
                "`{}` failed (exit {code})",
                node.path
            )))
        }
    }
}

fn recipe(node: &Node, item: &ItemDecl, opts: &Options) -> Result<(), Error> {
    let progress = Progress::new(opts.json, node);

    // Decided once, before anything runs. A password prompt in the middle of a
    // transaction arrives when the user has looked away, and a prompt that
    // times out turns into a rollback of work that had already succeeded.
    let elevated = item.privilege == Privilege::System && !hook::is_root();

    if opts.dry_run {
        return dry_run(node, item, opts, elevated, &progress);
    }

    gate(node, opts)?;
    confirm(node, opts)?;
    if elevated && hook::probes() {
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

/// The item's `confirm`, answered before anything runs.
///
/// One rule, and no bypass that depends on the output mode: an item that
/// declares a question needs `--yes`, unless there is a terminal that can be
/// asked live. A front end reads the question out of the listing, draws its own
/// dialog and passes `--yes`, which makes that flag the single signal that the
/// question has been answered, by a human or on a human's behalf.
///
/// Refusing when there is nobody to ask is the whole point. Proceeding instead
/// would make a piped terminal more dangerous than the menu, which is the
/// inversion this exists to prevent.
///
/// `--force` deliberately does not answer it: it skips the `check` gate and
/// nothing else. The two guard different things, the system's state and the
/// user's intent.
fn confirm(node: &Node, opts: &Options) -> Result<(), Error> {
    let Some(question) = node
        .runnable()
        .and_then(|run| run.confirm)
        .map(str::to_owned)
    else {
        return Ok(());
    };
    if opts.yes {
        return Ok(());
    }

    // Under `--json` stdout is a stream of objects and the caller is a program,
    // so there is no live asking even from a terminal.
    if opts.json || !stdin_is_a_terminal() {
        return Err(Error::Refused(format!(
            "`{}` asks: {question}\n  there is no terminal to answer in; `--yes` answers it",
            node.path
        )));
    }

    eprint!("{question} [y/N] ");
    let _ = stderr().flush();

    let mut answer = String::new();
    if stdin().read_line(&mut answer).is_err() {
        answer.clear();
    }

    // Anything that is not a yes is a no. Nothing carrying a confirmation
    // should run because a keystroke was ambiguous.
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        Err(Error::Refused(format!("`{}` was not confirmed", node.path)))
    }
}

/// Whether there is somebody at the other end of stdin to answer a question.
fn stdin_is_a_terminal() -> bool {
    // SAFETY: isatty only inspects a descriptor and reports on it.
    unsafe { libc::isatty(libc::STDIN_FILENO) == 1 }
}

/// One prompt, up front, before any hook runs, which is what the credential
/// cache it fills is for. Skipped entirely when `SHIRO_SUDO_PROBE=0` says the
/// elevator keeps no cache for it to fill.
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
    let policy = if opts.keep_partial {
        Rollback::Phase
    } else {
        item.rollback
    };

    let mut completed: Vec<Phase> = Vec::new();
    let mut recorded: Option<registry::Record> = None;

    let steps = [
        Step::Hook(Phase::Pre, &item.hooks.pre),
        Step::Hook(Phase::Install, &item.hooks.install),
        Step::Permissions,
        Step::Hook(Phase::Post, &item.hooks.post),
    ];

    for step in steps {
        match step {
            Step::Hook(phase, declared) => {
                let Some(declared) = declared else {
                    continue;
                };

                match run_phase(node, declared, phase, elevated, progress)? {
                    0 => completed.push(phase),
                    code => {
                        return undo(
                            node, item, &completed, recorded, phase, code, policy, elevated,
                            progress,
                        );
                    }
                }
            }
            // The engine's own mutation, and the only call the executor makes
            // into the permissions module. It is a data write: no bwrap, no
            // flatpak, no wrapper, no desktop entry. It happens after `install`
            // so that a `post` generating whatever calls `shiro run` finds the
            // profile already there.
            Step::Permissions => {
                let Some(decl) = &item.permissions else {
                    continue;
                };

                progress.phase_start(Phase::Permissions);
                match registry::record(decl, item.privilege) {
                    Ok(record) => {
                        recorded = Some(record);
                        // Recorded like any other completed step, so that an
                        // undo puts it back in the right place in the sequence
                        // rather than in a special case ahead of everything.
                        completed.push(Phase::Permissions);
                        progress.phase_end(Phase::Permissions, 0);
                    }
                    Err(err) => {
                        progress.phase_end(Phase::Permissions, 1);
                        eprintln!("shiro: {err}");
                        return undo(
                            node,
                            item,
                            &completed,
                            None,
                            Phase::Permissions,
                            1,
                            policy,
                            elevated,
                            progress,
                        );
                    }
                }
            }
        }
    }

    progress.result(Outcome::Installed, 0);
    Ok(())
}

enum Step<'a> {
    Hook(Phase, &'a Option<Hook>),
    Permissions,
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
    recorded: Option<registry::Record>,
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
        // The registry write was the engine's own, so the engine owns undoing
        // it, with nothing declared by the recipe.
        if phase == Phase::Permissions {
            let Some(record) = &recorded else {
                continue;
            };
            progress.phase_start(Phase::Permissions);
            if let Err(err) = registry::restore(record) {
                progress.result(Outcome::RollbackFailed, 4);
                return Err(err);
            }
            progress.phase_end(Phase::Permissions, 0);
            continue;
        }

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
