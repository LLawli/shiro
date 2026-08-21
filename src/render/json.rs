//! The `--json` payload, from `docs/architecture.md` section 6.
//!
//! Versioned by the `schema` field, because the Vicinae extension is a
//! separately released artifact: an unversioned break surfaces on a user's
//! machine rather than in CI. A field is omitted when it has no value rather
//! than emitted as null, so a consumer's absence check and its emptiness check
//! are the same check.

use serde_json::{Map, Value, json};

use crate::render::{Child, Listing};

/// Bumped by any change a consumer could notice, with a note in `CHANGELOG.md`.
pub const SCHEMA: u32 = 2;

pub fn menu(listing: &Listing<'_>) -> String {
    let payload = json!({
        "schema": SCHEMA,
        "kind": "menu",
        "path": listing.path(),
        "title": listing.title(),
        "children": listing.children.iter().map(child).collect::<Vec<Value>>(),
    });

    payload.to_string()
}

fn child(child: &Child<'_>) -> Value {
    let node = child.node;
    let mut object = Map::new();

    object.insert("path".into(), node.path.clone().into());
    object.insert("kind".into(), node.kind().as_str().into());
    object.insert("title".into(), node.title.clone().into());
    if let Some(description) = &node.description {
        object.insert("description".into(), description.clone().into());
    }
    if let Some(icon) = &node.icon {
        object.insert("icon".into(), icon.clone().into());
    }
    if !node.keywords.is_empty() {
        object.insert("keywords".into(), node.keywords.clone().into());
    }

    if let Some(run) = node.runnable() {
        if let Some(mechanism) = run.mechanism {
            object.insert("mechanism".into(), mechanism.into());
        }
        object.insert("privilege".into(), run.privilege.as_str().into());
        if let Some(confirm) = run.confirm {
            object.insert("confirm".into(), confirm.into());
        }
        // A false flag is omitted rather than emitted. It is the same rule as
        // an absent field: "is it set" and "is it true" are one check, and a
        // menu of two hundred children pays for every key that says nothing.
        for (key, set) in [
            ("destructive", run.destructive),
            ("interactive", run.interactive),
            ("keep_open", run.keep_open),
        ] {
            if set {
                object.insert(key.into(), true.into());
            }
        }
    }

    object.insert("layer".into(), node.source.layer.as_str().into());
    if let Some(status) = child.status {
        object.insert("status".into(), status.as_str().into());
    }

    Value::Object(object)
}
