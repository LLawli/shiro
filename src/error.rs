//! Errors, and the exit code each one leaves behind.

use std::fmt;

pub enum Error {
    /// A scaffolded entry point that parses its way here and stops. Every
    /// occurrence disappears as the module behind it is written.
    NotImplemented(&'static str),
}

impl Error {
    pub fn code(&self) -> u8 {
        match self {
            Error::NotImplemented(_) => 70,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotImplemented(what) => write!(f, "{what} is not implemented yet"),
        }
    }
}
