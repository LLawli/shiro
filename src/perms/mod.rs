//! The permissions module: the one part of shiro that is allowed to know a
//! mechanism, because knowing about Flatpak overrides and bwrap is its purpose.
//!
//! Section 9 of `docs/architecture.md`, and the second rule in `CLAUDE.md`. The
//! exception survives only while the module stays sealed:
//!
//! 1. it is reachable from exactly two places, its own native commands and the
//!    engine recording an `[item.permissions]` declaration into the registry;
//! 2. `crate::exec` never calls into it;
//! 3. recording a declaration is a data write and nothing more: no bwrap, no
//!    flatpak, no wrapper script, no `.desktop` file.

pub mod bwrap;
pub mod flatpak;
pub mod profile;
pub mod registry;

use crate::error::Error;

/// `shiro perms <backend> …`, where the backend is always named in the command
/// and never inferred from the application. The verbosity is the feature: a
/// permission tool that guesses which mechanism it is talking to cannot be
/// audited.
pub fn perms(_args: Vec<String>) -> Result<(), Error> {
    Err(Error::NotImplemented("perms"))
}

/// `shiro run <app> [args…]`: resolve a profile, build the bwrap invocation
/// from it, exec the program.
///
/// A missing profile is a fallback, not an error, and the fallback grants close
/// to nothing: no network, no home, no devices, no session bus. It never runs
/// unconfined, and it warns on stderr naming the profile it looked for.
pub fn run(_args: Vec<String>) -> Result<(), Error> {
    Err(Error::NotImplemented("run"))
}
