//! Undoing what a failed transaction already did.
//!
//! Policy is declared per item: `atomic` (the default) undoes everything in
//! reverse, `phase` undoes only the phase that failed, `none` undoes nothing.
//! `--keep-partial` downgrades `atomic` to `phase` for one invocation.
//!
//! A failing rollback is its own outcome, not a second failure of the same
//! kind: the system is now in a state nobody intended, and the report says so,
//! names the hook, and exits with a code of its own.
//!
//! `uninstall` is derived here when a recipe does not declare it: `roll-post`,
//! `roll-install`, `roll-pre`, skipping what is absent.
