//! The declarations a catalog file may contain: menus, items, hooks and the
//! `[item.permissions]` block, as the serde types they deserialize into.
//!
//! Format in `docs/architecture.md` section 3. Two things this file has to keep
//! honest: `mechanism` is a label the engine never branches on, and a hook is a
//! shell string or a `{ file = "..." }` table, with no third form.
//!
//! Unknown keys are rejected everywhere. A catalog is hand-written, and the
//! failure mode of ignoring `mechnism = "flatpak"` is an item that silently
//! loses a field its author believed they had set.

use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

/// One `.toml` file in a layer.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogFile {
    #[serde(default)]
    pub menu: Vec<MenuDecl>,
    #[serde(default)]
    pub item: Vec<ItemDecl>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuDecl {
    pub path: String,
    pub title: String,
    pub description: Option<String>,
    pub order: Option<i64>,
    /// Suppress a node inherited from a lower layer without replacing it.
    #[serde(default)]
    pub hidden: bool,
    /// A Nerd Font glyph or an XDG icon name. shiro does not interpret it.
    pub icon: Option<String>,
    /// Search terms beyond the title, so that "wifi" finds "Rede".
    #[serde(default)]
    pub keywords: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemDecl {
    pub path: String,
    pub title: String,
    pub description: Option<String>,
    pub order: Option<i64>,
    #[serde(default)]
    pub hidden: bool,
    /// A label, for grouping and display. The engine never branches on it.
    pub mechanism: Option<String>,
    #[serde(default)]
    pub privilege: Privilege,
    #[serde(default)]
    pub rollback: Rollback,
    /// Declares that `pre` changes the system, which makes `roll-pre`
    /// mandatory.
    #[serde(default)]
    pub pre_mutates: bool,
    /// A Nerd Font glyph or an XDG icon name. shiro does not interpret it.
    pub icon: Option<String>,
    /// Search terms beyond the title.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// The question to ask before running. Its presence is what asks, and its
    /// value is the wording, so that the layer which declared the item is also
    /// the one that decides what language the question is in.
    pub confirm: Option<String>,
    /// Style it as destructive. Not the same as `confirm`, and neither implies
    /// the other.
    #[serde(default)]
    pub destructive: bool,
    /// The recipe needs a terminal: it prompts, or its progress is the point.
    /// No engine behaviour, since a hook already inherits stdin. It tells a
    /// front end to open a terminal rather than render a progress stream.
    #[serde(default)]
    pub interactive: bool,
    /// After running, the menu stays where it is. Right for "next wallpaper",
    /// wrong for "reboot".
    #[serde(default)]
    pub keep_open: bool,
    #[serde(default)]
    pub hooks: Hooks,
    /// Recorded into the profile registry by the engine, and nothing else.
    pub permissions: Option<PermissionsDecl>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Privilege {
    #[default]
    User,
    System,
}

impl Privilege {
    pub fn as_str(self) -> &'static str {
        match self {
            Privilege::User => "user",
            Privilege::System => "system",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rollback {
    #[default]
    Atomic,
    Phase,
    None,
}

impl Rollback {
    pub fn as_str(self) -> &'static str {
        match self {
            Rollback::Atomic => "atomic",
            Rollback::Phase => "phase",
            Rollback::None => "none",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hooks {
    pub check: Option<Hook>,
    pub pre: Option<Hook>,
    pub install: Option<Hook>,
    pub post: Option<Hook>,
    #[serde(rename = "roll-pre")]
    pub roll_pre: Option<Hook>,
    #[serde(rename = "roll-install")]
    pub roll_install: Option<Hook>,
    #[serde(rename = "roll-post")]
    pub roll_post: Option<Hook>,
    pub uninstall: Option<Hook>,
}

/// A shell command, or a script file relative to the directory of the TOML
/// that declared it. There is deliberately no third form.
#[derive(Debug, Clone)]
pub enum Hook {
    Shell(String),
    Script(String),
}

// Hand-written so that a table with a stray key fails with a message naming the
// key, rather than serde reporting that no untagged variant matched. A hook
// table that quietly ignores `privilege = "system"` is exactly the kind of
// silent loss this schema cannot afford.
impl<'de> Deserialize<'de> for Hook {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct HookVisitor;

        impl<'de> Visitor<'de> for HookVisitor {
            type Value = Hook;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a shell command, or a table { file = \"path\" }")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Hook, E> {
                Ok(Hook::Shell(value.to_owned()))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Hook, A::Error> {
                let mut file = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "file" {
                        file = Some(map.next_value::<String>()?);
                    } else {
                        return Err(de::Error::custom(format!(
                            "unknown key `{key}` in a hook; the only key is `file`"
                        )));
                    }
                }
                file.map(Hook::Script)
                    .ok_or_else(|| de::Error::missing_field("file"))
            }
        }

        deserializer.deserialize_any(HookVisitor)
    }
}

/// The profile a recipe declares for what it installs. The body past `backend`
/// and `app` belongs to the backend, and the engine does not interpret it.
#[derive(Debug, Deserialize)]
pub struct PermissionsDecl {
    pub backend: String,
    pub app: String,
    /// Read by the profile registry when the engine records the declaration.
    /// It is captured rather than ignored because dropping it here would make
    /// a recipe's profile silently smaller than what its author wrote.
    #[serde(flatten)]
    pub body: toml::Table,
}
