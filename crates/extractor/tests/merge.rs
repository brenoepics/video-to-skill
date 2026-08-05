//! Update mode: folding a second video's IR into an existing skill IR.
//! Semantics under test: confirm (evidence + confidence), variant
//! (never overwrite), add, and unconfirmed-keep.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use vts_extract::compile::{Evidence, ProcedureIr, SourceRef, Step};
use vts_extract::merge::{merge, PlanAction};

fn step(id: &str, goal: &str, actions: &str, confidence: &str) -> Step {
    Step {
        id: id.into(),
        goal: goal.into(),
        actions: actions.into(),
        success_criteria: "it worked".into(),
        confidence: confidence.into(),
        caveats: None,
        evidence: vec![Evidence {
            timestamp_secs: 10.0,
            frame: None,
            quote: Some("seen".into()),
            source: None,
        }],
        scripts: vec![],
        variants: vec![],
    }
}

fn ir(url: &str, steps: Vec<Step>) -> ProcedureIr {
    ProcedureIr {
        schema_version: 1,
        skill_name: "demo".into(),
        title: "Demo".into(),
        description: "Use for testing.".into(),
        overview: "overview".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: url.into(),
            title: None,
            duration_secs: 60.0,
        },
        steps,
        artifacts: vec![],
        gaps: vec![],
        history: vec![],
    }
}

#[test]
fn matching_steps_are_confirmed_with_tagged_evidence_and_confidence_bump() {
    let existing = ir(
        "https://a.example/v1",
        vec![step(
            "01-quit-vim",
            "Quit Vim discarding changes",
            "Type `:q!`",
            "low",
        )],
    );
    let incoming = ir(
        "https://b.example/v2",
        vec![step(
            "01-quit",
            "Quit Vim and discard changes",
            "Type `:q!`",
            "high",
        )],
    );

    let result = merge(&existing, &incoming, "2026-08-05");

    assert_eq!(result.plan.len(), 1);
    assert_eq!(result.plan[0].action, PlanAction::Confirm);
    let merged_step = &result.merged.steps[0];
    assert_eq!(merged_step.id, "01-quit-vim", "existing id kept");
    assert_eq!(merged_step.actions, "Type `:q!`", "existing text kept");
    assert_eq!(merged_step.confidence, "medium", "low bumps one level");
    assert_eq!(merged_step.evidence.len(), 2, "second provenance added");
    assert_eq!(
        merged_step.evidence[1].source.as_deref(),
        Some("https://b.example/v2"),
        "new evidence tagged with its video"
    );
}

#[test]
fn conflicting_actions_become_a_variant_never_an_overwrite() {
    let existing = ir(
        "https://a.example/v1",
        vec![step(
            "02-save",
            "Save the current file",
            "Type `:w` in normal mode",
            "high",
        )],
    );
    let incoming = ir(
        "https://b.example/v2",
        vec![step(
            "02-save-file",
            "Save the current file",
            "Press Ctrl+S with the mswin plugin enabled",
            "high",
        )],
    );

    let result = merge(&existing, &incoming, "2026-08-05");

    assert_eq!(result.plan[0].action, PlanAction::Variant);
    let merged_step = &result.merged.steps[0];
    assert_eq!(
        merged_step.actions, "Type `:w` in normal mode",
        "original untouched"
    );
    assert_eq!(merged_step.variants.len(), 1);
    let variant = &merged_step.variants[0];
    assert!(variant.actions.contains("Ctrl+S"));
    assert_eq!(variant.source_original, "https://b.example/v2");
    assert!(!variant.evidence.is_empty(), "variant cites its own video");
}

#[test]
fn new_steps_are_added_and_absent_steps_kept_as_unconfirmed() {
    let existing = ir(
        "https://a.example/v1",
        vec![step("01-quit", "Quit Vim", "Type `:q!`", "high")],
    );
    let incoming = ir(
        "https://b.example/v2",
        vec![step(
            "05-macros",
            "Record and replay a macro",
            "Press `q` then a register letter",
            "medium",
        )],
    );

    let result = merge(&existing, &incoming, "2026-08-05");

    let actions: Vec<&PlanAction> = result.plan.iter().map(|p| &p.action).collect();
    assert!(actions.contains(&&PlanAction::Add));
    assert!(actions.contains(&&PlanAction::Unconfirmed));
    assert_eq!(result.merged.steps.len(), 2);
    let added = result
        .merged
        .steps
        .iter()
        .find(|s| s.id == "05-macros")
        .unwrap();
    assert_eq!(
        added.evidence[0].source.as_deref(),
        Some("https://b.example/v2")
    );
    // The unconfirmed step survives unchanged.
    assert!(result.merged.steps.iter().any(|s| s.id == "01-quit"));
}

#[test]
fn history_records_the_fold_with_date_and_classification() {
    let existing = ir(
        "https://a.example/v1",
        vec![step("01-quit", "Quit Vim", "Type `:q!`", "high")],
    );
    let incoming = ir(
        "https://b.example/v2",
        vec![
            step("01-exit", "Quit Vim editor", "Type `:q!`", "high"),
            step("05-macros", "Record a macro", "Press `q`", "medium"),
        ],
    );

    let result = merge(&existing, &incoming, "2026-08-05");

    assert_eq!(result.merged.history.len(), 1);
    let record = &result.merged.history[0];
    assert_eq!(record.source.original, "https://b.example/v2");
    assert_eq!(record.date, "2026-08-05");
    assert_eq!(record.confirmed, vec!["01-quit"]);
    assert_eq!(record.added, vec!["05-macros"]);
    assert!(record.variants.is_empty());
}

#[test]
fn same_commands_in_different_prose_still_confirm() {
    let existing = ir(
        "https://a.example/v1",
        vec![step(
            "01-quit-vim",
            "Quit Vim, discarding changes if needed",
            "In normal mode type `:q` to close an unmodified file. If the buffer has unsaved changes, `:q!` quits and discards them.",
            "high",
        )],
    );
    let incoming = ir(
        "https://b.example/v2",
        vec![step(
            "01-quit-vim",
            "Quit Vim, discarding changes if needed",
            "Type `:q` and press Enter to quit. To quit without saving changes, add the bang: `:q!`.",
            "high",
        )],
    );

    let result = merge(&existing, &incoming, "2026-08-05");

    assert_eq!(
        result.plan[0].action,
        PlanAction::Confirm,
        "identical code spans (`:q`, `:q!`) must confirm despite different prose"
    );
}

#[test]
fn merge_is_pure_and_leaves_inputs_untouched() {
    let existing = ir(
        "https://a.example/v1",
        vec![step("01-quit", "Quit Vim", "Type `:q!`", "low")],
    );
    let incoming = ir(
        "https://b.example/v2",
        vec![step("01-quit", "Quit Vim", "Type `:q!`", "high")],
    );
    let existing_before = existing.clone();

    let first = merge(&existing, &incoming, "2026-08-05");
    let second = merge(&existing, &incoming, "2026-08-05");

    assert_eq!(existing, existing_before, "inputs are not mutated");
    assert_eq!(first.merged, second.merged, "deterministic");
}
