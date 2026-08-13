//! Reporting, in the two forms every invocation supports.
//!
//! The two consumers are a human at a terminal and a menu, and neither is the
//! afterthought: `--json` is a versioned contract with a `schema` field, and a
//! breaking change to it is a breaking change to shiro.

pub mod json;
pub mod text;
