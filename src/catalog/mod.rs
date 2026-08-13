//! The catalog: the command surface itself.
//!
//! Sections 1 to 4 of `docs/architecture.md` live here. A path is resolved
//! against the merged tree, and the kind of the node it names decides what the
//! invocation means: a menu lists its children, an item runs its recipe.

pub mod load;
pub mod model;
pub mod resolve;
pub mod sources;
pub mod validate;

use std::collections::BTreeMap;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::catalog::model::ItemDecl;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    BuiltIn,
    Image,
    Machine,
    User,
}

impl Layer {
    /// Lowest precedence first, which is also load order: a later layer
    /// replaces what an earlier one declared.
    pub const ALL: [Layer; 4] = [Layer::BuiltIn, Layer::Image, Layer::Machine, Layer::User];

    pub fn as_str(self) -> &'static str {
        match self {
            Layer::BuiltIn => "built-in",
            Layer::Image => "image",
            Layer::Machine => "machine",
            Layer::User => "user",
        }
    }
}

/// Where a declaration came from.
#[derive(Debug, Clone)]
pub struct Source {
    pub layer: Layer,
    /// The TOML file, as it should be shown to a human.
    pub file: String,
    /// What relative script paths resolve against, and what a hook is given as
    /// `SHIRO_RECIPE_DIR`. The built-in layer has none: it lives in the binary.
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Menu,
    Item,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Menu => "menu",
            Kind::Item => "item",
        }
    }
}

#[derive(Debug)]
pub struct Node {
    pub path: String,
    pub kind: Kind,
    pub title: String,
    pub description: Option<String>,
    pub order: Option<i64>,
    pub hidden: bool,
    /// The recipe, for an item.
    pub item: Option<ItemDecl>,
    pub source: Source,
    /// Declarations this one replaced, lowest layer first. Empty for a node
    /// that only one layer declares.
    pub shadowed: Vec<Source>,
}

impl Node {
    /// The last segment: what the user types to reach this node from its
    /// parent.
    pub fn segment(&self) -> &str {
        self.path.rsplit('.').next().unwrap_or(&self.path)
    }
}

/// What a layer contributed, for `doctor` and `catalog sources`.
#[derive(Debug)]
pub struct LayerReport {
    pub layer: Layer,
    pub location: String,
    pub present: bool,
    pub files: usize,
    pub nodes: usize,
}

#[derive(Debug)]
pub struct Catalog {
    nodes: BTreeMap<String, Node>,
    pub layers: Vec<LayerReport>,
}

impl Catalog {
    pub fn new(nodes: BTreeMap<String, Node>, layers: Vec<LayerReport>) -> Self {
        Catalog { nodes, layers }
    }

    pub fn get(&self, path: &str) -> Option<&Node> {
        self.nodes.get(path)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// The direct children of a node, or of the root when `path` is empty.
    ///
    /// Ordered by `order`, then by title, so that a menu that declares no order
    /// still draws the same way twice. Nodes without `order` sort after the
    /// ones that have it: a partial ordering is a common state in a catalog
    /// assembled from several layers, and it should not scatter the ordered
    /// ones.
    pub fn children(&self, path: &str) -> Vec<&Node> {
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}.")
        };

        let mut children: Vec<&Node> = self
            .nodes
            .range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
            .map(|(_, node)| node)
            .filter(|node| !node.hidden && !node.path[prefix.len()..].contains('.'))
            .collect();

        children.sort_by(|a, b| {
            a.order
                .unwrap_or(i64::MAX)
                .cmp(&b.order.unwrap_or(i64::MAX))
                .then_with(|| a.title.cmp(&b.title))
        });
        children
    }

    /// A short digest of the merged tree, for answering "are these two hosts
    /// running the same catalog".
    ///
    /// It covers what a node declares, not where the file sits, so the same
    /// catalog reached through a different mount point digests the same. It is
    /// stable for one build of shiro, which is the comparison it is for: the
    /// recipe body is hashed through its `Debug` form, and that shape follows
    /// the schema rather than any promise across versions.
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();

        for (path, node) in &self.nodes {
            for field in [
                path.as_str(),
                node.kind.as_str(),
                node.title.as_str(),
                node.description.as_deref().unwrap_or(""),
                &node
                    .order
                    .map(|order| order.to_string())
                    .unwrap_or_default(),
                if node.hidden { "hidden" } else { "" },
                node.source.layer.as_str(),
                &node
                    .item
                    .as_ref()
                    .map(|item| format!("{item:?}"))
                    .unwrap_or_default(),
            ] {
                hasher.update(field.as_bytes());
                hasher.update(b"\0");
            }
        }

        hasher
            .finalize()
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}
