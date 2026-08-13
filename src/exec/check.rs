//! The `check` hook, which is the only one invoked in bulk and unbidden.
//!
//! Three hard rules, from `docs/architecture.md` section 4: no side effects,
//! never elevated, always bounded. Batch invocation runs the children of a menu
//! in parallel, and that is contract rather than optimization: a menu that
//! spawns 200 checks serially is a menu nobody opens.
//!
//! The four answers are `installed`, `absent`, `unknown` and `timeout`. The
//! last two are honest answers, not failures.
