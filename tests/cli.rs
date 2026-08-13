//! The contract shiro keeps is a process contract: arguments in, text or JSON
//! out, an exit code that means something. Integration tests therefore run the
//! real binary rather than calling into it.
//!
//! The scaffold has one test, and its job is to prove the harness works. It is
//! replaced by the first entry point that does something.

use std::process::Command;

#[test]
fn every_entry_point_is_reachable() {
    let invocations = [
        vec!["doctor"],
        vec!["catalog", "validate"],
        vec!["version"],
        vec!["perms", "flatpak", "com.visualstudio.code"],
        vec!["run", "brave"],
        vec!["install", "code", "vs-code"],
    ];

    for args in invocations {
        let out = Command::new(env!("CARGO_BIN_EXE_shiro"))
            .args(&args)
            .output()
            .expect("the binary runs");

        // 70 is the scaffold's "parsed its way here and stopped".
        assert_eq!(out.status.code(), Some(70), "{args:?}");
    }
}
