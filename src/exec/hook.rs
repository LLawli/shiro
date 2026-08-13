//! Turning a declared hook into a process.
//!
//! A hook is a shell string passed to `sh -c`, or a `{ file = "..." }` script
//! resolved relative to the TOML file that declared it. Privilege is decided
//! per item, before anything runs, so that a transaction never stops halfway to
//! ask for a password.
