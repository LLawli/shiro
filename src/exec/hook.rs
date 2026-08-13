//! Turning a declared hook into a process.
//!
//! A hook is a shell string passed to `sh -c`, or a `{ file = "..." }` script
//! resolved relative to the TOML file that declared it. Privilege is decided
//! per item, before anything runs, so that a transaction never stops halfway to
//! ask for a password.

use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};

use crate::catalog::Node;
use crate::catalog::model::Hook;
use crate::exec::env;

/// What shiro elevates with, as a command and its arguments.
///
/// `sudo` because its credential cache is what makes one prompt cover a whole
/// transaction; `SHIRO_SUDO` is for the machine that uses `run0`, `doas` or
/// `pkexec` instead, and it may carry arguments (`SHIRO_SUDO="sudo -A"`).
pub fn elevator() -> Vec<String> {
    let configured = std::env::var("SHIRO_SUDO").unwrap_or_default();
    let parts: Vec<String> = configured.split_whitespace().map(str::to_owned).collect();

    if parts.is_empty() {
        vec!["sudo".to_owned()]
    } else {
        parts
    }
}

pub fn is_root() -> bool {
    // SAFETY: geteuid cannot fail and touches nothing.
    unsafe { libc::geteuid() == 0 }
}

/// A resolved hook: exactly what would run, which is also exactly what
/// `--dry-run` prints.
pub struct Invocation {
    program: String,
    args: Vec<String>,
    /// Applied to the child directly. Empty when elevated, because the
    /// environment then travels inside the shell script instead.
    env: Vec<(String, String)>,
    cwd: Option<PathBuf>,
    elevated: bool,
}

pub fn build(node: &Node, hook: &Hook, phase: &str, elevated: bool, dry_run: bool) -> Invocation {
    let dir = node.source.dir.clone();
    let variables = env::variables(node, phase, dry_run);

    if elevated {
        // sudo resets the environment, and passing `VAR=value` through it is
        // refused by a default sudoers. Exporting inside the script is the form
        // that needs no configuration on the host, and it is visible in
        // `--dry-run` rather than hidden in a flag.
        let mut script = String::new();
        for (key, value) in &variables {
            script.push_str(&format!("export {key}={}; ", quote(value)));
        }
        match hook {
            Hook::Shell(body) => script.push_str(body),
            Hook::Script(file) => {
                let path = dir.as_ref().map(|dir| dir.join(file)).unwrap_or_default();
                script.push_str(&format!("exec {}", quote(&path.display().to_string())));
            }
        }

        let mut elevator = elevator();
        let program = elevator.remove(0);
        elevator.extend(["sh".to_owned(), "-c".to_owned(), script]);

        return Invocation {
            program,
            args: elevator,
            env: Vec::new(),
            cwd: dir,
            elevated,
        };
    }

    let (program, args) = match hook {
        Hook::Shell(body) => ("sh".to_owned(), vec!["-c".to_owned(), body.clone()]),
        Hook::Script(file) => {
            let path = dir.as_ref().map(|dir| dir.join(file)).unwrap_or_default();
            (path.display().to_string(), Vec::new())
        }
    };

    Invocation {
        program,
        args,
        env: variables,
        cwd: dir,
        elevated,
    }
}

impl Invocation {
    /// Run it, with the hook's output going wherever `output` points. A hook
    /// that installs software talks to the user while it works, so its output
    /// is never swallowed.
    pub fn run(&self, output: Stdio) -> io::Result<ExitStatus> {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        for (key, value) in &self.env {
            command.env(key, value);
        }
        if let Some(dir) = &self.cwd {
            command.current_dir(dir);
        }
        command.stdout(output).stderr(Stdio::inherit());
        command.status()
    }

    /// The line a human could paste into a shell to get the same effect. This
    /// is what a tool that runs arbitrary scripts as root owes the user.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        for (key, value) in &self.env {
            parts.push(format!("{key}={}", quote(value)));
        }
        parts.push(self.program.clone());
        parts.extend(self.args.iter().map(|arg| quote(arg)));
        parts.join(" ")
    }

    pub fn elevated(&self) -> bool {
        self.elevated
    }
}

/// Single quotes, with the one escape that form needs. Correct beats clever
/// here: this string is shown to a user deciding whether to run something as
/// root.
fn quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/:=".contains(c))
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}
