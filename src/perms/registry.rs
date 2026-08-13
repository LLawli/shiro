//! The profile registry, and the single write the engine performs on its own
//! initiative: recording an item's `[item.permissions]` declaration.
//!
//! The write lands in the layer the item's privilege implies, `/etc` for a
//! `system` item and `$XDG_DATA_HOME` for a `user` one, never `/usr/share`,
//! which belongs to the image and is read-only on bootc.
//!
//! Because it is the engine's own mutation rather than a recipe's, the engine
//! owns reverting it: it participates in the item's rollback policy with
//! nothing declared by the recipe.
//!
//! **This module writes data and does nothing else.** It does not invoke bwrap
//! or flatpak, does not generate a wrapper, does not touch a `.desktop` file.
//! Generating whatever calls `shiro run` is the recipe's `post`.

use std::fs;
use std::path::PathBuf;

use crate::catalog::model::{PermissionsDecl, Privilege};
use crate::error::Error;
use crate::layers::Layer;
use crate::perms::profile;

/// A write that happened, and what was there before it, so the engine can put
/// it back.
pub struct Record {
    pub path: PathBuf,
    previous: Option<String>,
}

/// Where the declaration would land. A `system` item writes to the machine
/// layer and a `user` item to the user's, never to `/usr/share`, which belongs
/// to the image and is read-only on bootc.
pub fn target(decl: &PermissionsDecl, privilege: Privilege) -> Result<PathBuf, Error> {
    profile::check_name(&decl.app)?;

    let layer = match privilege {
        Privilege::System => Layer::Machine,
        Privilege::User => Layer::User,
    };

    profile::path_for(layer, &decl.app).ok_or_else(|| {
        Error::Failed(format!(
            "cannot record a profile for `{}`: the {} layer has no directory here",
            decl.app,
            layer.as_str()
        ))
    })
}

pub fn record(decl: &PermissionsDecl, privilege: Privilege) -> Result<Record, Error> {
    let path = target(decl, privilege)?;
    let previous = fs::read_to_string(&path).ok();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| Error::Failed(format!("cannot create {}: {err}", parent.display())))?;
    }
    fs::write(&path, render(decl))
        .map_err(|err| Error::Failed(format!("cannot write {}: {err}", path.display())))?;

    Ok(Record { path, previous })
}

/// Put back exactly what was there, which for a profile that did not exist
/// means removing the file rather than leaving an empty one.
pub fn restore(record: &Record) -> Result<(), Error> {
    let result = match &record.previous {
        Some(contents) => fs::write(&record.path, contents),
        None => fs::remove_file(&record.path),
    };

    result.map_err(|err| {
        Error::RollbackFailed(format!("cannot restore {}: {err}", record.path.display()))
    })
}

/// The declaration, as a profile file. A recipe's `[item.permissions]` block and
/// a profile are the same shape, so this is a copy with the keys ordered rather
/// than a translation.
fn render(decl: &PermissionsDecl) -> String {
    let mut table = toml::Table::new();
    table.insert("backend".to_owned(), decl.backend.clone().into());
    table.insert("app".to_owned(), decl.app.clone().into());
    for (key, value) in &decl.body {
        table.insert(key.clone(), value.clone());
    }

    format!(
        "# Written by shiro from the `[item.permissions]` of a recipe.\n# Edits here are kept: \
         a higher layer replaces this file wholesale.\n\n{table}"
    )
}
