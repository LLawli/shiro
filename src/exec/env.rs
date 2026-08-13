//! The environment every hook is given, so a recipe never has to guess where it
//! is: `SHIRO_ITEM`, `SHIRO_PHASE`, `SHIRO_RECIPE_DIR`, `SHIRO_LAYER`,
//! `SHIRO_DRY_RUN`. Table in `docs/architecture.md` section 5.

use std::process::Command;

use crate::catalog::Node;

pub fn apply(command: &mut Command, node: &Node, phase: &str) {
    command
        .env("SHIRO_ITEM", &node.path)
        .env("SHIRO_PHASE", phase)
        .env("SHIRO_LAYER", node.source.layer.as_str())
        .env("SHIRO_DRY_RUN", "0");

    // The built-in layer has no directory on disk, and a variable pointing at
    // nowhere is worse than an absent one: `cd "$SHIRO_RECIPE_DIR"` would then
    // land in the current directory rather than fail.
    match node.source.dir.as_ref() {
        Some(dir) => command.env("SHIRO_RECIPE_DIR", dir),
        None => command.env_remove("SHIRO_RECIPE_DIR"),
    };
}
