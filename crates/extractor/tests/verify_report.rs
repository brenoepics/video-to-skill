//! Verification reports: validated against the package (step ids,
//! outcome vocabulary) and mirrored into the frontmatter metadata.

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
        steps: vec![step("01-first"), step("02-second")],
        artifacts: vec![],
        gaps: vec![],
        history: vec![],
    };
    let bundle = dir.join("bundle");
    fs::create_dir_all(&bundle).unwrap();
    let out = dir.join("demo-skill");
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
                repairs: vec![],
            },
            StepOutcome {
                id: "02-second".into(),
                outcome: "pass".into(),
                detail: "exit 0".into(),
                repairs: vec![],
            },
        ],
    }
}

/// verified == false, but nothing failed: one pass, one unverifiable.
fn partial_report() -> VerificationReport {
    VerificationReport {
        schema_version: 1,
        verified: false,
        attempts: 1,
        summary: "1 passed, 1 unverifiable in a sandbox".into(),
        steps: vec![
            StepOutcome {
                id: "01-first".into(),
                outcome: "pass".into(),
                detail: "exit 0, criteria met".into(),
                repairs: vec![],
            },
            StepOutcome {
                id: "02-second".into(),
                outcome: "unverifiable".into(),
                detail: "needs GUI hardware".into(),
                repairs: vec![],
            },
        ],
    }
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

#[test]
fn recording_updates_the_verified_steps_frontmatter_metadata() {
    let dir = scratch("metadata");
    let package = compiled_package(&dir);
    let metadata_line = |value: &str| format!(r#"  video-to-skill/verified-steps: "{value}""#);
    let skill_md = fs::read_to_string(package.join("SKILL.md")).unwrap();
    assert!(skill_md.contains(&metadata_line("0/2")), "{skill_md}");

    record_verification(&package, &passing_report()).unwrap();
    let skill_md = fs::read_to_string(package.join("SKILL.md")).unwrap();
    assert!(skill_md.contains(&metadata_line("2/2")), "{skill_md}");
    assert!(!skill_md.contains(&metadata_line("0/2")), "{skill_md}");

    record_verification(&package, &partial_report()).unwrap();
    let skill_md = fs::read_to_string(package.join("SKILL.md")).unwrap();
    assert!(
        skill_md.contains(&metadata_line("1/2")),
        "only passes count: {skill_md}"
    );
    assert_eq!(
        skill_md.matches("verified-steps").count(),
        1,
        "the line is replaced, never duplicated: {skill_md}"
    );
}
