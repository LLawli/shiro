//! Splitting argv into a native command or a path into the catalog.
//!
//! The curated surface is data (`docs/architecture.md` section 1), so there is
//! nothing for a declarative parser to describe: the subcommands are not known
//! until the catalog is loaded. What is fixed is the small native namespace,
//! and everything that is not in it is a path into the tree.
//!
//! That namespace is reserved. The validator rejects a catalog declaring a root
//! node with one of these names, because a shadowed command is worse than a
//! rejected one: it fails as a command that quietly does the wrong thing.

/// What the arguments turned out to be. Each variant carries what is left after
/// the command word is consumed, unparsed: flags belong to whoever handles them.
pub enum Invocation {
    Doctor(Vec<String>),
    Catalog(Vec<String>),
    Version(Vec<String>),
    Perms(Vec<String>),
    Run(Vec<String>),
    /// A path into the catalog. A menu lists itself, an item runs its recipe,
    /// and an empty path is the root menu.
    Path(Vec<String>),
}

pub fn parse(args: &[String]) -> Invocation {
    let Some((head, rest)) = args.split_first() else {
        return Invocation::Path(Vec::new());
    };
    let rest = rest.to_vec();

    match head.as_str() {
        "doctor" => Invocation::Doctor(rest),
        "catalog" => Invocation::Catalog(rest),
        "version" => Invocation::Version(rest),
        "perms" => Invocation::Perms(rest),
        "run" => Invocation::Run(rest),
        _ => Invocation::Path(args.to_vec()),
    }
}
