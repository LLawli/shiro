//! The environment every hook is given, so a recipe never has to guess where it
//! is: `SHIRO_ITEM`, `SHIRO_PHASE`, `SHIRO_RECIPE_DIR`, `SHIRO_LAYER`,
//! `SHIRO_DRY_RUN`, and `SHIRO_ENTRY` for the hook of a list. Table in
//! `docs/architecture.md` section 5.

use std::process::Command;

use crate::catalog::Node;

/// The variables a hook receives.
///
/// `SHIRO_DRY_RUN` is `1` only in the environment `--dry-run` prints, since a
/// dry run executes nothing at all: what is printed is then the whole truth
/// about what would have run, environment included.
pub fn variables(node: &Node, phase: &str, dry_run: bool) -> Vec<(String, String)> {
    let mut variables = vec![
        ("SHIRO_ITEM".to_owned(), node.path.clone()),
        ("SHIRO_PHASE".to_owned(), phase.to_owned()),
        (
            "SHIRO_LAYER".to_owned(),
            node.source.layer.as_str().to_owned(),
        ),
        (
            "SHIRO_DRY_RUN".to_owned(),
            if dry_run { "1" } else { "0" }.to_owned(),
        ),
    ];

    // The built-in layer has no directory on disk, and a variable pointing at
    // nowhere is worse than an absent one: `cd "$SHIRO_RECIPE_DIR"` would then
    // land in the current directory rather than fail.
    if let Some(dir) = node.source.dir.as_ref() {
        variables.push(("SHIRO_RECIPE_DIR".to_owned(), dir.display().to_string()));
    }

    // Absent for everything that is not an entry, by the same rule: a hook can
    // test whether it was given one rather than compare against an empty
    // string that a generator could also have printed.
    if let Some(entry) = node.entry.as_ref() {
        variables.push(("SHIRO_ENTRY".to_owned(), entry.clone()));
    }

    variables
}

/// The same environment, applied to a command directly. Used by `check`, which
/// is never elevated and so never needs the exported form.
pub fn apply(command: &mut Command, node: &Node, phase: &str) {
    for (key, value) in variables(node, phase, false) {
        command.env(key, value);
    }
    if node.source.dir.is_none() {
        command.env_remove("SHIRO_RECIPE_DIR");
    }
    if node.entry.is_none() {
        command.env_remove("SHIRO_ENTRY");
    }
}
