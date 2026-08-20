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

use crate::error::Error;

/// The names the catalog may not claim as root nodes.
pub const NATIVE: [&str; 5] = ["doctor", "catalog", "version", "perms", "run"];

/// What the arguments turned out to be.
pub enum Invocation {
    Doctor(Vec<String>, Options),
    Catalog(Vec<String>, Options),
    Version(Vec<String>, Options),
    /// Everything after `perms` is handed over unparsed: the backend owns its
    /// own arguments.
    Perms(Vec<String>),
    /// Likewise, and more so: every argument after the application name belongs
    /// to the application, not to shiro.
    Run(Vec<String>),
    /// A path into the catalog. A menu lists itself, an item runs its recipe,
    /// and an empty path is the root menu.
    Path(Vec<String>, Options),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Options {
    pub json: bool,
    /// Skip the `check` gate, and nothing else.
    pub force: bool,
    /// Print what each hook would run, resolved, and run none of it.
    pub dry_run: bool,
    /// Downgrade `atomic` to `phase` for this invocation.
    pub keep_partial: bool,
    /// Answer an item's `confirm` before it is asked.
    ///
    /// Separate from `--force`, which skips the `check` gate and nothing else.
    /// The two guard different things: the gate is about the system's state,
    /// the question is about the user's intent.
    pub yes: bool,
    /// Remove the item instead of installing it.
    ///
    /// A flag rather than a verb, because the path is the item's address and
    /// the verbs belong to the catalog: `shiro uninstall install code vs-code`
    /// reads worse than the contradiction it avoids.
    pub uninstall: bool,
}

pub fn parse(args: &[String]) -> Result<Invocation, Error> {
    let Some((head, rest)) = args.split_first() else {
        return Ok(Invocation::Path(Vec::new(), Options::default()));
    };

    match head.as_str() {
        "perms" => Ok(Invocation::Perms(rest.to_vec())),
        "run" => Ok(Invocation::Run(rest.to_vec())),
        "doctor" => reporting(rest).map(|(rest, opts)| Invocation::Doctor(rest, opts)),
        "catalog" => reporting(rest).map(|(rest, opts)| Invocation::Catalog(rest, opts)),
        "version" => reporting(rest).map(|(rest, opts)| Invocation::Version(rest, opts)),
        _ => flagged(args, true).map(|(rest, opts)| Invocation::Path(rest, opts)),
    }
}

/// A native command that only reports takes `--json` and nothing else: there is
/// no transaction for `--force` or `--dry-run` to mean anything about.
fn reporting(args: &[String]) -> Result<(Vec<String>, Options), Error> {
    flagged(args, false)
}

/// Pull the flags shiro understands out of the arguments, wherever they sit, and
/// leave the rest in order. An unknown flag is refused rather than passed on as
/// a path segment: `shiro install code --jsno` should not report that `--jsno`
/// is not a command.
fn flagged(args: &[String], executing: bool) -> Result<(Vec<String>, Options), Error> {
    let mut opts = Options::default();
    let mut rest = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--json" => opts.json = true,
            "--force" if executing => opts.force = true,
            "--dry-run" if executing => opts.dry_run = true,
            "--keep-partial" if executing => opts.keep_partial = true,
            "--uninstall" if executing => opts.uninstall = true,
            "--yes" if executing => opts.yes = true,
            flag if flag.starts_with('-') => {
                return Err(Error::Usage(format!("unknown flag `{flag}`")));
            }
            _ => rest.push(arg.clone()),
        }
    }

    Ok((rest, opts))
}
