//! The native commands: the ones that report on the engine's own state, and so
//! cannot be a recipe. Table in `docs/architecture.md` section 1.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::catalog::{Catalog, load, sources, validate};
use crate::cli::Options;
use crate::error::Error;
use crate::render::json::SCHEMA;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `shiro doctor`: which layers loaded, and what the merged catalog is made of.
pub fn doctor(args: Vec<String>, opts: Options) -> Result<(), Error> {
    no_arguments(&args, "doctor")?;
    let catalog = load::load()?;

    let mechanisms = mechanisms(&catalog);
    let nodes = catalog.iter().count();

    if opts.json {
        let layers: Vec<Value> = catalog
            .layers
            .iter()
            .map(|layer| {
                json!({
                    "layer": layer.layer.as_str(),
                    "location": layer.location,
                    "present": layer.present,
                    "files": layer.files,
                    "nodes": layer.nodes,
                })
            })
            .collect();

        println!(
            "{}",
            json!({
                "schema": SCHEMA,
                "kind": "doctor",
                "version": VERSION,
                "catalog": catalog.digest(),
                "nodes": nodes,
                "layers": layers,
                "mechanisms": mechanisms.iter().map(|(name, count)| json!({
                    "mechanism": name,
                    "items": count,
                })).collect::<Vec<Value>>(),
            })
        );
        return Ok(());
    }

    println!("shiro {VERSION}");
    println!();
    println!("Catalog layers, lowest precedence first:");
    let width = catalog
        .layers
        .iter()
        .map(|layer| layer.location.chars().count())
        .max()
        .unwrap_or(0);
    for layer in &catalog.layers {
        let state = if !layer.present {
            "absent".to_owned()
        } else {
            format!(
                "{} {}, {} nodes",
                layer.files,
                plural(layer.files, "file"),
                layer.nodes
            )
        };
        println!(
            "  {:9}  {:width$}  {state}",
            layer.layer.as_str(),
            layer.location
        );
    }
    println!();
    println!("{nodes} nodes, digest {}", catalog.digest());

    if !mechanisms.is_empty() {
        println!();
        println!("Mechanisms declared by items:");
        for (mechanism, count) in &mechanisms {
            println!("  {mechanism:20}  {count}");
        }
    }

    Ok(())
}

/// `shiro catalog validate` and `shiro catalog sources`.
pub fn catalog(args: Vec<String>, opts: Options) -> Result<(), Error> {
    match args.split_first() {
        Some((verb, rest)) if verb == "validate" => {
            no_arguments(rest, "catalog validate")?;
            validate_catalog(opts)
        }
        Some((verb, rest)) if verb == "sources" => {
            no_arguments(rest, "catalog sources")?;
            let catalog = load::load()?;
            if opts.json {
                println!("{}", sources::json(&catalog));
            } else {
                print!("{}", sources::text(&catalog));
            }
            Ok(())
        }
        Some((verb, _)) => Err(Error::Usage(format!(
            "no such command: `catalog {verb}`\ncatalog has: validate, sources"
        ))),
        None => Err(Error::Usage(
            "catalog needs a subcommand: validate, sources".to_owned(),
        )),
    }
}

fn validate_catalog(opts: Options) -> Result<(), Error> {
    let catalog = load::load()?;
    let findings = validate::run(&catalog);

    if opts.json {
        println!(
            "{}",
            json!({
                "schema": SCHEMA,
                "kind": "validation",
                "ok": findings.is_empty(),
                "findings": findings.iter().map(|finding| json!({
                    "path": finding.path,
                    "file": finding.file,
                    "message": finding.message,
                })).collect::<Vec<Value>>(),
            })
        );
    } else if findings.is_empty() {
        println!("The catalog is valid: {} nodes.", catalog.iter().count());
    } else {
        for finding in &findings {
            println!("{}: `{}` {}", finding.file, finding.path, finding.message);
        }
    }

    if findings.is_empty() {
        Ok(())
    } else {
        // The findings are the report; this only carries the exit code and the
        // count, so a caller that pipes stdout still learns it failed.
        Err(Error::Catalog(format!(
            "the catalog has {} {}",
            findings.len(),
            plural(findings.len(), "problem")
        )))
    }
}

/// `shiro version`: the version, and the digest of the merged catalog.
pub fn version(args: Vec<String>, opts: Options) -> Result<(), Error> {
    no_arguments(&args, "version")?;
    let catalog = load::load()?;

    if opts.json {
        println!(
            "{}",
            json!({
                "schema": SCHEMA,
                "kind": "version",
                "version": VERSION,
                "catalog": catalog.digest(),
            })
        );
    } else {
        println!("shiro {VERSION}");
        println!("catalog {}", catalog.digest());
    }
    Ok(())
}

/// What each declared `mechanism` label covers, by item count. shiro reports
/// the labels its catalog uses and counts them; it does not know what any of
/// them means, and does not probe the host for them.
fn mechanisms(catalog: &Catalog) -> Vec<(String, usize)> {
    let mut counted: BTreeMap<String, usize> = BTreeMap::new();

    for node in catalog.iter() {
        let Some(item) = &node.item else {
            continue;
        };
        let label = item
            .mechanism
            .clone()
            .unwrap_or_else(|| "(unlabelled)".to_owned());
        *counted.entry(label).or_default() += 1;
    }

    counted.into_iter().collect()
}

fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        word.to_owned()
    } else {
        format!("{word}s")
    }
}

fn no_arguments(args: &[String], command: &str) -> Result<(), Error> {
    match args.first() {
        Some(extra) => Err(Error::Usage(format!(
            "`{command}` takes no arguments, but got `{extra}`"
        ))),
        None => Ok(()),
    }
}
