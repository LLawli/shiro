//! The `flatpak` backend: shiro drives `flatpak override` rather than inventing
//! storage of its own, so that what is on disk stays readable by the tool that
//! owns it.
//!
//! Permissions are written in flatpak's own vocabulary (`network`,
//! `filesystem=~/Downloads`, `device=dri`, `socket=wayland`,
//! `bus=org.freedesktop.Flatpak`), because the point of this command is to be
//! checkable against what `flatpak override --show` prints.
//!
//! `apply` denies by default. A manifest is not a promise: the application that
//! asked for a screen and a download directory in one release asks for the
//! session bus in the next, and an override file that only says what was taken
//! away in 2026 grants whatever 2027 adds. So every class flatpak has is denied
//! first, by name, and the profile's allows are what comes back.

use std::process::Command;

use crate::error::Error;
use crate::perms::profile::{self, Backend};

/// The subsystems `--share` takes. Closed list, from `flatpak run --help`.
const SHARES: [&str; 2] = ["network", "ipc"];

/// The sockets `--socket` takes.
const SOCKETS: [&str; 11] = [
    "x11",
    "wayland",
    "fallback-x11",
    "pulseaudio",
    "system-bus",
    "session-bus",
    "ssh-auth",
    "pcsc",
    "cups",
    "gpg-agent",
    "inherit-wayland-socket",
];

/// The devices `--device` takes. `all` is one of them, and denying it does not
/// deny the others, which is why each is named.
const DEVICES: [&str; 6] = ["dri", "input", "usb", "kvm", "shm", "all"];

/// The features `--allow` takes.
const FEATURES: [&str; 3] = ["devel", "multiarch", "bluetooth"];

/// Where flatpak keeps what it would be asked to change: the override files
/// themselves, and the per-application data of every other application.
///
/// Denied on every `apply`, whatever the profile says, because an application
/// that can write here can rewrite the profile that confines it. This is depth
/// rather than a guarantee: flatpak resolves a narrow deny against a wide allow
/// by its own rules, and `filesystem=home` is still `filesystem=home`.
const NEVER_EXPOSED: [&str; 3] = ["~/.local/share/flatpak", "/var/lib/flatpak", "~/.var/app"];

/// Bus names that are equivalent to leaving the sandbox, denied on every
/// `apply` and refused as an allow.
const ESCAPE_NAMES: [(&str, &str); 2] = [
    (
        "org.freedesktop.Flatpak",
        "it is `flatpak-spawn --host`, which runs anything outside the sandbox",
    ),
    (
        "org.freedesktop.impl.portal.PermissionStore",
        "it is where the portals keep their answers, so an application that \
         reaches it grants itself what the portals would have asked about",
    ),
];

/// `shiro perms flatpak <app> [show | allow <perm>… | deny <perm>… | reset |
/// apply]`, with `--system` for the system-wide override instead of the user's.
pub fn dispatch(args: &[String]) -> Result<(), Error> {
    let (args, system) = take_system(args);

    let Some((app, rest)) = args.split_first() else {
        return Err(Error::Usage(
            "perms flatpak needs an application: shiro perms flatpak <app> [show|allow|deny|reset|apply]"
                .to_owned(),
        ));
    };

    match rest.split_first() {
        None => show(app, system),
        Some((verb, perms)) if verb == "show" => {
            no_arguments(perms, "show")?;
            show(app, system)
        }
        Some((verb, perms)) if verb == "allow" => override_with(app, system, perms, true),
        Some((verb, perms)) if verb == "deny" => override_with(app, system, perms, false),
        Some((verb, perms)) if verb == "reset" => {
            no_arguments(perms, "reset")?;
            run(&["override", scope(system), "--reset", app])
        }
        Some((verb, perms)) if verb == "apply" => {
            no_arguments(perms, "apply")?;
            apply(app, system)
        }
        Some((verb, _)) => Err(Error::Usage(format!(
            "no such command: `perms flatpak <app> {verb}`\nit takes: show, allow, deny, reset, apply"
        ))),
    }
}

fn show(app: &str, system: bool) -> Result<(), Error> {
    run(&["override", scope(system), "--show", app])
}

/// Apply what the registered profile declares. This is what a recipe's `post`
/// calls: the engine records the declaration, and applying it stays an explicit
/// act with a command behind it.
fn apply(app: &str, system: bool) -> Result<(), Error> {
    let Some(resolved) = profile::find(app)? else {
        return Err(Error::Usage(format!(
            "no profile for `{app}`; nothing to apply"
        )));
    };

    if resolved.profile.backend != Backend::Flatpak {
        return Err(Error::Usage(format!(
            "the profile for `{app}` in {} declares the `{}` backend, not `flatpak`",
            resolved.file,
            resolved.profile.backend.as_str()
        )));
    }

    let Some(declared) = resolved.profile.flatpak else {
        return Err(Error::Usage(format!(
            "the profile for `{app}` in {} declares no `[flatpak]` section",
            resolved.file
        )));
    };

    // One invocation carrying every side, never one per side.
    //
    // `flatpak override` merges into a file it keeps, and a deny that resets
    // (`filesystem=host:reset`) drops what is already in that file. Split across
    // two invocations, the second one therefore discards what the first just
    // wrote, and the allow is silently gone.
    //
    // Order inside the invocation is not cosmetic: the last mention of a key
    // wins, so `--nodevice=dri --device=dri` grants dri and the reverse denies
    // it. Everything is denied first, then what the profile takes back.
    let mut flags = baseline();
    flags.extend(translate_all(&declared.deny, false)?);
    flags.extend(translate_all(&declared.allow, true)?);

    invoke(app, system, flags)
}

