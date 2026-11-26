#![cfg(target_os = "linux")]

//! The Linux AT-SPI backend end to end.
//!
//! Without a desktop session every GUI check ends in "could not observe"
//! with the reason (exit 2, never a false fail); a session bus without an
//! accessibility bus is named as such, and observing never starts one.
//!
//! The live test builds a private desktop — its own session bus, an Xvfb
//! display and a GTK 3 probe window — and observes the window title, a
//! checkbox, an entry's text, a button and an open and a disabled menu
//! item. It needs `dbus-daemon`, `Xvfb`, `at-spi2-core` and `PyGObject`
//! with GTK 3 (`python3-gi`, `gir1.2-gtk-3.0`); without them it is
//! skipped with the reason, unless `VTS_REQUIRE_ATSPI` is set, as the
//! Linux CI job does after installing them.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use vts_extract::a11y::atspi::SUBSTRATE;
use vts_extract::a11y::observe::Report;

#[path = "a11y_linux/desktop.rs"]
mod desktop;

use desktop::{scratch, Desktop};

fn title_check(app: &str, text: &str) -> String {
    serde_json::json!({"check": "window_title_contains", "app": app, "text": text}).to_string()
}

#[test]
fn without_any_session_bus_a_gui_check_is_unobservable_with_the_reason() {
    let dir = scratch("no-session");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_vts-extract"))
        .args(["observe", "--check", &title_check("gedit", "x")])
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env_remove("AT_SPI_BUS_ADDRESS")
        .env("XDG_RUNTIME_DIR", dir.join("runtime"))
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        output.stdout.is_empty(),
        "no report when nothing was observed"
    );
    assert!(
        stderr.contains("window_title_contains: could not observe gedit"),
        "{stderr}"
    );
    assert!(stderr.contains("no D-Bus session bus"), "{stderr}");
}

#[test]
fn an_accessibility_bus_address_that_does_not_answer_is_named() {
    let dir = scratch("dead-a11y-bus");
    let socket = dir.join("runtime").join("no-such-bus");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_vts-extract"))
        .args(["observe", "--check", &title_check("gedit", "x")])
        .env(
            "AT_SPI_BUS_ADDRESS",
            format!("unix:path={}", socket.display()),
        )
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("accessibility bus"), "{stderr}");
    assert!(stderr.contains(&socket.display().to_string()), "{stderr}");
}

#[test]
fn a_session_without_an_accessibility_bus_is_named_and_none_is_started() {
    let Some(desktop) = Desktop::bus_only("no-a11y-bus") else {
        return;
    };

    let (code, stdout, stderr) = desktop.observe(&["--check", &title_check("gedit", "x")]);

    assert_eq!(code, 2, "{stdout}{stderr}");
    assert!(
        stderr.contains("no accessibility bus is running"),
        "{stderr}"
    );
    // NO_AUTO_START: asking must not have activated the bus launcher,
    // which would have made its socket directory.
    assert!(
        !desktop.dir.join("runtime").join("at-spi").exists(),
        "observe started the accessibility bus"
    );
}

/// Observe until it exits 0 or ten seconds pass (an application
/// registers with the accessibility bus asynchronously after it shows).
fn poll(desktop: &Desktop, args: &[&str]) -> (i32, String, String) {
    let mut last = desktop.observe(args);
    for _ in 0..40 {
        if last.0 == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
        last = desktop.observe(args);
    }
    last
}

#[test]
fn a_gtk_window_checkbox_entry_button_and_menu_are_observable_live() {
    let Some(mut desktop) = Desktop::with_display("live") else {
        return;
    };
    let app = "vts-atspi-probe";
    let pid = desktop.launch_probe(app, "vts-atspi-probe window");
    let checks = serde_json::json!([
        {"check": "window_title_contains", "app": app, "text": "probe window"},
        {"check": "element_exists", "app": app, "role": "AXButton", "title": "Save copy"},
        {"check": "checkbox_state", "app": app, "title": "Wrap lines", "checked": true},
        {"check": "element_value_equals", "app": app, "role": "textfield",
         "title": "Greeting", "value": "hello"},
        {"check": "menu_item_enabled", "app": app, "menu": ["File", "Open"]},
        {"check": "menu_item_enabled", "app": app, "menu": ["File", "Save"], "enabled": false}
    ])
    .to_string();

    let (code, stdout, stderr) = poll(&desktop, &["--dump", "--check", &checks]);

    assert_eq!(code, 0, "{stdout}{stderr}");
    let report: Report = serde_json::from_str(&stdout).unwrap();
    assert!(report
        .checks
        .iter()
        .all(|check| check.substrate == SUBSTRATE));
    assert_eq!(report.snapshots.len(), 1, "one read per application");
    let snapshot = &report.snapshots[0];
    assert_eq!(snapshot.app, app);
    assert_eq!(snapshot.pid, Some(i32::try_from(pid).unwrap()));
    assert!(!snapshot.truncated);
    assert_eq!(snapshot.root.role, "Application");

    // A check that does not hold is a fail (exit 1) quoting what was seen.
    let unchecked =
        serde_json::json!({"check": "checkbox_state", "app": app, "title": "Wrap lines",
                           "checked": false})
        .to_string();
    let (code, stdout, _) = desktop.observe(&["--check", &unchecked]);
    assert_eq!(code, 1, "{stdout}");
    assert!(stdout.contains("is checked"), "{stdout}");

    // An application that is not registered is unobservable, by name.
    let (code, _, stderr) = desktop.observe(&["--check", &title_check("vts-absent", "x")]);
    assert_eq!(code, 2, "{stderr}");
    assert!(
        stderr.contains("no application named \"vts-absent\""),
        "{stderr}"
    );
    assert!(stderr.contains(app), "lists what is registered: {stderr}");

    // Xvfb runs no window manager, so whether a window counts as active
    // is up to the toolkit: the frontmost read either finds the probe or
    // says that no window is active.
    let frontmost = serde_json::json!({"check": "window_title_contains", "text": "probe"});
    let (code, stdout, stderr) = desktop.observe(&["--check", &frontmost.to_string()]);
    match code {
        0 => assert!(stdout.contains(app), "{stdout}"),
        2 => assert!(stderr.contains("active window"), "{stderr}"),
        other => panic!("exit {other}: {stdout}{stderr}"),
    }
}
