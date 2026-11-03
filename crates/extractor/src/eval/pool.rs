//! From pooled counts to the metric list: proportions carry their
//! Wilson lower bound, F1 rows combine the bounds of their precision and
//! recall, and families whose inputs were unavailable are reported as
//! not scored with the reason.

use super::stats::{f1, Proportion};
use super::types::{Counts, Metric, Outcome};

/// Why a family of metrics could not be scored, when it could not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unavailable {
    pub transcript: Option<String>,
    pub packages: Option<String>,
    pub verification: Option<String>,
}

/// Every metric in a fixed order, from pooled counts.
#[must_use]
pub fn metrics_from(counts: &Counts, unavailable: &Unavailable) -> Vec<Metric> {
    let mut out = Vec::new();
    let precision = Proportion::new(counts.boundaries_matched, counts.boundaries_predicted);
    let recall = Proportion::new(counts.boundaries_matched, counts.boundaries_gold);
    push_pair(&mut out, "boundary", precision, recall);
    let precision = Proportion::new(counts.segments_clean, counts.segments_total);
    let recall = Proportion::new(counts.steps_recovered, counts.steps_gold);
    push_pair(&mut out, "step_iou", precision, recall);
    let spoken = Proportion::new(counts.commands_spoken, counts.commands_gold);
    push_gated(
        &mut out,
        "command_transcript_recall",
        spoken,
        unavailable.transcript.as_deref(),
    );
    let precision = Proportion::new(counts.package_matched, counts.package_steps);
    let recall = Proportion::new(counts.package_matched, counts.steps_gold);
    match &unavailable.packages {
        Some(reason) => {
            for name in [
                "package_step_precision",
                "package_step_recall",
                "package_step_f1",
            ] {
                out.push(not_scored(name, reason));
            }
        }
        None => push_pair(&mut out, "package_step", precision, recall),
    }
    let exact = Proportion::new(counts.commands_exact, counts.commands_gold);
    push_gated(
        &mut out,
        "command_exact_match",
        exact,
        unavailable.packages.as_deref(),
    );
    let passed = Proportion::new(counts.verification_passed, counts.verification_steps);
    push_gated(
        &mut out,
        "verification_pass_rate",
        passed,
        unavailable.verification.as_deref(),
    );
    out
}

fn push_pair(out: &mut Vec<Metric>, prefix: &str, precision: Proportion, recall: Proportion) {
    push_gated(out, &format!("{prefix}_precision"), precision, None);
    push_gated(out, &format!("{prefix}_recall"), recall, None);
    let name = format!("{prefix}_f1");
    match (
        precision.value().zip(precision.lower_bound()),
        recall.value().zip(recall.lower_bound()),
    ) {
        (Some((p, p_low)), Some((r, r_low))) => out.push(Metric {
            name,
            outcome: Outcome::Scored {
                value: f1(p, r),
                lower_bound: f1(p_low, r_low),
                numerator: None,
                denominator: None,
            },
        }),
        _ => out.push(not_scored(
            &name,
            "precision or recall had nothing to count",
        )),
    }
}

fn push_gated(out: &mut Vec<Metric>, name: &str, proportion: Proportion, blocked: Option<&str>) {
    if let Some(reason) = blocked {
        out.push(not_scored(name, reason));
        return;
    }
    match proportion.value().zip(proportion.lower_bound()) {
        Some((value, lower_bound)) => out.push(Metric {
            name: name.to_string(),
            outcome: Outcome::Scored {
                value,
                lower_bound,
                numerator: Some(proportion.numerator),
                denominator: Some(proportion.denominator),
            },
        }),
        None => out.push(not_scored(name, "nothing to count")),
    }
}

fn not_scored(name: &str, reason: &str) -> Metric {
    Metric {
        name: name.to_string(),
        outcome: Outcome::NotScored {
            reason: reason.to_string(),
        },
    }
}
