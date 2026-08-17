//! The permissions module, run as a process.
//!
//! What matters here is what a user can check: the sandbox a profile produces,
//! the fallback when there is none, and that a missing profile is loud rather
//! than permissive.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/perms")
}

fn shiro(args: &[&str]) -> Output {
    let root = root();
    Command::new(env!("CARGO_BIN_EXE_shiro"))
        .args(args)
        // A fixed PATH, so that resolving an application by name finds the same
        // program on every machine. A developer with Homebrew ahead of /usr on
        // PATH would otherwise get a `true` the sandbox does not contain, and
        // that is a real behavior worth its own test rather than an accident in
        // all of them.
        .env("PATH", "/usr/bin:/bin")
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
fn a_program_outside_the_sandbox_is_called_out() {
    // Homebrew, /opt, anything under home: the program is simply not there once
    // the sandbox is built, and bwrap's own error names the program rather than
    // the reason.
    let root = root();
    let out = Command::new(env!("CARGO_BIN_EXE_shiro"))
        .args(["perms", "run", "elsewhere"])
        .env("PATH", "/opt/somewhere/bin:/usr/bin")
        .env("SHIRO_ROOT", &root)
        .env("XDG_DATA_HOME", root.join("xdg"))
        .output()
        .expect("the binary runs");

    // The fixture profile puts the command outside /usr on purpose.
    assert!(out.status.success(), "{}", stderr(&out));
    let message = stderr(&out);
    assert!(message.contains("is outside /usr"), "{message}");
    assert!(message.contains("`read-only`"), "{message}");
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
        return;
    }

    // /usr/bin/true is in the sandbox, because /usr is bound read only.
    let out = shiro(&["run", "true"]);
    let message = stderr(&out);

    assert!(message.contains("no profile for `true`"), "{message}");
    assert!(message.contains("no network, no home"), "{message}");
    assert!(message.contains("shiro/profiles/true.toml"), "{message}");
    assert!(out.status.success(), "{message}");
}

#[test]
fn a_profile_launches_the_command_it_declares() {
    if !bwrap_works() {
        return;
    }

    // The brave fixture declares /usr/bin/true as its command, so a run that
    // exits 0 means the sandbox was built and the program inside it ran.
    let out = shiro(&["run", "brave"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!stderr(&out).contains("no profile"), "{}", stderr(&out));
}

#[test]
fn a_sandboxed_process_cannot_see_the_home_directory() {
    if !bwrap_works() {
        return;
    }

    let marker = Path::new(&std::env::var("HOME").expect("HOME is set")).join(".bashrc");
    if !marker.exists() {
        eprintln!("skipped: no file in HOME to look for");
        return;
    }

    // `test -e` on a real file in the real home, from inside the fallback. The
    // home directory is a tmpfs in there, so the answer has to be no.
    let out = shiro(&["run", "test", "-e", &marker.display().to_string()]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "the sandbox could see {}",
        marker.display()
    );
}

/// Whether bwrap can build a sandbox on this host at all.
///
/// The probe is the base `shiro run` builds, merged-usr symlinks included.
/// Leaving them out makes bwrap fail with `execvp: No such file or directory`,
/// because the dynamic loader is not in the sandbox, and that reads exactly like
/// "no sandbox here" when the sandbox was fine.
///
/// So the only accepted reason to skip is an environment that cannot create
/// namespaces. Any other failure is a broken probe, and it fails the test rather
/// than quietly turning it off.
fn bwrap_works() -> bool {
    let probe = Command::new("bwrap")
        .args([
            "--unshare-all",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib64",
            "/lib64",
            "--symlink",
            "usr/bin",
            "/bin",
            "/usr/bin/true",
        ])
        .output();

    let probe = match probe {
        Ok(probe) => probe,
        Err(err) => return skip(&format!("bwrap is not on this host ({err})")),
    };

    if probe.status.success() {
        return true;
    }

    let why = String::from_utf8_lossy(&probe.stderr).to_lowercase();
    let cannot_namespace = ["permitted", "namespace", "uid map", "setgroups", "denied"]
        .iter()
        .any(|reason| why.contains(reason));

    assert!(
        cannot_namespace,
        "the bwrap probe failed for a reason that is not a missing namespace, so these tests \
         would be skipped for the wrong reason: {}",
        why.trim()
    );

    skip(&format!(
        "bwrap cannot create a sandbox here ({})",
        why.trim()
    ))
}

/// Skipping is right on a machine with no bwrap and wrong in CI, where these are
/// the only tests that exercise the sandbox at all. `SHIRO_REQUIRE_SANDBOX`
/// turns a skip into a failure, so the coverage cannot go quietly missing where
/// it is supposed to exist.
fn skip(why: &str) -> bool {
    assert!(
        std::env::var_os("SHIRO_REQUIRE_SANDBOX").is_none(),
        "SHIRO_REQUIRE_SANDBOX is set, and the sandbox tests cannot run: {why}"
    );

    eprintln!("skipped: {why}");
    false
}

#[test]
fn apply_issues_a_single_override_invocation() {
    // `flatpak override` merges into a file it keeps, so a deny that resets
    // discards whatever an earlier invocation wrote. What broke was the number
    // of invocations, so a fake flatpak that records them is the test.
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("fake-flatpak");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the scratch directory is writable");

    let log = dir.join("invocations");
    let fake = dir.join("flatpak");
    fs::write(
        &fake,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$FLATPAK_LOG\"\n",
    )
    .expect("the fake is writable");
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).expect("chmod");

    let root = root();
    let out = Command::new(env!("CARGO_BIN_EXE_shiro"))
        .args(["perms", "flatpak", "com.brave.Browser", "apply"])
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .env("FLATPAK_LOG", &log)
        .env("SHIRO_ROOT", &root)
        .env("XDG_DATA_HOME", root.join("xdg"))
        .output()
        .expect("the binary runs");
    assert!(out.status.success(), "{}", stderr(&out));

    let recorded = fs::read_to_string(&log).expect("the fake flatpak ran");
    let invocations: Vec<&str> = recorded.lines().collect();
    assert_eq!(
        invocations.len(),
        1,
        "one invocation carrying both sides, not one per side: {invocations:#?}"
    );

    let args = invocations[0];
    assert!(args.contains("--nofilesystem=host:reset"), "{args}");
    assert!(args.contains("--nofilesystem=home"), "{args}");
    assert!(args.contains("--filesystem=xdg-download"), "{args}");
    assert!(args.contains("--device=dri"), "{args}");
    // Denies first, because that is how the profile reads. flatpak resolves the
    // set either way, which is why the fix is one invocation and not a sort.
    assert!(
        args.find("--nofilesystem=host:reset") < args.find("--filesystem=xdg-download"),
        "{args}"
    );
}
