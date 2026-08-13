//! The four layers, and where each one lives on disk.
//!
//! Catalog nodes and permission profiles use the same layers, the same
//! precedence and the same wholesale replacement, so they resolve their
//! directories through the same place.

use std::path::PathBuf;
use std::{env, fs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    BuiltIn,
    Image,
    Machine,
    User,
}

impl Layer {
    /// Lowest precedence first, which is also catalog load order: a later layer
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

/// The directory a layer keeps `kind` in, where `kind` is `catalog` or
/// `profiles`. The built-in layer has none: it lives in the binary.
pub fn dir(layer: Layer, kind: &str) -> Option<PathBuf> {
    match layer {
        Layer::BuiltIn => None,
        // SHIRO_ROOT reroots the two system layers. It exists so that the
        // loader can be exercised, and a machine's catalog inspected, without
        // writing to /usr or /etc.
        Layer::Image => Some(root().join("usr/share/shiro").join(kind)),
        Layer::Machine => Some(root().join("etc/shiro").join(kind)),
        Layer::User => Some(user_data_dir()?.join("shiro").join(kind)),
    }
}

pub fn root() -> PathBuf {
    env::var_os("SHIRO_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn user_data_dir() -> Option<PathBuf> {
    if let Some(xdg) = env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(xdg));
    }
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(|home| PathBuf::from(home).join(".local/share"))
}

/// Every `.toml` under a directory, sorted, so that a layer reads the same way
/// twice regardless of what order the filesystem hands entries back.
pub fn toml_files(dir: &std::path::Path) -> std::io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    collect(dir, &mut found)?;
    found.sort();
    Ok(found)
}

fn collect(dir: &std::path::Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, found)?;
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            found.push(path);
        }
    }
    Ok(())
}
