//! `shiro catalog validate`: the schema rules, enforced rather than documented.
//!
//! The list is in `docs/architecture.md` section 4, and it grows in the same
//! commit as the rule it enforces. What is here today:
//!
//! - a root node may not claim a name in the native namespace;
//! - `pre_mutates = true` requires `roll-pre`;
//! - `install` requires `roll-install` unless `rollback = "none"`;
//! - `check` may not declare `privilege = "system"`, and a `{ file = ... }`
//!   check may not point outside its own layer.
