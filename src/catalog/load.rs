//! Discovering the four layers and merging them into one tree.
//!
//! Layers and precedence in `docs/architecture.md` section 2: built-in, image,
//! machine, user. Merge is per node path and wholesale, never field-level, and
//! `hidden = true` suppresses an inherited node without replacing it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs};

use crate::catalog::model::{CatalogFile, ItemDecl, MenuDecl};
use crate::catalog::{Catalog, Kind, Layer, LayerReport, Node, Source};
use crate::error::Error;

include!(concat!(env!("OUT_DIR"), "/builtin_catalog.rs"));

/// Where each layer lives, relative to `SHIRO_ROOT` for the two system ones.
const IMAGE_DIR: &str = "usr/share/shiro/catalog";
const MACHINE_DIR: &str = "etc/shiro/catalog";
const USER_DIR: &str = "shiro/catalog";

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
            let dir = layer_dir(layer);
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

            let mut found = Vec::new();
            collect(&dir, &mut found)?;
            found.sort();

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

fn layer_dir(layer: Layer) -> Option<PathBuf> {
    match layer {
        Layer::BuiltIn => None,
        // SHIRO_ROOT reroots the two system layers. It exists so that the
        // loader can be exercised, and a machine's catalog inspected, without
        // writing to /usr or /etc.
        Layer::Image => Some(root().join(IMAGE_DIR)),
        Layer::Machine => Some(root().join(MACHINE_DIR)),
        Layer::User => Some(user_data_dir()?.join(USER_DIR)),
    }
}

fn root() -> PathBuf {
    env::var_os("SHIRO_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn user_data_dir() -> Option<PathBuf> {
    if let Some(xdg) = env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(xdg));
    }
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(|home| PathBuf::from(home).join(".local/share"))
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>) -> Result<(), Error> {
    let entries = fs::read_dir(dir)
        .map_err(|err| Error::Catalog(format!("cannot read {}: {err}", dir.display())))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, found)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            found.push(path);
        }
    }
    Ok(())
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
    Ok(())
}

fn node_from_menu(decl: MenuDecl, source: &Source) -> Result<Node, Error> {
    check_path(&decl.path, source)?;
    Ok(Node {
        path: decl.path,
        kind: Kind::Menu,
        title: decl.title,
        description: decl.description,
        order: decl.order,
        hidden: decl.hidden,
        item: None,
        source: source.clone(),
        shadowed: Vec::new(),
    })
}

fn node_from_item(decl: ItemDecl, source: &Source) -> Result<Node, Error> {
    check_path(&decl.path, source)?;
    Ok(Node {
        path: decl.path.clone(),
        kind: Kind::Item,
        title: decl.title.clone(),
        description: decl.description.clone(),
        order: decl.order,
        hidden: decl.hidden,
        item: Some(decl),
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
        if segment.is_empty() {
            return bad("it has an empty segment");
        }
        if !segment
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return bad("a segment starts with something other than a lowercase letter or digit");
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return bad("a segment has a character outside [a-z0-9_-]");
        }
    }
    Ok(())
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
