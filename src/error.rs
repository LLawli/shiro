//! Errors, and the exit code each one leaves behind.

use std::fmt;

pub enum Error {
    /// The invocation itself was wrong: an unknown flag, a path that stops
    /// matching, arguments after an item.
    Usage(String),
    /// The catalog could not be loaded, or does not hold together.
    Catalog(String),
    /// A scaffolded entry point that parses its way here and stops. Every
    /// occurrence disappears as the module behind it is written.
    NotImplemented(&'static str),
}

impl Error {
    pub fn code(&self) -> u8 {
        match self {
            Error::Usage(_) => 2,
            Error::Catalog(_) => 1,
            Error::NotImplemented(_) => 70,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(message) | Error::Catalog(message) => f.write_str(message),
            Error::NotImplemented(what) => write!(f, "{what} is not implemented yet"),
        }
    }
}
