//! What an invocation means once the arguments are known.
//!
//! The kind of the node a path resolves to decides everything: a menu lists its
//! children, an item runs its recipe. There is no `list` verb to collide with a
//! catalog author's naming, because a menu with no arguments left to consume
//! *is* the list.

use crate::catalog::{Kind, load, resolve};
use crate::cli::Options;
use crate::error::Error;
use crate::exec::check;
use crate::render::{Child, Listing, json, text};

pub fn path(segments: &[String], opts: &Options) -> Result<(), Error> {
    let catalog = load::load()?;
    let node = resolve::resolve(&catalog, segments)?;

    if let Some(node) = node
        && node.kind == Kind::Item
    {
        return Err(Error::NotImplemented("running a recipe"));
    }

    let children = catalog.children(node.map(|node| node.path.as_str()).unwrap_or(""));
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
