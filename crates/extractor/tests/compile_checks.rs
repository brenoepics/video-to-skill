//! Structured per-step checks: the read-only `observe` vocabulary written
//! into the Procedure IR, validated with `Check::parse`'s rules, held to
//! the `{workspace}` placeholder rule, and rendered as "Checkable:" lines
//! under the step's success criteria.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use vts_extract::compile::{compile, ProcedureIr};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-compile-checks-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(dir.join("bundle")).unwrap();
    dir
}

/// A one-step IR whose step carries `checks` (left out entirely when
/// `checks` is JSON null, as every IR written before the field did).
fn ir(checks: &Value) -> ProcedureIr {
    let mut step = json!({
        "id": "01-save", "goal": "Save the document",
        "actions": "Press Cmd-S and name the file `notes.txt`.",
        "success_criteria": "The window title reads notes.txt.",
        "confidence": "high", "caveats": null,
        "evidence": [{"timestamp_secs": 12.0, "frame": null, "quote": "save it"}]
    });
    if !checks.is_null() {
        step["checks"] = checks.clone();
    }
    serde_json::from_value(json!({
        "schema_version": 2, "skill_name": "textedit-save", "title": "Save in TextEdit",
        "description": "Use when saving a TextEdit document.", "overview": "Saving.",
        "genre": "screencast",
        "source": {"original": "video.mp4", "title": null, "duration_secs": 60.0},
        "steps": [step]
    }))
    .unwrap()
}

fn compiled(dir: &Path, ir: &ProcedureIr) -> anyhow::Result<PathBuf> {
    let out = dir.join(&ir.skill_name);
    compile(ir, &dir.join("bundle"), &out).map(|()| out)
}

fn refusal(dir: &Path, check: &Value) -> String {
    let err = compiled(dir, &ir(&json!([check]))).unwrap_err().to_string();
    assert!(err.contains("step '01-save'"), "names the step: {err}");
    err
}

#[test]
fn checks_render_as_checkable_lines_under_the_success_criteria() {
    let dir = scratch("render");
    let checks = json!([
        {"check": "window_title_contains", "app": "TextEdit", "text": "notes.txt"},
        {"check": "file_exists", "path": "{workspace}/notes.txt"},
        {"check": "menu_item_enabled", "menu": ["File", "Save"], "enabled": false}
    ]);

    let out = compiled(&dir, &ir(&checks)).unwrap();

    let step = fs::read_to_string(out.join("steps/01-save.md")).unwrap();
    let expected = "## Success criteria\n\nThe window title reads notes.txt.\n\n\
        - Checkable: the application \"TextEdit\" has a window whose title contains \
        \"notes.txt\" — `{\"check\":\"window_title_contains\",\"app\":\"TextEdit\",\
        \"text\":\"notes.txt\"}`\n\
        - Checkable: a file exists at \"{workspace}/notes.txt\" — \
        `{\"check\":\"file_exists\",\"path\":\"{workspace}/notes.txt\"}`\n\
        - Checkable: the menu item \"File\" > \"Save\" in the frontmost application is \
        disabled — `{\"check\":\"menu_item_enabled\",\"menu\":[\"File\",\"Save\"],\
        \"enabled\":false}`\n\n";
    assert!(step.contains(expected), "missing {expected:?} in:\n{step}");

    let provenance: Value =
        serde_json::from_str(&fs::read_to_string(out.join("provenance.json")).unwrap()).unwrap();
    assert_eq!(provenance["steps"][0]["checks"], checks);
}

#[test]
fn every_check_kind_has_a_sentence_a_human_can_read() {
    let dir = scratch("sentences");
    let checks = json!([
        {"check": "element_exists", "app": "TextEdit", "role": "button", "title": "Save"},
        {"check": "element_value_equals", "role": "AXTextArea", "identifier": "body",
         "value": "hello"},
        {"check": "element_value_equals", "role": "slider", "title": "Zoom", "value": 2.0},
        {"check": "checkbox_state", "title": "Wrap lines", "identifier": "wrap",
         "checked": true}
    ]);

    let out = compiled(&dir, &ir(&checks)).unwrap();

    let step = fs::read_to_string(out.join("steps/01-save.md")).unwrap();
    for wanted in [
        "- Checkable: an element with role \"button\" and title \"Save\" exists in the \
         application \"TextEdit\" — `",
        "- Checkable: the element with role \"AXTextArea\" and identifier \"body\" in the \
         frontmost application has the value \"hello\" — `",
        "- Checkable: the element with role \"slider\" and title \"Zoom\" in the frontmost \
         application has the value 2 — `",
        "- Checkable: the checkbox with title \"Wrap lines\" and identifier \"wrap\" in the \
         frontmost application is checked — `",
    ] {
        assert!(step.contains(wanted), "missing {wanted:?} in:\n{step}");
    }
}

