//! Step outcomes carry what `vts-extract observe` reported — the
//! substrate and one record per check — so a GUI-observed pass is
//! reproducible from verification.json alone. A pass may only cite
//! observations that held.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use vts_extract::compile::{compile, ProcedureIr};
use vts_extract::verify::{record_verification, VerificationReport, VERIFICATION_FILE};

const SUBSTRATE: &str = "macOS AXUIElement (read-only)";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-verify-observations-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(dir.join("bundle")).unwrap();
    dir
}

fn package(dir: &Path) -> PathBuf {
    let ir: ProcedureIr = serde_json::from_value(json!({
        "schema_version": 2, "skill_name": "textedit-save", "title": "Save in TextEdit",
        "description": "Use when saving a TextEdit document.", "overview": "Saving.",
        "genre": "screencast",
        "source": {"original": "video.mp4", "title": null, "duration_secs": 60.0},
        "steps": [{
            "id": "01-save", "goal": "Save the document", "actions": "Press Cmd-S.",
            "success_criteria": "The window title reads notes.txt.",
            "confidence": "high", "caveats": null,
            "evidence": [{"timestamp_secs": 12.0, "frame": null, "quote": "save it"}],
            "checks": [{"check": "window_title_contains", "app": "TextEdit", "text": "notes.txt"}]
        }]
    }))
    .unwrap();
    let out = dir.join(&ir.skill_name);
    compile(&ir, &dir.join("bundle"), &out).unwrap();
    out
}

/// One per-check record exactly as `observe` prints it.
fn observation(passed: bool) -> Value {
    json!({
        "check": {"check": "window_title_contains", "app": "TextEdit", "text": "notes.txt"},
        "substrate": SUBSTRATE,
        "passed": passed,
        "observed": if passed { "TextEdit: window titles [\"notes.txt\"]" }
                    else { "TextEdit: window titles [\"Untitled\"]" },
        "not_proved": "that the step's actions produced the window"
    })
}

fn report(outcome: &str, observations: &[Value]) -> VerificationReport {
    serde_json::from_value(json!({
        "schema_version": 1, "verified": outcome == "pass", "attempts": 1,
        "summary": "GUI-observed",
        "steps": [{
            "id": "01-save", "outcome": outcome,
            "detail": "GUI-observed via macOS AXUIElement",
            "substrate": SUBSTRATE, "observations": observations
        }]
    }))
    .unwrap()
}

fn recorded(package: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(package.join(VERIFICATION_FILE)).unwrap()).unwrap()
}

#[test]
fn a_pass_citing_observations_that_held_is_recorded_with_them() {
    let dir = scratch("held");
    let package = package(&dir);

    record_verification(&package, &report("pass", &[observation(true)])).unwrap();

    let step = &recorded(&package)["steps"][0];
    assert_eq!(step["substrate"], SUBSTRATE);
    assert_eq!(step["observations"], json!([observation(true)]));
}

#[test]
fn a_pass_citing_an_observation_that_did_not_hold_is_refused() {
    let dir = scratch("refused");
    let package = package(&dir);

    let err = record_verification(
        &package,
        &report("pass", &[observation(true), observation(false)]),
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("step '01-save'"), "{err}");
    assert!(
        err.contains("observation 2"),
        "names the observation: {err}"
    );
    assert!(err.contains("window_title_contains"), "{err}");
    assert!(
        err.contains("[\"Untitled\"]"),
        "quotes what was observed: {err}"
    );
    assert!(
        !package.join(VERIFICATION_FILE).exists(),
        "nothing is written for a refused report"
    );
}

#[test]
fn a_fail_may_cite_the_observation_that_did_not_hold() {
    let dir = scratch("fail");
    let package = package(&dir);

    record_verification(&package, &report("fail", &[observation(false)])).unwrap();

    assert_eq!(
        recorded(&package)["steps"][0]["observations"][0]["passed"],
        false
    );
}

#[test]
fn observations_with_fields_this_build_does_not_know_still_load() {
    let dir = scratch("loose");
    let package = package(&dir);
    let mut record = observation(true);
    record["elapsed_ms"] = json!(12);
    let raw = json!({
        "schema_version": 1, "verified": true, "attempts": 1, "summary": "ok",
        "steps": [{
            "id": "01-save", "outcome": "pass", "detail": "observed",
            "observations": [record], "isolation": "host-observed"
        }]
    });

    let report: VerificationReport = serde_json::from_value(raw).unwrap();
    record_verification(&package, &report).unwrap();

    let step = &recorded(&package)["steps"][0];
    assert!(step["substrate"].is_null(), "substrate stays optional");
    assert_eq!(
        step["observations"][0]["observed"],
        observation(true)["observed"]
    );
}

#[test]
fn reports_without_observations_are_written_as_before() {
    let dir = scratch("plain");
    let package = package(&dir);
    let plain: VerificationReport = serde_json::from_value(json!({
        "schema_version": 1, "verified": true, "attempts": 1, "summary": "ok",
        "steps": [{"id": "01-save", "outcome": "pass", "detail": "exit 0"}]
    }))
    .unwrap();

    record_verification(&package, &plain).unwrap();

    let step = recorded(&package)["steps"][0].clone();
    assert_eq!(
        step,
        json!({"id": "01-save", "outcome": "pass", "detail": "exit 0", "repairs": []}),
        "no substrate or observations keys appear when none were given"
    );
}
