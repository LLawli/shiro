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
pub use crate::layers::Layer;

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

/// What a node is, and what it carries with it.
///
/// The kind is derived from this rather than stored beside it. Kept as two
/// fields, a node whose kind says one thing and whose contents say another is
/// representable, and every reader has to remember which of the two to trust.
///
/// The recipe is boxed so that a menu costs a pointer rather than the size of
/// the largest variant. The `Option<ItemDecl>` this replaced had the same hole
/// and nothing was there to notice it: every menu node in the tree carried an
/// unused recipe's worth of padding through every move the map made.
#[derive(Debug)]
pub enum Body {
    Menu,
    Item(Box<ItemDecl>),
}

impl Body {
    fn kind(&self) -> Kind {
        match self {
            Body::Menu => Kind::Menu,
            Body::Item(_) => Kind::Item,
        }
    }
}

#[derive(Debug)]
pub struct Node {
    pub path: String,
    pub title: String,
    pub description: Option<String>,
    pub order: Option<i64>,
    pub hidden: bool,
    /// Lifted out of the declaration, like the title, because a menu declares
    /// them too and a menu has no `ItemDecl` to read them from.
    pub icon: Option<String>,
    pub keywords: Vec<String>,
    pub body: Body,
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

    pub fn kind(&self) -> Kind {
        self.body.kind()
    }

    /// The recipe, for an item, and nothing for anything else.
    pub fn item(&self) -> Option<&ItemDecl> {
        match &self.body {
            Body::Item(item) => Some(item),
            _ => None,
        }
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
                node.title.as_str(),
                node.description.as_deref().unwrap_or(""),
                &node
                    .order
                    .map(|order| order.to_string())
                    .unwrap_or_default(),
                if node.hidden { "hidden" } else { "" },
                node.icon.as_deref().unwrap_or(""),
                &node.keywords.join("\u{1f}"),
                node.source.layer.as_str(),
                // The body covers the kind as well, since its `Debug` names the
                // variant before anything else.
                &format!("{:?}", node.body),
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
