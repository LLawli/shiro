//! The contract shiro keeps is a process contract: arguments in, text or JSON
//! out, an exit code that means something. Integration tests therefore run the
//! real binary rather than calling into it.
//!
//! The catalog under test is a fixture tree, reached through `SHIRO_ROOT` and
//! `XDG_DATA_HOME`, so a test never reads the machine it runs on.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn shiro(catalog: &str, args: &[&str]) -> Output {
    shiro_at(&fixture(catalog), args)
}

fn shiro_at(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shiro"))
        .args(args)
        .env("SHIRO_ROOT", root)
        .env("XDG_DATA_HOME", root.join("xdg"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("SHIRO_CHECK_TIMEOUT", "1")
        .output()
        .expect("the binary runs")
}

/// A catalog written for one test, for the declarations that a checked-in
/// fixture cannot hold: the ones the loader refuses outright.
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

/// A child by path, so that a test says which node it means rather than which
/// slot the fixture's ordering happens to put it in.
fn child<'a>(payload: &'a Value, path: &str) -> &'a Value {
    payload["children"]
        .as_array()
        .expect("children is a list")
        .iter()
        .find(|child| child["path"] == path)
        .unwrap_or_else(|| panic!("no child at `{path}`: {payload}"))
}

#[test]
fn the_root_lists_what_the_layers_contributed() {
    let out = shiro("valid", &[]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("install"));
}

