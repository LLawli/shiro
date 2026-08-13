//! Discovering the four layers and merging them into one tree.
//!
//! Layers and precedence in `docs/architecture.md` section 2: built-in, image,
//! machine, user. Merge is per node path and wholesale, never field-level, and
//! `hidden = true` suppresses an inherited node without replacing it.
//!
//! Startup time is the constraint that shapes this module: a layer that cannot
//! contribute to the requested path is not parsed.
