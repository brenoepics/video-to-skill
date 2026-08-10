//! End-to-end proof that the sandbox confines for real.
//!
//! The unit tests in `sandbox::profile` assert the policy *text*; these
//! assert the kernel actually enforces it, because a policy that reads
//! correctly but is never applied would be the worst possible outcome
//! for the finding it answers.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use vts_extract::sandbox;

struct Workspace(PathBuf);

impl Workspace {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("vts-sbx-{tag}-{}", std::process::id()));
        fs::create_dir_all(&dir)
            .unwrap_or_else(|e| panic!("create workspace {}: {e}", dir.display()));
        Self(dir)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn sh(script: &str) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), script.into()]
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| panic!("HOME is not set")))
}

/// Every test is a no-op where no mechanism exists — the refusal path
/// is covered separately by the unit tests.
fn skip() -> bool {
    if sandbox::detect().is_none() {
        eprintln!("no sandbox mechanism available — skipping");
        return true;
    }
    false
}

#[test]
fn a_confined_command_still_runs_and_writes_inside_the_workspace() {
    if skip() {
        return;
    }
    let ws = Workspace::new("write-in");
    let code = sandbox::run(&ws.0, &sh("echo hello > inside.txt")).expect("run");
    assert_eq!(code, 0, "a benign command must still work inside");
    let written = fs::read_to_string(ws.0.join("inside.txt")).expect("file written");
    assert_eq!(written.trim(), "hello");
}

#[test]
fn writes_outside_the_workspace_are_refused() {
    if skip() {
        return;
    }
    let ws = Workspace::new("write-out");
    let escape = home().join("vts-sandbox-escape-probe.txt");
    fs::remove_file(&escape).ok();

    let code = sandbox::run(
        &ws.0,
        &sh(r#"echo pwned > "$HOME/vts-sandbox-escape-probe.txt""#),
    )
    .expect("run");

    assert_ne!(code, 0, "writing into $HOME must fail");
    assert!(
        !escape.exists(),
        "the sandbox let a video-derived command write to {}",
        escape.display()
    );
    fs::remove_file(&escape).ok();
}

#[test]
fn the_network_is_unreachable() {
    if skip() {
        return;
    }
    // Only meaningful if the host itself has network; otherwise the
    // assertion would pass vacuously and prove nothing.
    let reachable = Command::new("/usr/bin/curl")
        .args(["-s", "-m", "8", "-o", "/dev/null", "https://example.com"])
        .status()
        .is_ok_and(|s| s.success());
    if !reachable {
        eprintln!("host has no network — skipping the confinement half");
        return;
    }

    let ws = Workspace::new("net");
    let code = sandbox::run(
        &ws.0,
        &sh("/usr/bin/curl -s -m 8 -o /dev/null https://example.com"),
    )
    .expect("run");
    assert_ne!(
        code, 0,
        "the same request that succeeds unconfined must fail inside the sandbox"
    );
}

#[test]
fn credential_stores_are_unreadable() {
    if skip() {
        return;
    }
    // Pick a credential dir that actually exists on this machine, so
    // the assertion has something real to be denied.
    let Some(dir) = [".ssh", ".aws", ".claude", ".config/gh"]
        .iter()
        .map(|d| home().join(d))
        .find(|p| p.is_dir())
    else {
        eprintln!("no credential store present — skipping");
        return;
    };
    assert!(
        fs::read_dir(&dir).is_ok(),
        "precondition: readable outside the sandbox"
    );

    let ws = Workspace::new("creds");
    let script = format!("ls {} > /dev/null 2>&1", shell_quote(&dir));
    let code = sandbox::run(&ws.0, &sh(&script)).expect("run");
    assert_ne!(
        code,
        0,
        "the sandbox let a video-derived command read {}",
        dir.display()
    );
}

fn shell_quote(path: &std::path::Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}
