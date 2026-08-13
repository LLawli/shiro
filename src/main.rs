//! shiro turns a curated TOML catalog into an executable, navigable command
//! tree, and owns what the tools it installs are allowed to touch.
//!
//! This is the scaffold: the module layout of `docs/architecture.md` section 8
//! exists and compiles, the argument split between the reserved native
//! namespace and the catalog path is real, and every entry point reports that
//! it has no behavior yet.

mod catalog;
mod cli;
mod error;
mod exec;
mod native;
mod perms;
mod render;

use std::process::ExitCode;

use crate::cli::Invocation;
use crate::error::Error;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("shiro: {err}");
            ExitCode::from(err.code())
        }
    }
}

fn run(args: &[String]) -> Result<(), Error> {
    match cli::parse(args) {
        Invocation::Doctor(rest) => native::doctor(rest),
        Invocation::Catalog(rest) => native::catalog(rest),
        Invocation::Version(rest) => native::version(rest),
        Invocation::Perms(rest) => perms::perms(rest),
        Invocation::Run(rest) => perms::run(rest),
        Invocation::Path(rest) => catalog::dispatch(rest),
    }
}
