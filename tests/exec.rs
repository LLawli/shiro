//! The executor, run as a process against recipes that write to a scratch
//! directory instead of the machine.
//!
//! What these check is the part a recipe author cannot verify by reading: the
//! phase order, what a failure undoes, and the exit code each outcome leaves.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// A fresh scratch directory per test, under cargo's own tmp dir.
fn state(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the scratch directory is writable");
    dir
}

fn shiro(state: &Path, args: &[&str]) -> Output {
    shiro_with(state, args, &[])
}

fn shiro_with(state: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exec");
    let mut command = Command::new(env!("CARGO_BIN_EXE_shiro"));
    command
        .args(args)
        .env("SHIRO_ROOT", &root)
        .env("XDG_DATA_HOME", state.join("xdg"))
        .env("SHIRO_CHECK_TIMEOUT", "5")
        .env("STATE", state);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("the binary runs")
}

/// An elevator that records what it was asked to run and then runs it, so that
/// a test can count authentications without one ever reaching a real `sudo`.
/// It logs the first argument alone: `true` for the probe, `sh` for a hook.
fn elevator(state: &Path) -> String {
    let path = state.join("elevator");
    fs::write(
        &path,
        "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$STATE/elevator.log\"\nexec \"$@\"\n",
    )
    .expect("the scratch directory is writable");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("the script is ours");
    path.display().to_string()
}

