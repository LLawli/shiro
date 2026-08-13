//! The catalog: the command surface itself.
//!
//! Sections 1 to 4 of `docs/architecture.md` live here. A path is resolved
//! against the merged tree, and the kind of the node it names decides what the
//! invocation means: a menu lists its children, an item runs its recipe.

pub mod load;
pub mod model;
pub mod resolve;
pub mod sources;
pub mod validate;

use crate::error::Error;

/// Resolve an argument path against the merged catalog and act on the node.
pub fn dispatch(_args: Vec<String>) -> Result<(), Error> {
    Err(Error::NotImplemented("catalog dispatch"))
}
