//! Lists whose children only exist at run time.
//!
//! What these check is the half a recipe author cannot see by reading: when the
//! generator runs, what it may print, and what the chosen entry becomes. The
//! generators write to a scratch directory instead of the machine, and the
//! cache is pointed at one too, so a test never reads or writes the real one.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/entries");
    shiro_at(&root, state, args, env)
}

fn shiro_at(root: &Path, state: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_shiro"));
    command
        .args(args)
        .env("SHIRO_ROOT", root)
        .env("XDG_DATA_HOME", state.join("xdg"))
        .env("XDG_CACHE_HOME", state.join("cache"))
        .env("SHIRO_LIST_TIMEOUT", "2")
        .env("STATE", state);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("the binary runs")
}

/// A catalog written for one test, for the declarations the loader refuses.
fn written(name: &str, toml: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let dir = root.join("etc/shiro/catalog");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&dir).expect("the scratch directory is writable");
    fs::write(dir.join("one.toml"), toml).expect("the scratch directory is writable");
    root
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("output is utf-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("output is utf-8")
}

fn json(output: &Output) -> Value {
    serde_json::from_str(&stdout(output)).expect("the payload parses")
}

fn lines(state: &Path, file: &str) -> Vec<String> {
    fs::read_to_string(state.join(file))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_list_draws_the_entries_its_generator_prints() {
    let state = state("entries-listing");
    let out = shiro(&state, &["pick", "themes", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let payload = json(&out);
    assert_eq!(payload["schema"], 2);
    // The level says it was generated. A front end caching a level should not
    // have to guess which kind of level it is holding.
    assert_eq!(payload["kind"], "list");
    assert_eq!(payload["path"], "pick.themes");

    let children = payload["children"].as_array().expect("children is a list");
    assert_eq!(children.len(), 2);

    // The generator's order is the order drawn: it is the only thing that knows
    // what order its subject has, and `order` is not a field it can set.
    assert_eq!(children[0]["path"], "pick.themes.nord");
    assert_eq!(children[0]["title"], "Nord");

    // An entry is an action: it runs, and there is no sense in which it is
    // installed, so it carries no `status` at all.
    assert_eq!(children[0]["kind"], "action");
    assert!(children[0].get("status").is_none(), "{}", children[0]);

    // Everything the generator said about presentation reaches the front end.
    assert_eq!(children[1]["description"], "Warm");
    assert_eq!(children[1]["icon"], "G");
    assert_eq!(children[1]["keywords"][0], "retro");
    // ... and `value` is not presentation: it is what the hook is given.
    assert!(children[1].get("value").is_none(), "{}", children[1]);
}

#[test]
fn a_list_in_a_menu_costs_nothing_until_it_is_opened() {
    // The generator is a process spawn, and drawing the level above must not
    // pay for every generator below it.
    let state = state("entries-not-eager");
    let out = shiro(&state, &["pick", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let payload = json(&out);
    let themes = payload["children"]
        .as_array()
        .expect("children is a list")
        .iter()
        .find(|child| child["path"] == "pick.themes")
        .expect("the list is a child of the menu")
        .clone();

    assert_eq!(themes["kind"], "list");
    assert_eq!(themes["icon"], "preferences-desktop-theme");
    // A list is a place to go, so it carries none of what describes running.
    assert!(themes.get("privilege").is_none(), "{themes}");
    assert!(themes.get("status").is_none(), "{themes}");

    assert!(lines(&state, "generated").is_empty(), "the generator ran");
}

#[test]
fn choosing_an_entry_runs_the_hook_the_catalog_declared() {
    let state = state("entries-run");
    let out = shiro(&state, &["pick", "themes", "gruv"]);
    assert!(out.status.success(), "{}", stderr(&out));

    // `value` is what the hook receives, because the string a user types and
    // the string a command needs are not always the same one.
    assert_eq!(
        lines(&state, "log"),
        ["chose /themes/gruvbox as pick.themes.gruv in run"]
    );

    // An entry with no `value` of its own is its own id.
    let out = shiro(&state, &["pick", "themes", "nord"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(
        lines(&state, "log")[1],
        "chose nord as pick.themes.nord in run"
    );
}

#[test]
fn an_entry_the_generator_does_not_offer_is_refused_with_what_it_does() {
    let state = state("entries-unknown");
    let out = shiro(&state, &["pick", "themes", "solarized"]);
    assert_eq!(out.status.code(), Some(2));

    let message = stderr(&out);
    assert!(message.contains("no such entry: `solarized`"), "{message}");
    assert!(message.contains("nord, gruv"), "{message}");
    assert!(lines(&state, "log").is_empty(), "the hook ran anyway");
}

#[test]
fn an_entry_takes_no_further_arguments() {
    let state = state("entries-extra-argument");
    let out = shiro(&state, &["pick", "themes", "nord", "extra"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("takes no further arguments"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn what_the_list_declares_about_running_is_carried_by_every_entry() {
    let state = state("entries-confirm");
    let out = shiro(&state, &["pick", "asks", "nord"]);
    assert_eq!(out.status.code(), Some(3));

    let message = stderr(&out);
    assert!(message.contains("Really?"), "{message}");
    assert!(lines(&state, "log").is_empty(), "it ran unconfirmed");

    let out = shiro(&state, &["pick", "asks", "nord", "--yes"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(lines(&state, "log"), ["asked"]);
}

#[test]
fn a_ttl_serves_the_second_listing_without_spawning_the_generator() {
    let cached = state("entries-cached");
    for _ in 0..3 {
        let out = shiro(&cached, &["pick", "cached", "--json"]);
        assert!(out.status.success(), "{}", stderr(&out));
    }
    assert_eq!(lines(&cached, "generated").len(), 1);

    // Without a ttl there is no cache at all, which is what a list that must
    // not go stale declares by saying nothing.
    let uncached = state("entries-uncached");
    for _ in 0..3 {
        let out = shiro(&uncached, &["pick", "themes", "--json"]);
        assert!(out.status.success(), "{}", stderr(&out));
    }
    assert_eq!(lines(&uncached, "generated").len(), 3);
}

#[test]
fn the_cache_can_be_turned_off_for_the_invocation() {
    let state = state("entries-cache-off");
    for _ in 0..2 {
        let out = shiro_with(
            &state,
            &["pick", "cached", "--json"],
            &[("SHIRO_LIST_CACHE", "0")],
        );
        assert!(out.status.success(), "{}", stderr(&out));
    }
    assert_eq!(lines(&state, "generated").len(), 2);
}

#[test]
fn a_generator_that_fails_says_so_rather_than_drawing_an_empty_menu() {
    let state = state("entries-failing");
    let out = shiro(&state, &["pick", "failing", "--json"]);
    assert_eq!(out.status.code(), Some(1));

    let message = stderr(&out);
    assert!(message.contains("exit 3"), "{message}");
    // Its own stderr is inherited: when a generator fails, what it says is the
    // only explanation anybody gets.
    assert!(
        message.contains("the generator explains itself"),
        "{message}"
    );
}

#[test]
fn a_generator_that_never_answers_is_killed() {
    let state = state("entries-slow");
    let out = shiro(&state, &["pick", "slow", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("did not answer within 2 seconds"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_generator_is_held_to_the_contract_it_prints_against() {
    let state = state("entries-contract");

    let refused = |path: &str, needle: &str| {
        let out = shiro(&state, &["pick", path, "--json"]);
        assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
        assert!(stderr(&out).contains(needle), "{}", stderr(&out));
    };

    refused("garbage", "did not print a list of entries");
    // An id is a path segment, because it is what a user types to reach the
    // entry. One that needs quoting is one nobody can type out of a menu.
    refused("untypable", "not a segment a user can type");
    refused("twice", "printed the id `nord` twice");
}

#[test]
fn a_list_that_offers_nothing_lists_nothing() {
    let state = state("entries-empty");
    let out = shiro(&state, &["pick", "empty", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(json(&out)["children"].as_array().expect("a list").len(), 0);

    let out = shiro(&state, &["pick", "empty", "anything"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("which is empty"), "{}", stderr(&out));
}

#[test]
fn an_entry_elevates_when_its_list_says_so() {
    let state = state("entries-root");
    let elevator = state.join("elevator");
    fs::write(
        &elevator,
        "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$STATE/elevator.log\"\nexec \"$@\"\n",
    )
    .expect("the scratch directory is writable");
    fs::set_permissions(
        &elevator,
        <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
    )
    .expect("the script is ours");

    let out = shiro_with(
        &state,
        &["pick", "rooted", "nord"],
        &[("SHIRO_SUDO", elevator.to_str().expect("a utf-8 path"))],
    );
    assert!(out.status.success(), "{}", stderr(&out));

    // The probe, then the hook, exactly as for an action that declares `system`.
    assert_eq!(lines(&state, "elevator.log"), ["true", "sh"]);
    assert_eq!(lines(&state, "log"), ["root nord"]);
}

#[test]
fn a_dry_run_shows_the_entry_the_hook_would_be_given() {
    let state = state("entries-dry-run");
    let out = shiro(&state, &["pick", "themes", "gruv", "--dry-run"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let printed = stdout(&out);
    assert!(printed.contains("SHIRO_ENTRY=/themes/gruvbox"), "{printed}");
    assert!(lines(&state, "log").is_empty(), "the hook ran");
}

#[test]
fn a_ttl_that_is_not_a_duration_is_refused_by_the_parser() {
    let state = state("entries-bad-ttl");
    let root = written(
        "entries-bad-ttl-catalog",
        "[[list]]\npath = \"pick\"\ntitle = \"Pick\"\n\n\
         [list.entries]\ncommand = \"true\"\nttl = \"5 seconds\"\n\n\
         [list.hooks]\nrun = \"true\"\n",
    );

    let out = shiro_at(&root, &state, &["--json"], &[]);
    assert_eq!(out.status.code(), Some(1));

    let message = stderr(&out);
    assert!(
        message.contains("`5 seconds` is not a duration"),
        "{message}"
    );
    assert!(message.contains("ttl"), "{message}");
}

#[test]
fn a_list_may_not_declare_children_of_its_own() {
    let state = state("entries-declared-child");
    let root = written(
        "entries-declared-child-catalog",
        "[[list]]\npath = \"pick\"\ntitle = \"Pick\"\n\n\
         [list.entries]\ncommand = \"true\"\n\n\
         [list.hooks]\nrun = \"true\"\n\n\
         [[item]]\npath = \"pick.one\"\ntitle = \"One\"\n",
    );

    let out = shiro_at(&root, &state, &["catalog", "validate", "--json"], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stdout(&out).contains("come from its generator"),
        "{}",
        stdout(&out)
    );
}