fn authentications(state: &Path) -> Vec<String> {
    fs::read_to_string(state.join("elevator.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn log(state: &Path) -> Vec<String> {
    fs::read_to_string(state.join("log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("output is utf-8")
}

fn events(output: &Output) -> Vec<Value> {
    String::from_utf8(output.stdout.clone())
        .expect("output is utf-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("every line is an object"))
        .collect()
}

#[test]
fn a_recipe_runs_pre_install_post_in_order() {
    let state = state("clean");
    let out = shiro(&state, &["demo", "clean"]);
    assert!(out.status.success(), "{}", stderr(&out));

    assert_eq!(
        log(&state),
        [
            // The environment is what section 5 promises, and SHIRO_DRY_RUN is
            // 0 when the hook actually runs.
            "pre pre demo.clean image 0",
            "install",
            "post",
        ]
    );
    assert!(state.join("installed").exists());
}

#[test]
fn an_installed_item_refuses_to_reinstall_until_forced() {
    let state = state("refuses");
    assert!(shiro(&state, &["demo", "clean"]).status.success());

    let again = shiro(&state, &["demo", "clean"]);
    assert_eq!(again.status.code(), Some(3));
    assert!(
        stderr(&again).contains("already installed"),
        "{}",
        stderr(&again)
    );

    let forced = shiro(&state, &["demo", "clean", "--force"]);
    assert!(forced.status.success(), "{}", stderr(&forced));
    assert_eq!(log(&state).len(), 6, "the recipe ran twice");
}

#[test]
fn atomic_undoes_the_failed_phase_and_then_everything_before_it() {
    let state = state("atomic");
    let out = shiro(&state, &["demo", "atomic"]);

    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("failed in `install` (exit 7)"),
        "{}",
        stderr(&out)
    );
    assert_eq!(log(&state), ["pre", "install", "roll-install", "roll-pre"]);
}

#[test]
fn phase_undoes_only_what_failed() {
    let state = state("phase");
    let out = shiro(&state, &["demo", "phase"]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(log(&state), ["pre", "install", "roll-install"]);
}

#[test]
fn keep_partial_downgrades_atomic_for_one_invocation() {
    let state = state("keep-partial");
    let out = shiro(&state, &["demo", "atomic", "--keep-partial"]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(log(&state), ["pre", "install", "roll-install"]);
}

#[test]
fn rollback_none_undoes_nothing() {
    let state = state("none");
    let out = shiro(&state, &["demo", "none"]);

    assert_eq!(out.status.code(), Some(1));
    assert_eq!(log(&state), ["install"]);
}

#[test]
fn a_failing_rollback_is_its_own_outcome() {
    let state = state("broken-rollback");
    let out = shiro(&state, &["demo", "broken-rollback"]);

    // Not a second failure of the same kind: the system is in a state nobody
    // intended, and it gets a code of its own.
    assert_eq!(out.status.code(), Some(4));
    let message = stderr(&out);
    assert!(
        message.contains("roll-install` failed too (exit 9)"),
        "{message}"
    );
    assert!(
        message.contains("neither you nor the recipe intended"),
        "{message}"
    );
}

#[test]
fn an_item_removes_with_its_own_uninstall() {
    let state = state("removable");
    assert!(shiro(&state, &["demo", "removable"]).status.success());

    let out = shiro(&state, &["demo", "removable", "--uninstall"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(log(&state), ["uninstall"]);
    assert!(!state.join("installed").exists());
}

#[test]
fn removing_what_is_not_installed_is_refused() {
    let state = state("not-installed");
    let out = shiro(&state, &["demo", "removable", "--uninstall"]);

    assert_eq!(out.status.code(), Some(3));
    assert!(
        stderr(&out).contains("is not installed"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn removal_is_derived_from_the_rollback_hooks_when_absent() {
    let state = state("derived");
    let out = shiro(&state, &["demo", "derived", "--uninstall"]);

    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(log(&state), ["roll-post", "roll-install", "roll-pre"]);
}

#[test]
fn an_item_with_no_way_back_says_so_instead_of_doing_nothing() {
    let state = state("nothing-to-remove");
    let out = shiro(&state, &["demo", "nothing-to-remove", "--uninstall"]);

    assert_eq!(out.status.code(), Some(3));
    assert!(
        stderr(&out).contains("nothing to remove"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_dry_run_prints_the_commands_and_runs_none_of_them() {
    let state = state("dry-run");
    let out = shiro(&state, &["demo", "clean", "--dry-run"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let printed = String::from_utf8(out.stdout).expect("output is utf-8");
    assert!(printed.contains("SHIRO_DRY_RUN=1"), "{printed}");
    assert!(printed.contains("SHIRO_ITEM=demo.clean"), "{printed}");
    assert!(printed.contains("sh -c"), "{printed}");

    assert!(log(&state).is_empty(), "a dry run executed something");
    assert!(!state.join("installed").exists());
}

#[test]
fn json_reports_one_object_per_phase_transition() {
    let state = state("json");
    let out = shiro(&state, &["demo", "clean", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let events = events(&out);
    let shape: Vec<(&str, &str)> = events
        .iter()
        .map(|event| {
            (
                event["phase"]
                    .as_str()
                    .unwrap_or(event["outcome"].as_str().unwrap_or("")),
                event["state"].as_str().unwrap_or("result"),
            )
        })
        .collect();

    assert_eq!(
        shape,
        [
            ("pre", "start"),
            ("pre", "ok"),
            ("install", "start"),
            ("install", "ok"),
            ("post", "start"),
            ("post", "ok"),
            ("installed", "result"),
        ]
    );
    assert!(events.iter().all(|event| event["schema"] == 2));
    assert_eq!(events.last().expect("a result")["exit"], 0);
}

#[test]
fn json_names_the_outcome_of_a_rolled_back_transaction() {
    let state = state("json-rollback");
    let out = shiro(&state, &["demo", "atomic", "--json"]);
    assert_eq!(out.status.code(), Some(1));

    let events = events(&out);
    let result = events.last().expect("a result");
    assert_eq!(result["kind"], "result");
    assert_eq!(result["outcome"], "rolled-back");
    assert_eq!(result["exit"], 1);

    // The failing phase reports the exit code the hook gave, not a generic one.
    let failed = events
        .iter()
        .find(|event| event["state"] == "failed")
        .expect("a failed phase");
    assert_eq!(failed["phase"], "install");
    assert_eq!(failed["exit"], 7);
}

#[test]
fn a_system_item_carries_its_environment_through_the_elevator() {
    let state = state("system");
    // A dry run, so the test never authenticates: what matters is the shape of
    // the command, which is what a user reads before letting it run as root.
    let out = shiro(&state, &["demo", "system", "--dry-run"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let printed = String::from_utf8(out.stdout).expect("output is utf-8");
    assert!(printed.contains(", as root"), "{printed}");
    // sudo resets the environment, so the variables travel inside the script
    // rather than in front of it, where a default sudoers would refuse them.
    assert!(printed.contains("sudo sh -c"), "{printed}");
    assert!(
        printed.contains("export SHIRO_ITEM=demo.system;"),
        "{printed}"
    );
}

#[test]
fn the_up_front_probe_authenticates_before_the_first_hook() {
    let state = state("probe-on");
    let elevator = elevator(&state);
    let out = shiro_with(&state, &["demo", "system"], &[("SHIRO_SUDO", &elevator)]);
    assert!(out.status.success(), "{}", stderr(&out));

    // `true` is the probe, whose only job is to fill the credential cache that
    // then covers the transaction; `sh` is the hook it covers.
    assert_eq!(authentications(&state), ["true", "sh"]);
    assert_eq!(log(&state), ["system"]);
}

#[test]
fn the_probe_is_dropped_for_an_elevator_that_keeps_no_cache() {
    let state = state("probe-off");
    let elevator = elevator(&state);
    let out = shiro_with(
        &state,
        &["demo", "system"],
        &[("SHIRO_SUDO", &elevator), ("SHIRO_SUDO_PROBE", "0")],
    );
    assert!(out.status.success(), "{}", stderr(&out));

    // There is no cache to fill under `pkexec`, so the probe is a password
    // dialog that authenticates nothing. The hook prompts instead, which for a
    // recipe with one hook is one dialog rather than two.
    assert_eq!(authentications(&state), ["sh"]);
    assert_eq!(log(&state), ["system"]);
}

#[test]
fn execution_flags_are_refused_on_a_menu() {
    let state = state("menu-flags");
    let out = shiro(&state, &["demo", "--uninstall"]);

    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("act on an item"), "{}", stderr(&out));
}

#[test]
fn the_engine_records_a_declared_profile_between_install_and_post() {
    let state = state("confined");
    let out = shiro(&state, &["demo", "confined", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    // The profile is on disk before `post` runs, which is what lets a post
    // generate whatever calls `shiro run` without ordering games.
    assert_eq!(log(&state), ["post"]);

    let profile = state.join("xdg/shiro/profiles/demo-app.toml");
    let written = fs::read_to_string(&profile).expect("the profile was written");
    assert!(written.contains("backend = \"run\""), "{written}");
    assert!(written.contains("network = true"), "{written}");

    // It is a phase like any other in the report, because a front end showing
    // progress should not have a silent step in the middle.
    let phases: Vec<String> = events(&out)
        .iter()
        .filter_map(|event| event["phase"].as_str().map(str::to_owned))
        .collect();
    assert!(phases.contains(&"permissions".to_owned()), "{phases:?}");
}

#[test]
fn a_recorded_profile_is_taken_back_out_when_the_transaction_is_undone() {
    let state = state("confined-fails");
    let out = shiro(&state, &["demo", "confined-fails"]);
    assert_eq!(out.status.code(), Some(1));

    // The engine wrote it on its own initiative, so the engine takes it back
    // out, with nothing declared by the recipe.
    assert!(!state.join("xdg/shiro/profiles/demo-ghost.toml").exists());
    assert_eq!(log(&state), ["roll-install"]);
}
