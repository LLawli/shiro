//! `shiro catalog validate`: the schema rules, enforced rather than documented.
//!
//! The list is in `docs/architecture.md` section 4, and it grows in the same
//! commit as the rule it enforces. A rule that lives only in prose is a rule
//! that gets violated by the first recipe written after it.
//!
//! Two rules from that section are enforced earlier and do not appear here. A
//! malformed node path is refused by the loader, because a path the tree cannot
//! hold is not a finding, it is a broken file. And a hook that carries anything
//! beyond `file` (`privilege = "system"` on a `check`, say) is refused by the
//! parser, which is the only place that can name the stray key.

use std::path::Path;

use crate::catalog::model::{Hook, Hooks, ItemDecl, ListDecl, Rollback};
use crate::catalog::{Body, Catalog, Kind, Layer, Node};
use crate::cli::NATIVE;
use crate::perms;

pub struct Finding {
    pub path: String,
    pub file: String,
    pub message: String,
}

pub fn run(catalog: &Catalog) -> Vec<Finding> {
    let mut findings = Vec::new();

    for node in catalog.iter() {
        let mut report = |message: String| {
            findings.push(Finding {
                path: node.path.clone(),
                file: node.source.file.clone(),
                message,
            })
        };

        if node.path.contains('.') {
            parent(catalog, node, &mut report);
        } else if NATIVE.contains(&node.path.as_str()) {
            report(format!(
                "`{}` is a native command; a catalog may not claim it",
                node.path
            ));
        }

        presentation(node, &mut report);

        match &node.body {
            Body::Menu => {}
            Body::Item(item) => {
                recipe(item, &mut report);
                hooks(&item.hooks, node, &mut report);
                permissions(item, &mut report);
            }
            // One hook, so one thing to check about it. Everything a recipe
            // declares that an action must not is absent from the declaration
            // rather than reported here.
            Body::Action(action) => script(&action.hooks.run, "run", node, &mut report),
            Body::List(list) => generated(list, node, &mut report),
        }
    }

    findings.sort_by(|a, b| a.path.cmp(&b.path));
    findings
}

/// Every node below the root needs a parent, and the parent has to be a menu.
/// An orphan is not a load error, because the tree still holds: it is a node
/// nobody can ever navigate to, which is worse and quieter.
fn parent(catalog: &Catalog, node: &Node, report: &mut impl FnMut(String)) {
    let Some((parent_path, _)) = node.path.rsplit_once('.') else {
        return;
    };

    match catalog.get(parent_path) {
        None => report(format!(
            "its parent `{parent_path}` is not declared, so nothing can navigate to it"
        )),
        Some(parent) if parent.kind() == Kind::Item => report(format!(
            "its parent `{parent_path}` is an item, and an item has no children"
        )),
        Some(parent) if parent.kind() == Kind::Action => report(format!(
            "its parent `{parent_path}` is an action, and an action has no children"
        )),
        // A list has children, and they are the ones its generator prints. A
        // declared one is unreachable: the segment after a list names an entry,
        // and the tree is never consulted for it.
        Some(parent) if parent.kind() == Kind::List => report(format!(
            "its parent `{parent_path}` is a list, whose children come from its generator, so \
             nothing can navigate to it"
        )),
        Some(_) => {}
    }
}

/// A list declares two commands: the one that prints the entries and the one
/// that runs for the chosen entry. Both are hooks, so both answer to the rules
/// about where a script may live.
fn generated(list: &ListDecl, node: &Node, report: &mut impl FnMut(String)) {
    script(&list.hooks.run, "run", node, report);
    script(&list.entries.command, "entries.command", node, report);

    if matches!(&list.entries.command, Hook::Shell(body) if body.trim().is_empty()) {
        report("`entries.command` is empty, so there is nothing to list".to_owned());
    }
}

