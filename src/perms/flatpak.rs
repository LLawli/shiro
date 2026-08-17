//! The `flatpak` backend: shiro drives `flatpak override` rather than inventing
//! storage of its own, so that what is on disk stays readable by the tool that
//! owns it.
//!
//! Permissions are written in flatpak's own vocabulary (`network`,
//! `filesystem=~/Downloads`, `device=dri`, `bus=org.freedesktop.Flatpak`),
//! because the point of this command is to be checkable against what `flatpak
//! override --show` prints.

use std::process::Command;

use crate::error::Error;
use crate::perms::profile::{self, Backend};

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

    let declared = resolved.profile.flatpak.unwrap_or_default();
    if declared.allow.is_empty() && declared.deny.is_empty() {
        return Err(Error::Usage(format!(
            "the profile for `{app}` in {} declares no permissions",
            resolved.file
        )));
    }

    // One invocation carrying both sides, never one per side.
    //
    // `flatpak override` merges into a file it keeps, and a deny that resets
    // (`filesystem=host:reset`) drops what is already in that file. Split across
    // two invocations, the second one therefore discards what the first just
    // wrote, and the allow is silently gone. Within one invocation flatpak
    // resolves the whole set coherently, in either order.
    //
    // The denies are emitted first because that is how the profile reads, "take
    // everything away, then give this back", and not because flatpak cares.
    let mut flags = translate_all(&declared.deny, false)?;
    flags.extend(translate_all(&declared.allow, true)?);

    invoke(app, system, flags)
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

    let flag = match (key, value, allow) {
        ("network", None, true) => "--share=network".to_owned(),
        ("network", None, false) => "--unshare=network".to_owned(),
        ("ipc", None, true) => "--share=ipc".to_owned(),
        ("ipc", None, false) => "--unshare=ipc".to_owned(),
        ("filesystem", Some(path), true) => format!("--filesystem={path}"),
        ("filesystem", Some(path), false) => format!("--nofilesystem={path}"),
        ("device", Some(device), true) => format!("--device={device}"),
        ("device", Some(device), false) => format!("--nodevice={device}"),
        ("bus", Some(name), true) => format!("--talk-name={name}"),
        ("bus", Some(name), false) => format!("--no-talk-name={name}"),
        _ => {
            return Err(Error::Usage(format!(
                "`{perm}` is not a permission shiro knows\nit takes: network, ipc, \
                 filesystem=<path>, device=<name>, bus=<name>"
            )));
        }
    };

    Ok(flag)
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
