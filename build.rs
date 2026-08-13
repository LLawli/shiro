//! Embeds the built-in catalog layer into the binary.
//!
//! `catalog/` is the base curation, versioned with the engine that runs it, so
//! that a freshly built binary is useful with no files on disk. The generated
//! file is a sorted list of (display name, contents), which keeps the embedded
//! layer byte-identical for identical sources.

use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    println!("cargo:rerun-if-changed=catalog");

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let root =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets the manifest dir"))
            .join("catalog");

    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    files.sort();

    let mut generated = String::from(
        "/// The built-in catalog layer: (display name, contents), sorted.\n\
         static BUILTIN_CATALOG: &[(&str, &str)] = &[\n",
    );
    for (name, path) in &files {
        generated.push_str(&format!(
            "    ({:?}, include_str!({:?})),\n",
            name,
            path.display().to_string()
        ));
    }
    generated.push_str("];\n");

    fs::write(out.join("builtin_catalog.rs"), generated).expect("the generated file is writable");
}

fn collect(root: &Path, dir: &Path, found: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, found);
        } else if path.extension().is_some_and(|ext| ext == "toml") {
            println!("cargo:rerun-if-changed={}", path.display());
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            found.push((name, path));
        }
    }
}
