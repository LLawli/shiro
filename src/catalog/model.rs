//! The declarations a catalog file may contain: menus, items, hooks and the
//! `[item.permissions]` block, as the serde types they deserialize into.
//!
//! Format in `docs/architecture.md` section 3. Two things this file has to keep
//! honest: `mechanism` is a label the engine never branches on, and a hook is a
//! shell string or a `{ file = "..." }` table, with no third form.
