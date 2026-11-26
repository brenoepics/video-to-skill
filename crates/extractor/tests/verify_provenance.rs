//! `vts-extract verify` records the verifier's per-step verdict in the
//! package's provenance.json: outcome, detail and cited repair signals,
//! an execution count, and unsolved failures as failure notes.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use vts_extract::compile::{compile, Evidence, ProcedureIr, SourceRef, Step, SCHEMA_VERSION};
use vts_extract::verify::{record_verification, StepOutcome, VerificationReport};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-verify-provenance-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn step(id: &str, usage_count: u32) -> Step {
    Step {
        id: id.into(),
        goal: format!("do {id}"),
        actions: "Type `:q`.".into(),
        success_criteria: "Back at the shell prompt.".into(),
        confidence: "high".into(),
        caveats: None,
        evidence: vec![Evidence {
            timestamp_secs: 10.0,
            frame: None,
            quote: Some("quit".into()),
            source: None,
        }],
        scripts: vec![],
        variants: vec![],
        preconditions: vec![],
        parameters: vec![],
        postcondition: None,
        checks: vec![],
        failure_notes: vec![],
        usage_count,
        verdict: None,
    }
}

fn compiled_package(dir: &Path) -> PathBuf {
    let ir = ProcedureIr {
        schema_version: SCHEMA_VERSION,
        skill_name: "demo-skill".into(),
        title: "Demo".into(),
        description: "Use for testing.".into(),
        license: None,
        compatibility: None,
        overview: "Test package.".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: "file.mp4".into(),
            title: None,
            duration_secs: 60.0,
            sha256: None,
        },
        steps: vec![
            step("01-first", 2),
            step("02-second", 0),
            step("03-third", 0),
        ],
        artifacts: vec![],
        gaps: vec![],
        history: vec![],
        task_graph: vts_extract::compile::TaskGraph::default(),
    };
    let bundle = dir.join("bundle");
    fs::create_dir_all(&bundle).unwrap();
    let out = dir.join("demo-skill");
    compile(&ir, &bundle, &out).unwrap();
    out
}

fn outcome(id: &str, outcome: &str, detail: &str) -> StepOutcome {
    StepOutcome {
        id: id.into(),
        outcome: outcome.into(),
        detail: detail.into(),
        repairs: vec![],
        substrate: None,
        observations: vec![],
    }
}

fn report(second: &str, detail: &str) -> VerificationReport {
    VerificationReport {
        schema_version: 1,
        verified: false,
        attempts: 1,
        summary: "mixed".into(),
        steps: vec![
            outcome("01-first", "pass", "exit 0"),
            outcome("02-second", second, detail),
            outcome("03-third", "skipped", "modifies the system"),
        ],
    }
}

fn provenance(package: &Path) -> ProcedureIr {
    serde_json::from_str(&fs::read_to_string(package.join("provenance.json")).unwrap()).unwrap()
}

#[test]
fn recording_writes_each_steps_verdict_usage_and_unsolved_failures() {
    let dir = scratch("verdicts");
    let package = compiled_package(&dir);

    record_verification(&package, &report("fail", "file notes.txt missing")).unwrap();

    let ir = provenance(&package);
    assert_eq!(ir.schema_version, SCHEMA_VERSION);
    let first = &ir.steps[0];
    assert_eq!(first.verdict.as_ref().unwrap().outcome, "pass");
    assert_eq!(first.verdict.as_ref().unwrap().detail, "exit 0");
    assert_eq!(first.usage_count, 3, "an executed step counts one more run");
    assert_eq!(first.failure_notes, Vec::<String>::new());

    let second = &ir.steps[1];
    assert_eq!(second.verdict.as_ref().unwrap().outcome, "fail");
    assert_eq!(second.usage_count, 1, "a failed run is still a run");
    assert_eq!(
        second.failure_notes,
        vec!["Unsolved: file notes.txt missing".to_string()]
    );

    let third = &ir.steps[2];
    assert_eq!(third.verdict.as_ref().unwrap().outcome, "skipped");
    assert_eq!(third.usage_count, 0, "a skipped step was not executed");
}

#[test]
fn repeated_recordings_accumulate_usage_without_duplicating_notes() {
    let dir = scratch("accumulate");
    let package = compiled_package(&dir);

    record_verification(&package, &report("fail", "file notes.txt missing")).unwrap();
    record_verification(&package, &report("fail", "file notes.txt missing")).unwrap();
    record_verification(&package, &report("pass", "exit 0 after fix")).unwrap();

    let ir = provenance(&package);
    assert_eq!(ir.steps[0].usage_count, 5);
    let second = &ir.steps[1];
    assert_eq!(second.usage_count, 3);
    assert_eq!(
        second.failure_notes,
        vec!["Unsolved: file notes.txt missing".to_string()],
        "the same failure is noted once"
    );
    assert_eq!(
        second.verdict.as_ref().unwrap().outcome,
        "pass",
        "the verdict is the latest one"
    );
}

#[test]
fn the_verdict_carries_the_repair_signals_the_step_cites() {
    let dir = scratch("signals");
    let package = compiled_package(&dir);
    // Hand the package a repaired step, as a recompile after a repair would.
    let mut ir = provenance(&package);
    ir.steps[1].caveats = Some("Repair 1: added Enter. Signal: exit code 1".into());
    fs::write(
        package.join("provenance.json"),
        serde_json::to_string_pretty(&ir).unwrap(),
    )
    .unwrap();
    let mut report = report("pass", "exit 0 after the repair");
    report.attempts = 2;

    record_verification(&package, &report).unwrap();

    let verdict = provenance(&package).steps[1].verdict.clone().unwrap();
    assert_eq!(verdict.repairs, vec!["exit code 1".to_string()]);
}
