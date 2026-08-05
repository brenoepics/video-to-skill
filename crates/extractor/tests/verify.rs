//! Verification records: typed report, validated against the package,
//! badge stamped into the generated SKILL.md.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use vts_extract::compile::{compile, Evidence, ProcedureIr, SourceRef, Step};
use vts_extract::verify::{record_verification, StepOutcome, VerificationReport};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-verify-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn step(id: &str) -> Step {
    Step {
        id: id.into(),
        goal: format!("do {id}"),
        actions: "run it".into(),
        success_criteria: "it worked".into(),
        confidence: "high".into(),
        caveats: None,
        evidence: vec![Evidence {
            timestamp_secs: 10.0,
            frame: None,
            quote: Some("said so".into()),
            source: None,
        }],
        scripts: vec![],
        variants: vec![],
    }
}

fn compiled_package(dir: &Path) -> PathBuf {
    let ir = ProcedureIr {
        schema_version: 1,
        skill_name: "demo-skill".into(),
        title: "Demo".into(),
        description: "Use for testing.".into(),
        overview: "Test package.".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: "file.mp4".into(),
            title: None,
            duration_secs: 60.0,
        },
        steps: vec![step("01-first"), step("02-second")],
        artifacts: vec![],
        gaps: vec![],
        history: vec![],
    };
    let bundle = dir.join("bundle");
    fs::create_dir_all(&bundle).unwrap();
    let out = dir.join("skill");
    compile(&ir, &bundle, &out).unwrap();
    out
}

fn passing_report() -> VerificationReport {
    VerificationReport {
        schema_version: 1,
        verified: true,
        attempts: 1,
        summary: "all steps passed in a clean sandbox".into(),
        steps: vec![
            StepOutcome {
                id: "01-first".into(),
                outcome: "pass".into(),
                detail: "exit 0, criteria met".into(),
            },
            StepOutcome {
                id: "02-second".into(),
                outcome: "pass".into(),
                detail: "exit 0".into(),
            },
        ],
    }
}

#[test]
fn recording_writes_report_and_stamps_a_verified_badge() {
    let dir = scratch("stamp");
    let package = compiled_package(&dir);

    record_verification(&package, &passing_report()).unwrap();

    let saved: VerificationReport =
        serde_json::from_str(&fs::read_to_string(package.join("verification.json")).unwrap())
            .unwrap();
    assert_eq!(saved, passing_report());
    let skill_md = fs::read_to_string(package.join("SKILL.md")).unwrap();
    assert!(
        skill_md.contains("Verified by execution"),
        "badge missing: {skill_md}"
    );
    // Badge sits after the frontmatter, before the title's content.
    let badge_pos = skill_md.find("Verified by execution").unwrap();
    let title_pos = skill_md.find("# Demo").unwrap();
    assert!(badge_pos < title_pos);
}

#[test]
fn failed_and_unverifiable_reports_stamp_honest_badges() {
    let dir = scratch("honest");
    let package = compiled_package(&dir);

    let mut failing = passing_report();
    failing.verified = false;
    failing.summary = "step 02 diverged: file not created".into();
    failing.steps[1].outcome = "fail".into();
    record_verification(&package, &failing).unwrap();
    let skill_md = fs::read_to_string(package.join("SKILL.md")).unwrap();
    assert!(skill_md.contains("NOT verified"), "got: {skill_md}");
    assert!(skill_md.contains("step 02 diverged: file not created"));
}

#[test]
fn restamping_replaces_the_badge_instead_of_duplicating() {
    let dir = scratch("restamp");
    let package = compiled_package(&dir);

    let mut first = passing_report();
    first.verified = false;
    record_verification(&package, &first).unwrap();
    record_verification(&package, &passing_report()).unwrap();

    let skill_md = fs::read_to_string(package.join("SKILL.md")).unwrap();
    assert_eq!(
        skill_md.matches("<!-- verification-badge -->").count(),
        1,
        "exactly one badge: {skill_md}"
    );
    assert!(skill_md.contains("Verified by execution"));
    assert!(!skill_md.contains("NOT verified"));
}

#[test]
fn reports_citing_unknown_steps_are_rejected() {
    let dir = scratch("unknown");
    let package = compiled_package(&dir);

    let mut report = passing_report();
    report.steps[0].id = "99-ghost".into();

    let err = record_verification(&package, &report).unwrap_err();
    assert!(err.to_string().contains("99-ghost"), "got: {err}");
}

#[test]
fn outcomes_must_use_the_known_vocabulary() {
    let dir = scratch("vocab");
    let package = compiled_package(&dir);

    let mut report = passing_report();
    report.steps[0].outcome = "maybe".into();

    let err = record_verification(&package, &report).unwrap_err();
    assert!(err.to_string().contains("maybe"), "got: {err}");
}
