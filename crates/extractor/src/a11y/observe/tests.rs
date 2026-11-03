use std::cell::Cell;

use anyhow::bail;

use super::{observe_with, FILESYSTEM_SUBSTRATE};
use crate::a11y::check::Check;
use crate::a11y::tree::{Node, Snapshot};

fn snapshot(app: &str, window: &str) -> Snapshot {
    Snapshot {
        app: app.into(),
        pid: None,
        truncated: false,
        root: Node::new("AXApplication")
            .with_title(app)
            .with_child(Node::new("AXWindow").with_title(window)),
    }
}

#[test]
fn one_snapshot_per_application_and_results_in_request_order() {
    let reads = Cell::new(0);
    let checks = Check::parse_many(
        r#"[
            {"check":"window_title_contains","app":"TextEdit","text":"Untitled"},
            {"check":"window_title_contains","app":"TextEdit","text":"Nope"},
            {"check":"window_title_contains","app":"Finder","text":"Desktop"},
            {"check":"window_title_contains","text":"Untitled"}
        ]"#,
    )
    .unwrap();

    let report = observe_with(&checks, false, |app| {
        reads.set(reads.get() + 1);
        Ok(match app {
            Some("TextEdit") => snapshot("TextEdit", "Untitled 3"),
            Some("Finder") => snapshot("Finder", "Desktop"),
            Some(other) => bail!("unexpected app {other}"),
            None => snapshot("Focused", "Untitled"),
        })
    })
    .unwrap();

    assert_eq!(reads.get(), 3, "TextEdit is read once for its two checks");
    let passed: Vec<bool> = report.checks.iter().map(|c| c.passed).collect();
    assert_eq!(passed, vec![true, false, true, true]);
    assert!(
        !report.passed,
        "one failed check fails the whole observation"
    );
    assert!(
        report.snapshots.is_empty(),
        "snapshots are only dumped on request"
    );
    assert_eq!(report.checks[0].check, checks[0]);
    assert_ne!(report.checks[0].substrate, "");
    assert_ne!(report.checks[0].not_proved, "");
}

#[test]
fn filesystem_checks_never_touch_the_gui_backend() {
    let dir = std::env::temp_dir().join(format!("vts-a11y-observe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let present = dir.join("present.txt");
    std::fs::write(&present, "x").unwrap();
    let checks = Check::parse_many(
        &serde_json::json!([
            {"check":"file_exists","path":present},
            {"check":"file_exists","path":dir.join("missing.txt")},
        ])
        .to_string(),
    )
    .unwrap();

    let report = observe_with(&checks, true, |_| bail!("no GUI backend here")).unwrap();

    assert!(report.checks[0].passed);
    assert!(!report.checks[1].passed);
    assert_eq!(report.checks[0].substrate, FILESYSTEM_SUBSTRATE);
    assert!(
        report.snapshots.is_empty(),
        "nothing was read, nothing to dump"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_backend_failure_names_the_check_and_application() {
    let checks = Check::parse_many(
        r#"{"check":"element_exists","app":"Mail","role":"AXButton","title":"Send"}"#,
    )
    .unwrap();
    let error = observe_with(&checks, false, |_| {
        bail!("Accessibility permission missing")
    })
    .unwrap_err();
    let text = format!("{error:#}");
    assert!(text.contains("element_exists"), "{text}");
    assert!(text.contains("Mail"), "{text}");
    assert!(text.contains("Accessibility permission missing"), "{text}");
}

#[test]
fn dump_includes_every_snapshot_that_was_read() {
    let checks = Check::parse_many(
        r#"[{"check":"window_title_contains","app":"TextEdit","text":"a"},{"check":"window_title_contains","text":"b"}]"#,
    )
    .unwrap();
    let report = observe_with(&checks, true, |app| {
        Ok(snapshot(app.unwrap_or("Focused"), "a b"))
    })
    .unwrap();
    assert_eq!(report.snapshots.len(), 2);
    assert_eq!(report.snapshots[0].app, "TextEdit");
    assert_eq!(report.snapshots[1].app, "Focused");
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["snapshots"][0]["root"]["children"][0]["title"], "a b");
}
