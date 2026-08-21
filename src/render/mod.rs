//! Reporting, in the two forms every invocation supports.
//!
//! The two consumers are a human at a terminal and a menu, and neither is the
//! afterthought: `--json` is a versioned contract with a `schema` field, and a
//! breaking change to it is a breaking change to shiro.

pub mod json;
pub mod progress;
pub mod text;

use crate::catalog::Node;
use crate::exec::check::Status;

/// A menu and its children, with whatever each child's `check` answered. The
/// root is a listing too, with no node behind it.
pub struct Listing<'a> {
    pub node: Option<&'a Node>,
    pub children: Vec<Child<'a>>,
}

pub struct Child<'a> {
    pub node: &'a Node,
    pub status: Option<Status>,
}

impl Listing<'_> {
    pub fn path(&self) -> &str {
        self.node.map(|node| node.path.as_str()).unwrap_or("")
    }

    /// What is being listed. A list says so rather than passing for a menu: a
    /// front end that caches a level should know that this one was generated,
    /// and the root, which no catalog declares, is a menu.
    pub fn kind(&self) -> &'static str {
        self.node.map(|node| node.kind().as_str()).unwrap_or("menu")
    }

    pub fn title(&self) -> &str {
        self.node.map(|node| node.title.as_str()).unwrap_or("shiro")
    }
}
