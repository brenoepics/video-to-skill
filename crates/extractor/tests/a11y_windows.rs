#![cfg(windows)]

//! The Windows UI Automation backend end to end.
//!
//! The default tests run on any Windows machine, CI included: an
//! application that is not running and a process without windows both
//! end in "could not observe" with the reason (exit 2, never a false
//! fail), a submenu path is refused before anything is read, and
//! Explorer is either read into a well-formed tree or the reason is
//! named.
//!
//! The ignored test starts Notepad on a scratch file, observes its window
//! title and menu bar, and closes the Notepad it started. Windows asks
//! for no permission, but the test needs an interactive desktop and opens
//! a window on it, which is why it is not part of the default suite:
//!
//! ```text
//! cargo test --test a11y_windows -- --ignored
//! ```

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use vts_extract::a11y::check::Check;
use vts_extract::a11y::observe::{observe, Report};
use vts_extract::a11y::windows::{pids_named, SUBSTRATE};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-a11y-windows-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
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

/// TEMPORARY CI probe: raw stderr is not captured by libtest.
fn probe(text: &str) {
    use std::io::Write as _;
    let _ = writeln!(std::io::stderr().lock(), "VTS-PROBE {text}");
}

fn summary(report: &Report) -> String {
    let mut lines = Vec::new();
    for check in &report.checks {
        lines.push(format!(
            "check {} passed={} observed={}",
            check.check.kind(),
            check.passed,
            check.observed
        ));
    }
    for snapshot in &report.snapshots {
        lines.push(format!(
            "snapshot app={} pid={:?} truncated={} nodes={}",
            snapshot.app,
            snapshot.pid,
            snapshot.truncated,
            snapshot.root.descendants().len()
        ));
        for window in &snapshot.root.children {
            lines.push(format!(
                "  top {} children={}",
                window.describe(),
                window.children.len()
            ));
        }
        let mut roles: Vec<&str> = snapshot
            .root
            .descendants()
            .iter()
            .map(|n| n.role.as_str())
            .collect();
        roles.sort_unstable();
        roles.dedup();
        lines.push(format!("  roles {roles:?}"));
        for node in snapshot
            .root
            .descendants()
            .iter()
            .filter(|n| n.value.is_some() || n.has_role("menuitem"))
            .take(25)
        {
            lines.push(format!(
                "  node {}",
                node.describe().chars().take(160).collect::<String>()
            ));
        }
    }
    lines.join("\n")
}

fn absent_app() -> String {
    format!("vts-no-such-app-{}", std::process::id())
}

#[test]
fn an_application_that_is_not_running_exits_two_with_the_reason() {
    let app = absent_app();
    let check = serde_json::json!({"check": "window_title_contains", "app": app, "text": "x"});

    let (code, stdout, stderr) = observe_cli(&["--check", &check.to_string()]);

    assert_eq!(code, 2, "{stderr}");
    assert!(
        stdout.trim().is_empty(),
        "no report when nothing was observed: {stdout}"
    );
    assert!(stderr.contains("no running process named"), "{stderr}");
    assert!(stderr.contains(&app), "{stderr}");
}

