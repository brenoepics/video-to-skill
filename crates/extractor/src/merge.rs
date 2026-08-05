//! Update mode: fold a second video's Procedure IR into an existing one.
//!
//! Semantics: a matching step with agreeing actions is **confirmed**
//! (second provenance reference, confidence bumped one level); a
//! matching step with different actions gains a **variant** (the
//! original is never overwritten); unmatched incoming steps are
//! **added**; existing steps the new video didn't cover are kept as
//! **unconfirmed**. Pure and deterministic — the caller supplies the
//! fold date.

use crate::compile::{FoldRecord, ProcedureIr, Step, StepVariant};

const MATCH_THRESHOLD: f64 = 0.5;

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
    // Greedy best-match assignment: each incoming step claims at most
    // one existing step, by goal similarity.
    let mut claimed_by: Vec<Option<usize>> = vec![None; existing.steps.len()];
    for (incoming_idx, incoming_step) in incoming.steps.iter().enumerate() {
        let best = existing
            .steps
            .iter()
            .enumerate()
            .filter(|(existing_idx, existing_step)| {
                claimed_by[*existing_idx].is_none()
                    && (existing_step.id == incoming_step.id
                        || similarity(&existing_step.goal, &incoming_step.goal) >= MATCH_THRESHOLD)
            })
            .max_by(|(_, a), (_, b)| {
                similarity(&a.goal, &incoming_step.goal)
                    .total_cmp(&similarity(&b.goal, &incoming_step.goal))
            })
            .map(|(existing_idx, _)| existing_idx);
        if let Some(existing_idx) = best {
            claimed_by[existing_idx] = Some(incoming_idx);
        }
    }

    let mut merged = existing.clone();
    let mut plan = Vec::new();
    let mut record = FoldRecord {
        source: incoming.source.clone(),
        date: date.to_string(),
        confirmed: vec![],
        added: vec![],
        variants: vec![],
    };

    for (existing_idx, claim) in claimed_by.iter().enumerate() {
        let step = &mut merged.steps[existing_idx];
        match claim {
            Some(incoming_idx) => {
                let incoming_step = &incoming.steps[*incoming_idx];
                if actions_agree(&step.actions, &incoming_step.actions) {
                    confirm(step, incoming_step, &incoming.source.original);
                    record.confirmed.push(step.id.clone());
                    plan.push(entry(
                        Some(&step.id),
                        Some(&incoming_step.id),
                        PlanAction::Confirm,
                    ));
                } else {
                    step.variants.push(StepVariant {
                        actions: incoming_step.actions.clone(),
                        source_original: incoming.source.original.clone(),
                        evidence: tagged(incoming_step, &incoming.source.original),
                        note: None,
                    });
                    record.variants.push(step.id.clone());
                    plan.push(entry(
                        Some(&step.id),
                        Some(&incoming_step.id),
                        PlanAction::Variant,
                    ));
                }
            }
            None => plan.push(entry(
                Some(&merged.steps[existing_idx].id),
                None,
                PlanAction::Unconfirmed,
            )),
        }
    }

    let matched: Vec<usize> = claimed_by.iter().filter_map(|c| *c).collect();
    for (incoming_idx, incoming_step) in incoming.steps.iter().enumerate() {
        if matched.contains(&incoming_idx) {
            continue;
        }
        let mut added = incoming_step.clone();
        added.evidence = tagged(incoming_step, &incoming.source.original);
        if merged.steps.iter().any(|s| s.id == added.id) {
            added.id = format!("{}-b", added.id);
        }
        record.added.push(added.id.clone());
        plan.push(entry(None, Some(&incoming_step.id), PlanAction::Add));
        merged.steps.push(added);
    }

    merged.history.push(record);
    MergeResult { merged, plan }
}

fn confirm(step: &mut Step, incoming: &Step, source: &str) {
    step.evidence.extend(tagged(incoming, source));
    step.confidence = match step.confidence.as_str() {
        "low" => "medium".to_string(),
        _ => "high".to_string(),
    };
}

fn tagged(step: &Step, source: &str) -> Vec<crate::compile::Evidence> {
    step.evidence
        .iter()
        .cloned()
        .map(|mut e| {
            e.source.get_or_insert_with(|| source.to_string());
            e
        })
        .collect()
}

fn entry(existing: Option<&str>, incoming: Option<&str>, action: PlanAction) -> PlanEntry {
    PlanEntry {
        existing_id: existing.map(str::to_string),
        incoming_id: incoming.map(str::to_string),
        action,
    }
}

/// Do two action descriptions teach the same thing? Prose similarity,
/// or — the stronger signal for CLI content — agreement of their
/// backtick code spans (`:q!` means `:q!` regardless of surrounding
/// wording).
fn actions_agree(a: &str, b: &str) -> bool {
    if similarity(a, b) >= MATCH_THRESHOLD {
        return true;
    }
    let (ca, cb) = (code_spans(a), code_spans(b));
    if ca.is_empty() || cb.is_empty() {
        return false;
    }
    let intersection = ca.intersection(&cb).count();
    let union = ca.union(&cb).count();
    #[allow(clippy::cast_precision_loss)] // span counts are tiny
    {
        intersection as f64 / union as f64 >= MATCH_THRESHOLD
    }
}

/// Contents of `backtick` spans.
fn code_spans(s: &str) -> std::collections::HashSet<String> {
    s.split('`')
        .skip(1)
        .step_by(2)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Jaccard similarity over lowercase word tokens.
fn similarity(a: &str, b: &str) -> f64 {
    let tokens = |s: &str| -> std::collections::HashSet<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect()
    };
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.is_empty() && tb.is_empty() {
        return 1.0;
    }
    let intersection = ta.intersection(&tb).count();
    let union = ta.union(&tb).count();
    #[allow(clippy::cast_precision_loss)] // token counts are tiny
    {
        intersection as f64 / union as f64
    }
}
