//! Building a bwrap invocation from a profile, for native applications, which
//! have no permission mechanism of their own.
//!
//! The sandbox is built by adding to nothing. The base gives a process what it
//! needs to start and nothing that belongs to the user: `/usr` and `/etc` read
//! only, a private `/proc`, a minimal `/dev`, a tmpfs over `/tmp` and over the
//! home directory. Network, devices, desktop sockets and any path under home
//! are added only where a profile names them.
//!
//! Including the fallback used when no profile exists, which is that base and
//! nothing else.

use std::path::{Path, PathBuf};
use std::{env, fs};

use crate::error::Error;
use crate::perms::profile::{Profile, Run, Share};

/// The full bwrap argument list, ending with the program and its arguments.
///
/// Returned rather than executed so that `shiro perms run <app>` can print
/// exactly what `shiro run <app>` would do. A sandbox nobody can read is a
/// sandbox nobody can check.
pub fn arguments(
    profile: Option<&Profile>,
    app: &str,
    extra: &[String],
) -> Result<Vec<String>, Error> {
    let run = profile.and_then(|profile| profile.run.as_ref());
    let mut args = base();

    if let Some(run) = run {
        grant(&mut args, run)?;
    }

    let command = program(run, app)?;
    args.push("--".to_owned());
    args.push(command);
    args.extend(run.map(|run| run.args.clone()).unwrap_or_default());
    args.extend(extra.iter().cloned());

    Ok(args)
}

/// Everything the sandbox always has, and everything it always denies.
fn base() -> Vec<String> {
    let mut args: Vec<String> = [
        // Nothing outlives the launcher, and no terminal is inherited: a
        // sandboxed process must not be able to push characters back into the
        // shell that started it.
        "--die-with-parent",
        "--new-session",
        // Every namespace, then network back only if the profile says so.
        "--unshare-all",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--ro-bind",
        "/usr",
        "/usr",
        // /etc is read only and holds no user data: fonts, ld.so.conf, locale
        // and CA certificates are what make a process start at all.
        "--ro-bind",
        "/etc",
        "/etc",
        // The environment is emptied and rebuilt, so that tokens and paths from
        // the launching shell do not leak into the sandbox by default.
        "--clearenv",
        "--setenv",
        "PATH",
        "/usr/bin:/bin",
    ]
    .iter()
    .map(|arg| (*arg).to_owned())
    .collect();

    // Fedora and every other merged-usr host reaches libraries through these.
    for link in ["lib", "lib64", "bin", "sbin"] {
        if Path::new(&format!("/{link}")).is_symlink() {
            args.extend([
                "--symlink".to_owned(),
                format!("usr/{link}"),
                format!("/{link}"),
            ]);
        }
    }

    // The home directory exists and is empty. An application that writes to it
    // works, and writes nowhere the user can see, until a profile says which
    // paths are real.
    if let Some(home) = home() {
        args.extend([
            "--tmpfs".to_owned(),
            home.display().to_string(),
            "--setenv".to_owned(),
            "HOME".to_owned(),
            home.display().to_string(),
        ]);
    }

    for passthrough in ["TERM", "LANG", "LC_ALL"] {
        if let Ok(value) = env::var(passthrough) {
            args.extend(["--setenv".to_owned(), passthrough.to_owned(), value]);
        }
    }

    args
}

/// What the profile adds on top of the base.
fn grant(args: &mut Vec<String>, run: &Run) -> Result<(), Error> {
    if run.network {
        args.push("--share-net".to_owned());
    }

    for device in &run.devices {
        let path = if device.starts_with('/') {
            PathBuf::from(device)
        } else {
            PathBuf::from("/dev").join(device)
        };
        if path.exists() {
            args.extend([
                "--dev-bind".to_owned(),
                path.display().to_string(),
                path.display().to_string(),
            ]);
        }
    }

    for share in &run.share {
        socket(args, *share);
    }

    for (mode, paths) in [
        ("--ro-bind-try", &run.read_only),
        ("--bind-try", &run.read_write),
    ] {
        for path in paths {
            let resolved = expand(path)?;
            args.extend([
                mode.to_owned(),
                resolved.display().to_string(),
                resolved.display().to_string(),
            ]);
        }
    }

    for (key, value) in &run.env {
        args.extend(["--setenv".to_owned(), key.clone(), value.clone()]);
    }

    Ok(())
}

