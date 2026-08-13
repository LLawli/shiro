//! The profile registry, and the single write the engine performs on its own
//! initiative: recording an item's `[item.permissions]` declaration.
//!
//! The write lands in the layer the item's privilege implies, `/etc` for a
//! `system` item and `$XDG_DATA_HOME` for a `user` one, never `/usr/share`,
//! which belongs to the image and is read-only on bootc.
//!
//! Because it is the engine's own mutation rather than a recipe's, the engine
//! owns reverting it: it participates in the item's rollback policy with
//! nothing declared by the recipe.
