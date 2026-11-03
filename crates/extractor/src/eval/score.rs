//! Scoring one gold video against what the extractor produced for it
//! (the bundle's timeline and transcript) and, when given, against a
//! compiled package authored from that bundle.

use super::gold::GoldVideo;
use super::metrics::{
    backtick_commands, exact_match, match_boundaries, recover_steps, spoken_in_window, Span,
};
use super::{Counts, VideoReport, BOUNDARY_TOLERANCE_SECS, STEP_IOU_THRESHOLD};
use crate::bundle::Transcript;
use crate::compile::ProcedureIr;
use crate::timeline::Timeline;
use crate::verify::VerificationReport;

/// Score the extractor-only metrics: segment boundaries against the
/// recipe's change times, step recovery by `IoU`, and — with a transcript
/// — gold commands spoken within their step.
#[must_use]
pub fn score_bundle(
    gold: &GoldVideo,
    timeline: &Timeline,
    transcript: Option<&Transcript>,
) -> VideoReport {
    let predicted: Vec<f64> = timeline
        .segments
        .iter()
        .skip(1)
        .map(|s| s.start_secs)
        .collect();
    let boundaries = match_boundaries(
        &predicted,
        &gold.recipe.change_times(),
        BOUNDARY_TOLERANCE_SECS,
    );
    let segments: Vec<Span> = timeline
        .segments
        .iter()
        .map(|s| Span::new(s.start_secs, s.end_secs))
        .collect();
    let steps: Vec<Span> = gold.steps.iter().map(|s| s.span).collect();
    let recovered = recover_steps(
        &segments,
        &steps,
        STEP_IOU_THRESHOLD,
        BOUNDARY_TOLERANCE_SECS,
    );

    let mut counts = Counts {
        boundaries_predicted: boundaries.predicted,
        boundaries_gold: boundaries.gold,
        boundaries_matched: boundaries.matched,
        steps_gold: recovered.steps_gold,
        steps_recovered: recovered.steps_recovered,
        segments_total: recovered.segments_total,
        segments_clean: recovered.segments_clean,
        commands_gold: gold.command_count() as u64,
        ..Counts::default()
    };
    if let Some(transcript) = transcript {
        counts.commands_spoken = gold
            .steps
            .iter()
            .flat_map(|step| step.commands.iter().map(move |c| (step, c)))
            .filter(|(step, command)| {
                spoken_in_window(
                    command,
                    &transcript.segments,
                    step.span.start_secs - BOUNDARY_TOLERANCE_SECS,
                    step.span.end_secs + BOUNDARY_TOLERANCE_SECS,
                )
            })
            .count() as u64;
    }
    VideoReport {
        id: gold.id.clone(),
        title: gold.recipe.title.clone(),
        duration_secs: gold.recipe.duration_secs,
        segments: timeline.segments.len() as u64,
        transcript_segments: transcript.map(|t| t.segments.len() as u64),
        counts,
        packages_scored: 0,
        verifications_scored: 0,
        notes: Vec::new(),
    }
}

/// Add the agent-dependent metrics from a compiled package: its steps
/// matched to gold steps by evidence timestamp (a candidate matches the
/// first unmatched gold step whose span contains its earliest
/// evidence), gold commands reproduced verbatim, and verification
/// outcomes when a report was recorded.
pub fn score_package(
    gold: &GoldVideo,
    ir: &ProcedureIr,
    verification: Option<&VerificationReport>,
    report: &mut VideoReport,
) {
    let mut taken = vec![false; gold.steps.len()];
    let mut matched = 0u64;
    for step in &ir.steps {
        let anchor = step
            .evidence
            .iter()
            .map(|e| e.timestamp_secs)
            .fold(f64::INFINITY, f64::min);
        let hit = gold.steps.iter().enumerate().find(|(idx, g)| {
            !taken[*idx] && anchor >= g.span.start_secs && anchor <= g.span.end_secs
        });
        if let Some((idx, _)) = hit {
            taken[idx] = true;
            matched += 1;
        }
    }
    let candidates: Vec<String> = ir
        .steps
        .iter()
        .flat_map(|s| backtick_commands(&s.actions))
        .collect();
    let counts = &mut report.counts;
    counts.package_steps = ir.steps.len() as u64;
    counts.package_matched = matched;
    counts.commands_exact = gold
        .steps
        .iter()
        .flat_map(|s| &s.commands)
        .filter(|c| exact_match(c, &candidates))
        .count() as u64;
    if let Some(verification) = verification {
        counts.verification_steps = verification.steps.len() as u64;
        counts.verification_passed = verification
            .steps
            .iter()
            .filter(|s| s.outcome == "pass")
            .count() as u64;
    }
    report.packages_scored = 1;
    report.verifications_scored = u64::from(verification.is_some());
}
