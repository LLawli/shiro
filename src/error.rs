//! Errors, and the exit code each one leaves behind.
//!
//! The codes are part of the contract, because a front end acts on them: they
//! are listed in `docs/architecture.md` section 5 and change with it.

use std::fmt;

pub enum Error {
    /// The invocation itself was wrong: an unknown flag, a path that stops
    /// matching, arguments after an item.
    Usage(String),
    /// The catalog could not be loaded, or does not hold together.
    Catalog(String),
    /// shiro declined to act, and nothing happened: the item is already
    /// installed, or is not installed and cannot be removed.
    Refused(String),
    /// A hook failed. Whatever the rollback policy asked for was done.
    Failed(String),
    /// A rollback hook failed, which is a different kind of bad: the system is
    /// in a state neither the user nor the recipe author intended.
    RollbackFailed(String),
}

impl Error {
    pub fn code(&self) -> u8 {
        match self {
            Error::Catalog(_) | Error::Failed(_) => 1,
            Error::Usage(_) => 2,
            Error::Refused(_) => 3,
            Error::RollbackFailed(_) => 4,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(message)
            | Error::Catalog(message)
            | Error::Refused(message)
            | Error::Failed(message)
            | Error::RollbackFailed(message) => f.write_str(message),
        }
    }
}
