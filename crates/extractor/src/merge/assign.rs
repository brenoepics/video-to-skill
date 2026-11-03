//! Assigning incoming steps to keystep nodes: confident text matches
//! first, then placement by position in the graph.

use crate::compile::graph::{END, START};
use crate::compile::{ProcedureIr, TaskGraph};

/// Goal-word overlap that counts as the same step outright.
const MATCH_THRESHOLD: f64 = 0.5;
/// Goal-word overlap enough to accept a node the graph position suggests.
const RELABEL_THRESHOLD: f64 = 0.25;

#[derive(Debug, Clone, Copy)]
pub(super) struct Assignment {
    /// Index into the existing steps.
    pub existing: usize,
    /// Placed by graph position rather than by goal words.
    pub relabelled: bool,
}

/// For each incoming step, the existing step it stands for, if any.
/// Each existing step is claimed at most once.
pub(super) fn assign(
    existing: &ProcedureIr,
    incoming: &ProcedureIr,
    graph: &TaskGraph,
) -> Vec<Option<Assignment>> {
    let mut claimed = vec![false; existing.steps.len()];
    let mut out: Vec<Option<Assignment>> = vec![None; incoming.steps.len()];

    // Pass 1: confident matches, greedy best-first per incoming step.
    for (i, incoming_step) in incoming.steps.iter().enumerate() {
        let best = existing
            .steps
            .iter()
            .enumerate()
            .filter(|(e, step)| {
                !claimed[*e]
                    && (step.id == incoming_step.id
                        || similarity(&step.goal, &incoming_step.goal) >= MATCH_THRESHOLD)
            })
            .max_by(|(_, a), (_, b)| {
                similarity(&a.goal, &incoming_step.goal)
                    .total_cmp(&similarity(&b.goal, &incoming_step.goal))
            })
            .map(|(e, _)| e);
        if let Some(e) = best {
            claimed[e] = true;
            out[i] = Some(Assignment {
                existing: e,
                relabelled: false,
            });
        }
    }

    // Pass 2: the rest, by where they sit between assigned neighbours —
    // a candidate must lie on an observed path from the previous anchor
    // to the next, and look at least a little like the incoming step.
    for i in 0..incoming.steps.len() {
        if out[i].is_some() {
            continue;
        }
        let anchor = |j: usize| out[j].map(|a| existing.steps[a.existing].id.as_str());
        let before = (0..i).rev().find_map(anchor).unwrap_or(START);
        let after = (i + 1..incoming.steps.len())
            .find_map(anchor)
            .unwrap_or(END);
        let incoming_step = &incoming.steps[i];
        let score = |step: &crate::compile::Step| {
            (
                u8::from(actions_agree(&step.actions, &incoming_step.actions)),
                similarity(&step.goal, &incoming_step.goal),
            )
        };
        let best = existing
            .steps
            .iter()
            .enumerate()
            .filter(|(e, step)| {
                !claimed[*e] && graph.reaches(before, &step.id) && graph.reaches(&step.id, after)
            })
            .filter(|(_, step)| {
                let (agree, similar) = score(step);
                agree == 1 || similar >= RELABEL_THRESHOLD
            })
            .max_by(|(_, a), (_, b)| {
                let (agree_a, sim_a) = score(a);
                let (agree_b, sim_b) = score(b);
                agree_a.cmp(&agree_b).then(sim_a.total_cmp(&sim_b))
            })
            .map(|(e, _)| e);
        if let Some(e) = best {
            claimed[e] = true;
            out[i] = Some(Assignment {
                existing: e,
                relabelled: true,
            });
        }
    }
    out
}

/// Do two action descriptions teach the same thing? Prose similarity,
/// or — the stronger signal for CLI content — agreement of their
/// backtick code spans (`:q!` means `:q!` regardless of surrounding
/// wording).
pub(super) fn actions_agree(a: &str, b: &str) -> bool {
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
