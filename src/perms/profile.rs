//! Profiles: one TOML file per application, in the same four layers as the
//! catalog and with the same wholesale replacement, because a half-overridden
//! sandbox is a sandbox nobody can reason about.
//!
//! The user layer may loosen a profile, not only tighten it. A user who cannot
//! grant their own browser one directory will edit the `.desktop` file and
//! bypass `shiro run` entirely, which removes the sandbox instead of adjusting
//! it.
//!
//! A profile is exactly what a recipe's `[item.permissions]` block is, so what
//! the engine records and what `shiro run` reads are the same shape.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use crate::error::Error;
use crate::layers::{self, Layer};

include!(concat!(env!("OUT_DIR"), "/builtin_profiles.rs"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// A native application, launched under bwrap by `shiro run`.
    Run,
    /// An application that already has a permission mechanism of its own.
    Flatpak,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Run => "run",
            Backend::Flatpak => "flatpak",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub backend: Backend,
    pub app: String,
    pub run: Option<Run>,
    pub flatpak: Option<Flatpak>,
}

/// What a native application is allowed to touch. Everything not named here is
/// not there: the sandbox is built by adding to nothing, never by removing from
/// the host.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Run {
    /// The executable to launch. Without it, the application name is resolved
    /// on `PATH`, which is a loop waiting to happen if that name is itself a
    /// wrapper calling `shiro run`.
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub share: Vec<Share>,
    /// Device nodes, by short name (`dri`) or absolute path (`/dev/dri`).
    #[serde(default)]
    pub devices: Vec<String>,
    #[serde(default)]
    pub read_only: Vec<String>,
    #[serde(default)]
    pub read_write: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// Desktop sockets, which are the part of a session an application actually
/// needs and the part a fallback must not hand over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Share {
    Wayland,
    X11,
    Pipewire,
    Pulse,
    SessionBus,
}

impl Share {
    pub fn as_str(self) -> &'static str {
        match self {
            Share::Wayland => "wayland",
            Share::X11 => "x11",
            Share::Pipewire => "pipewire",
            Share::Pulse => "pulse",
            Share::SessionBus => "session-bus",
        }
    }
}

/// Overrides to hand to `flatpak override`, in flatpak's own vocabulary, so
/// that what is declared here can be read against what `flatpak override
/// --show` prints.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flatpak {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

pub struct Resolved {
    pub profile: Profile,
    pub layer: Layer,
    pub file: String,
}

/// The profile in effect for an application, or `None` if no layer declares
/// one. Searched highest precedence first: the first hit wins outright, because
/// a profile replaces rather than merges.
pub fn find(app: &str) -> Result<Option<Resolved>, Error> {
    check_name(app)?;

    for layer in Layer::ALL.into_iter().rev() {
        let Some(path) = file_for(layer, app) else {
            // The built-in layer is embedded, so it is searched by name rather
            // than on disk.
            if layer == Layer::BuiltIn
                && let Some((name, contents)) = BUILTIN_PROFILES
                    .iter()
                    .find(|(name, _)| *name == format!("{app}.toml"))
            {
                return Ok(Some(Resolved {
                    profile: parse(contents, &format!("<built-in>/{name}"), app)?,
                    layer,
                    file: format!("<built-in>/{name}"),
                }));
            }
            continue;
        };

        if path.is_file() {
            let contents = fs::read_to_string(&path)
                .map_err(|err| Error::Catalog(format!("cannot read {}: {err}", path.display())))?;
            let file = path.display().to_string();
            return Ok(Some(Resolved {
                profile: parse(&contents, &file, app)?,
                layer,
                file,
            }));
        }
    }

    Ok(None)
}

/// Where a profile for this application would go, in the layer a given
/// privilege writes to.
pub fn path_for(layer: Layer, app: &str) -> Option<PathBuf> {
    file_for(layer, app)
}

fn file_for(layer: Layer, app: &str) -> Option<PathBuf> {
    layers::dir(layer, "profiles").map(|dir| dir.join(format!("{app}.toml")))
}

fn parse(contents: &str, file: &str, app: &str) -> Result<Profile, Error> {
    let profile: Profile =
        toml::from_str(contents).map_err(|err| Error::Catalog(format!("{file}: {err}")))?;

    // The file name is how a profile is found and the `app` key is what it
    // claims to be. When they disagree, one of them is a typo, and guessing
    // which would mean confining the wrong program.
    if profile.app != app {
        return Err(Error::Catalog(format!(
            "{file}: the profile declares `app = \"{}\"`, but its file name says `{app}`",
            profile.app
        )));
    }

    Ok(profile)
}

/// An application name is a file name in the registry, so it may not be a path.
pub fn check_name(app: &str) -> Result<(), Error> {
    if app.is_empty()
        || app.contains('/')
        || app.contains(std::path::MAIN_SEPARATOR)
        || app.starts_with('.')
    {
        return Err(Error::Usage(format!(
            "`{app}` is not an application name; it names a profile file, so it cannot be a path"
        )));
    }
    Ok(())
}
