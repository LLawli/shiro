//! Walking an argument path into the merged tree.
//!
//! Consumes arguments while they name children. What is left when the walk
//! stops is either the node the caller meant, or an error that says how far the
//! path got and what was available there.

use crate::catalog::{Catalog, Kind, Node};
use crate::error::Error;

/// Where the arguments landed.
pub struct Resolved<'a> {
    /// The node they name, or `None` for the root, which every catalog has and
    /// no catalog declares.
    pub node: Option<&'a Node>,
    /// The segment after a list node, which is the entry it names. Whether
    /// such an entry exists is not a question the tree can answer: only the
    /// generator knows, and asking it is the caller's job.
    pub entry: Option<String>,
}

pub fn resolve<'a>(catalog: &'a Catalog, segments: &[String]) -> Result<Resolved<'a>, Error> {
    let mut walked = String::new();
    let mut current: Option<&Node> = None;

    for (index, segment) in segments.iter().enumerate() {
        if let Some(node) = current {
            if node.kind() == Kind::List {
                let rest = &segments[index + 1..];
                if let Some(extra) = rest.first() {
                    return Err(Error::Usage(format!(
                        "`{} {segment}` is an entry and takes no further arguments, but got \
                         `{extra}`",
                        node.path.replace('.', " ")
                    )));
                }
                return Ok(Resolved {
                    node: current,
                    entry: Some(segment.clone()),
                });
            }

            if node.kind() != Kind::Menu {
                return Err(Error::Usage(format!(
                    "`{}` is an {} and takes no further arguments, but got `{segment}`",
                    node.path,
                    node.kind().as_str()
                )));
            }
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

    Ok(Resolved {
        node: current,
        entry: None,
    })
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
