//! shiro turns a curated TOML catalog into an executable, navigable command
//! tree, and owns what the tools it installs are allowed to touch.

mod catalog;
mod cli;
mod dispatch;
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
    match cli::parse(args)? {
        Invocation::Doctor(rest, opts) => native::doctor(rest, opts),
        Invocation::Catalog(rest, opts) => native::catalog(rest, opts),
        Invocation::Version(rest, opts) => native::version(rest, opts),
        Invocation::Perms(rest) => perms::perms(rest),
        Invocation::Run(rest) => perms::run(rest),
        Invocation::Path(rest, opts) => dispatch::path(&rest, &opts),
    }
}
