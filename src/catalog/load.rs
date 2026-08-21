//! Discovering the four layers and merging them into one tree.
//!
//! Layers and precedence in `docs/architecture.md` section 2: built-in, image,
//! machine, user. Merge is per node path and wholesale, never field-level, and
//! `hidden = true` suppresses an inherited node without replacing it.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::catalog::model::{ActionDecl, CatalogFile, ItemDecl, ListDecl, MenuDecl};
use crate::catalog::{Body, Catalog, Layer, LayerReport, Node, Source};
use crate::error::Error;
use crate::layers;

include!(concat!(env!("OUT_DIR"), "/builtin_catalog.rs"));

pub fn load() -> Result<Catalog, Error> {
    let mut nodes = BTreeMap::new();
    let mut layers = Vec::new();

    for layer in Layer::ALL {
        layers.push(load_layer(layer, &mut nodes)?);
    }

    // Counted once everything is merged, so a layer reports what it still owns
    // rather than what it contributed before a higher layer took some of it.
    for report in &mut layers {
        report.nodes = nodes
            .values()
            .filter(|node| node.source.layer == report.layer)
            .count();
    }

    Ok(Catalog::new(nodes, layers))
}

fn load_layer(layer: Layer, nodes: &mut BTreeMap<String, Node>) -> Result<LayerReport, Error> {
    let (location, present, files) = match layer {
        Layer::BuiltIn => {
            for (name, contents) in BUILTIN_CATALOG {
                let source = Source {
                    layer,
                    file: format!("<built-in>/{name}"),
                    dir: None,
                };
                merge_file(contents, &source, nodes)?;
            }
            ("<built-in>".to_owned(), true, BUILTIN_CATALOG.len())
        }
        _ => {
            let dir = layers::dir(layer, "catalog");
            let location = dir
                .as_ref()
                .map(|dir| dir.display().to_string())
                .unwrap_or_else(|| "<unset>".to_owned());

            let Some(dir) = dir.filter(|dir| dir.is_dir()) else {
                return Ok(LayerReport {
                    layer,
                    location,
                    present: false,
                    files: 0,
                    nodes: 0,
                });
            };

            let found = layers::toml_files(&dir)
                .map_err(|err| Error::Catalog(format!("cannot read {}: {err}", dir.display())))?;

            for file in &found {
                let contents = fs::read_to_string(file).map_err(|err| {
                    Error::Catalog(format!("cannot read {}: {err}", file.display()))
                })?;
                let source = Source {
                    layer,
                    file: file.display().to_string(),
                    dir: file.parent().map(Path::to_path_buf),
                };
                merge_file(&contents, &source, nodes)?;
            }

            (location, true, found.len())
        }
    };

    Ok(LayerReport {
        layer,
        location,
        present,
        files,
        nodes: 0,
    })
}

fn merge_file(
    contents: &str,
    source: &Source,
    nodes: &mut BTreeMap<String, Node>,
) -> Result<(), Error> {
    let parsed: CatalogFile = toml::from_str(contents)
        .map_err(|err| Error::Catalog(format!("{}: {err}", source.file)))?;

    for menu in parsed.menu {
        merge(node_from_menu(menu, source)?, source, nodes)?;
    }
    for item in parsed.item {
        merge(node_from_item(item, source)?, source, nodes)?;
    }
    for action in parsed.action {
        merge(node_from_action(action, source)?, source, nodes)?;
    }
    for list in parsed.list {
        merge(node_from_list(list, source)?, source, nodes)?;
    }
    Ok(())
}

fn node_from_menu(decl: MenuDecl, source: &Source) -> Result<Node, Error> {
    check_path(&decl.path, source)?;
    Ok(Node {
        path: decl.path,
        title: decl.title,
        description: decl.description,
        order: decl.order,
        hidden: decl.hidden,
        icon: decl.icon,
        keywords: decl.keywords,
        body: Body::Menu,
        entry: None,
        source: source.clone(),
        shadowed: Vec::new(),
    })
}

fn node_from_item(decl: ItemDecl, source: &Source) -> Result<Node, Error> {
    check_path(&decl.path, source)?;
    Ok(Node {
        path: decl.path.clone(),
        title: decl.title.clone(),
        description: decl.description.clone(),
        order: decl.order,
        hidden: decl.hidden,
        icon: decl.icon.clone(),
        keywords: decl.keywords.clone(),
        body: Body::Item(Box::new(decl)),
        entry: None,
        source: source.clone(),
        shadowed: Vec::new(),
    })
}

fn node_from_action(decl: ActionDecl, source: &Source) -> Result<Node, Error> {
    check_path(&decl.path, source)?;
    Ok(Node {
        path: decl.path.clone(),
        title: decl.title.clone(),
        description: decl.description.clone(),
        order: decl.order,
        hidden: decl.hidden,
        icon: decl.icon.clone(),
        keywords: decl.keywords.clone(),
        body: Body::Action(Box::new(decl)),
        entry: None,
        source: source.clone(),
        shadowed: Vec::new(),
    })
}

fn node_from_list(decl: ListDecl, source: &Source) -> Result<Node, Error> {
    check_path(&decl.path, source)?;
    Ok(Node {
        path: decl.path.clone(),
        title: decl.title.clone(),
        description: decl.description.clone(),
        order: decl.order,
        hidden: decl.hidden,
        icon: decl.icon.clone(),
        keywords: decl.keywords.clone(),
        body: Body::List(Box::new(decl)),
        entry: None,
        source: source.clone(),
        shadowed: Vec::new(),
    })
}

/// A path is segments joined by dots, and a segment is what a user types. The
/// character set is narrow on purpose: a segment that needs quoting is a
/// segment nobody can type from a menu.
fn check_path(path: &str, source: &Source) -> Result<(), Error> {
    let bad = |reason: &str| {
        Err(Error::Catalog(format!(
            "{}: invalid path `{path}`: {reason}",
            source.file
        )))
    };

    if path.is_empty() {
        return bad("it is empty");
    }
    for segment in path.split('.') {
        if let Some(reason) = segment_problem(segment) {
            return bad(reason);
        }
    }
    Ok(())
}

/// What is wrong with one segment, or `None`. Shared with the entries a
/// generator prints, because an entry's `id` is a segment a user types: it
/// reaches the tree through the same rule or it does not reach it at all.
pub fn segment_problem(segment: &str) -> Option<&'static str> {
    if segment.is_empty() {
        return Some("it has an empty segment");
    }
    if !segment
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return Some("a segment starts with something other than a lowercase letter or digit");
    }
    if !segment
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Some("a segment has a character outside [a-z0-9_-]");
    }
    None
}

/// Wholesale replacement, and a duplicate within one layer is an error: two
/// files in the same layer declaring one path is a mistake nobody meant, and
/// the winner would depend on filesystem order.
fn merge(node: Node, source: &Source, nodes: &mut BTreeMap<String, Node>) -> Result<(), Error> {
    let mut node = node;

    if let Some(existing) = nodes.remove(&node.path) {
        if existing.source.layer == source.layer {
            return Err(Error::Catalog(format!(
                "{}: `{}` is already declared in {} of the same layer",
                source.file, node.path, existing.source.file
            )));
        }
        node.shadowed = existing.shadowed;
        node.shadowed.push(existing.source);
    }

    nodes.insert(node.path.clone(), node);
    Ok(())
}
