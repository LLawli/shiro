//! The contract shiro keeps is a process contract: arguments in, text or JSON
//! out, an exit code that means something. Integration tests therefore run the
//! real binary rather than calling into it.
//!
//! The catalog under test is a fixture tree, reached through `SHIRO_ROOT` and
//! `XDG_DATA_HOME`, so a test never reads the machine it runs on.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn shiro(catalog: &str, args: &[&str]) -> Output {
    let root = fixture(catalog);
    Command::new(env!("CARGO_BIN_EXE_shiro"))
        .args(args)
        .env("SHIRO_ROOT", &root)
        .env("XDG_DATA_HOME", root.join("xdg"))
        .env("SHIRO_CHECK_TIMEOUT", "1")
        .output()
        .expect("the binary runs")
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
    assert_eq!(payload["schema"], 1);
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
    let children = payload["children"].as_array().expect("children is a list");
    assert_eq!(children[0]["path"], "install.games.steam");
    assert_eq!(children[0]["status"], "timeout");
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
fn the_permissions_module_is_still_a_stub() {
    for args in [
        vec!["perms", "flatpak", "com.brave.Browser"],
        vec!["run", "brave"],
    ] {
        let out = shiro("valid", &args);
        assert_eq!(out.status.code(), Some(70), "{args:?}");
    }
}
