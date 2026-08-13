//! Running a recipe: the phase order, the rollback policy, and the report.
//!
//! Execution model in `docs/architecture.md` section 5. The rule this module
//! exists to obey is the first one in `CLAUDE.md`: it never learns a mechanism.
//! There is no branch on `mechanism` here, for any reason, and it never calls
//! into `crate::perms`.

pub mod check;
pub mod env;
pub mod hook;
pub mod rollback;