/// One desktop socket, bound at the same path it has on the host, with the
/// variable that points at it.
fn socket(args: &mut Vec<String>, share: Share) {
    let runtime = env::var("XDG_RUNTIME_DIR").ok().map(PathBuf::from);

    match share {
        Share::Wayland => {
            let display = env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".to_owned());
            if let Some(runtime) = &runtime {
                bind_ro(args, &runtime.join(&display));
                args.extend([
                    "--setenv".to_owned(),
                    "XDG_RUNTIME_DIR".to_owned(),
                    runtime.display().to_string(),
                    "--setenv".to_owned(),
                    "WAYLAND_DISPLAY".to_owned(),
                    display,
                ]);
            }
        }
        Share::X11 => {
            bind_ro(args, Path::new("/tmp/.X11-unix"));
            if let Ok(display) = env::var("DISPLAY") {
                args.extend(["--setenv".to_owned(), "DISPLAY".to_owned(), display]);
            }
        }
        Share::Pipewire => {
            if let Some(runtime) = &runtime {
                bind_ro(args, &runtime.join("pipewire-0"));
                args.extend([
                    "--setenv".to_owned(),
                    "XDG_RUNTIME_DIR".to_owned(),
                    runtime.display().to_string(),
                ]);
            }
        }
        Share::Pulse => {
            if let Some(runtime) = &runtime {
                let socket = runtime.join("pulse/native");
                bind_ro(args, &socket);
                args.extend([
                    "--setenv".to_owned(),
                    "PULSE_SERVER".to_owned(),
                    format!("unix:{}", socket.display()),
                ]);
            }
        }
        Share::SessionBus => {
            let address = env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default();
            if let Some(path) = address.strip_prefix("unix:path=") {
                let path = path.split(',').next().unwrap_or(path);
                bind_ro(args, Path::new(path));
                args.extend([
                    "--setenv".to_owned(),
                    "DBUS_SESSION_BUS_ADDRESS".to_owned(),
                    format!("unix:path={path}"),
                ]);
            }
        }
    }
}

fn bind_ro(args: &mut Vec<String>, path: &Path) {
    args.extend([
        "--ro-bind-try".to_owned(),
        path.display().to_string(),
        path.display().to_string(),
    ]);
}

/// The executable to launch.
fn program(run: Option<&Run>, app: &str) -> Result<String, Error> {
    if let Some(command) = run.and_then(|run| run.command.as_deref()) {
        return Ok(command.to_owned());
    }

    which(app).ok_or_else(|| {
        Error::Usage(format!(
            "cannot find `{app}` on PATH, and its profile declares no `command`"
        ))
    })
}

fn which(name: &str) -> Option<String> {
    env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|dir| Path::new(dir).join(name))
        .find(|path| path.is_file())
        .map(|path| path.display().to_string())
}

/// `~` is the user's home, and nothing else is expanded: a profile is read by
/// someone deciding what an application may reach, so a path in it should mean
/// what it looks like.
fn expand(path: &str) -> Result<PathBuf, Error> {
    let Some(rest) = path.strip_prefix('~') else {
        return Ok(PathBuf::from(path));
    };

    let home = home()
        .ok_or_else(|| Error::Usage(format!("`{path}` starts with ~, and HOME is not set")))?;
    Ok(home.join(rest.trim_start_matches('/')))
}

fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Whether bwrap is on the host at all. `shiro run` refuses rather than running
/// the program unconfined, which is the whole point of the command.
pub fn available() -> bool {
    which("bwrap").is_some_and(|path| fs::metadata(path).is_ok())
}

/// Whether the program is somewhere the sandbox actually mounts.
///
/// A program installed outside `/usr` (a Homebrew prefix, `/opt`, a path in the
/// user's home) is simply not there once the sandbox is built, and bwrap's own
/// "No such file or directory" points at the program rather than at the reason.
pub fn program_is_reachable(profile: Option<&Profile>, command: &str) -> bool {
    let mut mounted = vec![PathBuf::from("/usr"), PathBuf::from("/etc")];

    if let Some(run) = profile.and_then(|profile| profile.run.as_ref()) {
        for path in run.read_only.iter().chain(run.read_write.iter()) {
            if let Ok(path) = expand(path) {
                mounted.push(path);
            }
        }
    }

    let command = PathBuf::from(command);
    mounted.iter().any(|prefix| command.starts_with(prefix))
}

/// The program a profile would launch, for a caller that wants to check it
/// before building the whole invocation.
pub fn program_for(profile: Option<&Profile>, app: &str) -> Result<String, Error> {
    program(profile.and_then(|profile| profile.run.as_ref()), app)
}
