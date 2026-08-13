//! The `--json` payload, from `docs/architecture.md` section 6.
//!
//! Versioned by the `schema` field, because the Vicinae extension is a
//! separately released artifact: an unversioned break surfaces on a user's
//! machine rather than in CI. Running a recipe emits one object per phase
//! transition, so a front end can show progress instead of a spinner.
