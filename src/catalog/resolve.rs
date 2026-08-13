//! Walking an argument path into the merged tree.
//!
//! Consumes arguments while they name children. What is left when the walk
//! stops is either the node the caller meant, or an error that says how far the
//! path got and what was available there.

use crate::catalog::{Catalog, Kind, Node};
use crate::error::Error;

/// The node the arguments name, or `None` for the root, which every catalog has
/// and no catalog declares.
pub fn resolve<'a>(catalog: &'a Catalog, segments: &[String]) -> Result<Option<&'a Node>, Error> {
    let mut walked = String::new();
    let mut current: Option<&Node> = None;

    for segment in segments {
        if let Some(node) = current
            && node.kind == Kind::Item
        {
            return Err(Error::Usage(format!(
                "`{}` is an item and takes no further arguments, but got `{segment}`",
                node.path
            )));
        }

        let candidate = if walked.is_empty() {
            segment.clone()
        } else {
            format!("{walked}.{segment}")
        };

        match catalog.get(&candidate) {
            Some(node) if !node.hidden => {
                walked = candidate;
                current = Some(node);
            }
            _ => return Err(unknown(catalog, &walked, segment)),
        }
    }

    Ok(current)
}

fn unknown(catalog: &Catalog, walked: &str, segment: &str) -> Error {
    let available: Vec<&str> = catalog
        .children(walked)
        .iter()
        .map(|node| node.segment())
        .collect();

    let where_ = if walked.is_empty() {
        "shiro".to_owned()
    } else {
        format!("`{}`", walked.replace('.', " "))
    };

    if available.is_empty() {
        Error::Usage(format!("no such command: `{segment}` under {where_}"))
    } else {
        Error::Usage(format!(
            "no such command: `{segment}` under {where_}\n{where_} has: {}",
            available.join(", ")
        ))
    }
}