#[test]
fn a_process_without_windows_is_unobservable_not_failed() {
    // `System` (pid 4) runs on every Windows machine and owns no window,
    // so this walks COM, the client, the cache request and the desktop's
    // top-level windows for real, then refuses with the reason.
    let checks =
        Check::parse_many(r#"{"check":"window_title_contains","app":"System","text":"anything"}"#)
            .unwrap();

    let error = observe(&checks, false).unwrap_err();

    let text = format!("{error:#}");
    assert!(
        text.contains("window_title_contains: could not observe System"),
        "{text}"
    );
    assert!(
        text.contains("no top-level window UI Automation can see"),
        "{text}"
    );
}

#[test]
fn a_submenu_path_is_refused_before_anything_is_read() {
    // The application does not even exist: the refusal must come first.
    let check = serde_json::json!({
        "check": "menu_item_enabled", "app": absent_app(), "menu": ["File", "Save"]
    });

    let (code, stdout, stderr) = observe_cli(&["--check", &check.to_string()]);

    assert_eq!(code, 2, "{stderr}");
    assert!(stdout.trim().is_empty(), "{stdout}");
    assert!(stderr.contains("not observable on Windows"), "{stderr}");
    assert!(!stderr.contains("no running process"), "{stderr}");
}

#[test]
fn explorer_is_read_into_a_well_formed_tree_or_the_reason_is_named() {
    let checks = Check::parse_many(
        r#"{"check":"window_title_contains","app":"explorer","text":"vts-never-a-title"}"#,
    )
    .unwrap();

    let result = observe(&checks, true);
    match &result {
        Ok(report) => probe(&format!("explorer read\n{}", summary(report))),
        Err(error) => probe(&format!("explorer not read: {error:#}")),
    }
    match result {
        Ok(report) => {
            let check = &report.checks[0];
            assert_eq!(check.substrate, SUBSTRATE);
            assert!(
                !check.passed,
                "no window has that title: {}",
                check.observed
            );
            let snapshot = &report.snapshots[0];
            assert_eq!(snapshot.app, "explorer");
            assert_eq!(snapshot.root.role, "Application");
            assert_eq!(snapshot.root.title.as_deref(), Some("explorer"));
            assert!(!snapshot.root.children.is_empty(), "{snapshot:?}");
            let unknown: Vec<&str> = snapshot
                .root
                .descendants()
                .into_iter()
                .map(|node| node.role.as_str())
                .filter(|role| role.starts_with("ControlType") || role.is_empty())
                .collect();
            assert!(unknown.is_empty(), "unmapped control types: {unknown:?}");
        }
        // A session without a shell (some service accounts) has no
        // Explorer, or one without windows: both must say so.
        Err(error) => {
            let text = format!("{error:#}");
            assert!(
                text.contains("no running process named")
                    || text.contains("no top-level window UI Automation can see"),
                "{text}"
            );
        }
    }
}

/// Observe until every check holds or ten seconds of polling pass, and
/// return the last result.
fn poll(checks: &[Check]) -> anyhow::Result<Report> {
    let mut last = observe(checks, true);
    for _ in 0..40 {
        if matches!(&last, Ok(report) if report.passed) {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
        last = observe(checks, true);
    }
    last
}

#[test]
fn notepad_window_title_and_menu_bar_are_observable_live() {
    let dir = scratch("live");
    let document = dir.join("vts-a11y-probe.txt");
    fs::write(&document, "observed\r\n").unwrap();
    let before = pids_named("notepad").unwrap();
    let mut launcher = Command::new("notepad.exe").arg(&document).spawn().unwrap();

    let checks = Check::parse_many(
        r#"[
            {"check":"window_title_contains","app":"notepad","text":"vts-a11y-probe"},
            {"check":"element_exists","app":"notepad","role":"menuitem","title":"File"},
            {"check":"menu_item_enabled","app":"notepad","menu":["File"]}
        ]"#,
    )
    .unwrap();
    let observed = poll(&checks);
    let submenu = Check::parse_many(
        r#"{"check":"menu_item_enabled","app":"notepad","menu":["File","Save"]}"#,
    )
    .unwrap();
    let refused = observe(&submenu, false);

    // Close only what this test started: the launcher, and any Notepad
    // process that did not exist before (on Windows 11 the launcher hands
    // the file to a packaged Notepad and exits). A Notepad that was
    // already running keeps its windows.
    launcher.kill().ok();
    launcher.wait().ok();
    for pid in pids_named("notepad").unwrap() {
        if !before.contains(&pid) {
            Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output()
                .ok();
        }
    }

    match &observed {
        Ok(report) => probe(&format!("notepad before={before:?}\n{}", summary(report))),
        Err(error) => probe(&format!("notepad not read: {error:#}")),
    }
    let report = observed.unwrap();
    assert!(
        report.passed,
        "{}",
        serde_json::to_string_pretty(&report.checks).unwrap()
    );
    assert_eq!(report.snapshots[0].app, "notepad");
    assert!(report
        .checks
        .iter()
        .all(|check| check.substrate == SUBSTRATE));
    let refusal = format!("{:#}", refused.unwrap_err());
    assert!(refusal.contains("not observable on Windows"), "{refusal}");
}
