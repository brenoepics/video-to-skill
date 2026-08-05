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
            },
            StepOutcome {
                id: "02-second".into(),
                outcome: "unverifiable".into(),
                detail: "needs GUI hardware".into(),
            },
        ],
    }
}

/// verified == false with an actual failure among the outcomes.
fn failing_report() -> VerificationReport {
    let mut report = passing_report();
    report.verified = false;
    report.summary = "step 02 diverged: file not created".into();
    report.steps[1].outcome = "fail".into();
    report
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
fn reports_with_a_fail_outcome_stamp_the_not_verified_badge() {
    let dir = scratch("honest");
    let package = compiled_package(&dir);

    record_verification(&package, &failing_report()).unwrap();
    let skill_md = fs::read_to_string(package.join("SKILL.md")).unwrap();
    assert!(skill_md.contains("NOT verified"), "got: {skill_md}");
    assert!(skill_md.contains("step 02 diverged: file not created"));
    assert!(
        !skill_md.contains("Partially verified"),
        "a failure must not read as partial: {skill_md}"
    );
}

#[test]
fn no_failure_partial_reports_stamp_the_partial_badge() {
    let dir = scratch("partial");
    let package = compiled_package(&dir);

    record_verification(&package, &partial_report()).unwrap();

    let skill_md = fs::read_to_string(package.join("SKILL.md")).unwrap();
    assert!(
        skill_md.contains("🟡 **Partially verified** — no executed step failed"),
        "partial badge missing: {skill_md}"
    );
    assert!(
        skill_md.contains("1 passed, 0 skipped by safety policy, 1 unverifiable"),
        "counts missing: {skill_md}"
    );
    assert!(
        skill_md.contains("1 passed, 1 unverifiable in a sandbox"),
        "summary missing: {skill_md}"
    );
    assert!(
        skill_md.contains("[verification.json](verification.json)"),
        "details link missing: {skill_md}"
    );
    assert!(
        !skill_md.contains("NOT verified"),
        "no failures must not read as NOT verified: {skill_md}"
    );
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
fn restamping_transitions_between_all_three_states() {
    let dir = scratch("three-state");
    let package = compiled_package(&dir);

    let assert_single_badge = |wanted: &str, unwanted: [&str; 2]| {
        let skill_md = fs::read_to_string(package.join("SKILL.md")).unwrap();
        assert_eq!(
            skill_md.matches("<!-- verification-badge -->").count(),
            1,
            "exactly one badge block: {skill_md}"
        );
        assert!(skill_md.contains(wanted), "missing '{wanted}': {skill_md}");
        for text in unwanted {
            assert!(!skill_md.contains(text), "stale '{text}': {skill_md}");
        }
    };

    record_verification(&package, &passing_report()).unwrap();
    assert_single_badge(
        "Verified by execution",
        ["NOT verified", "Partially verified"],
    );

    record_verification(&package, &partial_report()).unwrap();
    assert_single_badge(
        "Partially verified",
        ["NOT verified", "Verified by execution"],
    );

    record_verification(&package, &failing_report()).unwrap();
    assert_single_badge(
        "NOT verified",
        ["Partially verified", "Verified by execution"],
    );

    record_verification(&package, &passing_report()).unwrap();
    assert_single_badge(
        "Verified by execution",
        ["NOT verified", "Partially verified"],
    );
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
