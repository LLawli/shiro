//! What an invocation means once the arguments are known.
//!
//! The kind of the node a path resolves to decides everything: a menu lists its
//! children, an item runs its recipe. There is no `list` verb to collide with a
//! catalog author's naming, because a menu with no arguments left to consume
//! *is* the list.
//!
//! A `[[list]]` node is the same rule with its children generated rather than
//! declared: with nothing left to consume it lists what the generator prints,
//! and with one segment left, that segment names the entry to run.

use crate::catalog::model::ListDecl;
use crate::catalog::{Kind, Node, load, resolve};
use crate::cli::Options;
use crate::error::Error;
use crate::exec::{self, check, entries};
use crate::render::{Child, Listing, json, text};

pub fn path(segments: &[String], opts: &Options) -> Result<(), Error> {
    let catalog = load::load()?;
    let resolved = resolve::resolve(&catalog, segments)?;
    let node = resolved.node;

    if let Some(chosen) = resolved.entry {
        let node = node.expect("an entry is only reached through a list node");
        let list = node.list().expect("only a list node yields an entry");
        return exec::run(&entry(node, list, &chosen)?, opts);
    }

    if let Some(node) = node
        && !node.kind().navigable()
    {
        return exec::run(node, opts);
    }

    if opts.force || opts.dry_run || opts.keep_partial || opts.uninstall || opts.yes {
        return Err(Error::Usage(
            "those flags act on something that runs, and this path lists rather than runs"
                .to_owned(),
        ));
    }

    // Held here rather than inside the branch, because the listing borrows the
    // nodes and a generated one is owned by this invocation alone.
    let generated = match node.and_then(Node::list) {
        Some(list) => generate(node.expect("a list node was just read from it"), list)?,
        None => Vec::new(),
    };

    let children = match node.map(Node::kind) {
        Some(Kind::List) => generated.iter().collect(),
        _ => catalog.children(node.map(|node| node.path.as_str()).unwrap_or("")),
    };
    // One process per item, which is what having no state database costs. It is
    // paid in parallel, under a timeout, and never elevated.
    let statuses = check::batch(&children);

    let listing = Listing {
        node,
        children: children
            .into_iter()
            .zip(statuses)
            .map(|(node, status)| Child { node, status })
            .collect(),
    };

    if opts.json {
        println!("{}", json::menu(&listing));
    } else {
        print!("{}", text::menu(&listing));
    }
    Ok(())
}

/// The entries of a list, in the order the generator printed them. A generated
/// listing is not re-sorted: the generator is the only thing that knows what
/// order its subject has, and `order` is a field it cannot set.
fn generate(node: &Node, list: &ListDecl) -> Result<Vec<Node>, Error> {
    Ok(entries::of(node, list)?
        .iter()
        .map(|entry| entries::materialize(node, list, entry))
        .collect())
}

/// The entry a segment names, refused when the generator does not offer it.
///
/// Asking the generator before running is what makes the entry a choice out of
/// a list rather than a string handed to a hook. It also gives the same kind of
/// answer the tree gives for a wrong path: what was available where it stopped.
fn entry(node: &Node, list: &ListDecl, chosen: &str) -> Result<Node, Error> {
    let entries = entries::of(node, list)?;

    let Some(found) = entries.iter().find(|entry| entry.id == chosen) else {
        let available: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
        let where_ = node.path.replace('.', " ");
        return Err(Error::Usage(if available.is_empty() {
            format!("no such entry: `{chosen}` under `{where_}`, which is empty")
        } else {
            format!(
                "no such entry: `{chosen}` under `{where_}`\n`{where_}` has: {}",
                available.join(", ")
            )
        }));
    };

    Ok(entries::materialize(node, list, found))
}
