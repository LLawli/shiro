//! Output for a human at a terminal: a listed menu, and errors that say what
//! was available where the path stopped matching.

use crate::catalog::Kind;
use crate::layers::{self, Layer};
use crate::render::Listing;

pub fn menu(listing: &Listing<'_>) -> String {
    let mut out = String::new();

    out.push_str(listing.title());
    out.push('\n');
    if let Some(description) = listing.node.and_then(|node| node.description.as_deref()) {
        out.push_str(description);
        out.push('\n');
    }

    if listing.children.is_empty() {
        // The built-in layer holds the root nodes and no recipes, so an empty
        // menu is what a fresh binary shows. Saying where children come from
        // turns that from a dead end into an instruction.
        out.push_str("\n  (nothing here yet)\n\n");
        out.push_str("  Recipes come from a catalog layer:\n");
        for layer in [Layer::Image, Layer::Machine, Layer::User] {
            if let Some(dir) = layers::dir(layer, "catalog") {
                out.push_str(&format!("    {:8}  {}\n", layer.as_str(), dir.display()));
            }
        }
        return out;
    }

    let width = listing
        .children
        .iter()
        .map(|child| child.node.segment().chars().count())
        .max()
        .unwrap_or(0);
    let titles = listing
        .children
        .iter()
        .map(|child| child.node.title.chars().count())
        .max()
        .unwrap_or(0);

    out.push('\n');
    for child in &listing.children {
        let segment = child.node.segment();
        let status = match (child.node.kind(), child.status) {
            // A menu is not a thing that is installed or missing, it is a place
            // to go, and the arrow is what says so.
            (Kind::Menu, _) => "\u{203a}".to_owned(),
            (Kind::Item, Some(status)) => status.as_str().to_owned(),
            (Kind::Item, None) => String::new(),
        };

        out.push_str(&format!(
            "  {segment:width$}  {title:titles$}  {status}\n",
            title = child.node.title,
        ));
    }

    out
}
