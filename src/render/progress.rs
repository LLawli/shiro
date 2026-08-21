//! Reporting a running recipe, as it runs.
//!
//! A front end shows progress on a long install rather than a spinner, so the
//! JSON form is one object per phase transition, on its own line, ending with
//! the outcome. The text form is what a person watching a terminal wants: the
//! phase it is in, and the hook's own output underneath it.

use std::io::{Write, stderr, stdout};
use std::os::fd::AsFd;
use std::process::Stdio;

use serde_json::json;

use crate::catalog::Node;
use crate::exec::Phase;
use crate::render::json::SCHEMA;

#[derive(Debug, Clone, Copy)]
pub enum Outcome {
    Installed,
    Removed,
    /// An action ran. Not "installed": there is nothing now present that was
    /// absent before, which is the whole difference between the two.
    Ok,
    /// Everything the transaction did was undone.
    RolledBack,
    /// Only the phase that failed was undone, and the rest stands.
    Partial,
    Failed,
    RollbackFailed,
    DryRun,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Outcome::Installed => "installed",
            Outcome::Removed => "removed",
            Outcome::Ok => "ok",
            Outcome::RolledBack => "rolled-back",
            Outcome::Partial => "partial",
            Outcome::Failed => "failed",
            Outcome::RollbackFailed => "rollback-failed",
            Outcome::DryRun => "dry-run",
        }
    }
}

pub struct Progress<'a> {
    json: bool,
    node: &'a Node,
}

impl<'a> Progress<'a> {
    pub fn new(json: bool, node: &'a Node) -> Self {
        Progress { json, node }
    }

    /// Where a hook's stdout goes.
    ///
    /// Under `--json` it goes to stderr, so that a hook that prints a progress
    /// bar still reaches the user while stdout stays a clean stream of objects.
    /// Capturing it instead would hold a long install's output back until the
    /// phase ended, which is the opposite of the point.
    pub fn hook_output(&self) -> Stdio {
        if !self.json {
            return Stdio::inherit();
        }

        stderr()
            .as_fd()
            .try_clone_to_owned()
            .map(Stdio::from)
            .unwrap_or_else(|_| Stdio::inherit())
    }

    pub fn phase_start(&self, phase: Phase) {
        if self.json {
            self.emit(json!({
                "schema": SCHEMA,
                "kind": "phase",
                "item": self.node.path,
                "phase": phase.as_str(),
                "state": "start",
            }));
        } else {
            println!("==> {}", phase.as_str());
            let _ = stdout().flush();
        }
    }

    pub fn phase_end(&self, phase: Phase, code: i32) {
        if self.json {
            self.emit(json!({
                "schema": SCHEMA,
                "kind": "phase",
                "item": self.node.path,
                "phase": phase.as_str(),
                "state": if code == 0 { "ok" } else { "failed" },
                "exit": code,
            }));
        } else if code != 0 {
            println!("    failed (exit {code})");
        }
    }

    pub fn dry_run(&self, phase: Phase, command: &str, elevated: bool) {
        if self.json {
            self.emit(json!({
                "schema": SCHEMA,
                "kind": "phase",
                "item": self.node.path,
                "phase": phase.as_str(),
                "state": "dry-run",
                "elevated": elevated,
                "command": command,
            }));
        } else {
            println!(
                "==> {}{}",
                phase.as_str(),
                if elevated { ", as root" } else { "" }
            );
            println!("    {command}");
        }
    }

    pub fn result(&self, outcome: Outcome, exit: u8) {
        if self.json {
            self.emit(json!({
                "schema": SCHEMA,
                "kind": "result",
                "item": self.node.path,
                "outcome": outcome.as_str(),
                "exit": exit,
            }));
            return;
        }

        let path = &self.node.path;
        match outcome {
            Outcome::Installed => println!("\nInstalled `{path}`."),
            Outcome::Removed => println!("\nRemoved `{path}`."),
            Outcome::Ok => println!("\nRan `{path}`."),
            Outcome::RolledBack => println!("\nRolled back `{path}`: the system is as it was."),
            Outcome::Partial => {
                println!(
                    "\nStopped: `{path}` is partially applied, and only the failed phase was undone."
                )
            }
            // The error on stderr names the hook and the exit code, and saying
            // it twice in different words helps nobody.
            Outcome::Failed | Outcome::RollbackFailed => {}
            Outcome::DryRun => println!("\nNothing ran: this was a dry run."),
        }
    }

    fn emit(&self, value: serde_json::Value) {
        println!("{value}");
        let _ = stdout().flush();
    }
}
