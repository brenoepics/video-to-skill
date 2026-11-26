//! Repair discipline: a repair recorded in a step's caveats must cite
//! the external signal that motivated it (exit code, output, file or
//! state check), the compiler refuses one that does not, and
//! `vts-extract verify` carries the cited signals into the report.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use vts_extract::compile::{compile, Evidence, ProcedureIr, SourceRef, Step};
use vts_extract::verify::{record_verification, StepOutcome, VerificationReport};

const SIGNALLED_REPAIR: &str = "Repair 1: `:Q` replaced by `:q`. Signal: exit code 1 — \
                                E492: Not an editor command: Q";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-verify-repairs-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn step(id: &str, caveats: Option<&str>) -> Step {
    Step {
        id: id.into(),
        goal: format!("do {id}"),
        actions: "Type `:q`.".into(),
        success_criteria: "Back at the shell prompt.".into(),
        confidence: "high".into(),
        caveats: caveats.map(str::to_string),
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
        usage_count: 0,
        verdict: None,
    }
}

fn ir(caveats: Option<&str>) -> ProcedureIr {
    ProcedureIr {
        schema_version: 1,
        skill_name: "vim-basics".into(),
        title: "Vim Basics".into(),
        description: "Use when quitting Vim.".into(),
        license: None,
        compatibility: None,
        overview: "Quitting.".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: "video.mp4".into(),
            title: None,
            duration_secs: 60.0,
            sha256: None,
        },
        steps: vec![step("01-quit-vim", caveats), step("02-save", None)],
        artifacts: vec![],
        gaps: vec![],
        history: vec![],
        task_graph: vts_extract::compile::TaskGraph::default(),
    }
}

fn compiled(dir: &Path, caveats: Option<&str>) -> PathBuf {
    let bundle = dir.join("bundle");
    fs::create_dir_all(&bundle).unwrap();
    let out = dir.join("vim-basics");
    compile(&ir(caveats), &bundle, &out).unwrap();
    out
}

fn compile_error(dir: &Path, caveats: &str) -> String {
    let bundle = dir.join("bundle");
    fs::create_dir_all(&bundle).unwrap();
    compile(&ir(Some(caveats)), &bundle, &dir.join("vim-basics"))
        .unwrap_err()
        .to_string()
}

fn report(attempts: u32) -> VerificationReport {
    VerificationReport {
        schema_version: 1,
        verified: true,
        attempts,
        summary: "all steps passed".into(),
        steps: vec![
            StepOutcome {
                id: "01-quit-vim".into(),
                outcome: "pass".into(),
                detail: "exit 0 after the repair".into(),
                repairs: vec![],
                substrate: None,
                observations: vec![],
            },
            StepOutcome {
                id: "02-save".into(),
                outcome: "pass".into(),
                detail: "exit 0".into(),
                repairs: vec![],
                substrate: None,
                observations: vec![],
            },
        ],
    }
}

#[test]
fn a_repair_without_an_external_signal_is_refused_by_the_compiler() {
    let dir = scratch("no-signal");

    let err = compile_error(
        &dir,
        "Repair 1: changed `:Q` to `:q` because it looked wrong.",
    );

    assert!(err.contains("01-quit-vim"), "names the step: {err}");
    assert!(err.contains("Signal:"), "shows the expected form: {err}");
    assert!(err.contains("exit code"), "names the signal kinds: {err}");
    assert!(
        !dir.join("vim-basics").exists(),
        "validation happens before emission"
    );
}

#[test]
fn only_the_external_signal_kinds_are_accepted() {
    let dir = scratch("kinds");
    for signal in [
        "exit code 1 — E492",
        "stderr: \"E492: Not an editor command\"",
        "stdout: nothing printed, criteria expected the prompt",
        "output diff: expected `$`, got `-- INSERT --`",
        "file: notes.txt missing after the step",
        "state: buffer still modified (`:ls` shows +)",
    ] {
        let bundle = dir.join("bundle");
        fs::create_dir_all(&bundle).unwrap();
        let out = dir.join("vim-basics");
        fs::remove_dir_all(&out).ok();
        let caveats = format!("Repair 1: fixed. Signal: {signal}");
        compile(&ir(Some(&caveats)), &bundle, &out)
            .unwrap_or_else(|e| panic!("{signal:?} must be accepted: {e}"));
    }
    for signal in [
        "it looked wrong",
        "exit code",
        "my judgement of the step",
        "verifier opinion: the command seemed off",
    ] {
        let caveats = format!("Repair 1: fixed. Signal: {signal}");
        let err = compile_error(&dir, &caveats);
        assert!(err.contains("01-quit-vim"), "{signal:?}: {err}");
    }
}

#[test]
fn prose_mentioning_repairs_is_not_a_repair_record() {
    let dir = scratch("prose");
    let bundle = dir.join("bundle");
    fs::create_dir_all(&bundle).unwrap();
    let caveats = "Do not try to repair the config by hand: the plugin rewrites it.\n\
                   Repairs to the swap file are described in `:help recover`.";

    compile(&ir(Some(caveats)), &bundle, &dir.join("vim-basics")).unwrap();
}

#[test]
fn cited_signals_are_carried_into_the_verification_report() {
    let dir = scratch("carried");
    let caveats = format!(
        "{SIGNALLED_REPAIR}\nRepair 2: added Enter. Signal: state: command line still open"
    );
    let package = compiled(&dir, Some(&caveats));

    let recorded = record_verification(&package, &report(3)).unwrap();

    assert_eq!(
        recorded.steps[0].repairs,
        vec![
            "exit code 1 — E492: Not an editor command: Q".to_string(),
            "state: command line still open".to_string(),
        ]
    );
    assert_eq!(recorded.steps[1].repairs, Vec::<String>::new());
    let saved: VerificationReport =
        serde_json::from_str(&fs::read_to_string(package.join("verification.json")).unwrap())
            .unwrap();
    assert_eq!(saved, recorded, "the written report carries the signals");
    let skill_md = fs::read_to_string(package.join("SKILL.md")).unwrap();
    assert!(
        skill_md.contains("after 2 repairs"),
        "the badge says the skill needed repairing: {skill_md}"
    );
}

#[test]
fn reports_claiming_repairs_that_no_step_records_are_refused() {
    let dir = scratch("unrecorded");
    let package = compiled(&dir, None);

    let err = record_verification(&package, &report(2))
        .unwrap_err()
        .to_string();

    assert!(err.contains("attempts"), "{err}");
    assert!(err.contains("Repair"), "{err}");
    assert!(
        !package.join("verification.json").exists(),
        "nothing is recorded for an inconsistent report"
    );
}

#[test]
fn a_first_attempt_records_no_repairs_and_the_badge_stays_plain() {
    let dir = scratch("first-attempt");
    let package = compiled(&dir, None);

    let recorded = record_verification(&package, &report(1)).unwrap();

    assert!(recorded.steps.iter().all(|s| s.repairs.is_empty()));
    let skill_md = fs::read_to_string(package.join("SKILL.md")).unwrap();
    assert!(!skill_md.contains("repair"), "{skill_md}");
}
