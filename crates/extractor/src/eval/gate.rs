//! Regression gating: the current report's lower confidence bounds
//! against a committed baseline, metric by metric.

use super::{EvalReport, Metric, Outcome};

#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub max_drop: f64,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub name: String,
    pub current_lower: Option<f64>,
    pub baseline_lower: Option<f64>,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Within `max_drop` of the baseline either way.
    Ok,
    /// Lower bound fell more than `max_drop` below the baseline.
    Regressed { drop: f64 },
    /// Lower bound rose more than `max_drop`: refresh the baseline.
    Improved { gain: f64 },
    /// Scored in the baseline, not now.
    LostScoring { reason: String },
    /// Not in the baseline, or not scored there.
    NewMetric,
    /// Not scored in either.
    Unscored,
}

impl Comparison {
    /// One line per failing metric; empty when the gate passes.
    #[must_use]
    pub fn regressions(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter_map(|row| match &row.status {
                Status::Regressed { drop } => Some(format!(
                    "{}: lower bound {:.3} is {drop:.3} below the baseline {:.3} (max drop {:.3})",
                    row.name,
                    row.current_lower.unwrap_or(0.0),
                    row.baseline_lower.unwrap_or(0.0),
                    self.max_drop
                )),
                Status::LostScoring { reason } => Some(format!(
                    "{}: scored in the baseline but not now — {reason}",
                    row.name
                )),
                _ => None,
            })
            .collect()
    }

    /// Whether any metric improved beyond `max_drop`.
    #[must_use]
    pub fn is_stale(&self) -> bool {
        self.rows
            .iter()
            .any(|r| matches!(r.status, Status::Improved { .. }))
    }

    #[must_use]
    pub fn row(&self, name: &str) -> Option<&Row> {
        self.rows.iter().find(|r| r.name == name)
    }
}

/// Compare every metric of `current` and `baseline`.
#[must_use]
pub fn compare(current: &EvalReport, baseline: &EvalReport, max_drop: f64) -> Comparison {
    let mut rows: Vec<Row> = current
        .metrics
        .iter()
        .map(|metric| compare_one(metric, baseline.metric(&metric.name), max_drop))
        .collect();
    for metric in &baseline.metrics {
        if current.metric(&metric.name).is_none() {
            rows.push(Row {
                name: metric.name.clone(),
                current_lower: None,
                baseline_lower: lower_bound(metric),
                status: match metric.outcome {
                    Outcome::Scored { .. } => Status::LostScoring {
                        reason: "metric missing from the current report".to_string(),
                    },
                    Outcome::NotScored { .. } => Status::Unscored,
                },
            });
        }
    }
    Comparison { max_drop, rows }
}

fn compare_one(current: &Metric, baseline: Option<&Metric>, max_drop: f64) -> Row {
    let current_lower = lower_bound(current);
    let baseline_lower = baseline.and_then(lower_bound);
    let status = match (&current.outcome, current_lower, baseline_lower) {
        (_, Some(now), Some(then)) if now < then - max_drop => {
            Status::Regressed { drop: then - now }
        }
        (_, Some(now), Some(then)) if now > then + max_drop => {
            Status::Improved { gain: now - then }
        }
        (_, Some(_), Some(_)) => Status::Ok,
        (_, Some(_), None) => Status::NewMetric,
        (Outcome::NotScored { reason }, None, Some(_)) => Status::LostScoring {
            reason: reason.clone(),
        },
        (_, None, None) | (Outcome::Scored { .. }, None, _) => Status::Unscored,
    };
    Row {
        name: current.name.clone(),
        current_lower,
        baseline_lower,
        status,
    }
}

fn lower_bound(metric: &Metric) -> Option<f64> {
    match metric.outcome {
        Outcome::Scored { lower_bound, .. } => Some(lower_bound),
        Outcome::NotScored { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{compare, Status};
    use crate::bench::Machine;
    use crate::eval::{EvalReport, GoldSummary, Metric, Outcome};

    fn scored(name: &str, lower: f64) -> Metric {
        Metric {
            name: name.into(),
            outcome: Outcome::Scored {
                value: lower + 0.05,
                lower_bound: lower,
                numerator: None,
                denominator: None,
            },
        }
    }

    fn unscored(name: &str) -> Metric {
        Metric {
            name: name.into(),
            outcome: Outcome::NotScored {
                reason: "no packages".into(),
            },
        }
    }

    fn report(metrics: Vec<Metric>) -> EvalReport {
        EvalReport {
            schema_version: 1,
            version: "0.3.0".into(),
            generated_on: "2026-11-02".into(),
            machine: Machine {
                chip: "test".into(),
                logical_cores: 1,
                os: "test/test".into(),
                whisper_device: "cpu".into(),
            },
            tolerance_secs: 2.0,
            iou_threshold: 0.5,
            gold_set: GoldSummary::default(),
            metrics,
            videos: vec![],
            notes: vec![],
        }
    }

    #[test]
    fn a_drop_beyond_the_threshold_is_a_regression_and_within_it_is_ok() {
        let baseline = report(vec![scored("a", 0.80), scored("b", 0.80)]);
        let current = report(vec![scored("a", 0.70), scored("b", 0.76)]);

        let comparison = compare(&current, &baseline, 0.05);

        assert!(matches!(
            comparison.row("a").unwrap().status,
            Status::Regressed { .. }
        ));
        assert_eq!(comparison.row("b").unwrap().status, Status::Ok);
        assert_eq!(comparison.regressions().len(), 1);
        assert!(comparison.regressions()[0].contains("a: lower bound 0.700"));
    }

    #[test]
    fn losing_a_scored_metric_fails_and_new_or_unscored_ones_do_not() {
        let baseline = report(vec![
            scored("gone", 0.5),
            scored("lost", 0.5),
            unscored("never"),
        ]);
        let current = report(vec![
            unscored("lost"),
            unscored("never"),
            scored("fresh", 0.9),
        ]);

        let comparison = compare(&current, &baseline, 0.05);

        assert_eq!(
            comparison.regressions().len(),
            2,
            "{:?}",
            comparison.regressions()
        );
        assert_eq!(comparison.row("never").unwrap().status, Status::Unscored);
        assert_eq!(comparison.row("fresh").unwrap().status, Status::NewMetric);
        assert!(matches!(
            comparison.row("gone").unwrap().status,
            Status::LostScoring { .. }
        ));
    }

    #[test]
    fn a_gain_beyond_the_threshold_marks_the_baseline_stale() {
        let comparison = compare(
            &report(vec![scored("a", 0.9)]),
            &report(vec![scored("a", 0.6)]),
            0.05,
        );

        assert!(comparison.is_stale());
        assert_eq!(comparison.regressions(), Vec::<String>::new());
    }
}
