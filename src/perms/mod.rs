//! The permissions module: the one part of shiro that is allowed to know a
//! mechanism, because knowing about Flatpak overrides and bwrap is its purpose.
//!
//! Section 9 of `docs/architecture.md`, and the second rule in `CLAUDE.md`. The
//! exception survives only while the module stays sealed:
//!
//! 1. it is reachable from exactly two places, its own native commands and the
//!    engine recording an `[item.permissions]` declaration into the registry;
//! 2. `crate::exec` never calls into it, except for that recording;
//! 3. recording a declaration is a data write and nothing more: no bwrap, no
//!    flatpak, no wrapper script, no `.desktop` file.

pub mod bwrap;
pub mod flatpak;
pub mod profile;
pub mod registry;

use std::os::unix::process::CommandExt;
use std::process::Command;

use serde_json::json;

use crate::error::Error;
use crate::perms::profile::Backend;
use crate::render::json::SCHEMA;

/// `shiro perms <backend> …`, where the backend is always named in the command
/// and never inferred from the application. The verbosity is the feature: a
/// permission tool that guesses which mechanism it is talking to cannot be
/// audited.
pub fn perms(args: Vec<String>) -> Result<(), Error> {
    let (args, json) = take_json(&args);

    match args.split_first() {
        Some((backend, rest)) if backend == "flatpak" => flatpak::dispatch(rest),
        Some((backend, rest)) if backend == "run" => explain(rest, json),
        Some((backend, _)) => Err(Error::Usage(format!(
            "no such backend: `{backend}`\nperms takes: flatpak, run"
        ))),
        None => Err(Error::Usage(
            "perms needs a backend: shiro perms flatpak <app> …, shiro perms run <app>".to_owned(),
        )),
    }
}

/// `shiro perms run <app>`: the profile in effect, and the exact command
/// `shiro run` would execute. This is the audit surface for the bwrap side,
/// which otherwise cannot be read at all.
fn explain(args: &[String], json_out: bool) -> Result<(), Error> {
    let Some((app, rest)) = args.split_first() else {
        return Err(Error::Usage(
            "perms run needs an application: shiro perms run <app>".to_owned(),
        ));
    };
    if let Some(extra) = rest.first() {
        return Err(Error::Usage(format!(
            "`perms run` takes one application, but also got `{extra}`"
        )));
    }

    let resolved = profile::find(app)?;
    let profile = resolved.as_ref().map(|resolved| &resolved.profile);

    if let Some(resolved) = &resolved
        && resolved.profile.backend != Backend::Run
    {
        return Err(Error::Usage(format!(
            "the profile for `{app}` in {} declares the `{}` backend; try `shiro perms flatpak \
             {app}`",
            resolved.file,
            resolved.profile.backend.as_str()
        )));
    }

    let arguments = bwrap::arguments(profile, app, &[])?;

    if json_out {
        let mut command = vec!["bwrap".to_owned()];
        command.extend(arguments.iter().cloned());

        println!(
            "{}",
            json!({
                "schema": SCHEMA,
                "kind": "profile",
                "app": app,
                "profile": resolved.as_ref().map(|resolved| json!({
                    "layer": resolved.layer.as_str(),
                    "file": resolved.file,
                })),
                "command": command,
            })
        );
        return Ok(());
    }

    match &resolved {
        Some(resolved) => println!(
            "{app}: profile from the {} layer, {}",
            resolved.layer.as_str(),
            resolved.file
        ),
        None => println!("{app}: no profile; the fallback grants nothing"),
    }

    // The summary is what a person checks; the command below it is what they
    // check it against.
    if let Some(run) = profile.and_then(|profile| profile.run.as_ref()) {
        println!();
        println!("  network     {}", if run.network { "yes" } else { "no" });
        println!(
            "  sockets     {}",
            list(run.share.iter().map(|share| share.as_str().to_owned()))
        );
        println!("  devices     {}", list(run.devices.iter().cloned()));
        println!("  read only   {}", list(run.read_only.iter().cloned()));
        println!("  read write  {}", list(run.read_write.iter().cloned()));
    }

    println!();
    println!("bwrap {}", arguments.join(" "));

    warn_if_unreachable(profile, app)?;
    Ok(())
}

/// A program outside what the sandbox mounts is not there once it is built, and
/// bwrap's own "No such file or directory" points at the program rather than at
/// the reason.
fn warn_if_unreachable(profile: Option<&profile::Profile>, app: &str) -> Result<(), Error> {
    let command = bwrap::program_for(profile, app)?;
    if !bwrap::program_is_reachable(profile, &command) {
        eprintln!(
            "shiro: `{command}` is outside /usr, so the sandbox will not contain it; add its \
             directory to `read-only` in the profile"
        );
    }
    Ok(())
}

fn list(values: impl Iterator<Item = String>) -> String {
    let values: Vec<String> = values.collect();
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.join(", ")
    }
}

/// `shiro run <app> [args…]`: resolve a profile, build the bwrap invocation
/// from it, exec the program.
///
/// A missing profile is a fallback, not an error, and the fallback grants close
/// to nothing: no network, no home, no devices, no session bus. It never runs
/// unconfined, and it warns on stderr naming the profile it looked for.
pub fn run(args: Vec<String>) -> Result<(), Error> {
    let Some((app, rest)) = args.split_first() else {
        return Err(Error::Usage(
            "run needs an application: shiro run <app> [args…]".to_owned(),
        ));
    };

    let resolved = profile::find(app)?;

    if let Some(resolved) = &resolved
        && resolved.profile.backend != Backend::Run
    {
        return Err(Error::Usage(format!(
            "the profile for `{app}` in {} declares the `{}` backend, and `shiro run` launches \
             native applications",
            resolved.file,
            resolved.profile.backend.as_str()
        )));
    }

    if resolved.is_none() {
        warn_about_fallback(app);
    }

    if !bwrap::available() {
        return Err(Error::Failed(
            "bwrap is not on this host, and running unconfined is not an option".to_owned(),
        ));
    }

    let profile = resolved.as_ref().map(|resolved| &resolved.profile);
    warn_if_unreachable(profile, app)?;

    let arguments = bwrap::arguments(profile, app, rest)?;

    // exec, so that the sandboxed process is the process: no shiro left in the
    // tree to confuse a session manager, and signals go where they should.
    let error = Command::new("bwrap").args(&arguments).exec();
    Err(Error::Failed(format!("cannot run bwrap: {error}")))
}

/// The failure mode of the fallback is an application that cannot reach what it
/// was not given, and that has to be legible.
fn warn_about_fallback(app: &str) {
    let where_to_put_one = crate::layers::dir(crate::layers::Layer::User, "profiles")
        .map(|dir| dir.join(format!("{app}.toml")).display().to_string())
        .unwrap_or_else(|| format!("$XDG_DATA_HOME/shiro/profiles/{app}.toml"));

    eprintln!(
        "shiro: no profile for `{app}`, so it runs under the fallback: no network, no home, no \
         devices, no session bus."
    );
    eprintln!("shiro: write one at {where_to_put_one}");
}

fn take_json(args: &[String]) -> (Vec<String>, bool) {
    let json = args.iter().any(|arg| arg == "--json");
    let rest = args
        .iter()
        .filter(|arg| *arg != "--json")
        .cloned()
        .collect();
    (rest, json)
}