#[test]
fn a_step_without_checks_still_loads_and_renders_no_checkable_line() {
    let dir = scratch("absent");

    let ir = ir(&Value::Null);
    assert_eq!(ir.steps[0].checks, []);
    let out = compiled(&dir, &ir).unwrap();

    let step = fs::read_to_string(out.join("steps/01-save.md")).unwrap();
    assert!(!step.contains("Checkable"), "{step}");
    let provenance: Value =
        serde_json::from_str(&fs::read_to_string(out.join("provenance.json")).unwrap()).unwrap();
    assert_eq!(provenance["steps"][0]["checks"], json!([]));
}

#[test]
fn checks_the_observe_parser_would_refuse_are_refused_naming_the_step() {
    let dir = scratch("invalid");
    for (check, rule) in [
        (
            json!({"check": "element_exists", "role": "button"}),
            "a title or an identifier",
        ),
        (
            json!({"check": "element_exists", "role": " ", "title": "Save"}),
            "role must not be empty",
        ),
        (
            json!({"check": "checkbox_state", "title": "", "checked": true}),
            "a title or an identifier",
        ),
        (
            json!({"check": "window_title_contains", "text": "  "}),
            "text must not be empty",
        ),
        (
            json!({"check": "menu_item_enabled", "menu": []}),
            "at least one entry",
        ),
        (
            json!({"check": "menu_item_enabled", "menu": ["File", " "]}),
            "entries must not be empty",
        ),
        (
            json!({"check": "file_exists", "path": ""}),
            "path must not be empty",
        ),
    ] {
        let err = refusal(&dir, &check);
        assert!(err.contains(rule), "{check}: wanted {rule:?}, got: {err}");
        assert!(err.contains("check 1"), "{check}: names the check: {err}");
    }
}

#[test]
fn unknown_check_kinds_and_fields_do_not_load() {
    for check in [
        json!({"check": "teleport", "text": "x"}),
        json!({"check": "window_title_contains", "text": "x", "click": true}),
    ] {
        let step = json!({
            "id": "01-save", "goal": "g", "actions": "a", "success_criteria": "s",
            "confidence": "high", "caveats": null,
            "evidence": [{"timestamp_secs": 1.0, "frame": null, "quote": null}],
            "checks": [check]
        });
        let parsed = serde_json::from_value::<vts_extract::compile::Step>(step);
        assert!(parsed.is_err(), "{check} must not load");
    }
}

#[test]
fn file_exists_paths_are_rooted_in_the_workspace() {
    let dir = scratch("workspace");
    let ok = json!([{"check": "file_exists", "path": "{workspace}/out/notes.txt"}]);
    compiled(&dir, &ir(&ok)).unwrap();

    for (path, rule) in [
        ("/Users/creator/notes.txt", "must start with '{workspace}/'"),
        ("notes.txt", "must start with '{workspace}/'"),
        (
            "out/{workspace}/notes.txt",
            "must start with '{workspace}/'",
        ),
        ("{workspace}", "must name a file inside the workspace"),
        ("{workspace}/", "must name a file inside the workspace"),
        ("{workspace}/../notes.txt", "'..'"),
        ("{workspace}/out//notes.txt", "empty"),
        ("{workspace}/out\\notes.txt", "'/' separators"),
        ("{workspace}/{file-name}", "only placeholder"),
        ("{workspace}/{workspace}/x", "only placeholder"),
    ] {
        let err = refusal(&dir, &json!({"check": "file_exists", "path": path}));
        assert!(err.contains(rule), "{path}: wanted {rule:?}, got: {err}");
    }
}

#[test]
fn placeholders_are_not_substituted_outside_file_exists_paths() {
    let dir = scratch("literal");
    for check in [
        json!({"check": "window_title_contains", "text": "{workspace}/notes.txt"}),
        json!({"check": "element_exists", "role": "button", "title": "{file-name}"}),
        json!({"check": "menu_item_enabled", "menu": ["Open Recent", "{file-name}"]}),
    ] {
        let err = refusal(&dir, &check);
        assert!(
            err.contains("literal") && err.contains("{workspace}"),
            "{check}: {err}"
        );
    }
    // Braces that are not placeholders are text like any other.
    let braces = json!([{"check": "window_title_contains", "text": "{draft}"}]);
    compiled(&dir, &ir(&braces)).unwrap();
}
