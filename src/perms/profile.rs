//! Profiles: one TOML file per application, in the same four layers as the
//! catalog and with the same wholesale replacement, because a half-overridden
//! sandbox is a sandbox nobody can reason about.
//!
//! The user layer may loosen a profile, not only tighten it. A user who cannot
//! grant their own browser one directory will edit the `.desktop` file and
//! bypass `shiro run` entirely, which removes the sandbox instead of adjusting
//! it.