/// Every class flatpak has, denied by name.
///
/// This is what makes the profile the whole of what an application may do. A
/// manifest that starts asking for the session bus, a device or a directory
/// gets it from flatpak by default, and an override file that lists only what
/// the profile took away in one release grants whatever the next one adds.
///
/// `--nofilesystem=host:reset` is the one flag that does not need enumerating:
/// flatpak documents it as ignoring every filesystem permission inherited from
/// the manifest and from the override file, which is the deny-by-default the
/// other classes have to spell out one value at a time.
///
/// The session bus is deliberately not in the closed-off state its socket
/// suggests. With `--nosocket=session-bus`, flatpak proxies the bus instead of
/// handing it over, and its default policy already limits an application to its
/// own name, `org.freedesktop.DBus` and `org.freedesktop.portal.*`. Denying
/// name by name is impossible anyway: flatpak refuses `--no-talk-name=*`, and
/// denying prefixes broadly (`org.freedesktop.*`) would take the portals with
/// it, which is how an application asks for a file in the first place.
fn baseline() -> Vec<String> {
    let mut flags = Vec::new();

    for share in SHARES {
        flags.push(format!("--unshare={share}"));
    }
    for socket in SOCKETS {
        flags.push(format!("--nosocket={socket}"));
    }
    for device in DEVICES {
        flags.push(format!("--nodevice={device}"));
    }
    for feature in FEATURES {
        flags.push(format!("--disallow={feature}"));
    }

    flags.push("--nofilesystem=host:reset".to_owned());
    for path in NEVER_EXPOSED {
        flags.push(format!("--nofilesystem={path}"));
    }
    for (name, _) in ESCAPE_NAMES {
        flags.push(format!("--no-talk-name={name}"));
    }

    flags
}

fn override_with(app: &str, system: bool, perms: &[String], allow: bool) -> Result<(), Error> {
    let flags = translate_all(perms, allow)?;
    if flags.is_empty() {
        return Ok(());
    }

    invoke(app, system, flags)
}

fn translate_all(perms: &[String], allow: bool) -> Result<Vec<String>, Error> {
    perms.iter().map(|perm| translate(perm, allow)).collect()
}

/// Whether a permission word could be granted or revoked, without granting or
/// revoking anything.
///
/// This is what lets `catalog validate` hold a recipe's `[item.permissions]` to
/// the same closed lists `apply` holds it to, without the validator learning
/// what a socket is. The answer comes from the one translation table, so the
/// two cannot drift: there is nothing to keep in step.
pub fn check(perm: &str, allow: bool) -> Result<(), Error> {
    translate(perm, allow).map(|_| ())
}

fn invoke(app: &str, system: bool, flags: Vec<String>) -> Result<(), Error> {
    let mut args = vec!["override".to_owned(), scope(system).to_owned()];
    args.extend(flags);
    args.push(app.to_owned());

    run(&args.iter().map(String::as_str).collect::<Vec<_>>())
}

/// The one place a permission word becomes a flatpak flag. Anything not in this
/// table is refused with the table, rather than passed through: a permission
/// tool that forwards strings it does not understand cannot be audited.
fn translate(perm: &str, allow: bool) -> Result<String, Error> {
    let (key, value) = match perm.split_once('=') {
        Some((key, value)) => (key, Some(value)),
        None => (perm, None),
    };

    // Only granting is sealed. Denying one of these is the safe direction, and
    // `apply` denies all of them before it reads the profile at all.
    if allow && let Some(reason) = sealed(key, value) {
        return Err(Error::Usage(format!(
            "`{perm}` cannot be granted: {reason}\n  it would undo the profile that granted \
             it, so there is no flag for it and no profile may declare it"
        )));
    }

    let flag = match (key, value, allow) {
        ("network", None, true) => "--share=network".to_owned(),
        ("network", None, false) => "--unshare=network".to_owned(),
        ("ipc", None, true) => "--share=ipc".to_owned(),
        ("ipc", None, false) => "--unshare=ipc".to_owned(),
        ("filesystem", Some(path), true) => format!("--filesystem={path}"),
        ("filesystem", Some(path), false) => format!("--nofilesystem={path}"),
        ("socket", Some(socket), _) => {
            closed(socket, &SOCKETS, "socket")?;
            if allow {
                format!("--socket={socket}")
            } else {
                format!("--nosocket={socket}")
            }
        }
        ("device", Some(device), _) => {
            closed(device, &DEVICES, "device")?;
            if allow {
                format!("--device={device}")
            } else {
                format!("--nodevice={device}")
            }
        }
        ("feature", Some(feature), _) => {
            closed(feature, &FEATURES, "feature")?;
            if allow {
                format!("--allow={feature}")
            } else {
                format!("--disallow={feature}")
            }
        }
        // There is no `--no-own-name`, and none is needed: the session bus
        // policy is cumulative, and `--no-talk-name` writes the `none` that
        // takes owning away with talking.
        ("bus", Some(name), true) => format!("--talk-name={name}"),
        ("bus", Some(name), false) => format!("--no-talk-name={name}"),
        _ => {
            return Err(Error::Usage(format!(
                "`{perm}` is not a permission shiro knows\nit takes: network, ipc, \
                 filesystem=<path>, socket=<name>, device=<name>, feature=<name>, bus=<name>"
            )));
        }
    };

    Ok(flag)
}

