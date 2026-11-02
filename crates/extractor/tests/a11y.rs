//! GUI-observed postconditions end to end: the `observe` command's
//! exit-code contract, the filesystem check with no GUI backend, and
//! (ignored by default) a live read of a real application's accessibility
//! tree.
//!
//! The live test needs Accessibility permission for the process running
//! `cargo test` — grant it to your terminal (or the agent host) in
//! System Settings → Privacy & Security → Accessibility, then run:
//!
//! ```text
//! cargo test --test a11y -- --ignored
//! ```
//!
//! It opens a scratch text file in `TextEdit`, observes the window title
//! and the File menu, and quits `TextEdit` again if it was not running
//! before. CI runners have no Accessibility permission, which is why it
//! is not part of the default suite.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use vts_extract::a11y::check::Check;
use vts_extract::a11y::observe::{observe, Report};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-a11y-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn file_check(path: &std::path::Path) -> String {
    serde_json::json!({"check": "file_exists", "path": path}).to_string()
}

#[test]
fn file_exists_is_observed_without_any_gui_backend() {
    let dir = scratch("file-exists");
    let present = dir.join("out.txt");
    fs::write(&present, "x").unwrap();
    let checks = Check::parse_many(&format!(
        "[{},{}]",
        file_check(&present),
        file_check(&dir.join("missing.txt"))
    ))
    .unwrap();

    let report = observe(&checks, false).unwrap();

    assert!(report.checks[0].passed);
    assert!(!report.checks[1].passed);
    assert!(!report.passed);
    assert_eq!(report.checks[0].substrate, "filesystem");
}

fn observe_cli(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_vts-extract"))
        .arg("observe")
        .args(args)
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn observe_exits_zero_on_pass_one_on_fail_and_two_when_it_cannot_observe() {
    let dir = scratch("cli");
    let present = dir.join("out.txt");
    fs::write(&present, "x").unwrap();

    let (code, stdout, _) = observe_cli(&["--check", &file_check(&present)]);
    assert_eq!(code, 0, "{stdout}");
    let report: Report = serde_json::from_str(&stdout).expect("stdout is the JSON report");
    assert!(report.passed);
    assert_eq!(report.checks.len(), 1);

    let (code, stdout, _) = observe_cli(&[
        "--check",
        &file_check(&present),
        "--check",
        &file_check(&dir.join("missing.txt")),
    ]);
    assert_eq!(code, 1, "{stdout}");
    let report: Report = serde_json::from_str(&stdout).unwrap();
    assert!(!report.passed);
    assert_eq!(report.checks.len(), 2, "repeated --check flags accumulate");

    let (code, stdout, stderr) = observe_cli(&["--check", "{\"check\":\"teleport\"}"]);
    assert_eq!(
        code, 2,
        "an unparseable check is 'could not observe', not a failure"
    );
    assert!(
        stdout.trim().is_empty(),
        "no report is printed when nothing was observed"
    );
    assert!(stderr.contains("teleport"), "{stderr}");
}

#[cfg(target_os = "macos")]
#[test]
fn gui_checks_either_observe_or_explain_the_missing_permission() {
    // Most shells (and every CI runner) are not trusted Accessibility
    // clients. The refusal must say so, so the verifier records
    // `unverifiable` with the reason instead of a false `fail`.
    let checks =
        Check::parse_many(r#"{"check":"window_title_contains","app":"Finder","text":"x"}"#)
            .unwrap();
    match observe(&checks, false) {
        Ok(report) => assert_eq!(report.checks[0].substrate, "macOS AXUIElement (read-only)"),
        Err(error) => {
            let text = format!("{error:#}");
            assert!(text.contains("Accessibility"), "{text}");
            assert!(text.contains("System Settings"), "{text}");
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs Accessibility permission for the shell running cargo; see the module docs"]
fn textedit_window_title_and_menu_are_observable_live() {
    let dir = scratch("live");
    let document = dir.join("vts-a11y-probe.txt");
    fs::write(&document, "observed\n").unwrap();
    let was_running = Command::new("pgrep")
        .args(["-x", "TextEdit"])
        .output()
        .unwrap()
        .status
        .success();
    assert!(Command::new("open")
        .arg("-a")
        .arg("TextEdit")
        .arg(&document)
        .status()
        .unwrap()
        .success());

    let checks = Check::parse_many(
        r#"[
            {"check":"window_title_contains","app":"TextEdit","text":"vts-a11y-probe"},
            {"check":"element_exists","app":"TextEdit","role":"window","title":"vts-a11y-probe.txt"},
            {"check":"menu_item_enabled","app":"TextEdit","menu":["File","Close"]}
        ]"#,
    )
    .unwrap();
    // The window takes a moment to appear; poll rather than sleep blindly.
    let mut last = None;
    for _ in 0..40 {
        let report = observe(&checks, true).unwrap();
        if report.passed {
            last = Some(report);
            break;
        }
        last = Some(report);
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let report = last.unwrap();

    if !was_running {
        Command::new("pkill").args(["-x", "TextEdit"]).status().ok();
    }
    assert!(
        report.passed,
        "{}",
        serde_json::to_string_pretty(&report.checks).unwrap()
    );
    assert_eq!(report.snapshots[0].app, "TextEdit");
    assert!(report.snapshots[0].pid.is_some());
}
