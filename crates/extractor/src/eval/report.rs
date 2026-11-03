//! Markdown rendering of an eval report: the metrics table with the
//! baseline comparison, the gold-set line, and a per-video table.

use std::fmt::Write as _;

use super::gate::{Comparison, Status};
use super::{EvalReport, Metric, Outcome, VideoReport};

/// The summary printed after a run, written with `--summary`, and
/// posted to the CI job summary.
#[must_use]
pub fn render_summary_markdown(report: &EvalReport, comparison: Option<&Comparison>) -> String {
    let mut out = format!(
        "## Quality eval — vts-extract {} ({})\n\n",
        report.version, report.generated_on
    );
    out.push_str("| metric | value | 95% lower bound | baseline lower bound | status |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for metric in &report.metrics {
        let row = comparison.and_then(|c| c.row(&metric.name));
        // Writing into a String cannot fail.
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            metric.name,
            value_cell(metric),
            bound_cell(metric),
            row.and_then(|r| r.baseline_lower)
                .map_or_else(|| "—".to_string(), |b| format!("{b:.3}")),
            status_cell(
                metric,
                row.map(|r| &r.status),
                comparison.map(|c| c.max_drop)
            )
        );
    }
    let g = &report.gold_set;
    let _ = writeln!(
        out,
        "\nGold set: {} videos · {} steps · {} commands · {} visual changes. \
         Boundary tolerance ±{:.1} s · step IoU ≥ {:.2}.",
        g.videos, g.steps, g.commands, g.changes, report.tolerance_secs, report.iou_threshold
    );
    let m = &report.machine;
    let _ = writeln!(
        out,
        "\nMachine: {} · {} logical cores · {} · whisper on {}",
        m.chip, m.logical_cores, m.os, m.whisper_device
    );
    if comparison.is_some_and(Comparison::is_stale) {
        out.push_str(
            "\n> A metric improved beyond the gate threshold: refresh the baseline \
             (`vts-extract eval --out crates/extractor/eval/baseline.json`) so later \
             regressions are caught against the new level.\n",
        );
    }
    out.push_str("\n### Per video\n\n");
    out.push_str("| video | segments | boundaries matched/predicted/gold | steps recovered | segments clean | commands spoken |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for video in &report.videos {
        let _ = writeln!(out, "{}", video_row(video));
    }
    out
}

fn value_cell(metric: &Metric) -> String {
    match &metric.outcome {
        Outcome::Scored {
            value,
            numerator: Some(n),
            denominator: Some(d),
            ..
        } => format!("{value:.3} ({n}/{d})"),
        Outcome::Scored { value, .. } => format!("{value:.3}"),
        Outcome::NotScored { .. } => "—".to_string(),
    }
}

fn bound_cell(metric: &Metric) -> String {
    match &metric.outcome {
        Outcome::Scored { lower_bound, .. } => format!("{lower_bound:.3}"),
        Outcome::NotScored { .. } => "—".to_string(),
    }
}

fn status_cell(metric: &Metric, status: Option<&Status>, max_drop: Option<f64>) -> String {
    if let Outcome::NotScored { reason } = &metric.outcome {
        return format!("not scored: {reason}");
    }
    match status {
        None => "no baseline".to_string(),
        Some(Status::Ok) => "ok".to_string(),
        Some(Status::Regressed { drop }) => format!(
            "**regression** (−{drop:.3} > {:.3})",
            max_drop.unwrap_or(0.0)
        ),
        Some(Status::Improved { gain }) => format!("improved (+{gain:.3}) — refresh the baseline"),
        Some(Status::NewMetric) => "new (no baseline value)".to_string(),
        Some(Status::LostScoring { reason }) => format!("**lost** — {reason}"),
        Some(Status::Unscored) => "not scored".to_string(),
    }
}

fn video_row(video: &VideoReport) -> String {
    let c = &video.counts;
    let spoken = video.transcript_segments.map_or_else(
        || "—".to_string(),
        |_| format!("{}/{}", c.commands_spoken, c.commands_gold),
    );
    format!(
        "| {} | {} | {}/{}/{} | {}/{} | {}/{} | {} |",
        video.id,
        video.segments,
        c.boundaries_matched,
        c.boundaries_predicted,
        c.boundaries_gold,
        c.steps_recovered,
        c.steps_gold,
        c.segments_clean,
        c.segments_total,
        spoken
    )
}

#[cfg(test)]
mod tests {
    use super::render_summary_markdown;
    use crate::bench::Machine;
    use crate::eval::gate::compare;
    use crate::eval::{Counts, EvalReport, GoldSummary, Metric, Outcome, VideoReport};

    fn report(lower: f64) -> EvalReport {
        EvalReport {
            schema_version: 1,
            version: "0.3.0".into(),
            generated_on: "2026-11-02".into(),
            machine: Machine {
                chip: "Apple M2".into(),
                logical_cores: 8,
                os: "macos/aarch64".into(),
                whisper_device: "gpu".into(),
            },
            tolerance_secs: 2.0,
            iou_threshold: 0.5,
            gold_set: GoldSummary {
                videos: 1,
                steps: 2,
                commands: 3,
                changes: 4,
            },
            metrics: vec![
                Metric {
                    name: "boundary_recall".into(),
                    outcome: Outcome::Scored {
                        value: lower + 0.1,
                        lower_bound: lower,
                        numerator: Some(3),
                        denominator: Some(4),
                    },
                },
                Metric {
                    name: "verification_pass_rate".into(),
                    outcome: Outcome::NotScored {
                        reason: "no packages".into(),
                    },
                },
            ],
            videos: vec![VideoReport {
                id: "term-demo".into(),
                title: "Demo".into(),
                duration_secs: 30.0,
                segments: 5,
                transcript_segments: Some(4),
                counts: Counts {
                    boundaries_predicted: 4,
                    boundaries_gold: 4,
                    boundaries_matched: 3,
                    steps_gold: 2,
                    steps_recovered: 2,
                    segments_total: 4,
                    segments_clean: 3,
                    commands_gold: 3,
                    commands_spoken: 2,
                    ..Counts::default()
                },
                packages_scored: 0,
                verifications_scored: 0,
                notes: vec![],
            }],
            notes: vec![],
        }
    }

    #[test]
    fn summary_lists_metrics_reasons_and_per_video_counts() {
        let markdown = render_summary_markdown(&report(0.6), None);

        assert!(markdown.contains("| boundary_recall | 0.700 (3/4) | 0.600 | — | no baseline |"));
        assert!(
            markdown.contains("| verification_pass_rate | — | — | — | not scored: no packages |")
        );
        assert!(markdown.contains("Gold set: 1 videos · 2 steps · 3 commands · 4 visual changes."));
        assert!(markdown.contains("| term-demo | 5 | 3/4/4 | 2/2 | 3/4 | 2/3 |"));
    }

    #[test]
    fn summary_marks_regressions_and_stale_baselines_against_a_comparison() {
        let regressed = compare(&report(0.5), &report(0.6), 0.05);
        let markdown = render_summary_markdown(&report(0.5), Some(&regressed));
        assert!(markdown.contains("| 0.500 | 0.600 | **regression** (−0.100 > 0.050) |"));

        let improved = compare(&report(0.8), &report(0.6), 0.05);
        let markdown = render_summary_markdown(&report(0.8), Some(&improved));
        assert!(markdown.contains("improved (+0.200)"));
        assert!(markdown.contains("refresh the baseline"));
    }
}