#[test]
fn a_menu_lists_its_children_with_a_status_each() {
    let out = shiro("valid", &["install", "code", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let payload = json(&out);
    assert_eq!(payload["schema"], 2);
    assert_eq!(payload["kind"], "menu");
    assert_eq!(payload["path"], "install.code");
    assert_eq!(payload["title"], "Code editors");

    let children = payload["children"].as_array().expect("children is a list");
    assert_eq!(children.len(), 2, "helix is hidden by the machine layer");

    assert_eq!(children[0]["path"], "install.code.vs-code");
    assert_eq!(children[0]["status"], "installed");
    assert_eq!(children[0]["mechanism"], "flatpak");
    assert_eq!(children[0]["privilege"], "user");
    assert_eq!(children[0]["layer"], "image");

    // The machine layer replaced this node wholesale, label and all.
    assert_eq!(children[1]["path"], "install.code.zed");
    assert_eq!(children[1]["title"], "Zed, from the machine's own box");
    assert_eq!(children[1]["mechanism"], "distrobox");
    assert_eq!(children[1]["layer"], "machine");
    assert_eq!(children[1]["status"], "installed");
}

#[test]
fn a_check_that_never_answers_reports_timeout() {
    let out = shiro("valid", &["install", "games", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let payload = json(&out);
    assert_eq!(child(&payload, "install.games.steam")["status"], "timeout");
}

#[test]
fn a_path_that_stops_matching_says_what_was_there() {
    let out = shiro("valid", &["install", "kode"]);
    assert_eq!(out.status.code(), Some(2));

    let message = stderr(&out);
    assert!(message.contains("no such command: `kode`"), "{message}");
    assert!(message.contains("code"), "{message}");
    assert!(message.contains("games"), "{message}");
}

#[test]
fn an_unknown_flag_is_refused_rather_than_taken_as_a_path() {
    let out = shiro("valid", &["install", "--jsno"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("unknown flag `--jsno`"));
}

#[test]
fn an_item_takes_no_further_arguments() {
    let out = shiro("valid", &["install", "code", "vs-code", "extra"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("takes no further arguments"));
}

#[test]
fn a_hidden_node_is_not_reachable_by_name() {
    let out = shiro("valid", &["install", "code", "helix"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("no such command: `helix`"));
}

#[test]
fn sources_says_which_layer_won_and_what_it_replaced() {
    let out = shiro("valid", &["catalog", "sources", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let payload = json(&out);
    let zed = payload["nodes"]
        .as_array()
        .expect("nodes is a list")
        .iter()
        .find(|node| node["path"] == "install.code.zed")
        .expect("zed is in the catalog")
        .clone();

    assert_eq!(zed["layer"], "machine");
    assert_eq!(zed["overridden"][0]["layer"], "image");
}

#[test]
fn a_valid_catalog_validates() {
    let out = shiro("valid", &["catalog", "validate", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(json(&out)["ok"], true);
}

#[test]
fn every_schema_rule_is_enforced() {
    let out = shiro("broken", &["catalog", "validate", "--json"]);
    assert_eq!(out.status.code(), Some(1));

    let payload = json(&out);
    assert_eq!(payload["ok"], false);

    let findings: Vec<String> = payload["findings"]
        .as_array()
        .expect("findings is a list")
        .iter()
        .map(|finding| format!("{} {}", finding["path"], finding["message"]))
        .collect();
    let found = |needle: &str| {
        assert!(
            findings.iter().any(|finding| finding.contains(needle)),
            "no finding mentions {needle}: {findings:#?}"
        )
    };

    found("is a native command");
    found("is not declared");
    found("an item has no children");
    found("without `roll-install`");
    found("without `roll-pre`");
    found("does not exist next to the recipe");
    found("leaves the layer that declared it");
    found("it is `run` or `flatpak`");
    found("empty `app`");
    found("matches every search");
    found("no question to ask");
    found("an action has no children");
    // The permission vocabulary, which `apply` would refuse in the recipe's
    // `post`, on the machine of whoever installed.
    found("is not a device flatpak has");
    found("cannot be granted");
    found("which nothing will read");
    found("not a profile the registry could write");
    found("come from its generator");
    found("nothing to list");
    found("does not exist next to the recipe");
}

#[test]
fn a_recipe_is_held_to_the_permission_words_apply_is_held_to() {
    // The check existed and only ran at `apply`, which is the recipe's `post`,
    // on the machine of whoever installed. Under `rollback = "atomic"` that
    // failure takes the installation with it, and every gate before it, an
    // image build validating its whole catalog included, reported success.
    let out = shiro("broken", &["catalog", "validate", "--json"]);
    assert_eq!(out.status.code(), Some(1));

    let payload = json(&out);
    let about = |path: &str| -> Vec<String> {
        payload["findings"]
            .as_array()
            .expect("findings is a list")
            .iter()
            .filter(|finding| finding["path"] == path)
            .map(|finding| finding["message"].as_str().expect("a message").to_owned())
            .collect()
    };

    let vocabulary = about("broken.bad-vocabulary");
    assert_eq!(vocabulary.len(), 2, "{vocabulary:#?}");
    // A value flatpak does not have, reported with the list, exactly as the
    // terminal reports it.
    assert!(
        vocabulary.iter().any(
            |finding| finding.contains("`gpu` is not a device flatpak has")
                && finding.contains("dri, input, usb")
        ),
        "{vocabulary:#?}"
    );
    // And the one that can never be granted, wherever it is declared.
    assert!(
        vocabulary
            .iter()
            .any(|finding| finding.contains("flatpak-spawn --host")),
        "{vocabulary:#?}"
    );

    // A backend that does not exist stops the rest: nothing below it can be
    // read without knowing which shape the declaration is.
    let backend = about("broken.bad-permissions");
    assert_eq!(backend.len(), 1, "{backend:#?}");
    assert!(
        backend[0].contains("it is `run` or `flatpak`"),
        "{backend:#?}"
    );
}

#[test]
fn an_action_is_listed_as_a_verb_and_carries_no_status() {
    let out = shiro("valid", &["install", "games", "emulation", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let payload = json(&out);
    let action = child(&payload, "install.games.emulation.reset-pads");
    assert_eq!(action["kind"], "action");
    assert_eq!(action["icon"], "input-gaming");
    assert_eq!(action["destructive"], true);
    assert_eq!(action["privilege"], "user");

    // The whole request in one assertion: `unknown` is the answer to "is this
    // installed", and an action does not raise that question.
    assert!(action.get("status").is_none(), "{action}");

    // The item beside it does answer it, so the absence above is a statement
    // about the kind rather than about this fixture having no `check`.
    let item = child(&payload, "install.games.emulation.retroarch");
    assert_eq!(item["status"], "unknown");
}

#[test]
fn what_an_action_may_not_declare_is_refused_by_the_parser() {
    // The whole reason an action is its own declaration rather than an item
    // carrying a flag. The failure names the key and the line, at load, rather
    // than arriving as a validator finding about a field that exists and means
    // nothing here.
    let root = written(
        "action-with-check",
        "[[action]]\npath = \"lock\"\ntitle = \"Lock\"\n\n\
         [action.hooks]\nrun = \"true\"\ncheck = \"true\"\n",
    );
    let out = shiro_at(&root, &["--json"]);
    assert_eq!(out.status.code(), Some(1));

    let message = stderr(&out);
    assert!(message.contains("unknown field `check`"), "{message}");
    assert!(message.contains("expected `run`"), "{message}");

    let root = written(
        "action-with-rollback",
        "[[action]]\npath = \"lock\"\ntitle = \"Lock\"\nrollback = \"none\"\n\n\
         [action.hooks]\nrun = \"true\"\n",
    );
    let out = shiro_at(&root, &["--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("unknown field `rollback`"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn sources_lines_up_the_widest_kind() {
    let out = shiro("valid", &["catalog", "sources"]);
    assert!(out.status.success(), "{}", stderr(&out));

    // `action` is a character wider than `menu` and `item`, and a column that
    // is too narrow misaligns without failing anything.
    let printed = stdout(&out);
    let columns: Vec<usize> = printed
        .lines()
        .filter(|line| line.contains(" user "))
        .map(|line| line.find(" user ").expect("the layer column"))
        .collect();
    assert!(columns.len() > 1, "{printed}");
    assert!(
        columns.windows(2).all(|pair| pair[0] == pair[1]),
        "{printed}"
    );
}

#[test]
fn presentation_metadata_reaches_a_front_end_untouched() {
    let out = shiro("valid", &["install", "games", "emulation", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let payload = json(&out);
    let item = child(&payload, "install.games.emulation.retroarch");
    assert_eq!(item["icon"], "\u{f11b}");
    assert_eq!(item["keywords"][0], "emulador");
    assert_eq!(item["confirm"], "RetroArch baixa vários gigabytes. Seguir?");

    // A flag is emitted only when it is true, so that "is it set" and "is it
    // true" are one check for a consumer, as with any other absent field.
    assert_eq!(item["interactive"], true);
    assert_eq!(item["keep_open"], true);
    assert!(item.get("destructive").is_none(), "{item}");

    // A menu is drawn as a row like any other, so it carries an icon too, and
    // still carries none of the fields that describe running something.
    let games = json(&shiro("valid", &["install", "games", "--json"]));
    let menu = child(&games, "install.games.emulation");
    assert_eq!(menu["kind"], "menu");
    assert_eq!(menu["icon"], "applications-games");
    assert!(menu.get("privilege").is_none(), "{menu}");
    assert!(menu.get("status").is_none(), "{menu}");
}

#[test]
fn version_and_doctor_agree_on_the_catalog_digest() {
    let version = shiro("valid", &["version", "--json"]);
    let doctor = shiro("valid", &["doctor", "--json"]);
    assert!(version.status.success(), "{}", stderr(&version));
    assert!(doctor.status.success(), "{}", stderr(&doctor));

    let digest = json(&version)["catalog"].clone();
    assert!(digest.as_str().is_some_and(|value| value.len() == 16));
    assert_eq!(json(&doctor)["catalog"], digest);

    // A different catalog is a different digest, which is the whole point.
    let other = shiro("broken", &["version", "--json"]);
    assert_ne!(json(&other)["catalog"], digest);
}

#[test]
fn doctor_reports_each_layer_and_the_labels_it_found() {
    let out = shiro("valid", &["doctor", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let payload = json(&out);
    let layers = payload["layers"].as_array().expect("layers is a list");
    assert_eq!(layers.len(), 4);
    assert_eq!(layers[0]["layer"], "built-in");
    assert_eq!(layers[1]["layer"], "image");
    assert_eq!(layers[1]["present"], true);
    assert_eq!(layers[1]["files"], 1);

    let mechanisms: Vec<&str> = payload["mechanisms"]
        .as_array()
        .expect("mechanisms is a list")
        .iter()
        .map(|entry| entry["mechanism"].as_str().expect("a label"))
        .collect();
    assert!(mechanisms.contains(&"flatpak"), "{mechanisms:?}");
    assert!(mechanisms.contains(&"distrobox"), "{mechanisms:?}");
}

#[test]
fn naming_an_item_runs_its_recipe_and_the_gate_comes_first() {
    // This item's check answers `installed`, so the recipe is refused rather
    // than re-run. The executor's own behavior is covered in tests/exec.rs.
    let out = shiro("valid", &["install", "code", "vs-code"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(
        stderr(&out).contains("already installed"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn the_built_in_layer_gives_a_bare_machine_a_command_surface() {
    // No layers on disk at all: what is left is what ships in the binary.
    let out = shiro("no-such-fixture", &["--json"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let payload = json(&out);
    let roots: Vec<&str> = payload["children"]
        .as_array()
        .expect("children is a list")
        .iter()
        .map(|child| child["path"].as_str().expect("a path"))
        .collect();

    assert_eq!(roots, ["install", "update", "theme"]);
    assert!(
        payload["children"]
            .as_array()
            .expect("children is a list")
            .iter()
            .all(|child| child["layer"] == "built-in")
    );
}

#[test]
fn an_empty_menu_says_where_recipes_come_from() {
    // The built-in layer declares roots and no recipes, so this is the first
    // thing a fresh install shows. It has to be an instruction, not a dead end.
    let out = shiro("no-such-fixture", &["install"]);
    assert!(out.status.success(), "{}", stderr(&out));

    let printed = stdout(&out);
    assert!(printed.contains("nothing here yet"), "{printed}");
    assert!(printed.contains("shiro/catalog"), "{printed}");
    assert!(printed.contains("image"), "{printed}");
    assert!(printed.contains("user"), "{printed}");
}

#[test]
fn the_built_in_layer_validates_on_its_own() {
    // The layer that ships inside the binary is held to the schema it enforces.
    let out = shiro("no-such-fixture", &["catalog", "validate", "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(json(&out)["ok"], true);
}