/// A value flatpak itself would refuse, refused here instead, with the list.
///
/// The lists are closed because the deny-by-default baseline is built from
/// them: a value shiro does not know is a value it never denied, so accepting
/// one would mean granting something the baseline had no name for.
fn closed(value: &str, known: &[&str], what: &str) -> Result<(), Error> {
    if known.contains(&value) {
        return Ok(());
    }

    Err(Error::Usage(format!(
        "`{value}` is not a {what} flatpak has\nit takes: {}",
        known.join(", ")
    )))
}

/// Why this permission may never be granted, or `None`.
///
/// Each one of these hands over the thing that would let an application undo
/// its own confinement, which makes granting it indistinguishable from not
/// confining the application at all. There is deliberately no escape hatch: a
/// flag that turns the sealed list off is the flag every recipe would copy.
fn sealed(key: &str, value: Option<&str>) -> Option<&'static str> {
    match (key, value) {
        ("socket", Some("session-bus")) => Some(
            "it hands over the session bus unfiltered, which is every name on it at once, \
             including the ones below",
        ),
        ("socket", Some("system-bus")) => {
            Some("it hands over the system bus unfiltered, where the services run as root")
        }
        ("bus", Some(name)) => ESCAPE_NAMES
            .iter()
            .find(|(escape, _)| name == *escape || name == format!("{escape}.*"))
            .map(|(_, reason)| *reason),
        ("filesystem", Some(path)) => reaches_flatpak(path).then_some(
            "it reaches the directory where flatpak keeps the override files, so the \
             application could rewrite its own permissions",
        ),
        _ => None,
    }
}

/// Whether a declared path lands inside a flatpak installation.
///
/// Textual, after stripping the access suffix and expanding `~`, because that
/// is the form the declaration is written in and the form `flatpak override
/// --show` prints back. It does not resolve symlinks: a profile is read by
/// people, and a check that depended on what is on disk right now would answer
/// differently on two machines with the same profile.
fn reaches_flatpak(path: &str) -> bool {
    let bare = path.split(':').next().unwrap_or(path).trim_end_matches('/');

    let home = std::env::var("HOME").unwrap_or_default();
    let expanded = match bare.strip_prefix("~/") {
        Some(rest) if !home.is_empty() => format!("{home}/{rest}"),
        _ => bare.to_owned(),
    };

    let mut roots = vec![
        "~/.local/share/flatpak".to_owned(),
        "xdg-data/flatpak".to_owned(),
        "/var/lib/flatpak".to_owned(),
    ];
    if !home.is_empty() {
        roots.push(format!("{home}/.local/share/flatpak"));
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        roots.push(format!("{}/flatpak", xdg.trim_end_matches('/')));
    }

    roots.iter().any(|root| {
        let root = root.trim_end_matches('/');
        expanded == root || expanded.starts_with(&format!("{root}/")) || bare == root
    })
}

fn scope(system: bool) -> &'static str {
    if system { "--system" } else { "--user" }
}

fn take_system(args: &[String]) -> (Vec<String>, bool) {
    let system = args.iter().any(|arg| arg == "--system");
    let rest = args
        .iter()
        .filter(|arg| *arg != "--system" && *arg != "--user")
        .cloned()
        .collect();
    (rest, system)
}

fn run(args: &[&str]) -> Result<(), Error> {
    // Printed before it runs, because this command exists to be audited, and an
    // override applied without the user seeing the invocation is the thing it
    // is meant to replace.
    eprintln!("flatpak {}", args.join(" "));

    let status = Command::new("flatpak")
        .args(args)
        .status()
        .map_err(|err| Error::Failed(format!("cannot run flatpak: {err}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(Error::Failed(format!(
            "flatpak exited with {}",
            status.code().unwrap_or(128)
        )))
    }
}

fn no_arguments(args: &[String], verb: &str) -> Result<(), Error> {
    match args.first() {
        Some(extra) => Err(Error::Usage(format!(
            "`perms flatpak <app> {verb}` takes no arguments, but got `{extra}`"
        ))),
        None => Ok(()),
    }
}
