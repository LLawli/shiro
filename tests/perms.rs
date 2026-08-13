//! The permissions module, run as a process.
//!
//! What matters here is what a user can check: the sandbox a profile produces,
//! the fallback when there is none, and that a missing profile is loud rather
//! than permissive.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/perms")
}

fn shiro(args: &[&str]) -> Output {
    let root = root();
    Command::new(env!("CARGO_BIN_EXE_shiro"))
        .args(args)
        .env("SHIRO_ROOT", &root)
        .env("XDG_DATA_HOME", root.join("xdg"))
        .output()
        .expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("output is utf-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("output is utf-8")
}

#[test]
fn a_profile_grants_exactly_what_it_names() {
    let out = shiro(&["perms", "run", "brave"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let printed = stdout(&out);
    assert!(
        printed.contains("profile from the image layer"),
        "{printed}"
    );
    assert!(printed.contains("--share-net"), "{printed}");
    assert!(
        printed.contains("--dev-bind /dev/dri /dev/dri"),
        "{printed}"
    );
    assert!(
        printed.contains("--ro-bind-try /etc/fonts /etc/fonts"),
        "{printed}"
    );
    assert!(
        printed.contains("--setenv MOZ_ENABLE_WAYLAND 1"),
        "{printed}"
    );
    // The command comes from the profile, not from PATH, which is what keeps a
    // wrapper that calls `shiro run` from calling itself.
    assert!(printed.ends_with("-- /usr/bin/true\n"), "{printed}");
}

#[test]
fn the_fallback_grants_nothing_and_says_so() {
    let out = shiro(&["perms", "run", "true"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let printed = stdout(&out);
    assert!(printed.contains("no profile"), "{printed}");
    assert!(printed.contains("--unshare-all"), "{printed}");
    // Nothing of the user's session is in there.
    assert!(!printed.contains("--share-net"), "{printed}");
    assert!(!printed.contains("--dev-bind"), "{printed}");
    assert!(!printed.contains("wayland"), "{printed}");
    assert!(!printed.contains("DBUS_SESSION_BUS_ADDRESS"), "{printed}");
}

#[test]
fn the_base_is_read_only_and_has_no_real_home() {
    let out = shiro(&["perms", "run", "true", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let payload: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("json");
    let command: Vec<String> = payload["command"]
        .as_array()
        .expect("a command")
        .iter()
        .map(|arg| arg.as_str().expect("a string").to_owned())
        .collect();
    let joined = command.join(" ");

    assert_eq!(command[0], "bwrap");
    assert!(joined.contains("--ro-bind /usr /usr"), "{joined}");
    assert!(joined.contains("--ro-bind /etc /etc"), "{joined}");
    assert!(joined.contains("--proc /proc"), "{joined}");
    assert!(joined.contains("--clearenv"), "{joined}");
    assert!(joined.contains("--die-with-parent"), "{joined}");

    // The home directory is a tmpfs: an application that writes there works,
    // and writes nowhere the user can see.
    let home = std::env::var("HOME").expect("HOME is set");
    assert!(joined.contains(&format!("--tmpfs {home}")), "{joined}");
    assert!(payload["profile"].is_null());
}

#[test]
fn a_profile_that_names_another_application_is_refused() {
    let out = shiro(&["perms", "run", "mismatch"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("its file name says `mismatch`"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn the_backend_is_never_inferred() {
    // The flatpak profile exists, and `perms run` will not quietly do something
    // else with it.
    let out = shiro(&["perms", "run", "com.brave.Browser"]);
    assert_eq!(out.status.code(), Some(2));
    let message = stderr(&out);
    assert!(
        message.contains("declares the `flatpak` backend"),
        "{message}"
    );
    assert!(message.contains("shiro perms flatpak"), "{message}");

    // And `perms` itself will not accept an application without one.
    let out = shiro(&["perms", "brave"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("no such backend"), "{}", stderr(&out));
}

#[test]
fn a_permission_word_shiro_does_not_know_is_refused_with_the_list() {
    let out = shiro(&[
        "perms",
        "flatpak",
        "com.brave.Browser",
        "allow",
        "everything",
    ]);
    assert_eq!(out.status.code(), Some(2));

    let message = stderr(&out);
    assert!(
        message.contains("`everything` is not a permission"),
        "{message}"
    );
    assert!(message.contains("filesystem=<path>"), "{message}");
}

#[test]
fn run_warns_before_falling_back_and_never_runs_unconfined() {
    if !bwrap_works() {
        eprintln!("skipped: bwrap cannot create a sandbox here");
        return;
    }

    // /usr/bin/true exists inside the sandbox, because /usr is bound read only.
    let out = shiro(&["run", "true"]);
    let message = stderr(&out);

    assert!(message.contains("no profile for `true`"), "{message}");
    assert!(message.contains("no network, no home"), "{message}");
    assert!(message.contains("shiro/profiles/true.toml"), "{message}");
    assert!(out.status.success(), "{message}");
}

#[test]
fn a_sandboxed_process_cannot_see_the_home_directory() {
    if !bwrap_works() {
        eprintln!("skipped: bwrap cannot create a sandbox here");
        return;
    }

    let marker = Path::new(&std::env::var("HOME").expect("HOME is set")).join(".bashrc");
    if !marker.exists() {
        eprintln!("skipped: no file in HOME to look for");
        return;
    }

    // `test -e` on a real file in the real home, from inside the fallback.
    let out = shiro(&["run", "test", "-e", &marker.display().to_string()]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "the sandbox could see {}",
        marker.display()
    );
}

fn bwrap_works() -> bool {
    Command::new("bwrap")
        .args([
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "/usr/bin/true",
        ])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