/// The presentation fields carry no behaviour, so the only thing to enforce is
/// that they are not empty. An empty keyword matches every search, and an empty
/// `confirm` is a dialog with no question in it: both are a field whose author
/// believes they filled it.
fn presentation(node: &Node, report: &mut impl FnMut(String)) {
    if node
        .keywords
        .iter()
        .any(|keyword| keyword.trim().is_empty())
    {
        report("`keywords` holds an empty entry, which matches every search".to_owned());
    }

    if node
        .runnable()
        .and_then(|run| run.confirm)
        .is_some_and(|question| question.trim().is_empty())
    {
        report("`confirm` is empty, so there is no question to ask".to_owned());
    }
}

fn recipe(item: &ItemDecl, report: &mut impl FnMut(String)) {
    if item.pre_mutates && item.hooks.roll_pre.is_none() {
        report(
            "it declares `pre_mutates` without `roll-pre`, so a failure after `pre` leaves \
             debris nothing will clean up"
                .to_owned(),
        );
    }

    if item.hooks.install.is_some()
        && item.hooks.roll_install.is_none()
        && item.rollback != Rollback::None
    {
        report(format!(
            "it declares `install` without `roll-install` under `rollback = \"{}\"`",
            item.rollback.as_str()
        ));
    }
}

fn hooks(hooks: &Hooks, node: &Node, report: &mut impl FnMut(String)) {
    let declared = [
        ("check", &hooks.check),
        ("pre", &hooks.pre),
        ("install", &hooks.install),
        ("post", &hooks.post),
        ("roll-pre", &hooks.roll_pre),
        ("roll-install", &hooks.roll_install),
        ("roll-post", &hooks.roll_post),
        ("uninstall", &hooks.uninstall),
    ];

    for (name, hook) in declared {
        if let Some(hook) = hook {
            script(hook, name, node, report);
        }
    }
}

/// Where a `{ file = "..." }` hook may point. The rules are the same for every
/// hook, an action's `run` included, which is why they live in one place rather
/// than inside the loop that knows a recipe's hook names.
fn script(hook: &Hook, name: &str, node: &Node, report: &mut impl FnMut(String)) {
    let Hook::Script(file) = hook else {
        return;
    };

    // The built-in layer is embedded in the binary, so a script it points at
    // is not on the host at all. Inlining it is the fix, and the alternative
    // (extracting embedded scripts at run time) buys a temporary directory
    // in the hot path for a case the base curation does not need yet.
    if node.source.layer == Layer::BuiltIn {
        report(format!(
            "`{name}` points at the script `{file}`, but the built-in layer is embedded in \
             the binary and has no directory on disk; inline the script instead"
        ));
        return;
    }

    let path = Path::new(file);
    if path.is_absolute() || path.components().any(|part| part.as_os_str() == "..") {
        report(format!(
            "`{name}` points at `{file}`, which leaves the layer that declared it; a script \
             is relative to its own recipe"
        ));
        return;
    }

    if let Some(dir) = node.source.dir.as_ref()
        && !dir.join(path).is_file()
    {
        report(format!(
            "`{name}` points at `{file}`, which does not exist next to the recipe"
        ));
    }
}

/// The one rule in this file the validator does not know how to check.
///
/// `[item.permissions]` is written in the permission module's vocabulary, and
/// that module is the only part of shiro allowed to know a mechanism. So the
/// question is asked rather than answered here: the validator hands over the
/// declaration and prints the sentences it gets back, and learns nothing about
/// sockets, devices or flatpak in the process.
///
/// The alternative was to leave it to `apply`, which runs in the recipe's
/// `post`, on the machine of whoever installed. That is late by every measure
/// that matters: an image build validating its whole catalog reports success,
/// and under `rollback = "atomic"` the failure a user finally sees takes the
/// installation with it.
fn permissions(item: &ItemDecl, report: &mut impl FnMut(String)) {
    let Some(permissions) = &item.permissions else {
        return;
    };

    for finding in perms::findings(permissions) {
        report(finding);
    }
}
