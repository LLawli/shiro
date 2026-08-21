//! `shiro catalog sources`: which layer contributed or overrode each node.
//!
//! This exists to answer "why is this item behaving like that", which is the
//! worst class of bug an override system produces.

use serde_json::{Value, json};

use crate::catalog::Catalog;
use crate::render::json::SCHEMA;

pub fn text(catalog: &Catalog) -> String {
    let mut out = String::new();

    let width = catalog
        .iter()
        .map(|node| node.path.chars().count())
        .max()
        .unwrap_or(0);

    for node in catalog.iter() {
        // Hidden nodes are listed: this command is for auditing what the layers
        // did, and a node suppressed by a higher layer is exactly the kind of
        // thing someone comes here to find.
        out.push_str(&format!(
            "{path:width$}  {kind:6}  {layer:9}  {file}{hidden}\n",
            path = node.path,
            kind = node.kind().as_str(),
            layer = node.source.layer.as_str(),
            file = node.source.file,
            hidden = if node.hidden { " (hidden)" } else { "" },
        ));

        // Lowest first, which is the order they were replaced in.
        for shadowed in &node.shadowed {
            out.push_str(&format!(
                "{:width$}  {:6}  {:9}  {} (overridden)\n",
                "",
                "",
                shadowed.layer.as_str(),
                shadowed.file,
            ));
        }
    }

    if out.is_empty() {
        out.push_str("(the catalog is empty)\n");
    }
    out
}

pub fn json(catalog: &Catalog) -> String {
    let nodes: Vec<Value> = catalog
        .iter()
        .map(|node| {
            json!({
                "path": node.path,
                "kind": node.kind().as_str(),
                "layer": node.source.layer.as_str(),
                "file": node.source.file,
                "hidden": node.hidden,
                "overridden": node.shadowed.iter().map(|source| json!({
                    "layer": source.layer.as_str(),
                    "file": source.file,
                })).collect::<Vec<Value>>(),
            })
        })
        .collect();

    json!({ "schema": SCHEMA, "kind": "sources", "nodes": nodes }).to_string()
}
