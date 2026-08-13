//! The native commands: the ones that report on the engine's own state, and so
//! cannot be a recipe. Table in `docs/architecture.md` section 1.

use crate::error::Error;

/// `shiro doctor`: which layers loaded, which mechanisms exist on this host.
pub fn doctor(_args: Vec<String>) -> Result<(), Error> {
    Err(Error::NotImplemented("doctor"))
}

/// `shiro catalog validate` and `shiro catalog sources`.
pub fn catalog(_args: Vec<String>) -> Result<(), Error> {
    Err(Error::NotImplemented("catalog"))
}

/// `shiro version`: the version, and the digest of the merged catalog.
pub fn version(_args: Vec<String>) -> Result<(), Error> {
    Err(Error::NotImplemented("version"))
}
