//! Update mode: fold a second video's Procedure IR into an existing one.
//!
//! # Design note: folding as counting
//!
//! A skill's steps are the nodes of a task graph (`ProcedureIr::task_graph`),
//! and every video is one walk through it, START → steps → END. A fold
//! does three things, in this order:
//!
//! 1. **Assign** each incoming step to a node. Confident matches first
//!    (same id, or goal words that overlap by at least half); then the
//!    rest are placed by position: an incoming step between two confident
//!    anchors a and b is matched to an unclaimed node on a video-observed
//!    path a → … → b when its actions agree or its goal is at least a
//!    quarter similar — the relabelling of Video-Mined Task Graphs
//!    (Ashutosh et al., 2023). What matches nothing is a new node.
//! 2. **Count** the incoming walk into the graph: +1 per consecutive pair.
//! 3. **Order** the merged steps by the graph: a topological order over
//!    the dominant direction of every edge, following the heaviest
//!    transition out of the last placed step. Two videos that disagree on
//!    an order make a bidirectional edge; the heavier direction orders the
//!    steps and the lighter one is recorded as an order variant in the
//!    fold's history. A new step therefore lands where its video performed
//!    it, not at the end.
//!
//! The summary users see is unchanged: a matched step whose actions agree
//! is **confirmed** (second provenance reference, confidence bumped one
//! level); a matched step with different actions gains a **variant** (the
//! original is never overwritten); an unmatched incoming step is
//! **added**; an existing step the new video did not cover is kept as
//! **unconfirmed**. Pure and deterministic — the caller supplies the
//! fold date.

mod assign;

use crate::compile::{
    Evidence, FoldRecord, ProcedureIr, Step, StepVariant, TaskGraph, SCHEMA_VERSION,
};
use assign::{actions_agree, assign};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanAction {
    Confirm,
    Variant,
    Add,
    Unconfirmed,
}

#[derive(Debug, Clone)]
pub struct PlanEntry {
    pub existing_id: Option<String>,
    pub incoming_id: Option<String>,
    pub action: PlanAction,
    /// Matched by its position in the graph rather than by goal words.
    pub relabelled: bool,
}

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub merged: ProcedureIr,
    pub plan: Vec<PlanEntry>,
}

/// Fold `incoming` into `existing`. `date` is the ISO date recorded in
/// the history entry.
#[must_use]
pub fn merge(existing: &ProcedureIr, incoming: &ProcedureIr, date: &str) -> MergeResult {
    // A package from before task graphs is the chain of its own steps.
    let mut graph = if existing.task_graph.is_empty() {
        TaskGraph::from_sequence(&ids(existing))
    } else {
        existing.task_graph.clone()
    };
    let assignments = assign(existing, incoming, &graph);

    let mut merged = existing.clone();
    let mut plan = Vec::new();
    let mut record = FoldRecord {
        source: incoming.source.clone(),
        date: date.to_string(),
        confirmed: vec![],
        added: vec![],
        variants: vec![],
        order_variants: vec![],
    };

    for (existing_idx, step) in merged.steps.iter_mut().enumerate() {
        let claim = assignments
            .iter()
            .enumerate()
            .find_map(|(incoming_idx, a)| {
                a.as_ref()
                    .filter(|a| a.existing == existing_idx)
                    .map(|a| (incoming_idx, a.relabelled))
            });
        let Some((incoming_idx, relabelled)) = claim else {
            plan.push(entry(Some(&step.id), None, PlanAction::Unconfirmed, false));
            continue;
        };
        let incoming_step = &incoming.steps[incoming_idx];
        let action = if actions_agree(&step.actions, &incoming_step.actions) {
            confirm(step, incoming_step, &incoming.source.original);
            record.confirmed.push(step.id.clone());
            PlanAction::Confirm
        } else {
            step.variants.push(StepVariant {
                actions: incoming_step.actions.clone(),
                source_original: incoming.source.original.clone(),
                evidence: tagged(incoming_step, &incoming.source.original),
                note: None,
            });
            record.variants.push(step.id.clone());
            PlanAction::Variant
        };
        plan.push(entry(
            Some(&step.id),
            Some(&incoming_step.id),
            action,
            relabelled,
        ));
    }

    // The incoming video as a walk over node ids, adding nodes for the
    // steps it alone performs.
    let mut walk = Vec::with_capacity(incoming.steps.len());
    for (incoming_step, assignment) in incoming.steps.iter().zip(&assignments) {
        if let Some(assignment) = assignment {
            walk.push(existing.steps[assignment.existing].id.clone());
            continue;
        }
        let mut added = incoming_step.clone();
        added.evidence = tagged(incoming_step, &incoming.source.original);
        if merged.steps.iter().any(|s| s.id == added.id) {
            added.id = format!("{}-b", added.id);
        }
        record.added.push(added.id.clone());
        plan.push(entry(None, Some(&incoming_step.id), PlanAction::Add, false));
        walk.push(added.id.clone());
        merged.steps.push(added);
    }
    graph.record_walk(&walk);

    let (order, dropped) = graph.order(&ids(&merged));
    merged.steps.sort_by_key(|s| {
        order
            .iter()
            .position(|id| *id == s.id)
            .unwrap_or(usize::MAX)
    });
    record.order_variants = dropped
        .iter()
        .map(|(from, to)| {
            format!(
                "{from} before {to} ({} vs {} videos)",
                graph.count(from, to),
                graph.count(to, from)
            )
        })
        .collect();

    // A fold invalidates verification: every verdict goes; execution
    // counts are history and stay. The result is always a current-schema IR.
    for step in &mut merged.steps {
        step.verdict = None;
    }
    merged.schema_version = SCHEMA_VERSION;
    merged.task_graph = graph;
    merged.history.push(record);
    MergeResult { merged, plan }
}

fn ids(ir: &ProcedureIr) -> Vec<String> {
    ir.steps.iter().map(|s| s.id.clone()).collect()
}

fn confirm(step: &mut Step, incoming: &Step, source: &str) {
    step.evidence.extend(tagged(incoming, source));
    step.confidence = match step.confidence.as_str() {
        "low" => "medium".to_string(),
        _ => "high".to_string(),
    };
}

fn tagged(step: &Step, source: &str) -> Vec<Evidence> {
    step.evidence
        .iter()
        .cloned()
        .map(|mut e| {
            e.source.get_or_insert_with(|| source.to_string());
            e
        })
        .collect()
}

fn entry(
    existing: Option<&str>,
    incoming: Option<&str>,
    action: PlanAction,
    relabelled: bool,
) -> PlanEntry {
    PlanEntry {
        existing_id: existing.map(str::to_string),
        incoming_id: incoming.map(str::to_string),
        action,
        relabelled,
    }
}
