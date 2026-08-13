//! Undoing what a failed transaction already did.
//!
//! Policy is declared per item: `atomic` (the default) undoes everything in
//! reverse, `phase` undoes only the phase that failed, `none` undoes nothing.
//! `--keep-partial` downgrades `atomic` to `phase` for one invocation.
//!
//! A failing rollback is its own outcome, not a second failure of the same
//! kind: the system is now in a state nobody intended, and the report says so,
//! names the hook, and exits with a code of its own.

use crate::catalog::model::{Hook, Hooks, Rollback};
use crate::exec::Phase;

/// The phases to undo, in the order they should be undone.
///
/// The phase that failed is undone too, and first. Its hook ran and stopped
/// partway, which is exactly the state a `roll-*` hook is written to face.
pub fn plan(policy: Rollback, completed: &[Phase], failed: Phase) -> Vec<Phase> {
    match policy {
        Rollback::None => Vec::new(),
        Rollback::Phase => vec![failed],
        Rollback::Atomic => {
            let mut phases = vec![failed];
            phases.extend(completed.iter().rev().copied());
            phases
        }
    }
}

/// The hook that undoes a phase, if the recipe declares one. A phase with no
/// `roll-*` had nothing to undo, which is a legitimate shape: a `pre` that only
/// checks compatibility leaves nothing behind.
pub fn undo(hooks: &Hooks, phase: Phase) -> Option<(Phase, &Hook)> {
    let (undo_phase, hook) = match phase {
        Phase::Pre => (Phase::RollPre, &hooks.roll_pre),
        Phase::Install => (Phase::RollInstall, &hooks.roll_install),
        Phase::Post => (Phase::RollPost, &hooks.roll_post),
        // A rollback hook has no rollback of its own, and neither does a
        // removal: there is no third level, by design.
        Phase::Uninstall | Phase::RollPre | Phase::RollInstall | Phase::RollPost => (phase, &None),
    };

    hook.as_ref().map(|hook| (undo_phase, hook))
}

/// What removing an item runs.
///
/// A declared `uninstall` is what runs. Absent one, the rollback hooks are run
/// in reverse order, skipping the ones that are absent: for most recipes,
/// undoing an installation and removing an installed thing are the same
/// command, and the second copy is the one nobody updates.
pub fn removal(hooks: &Hooks) -> Vec<(Phase, &Hook)> {
    if let Some(hook) = &hooks.uninstall {
        return vec![(Phase::Uninstall, hook)];
    }

    [
        (Phase::RollPost, &hooks.roll_post),
        (Phase::RollInstall, &hooks.roll_install),
        (Phase::RollPre, &hooks.roll_pre),
    ]
    .into_iter()
    .filter_map(|(phase, hook)| hook.as_ref().map(|hook| (phase, hook)))
    .collect()
}
