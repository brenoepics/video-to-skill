//! Quality eval: the committed gold set is well-formed and
//! self-consistent, and the scorer behaves on hand-built extractor
//! output. The end-to-end run (ffmpeg, `say`, whisper) is ignored by
//! default — see `eval_runs_end_to_end_on_one_gold_video`.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use vts_extract::bundle::{Segment, Transcript};
use vts_extract::compile::{compile, SCHEMA_VERSION};
use vts_extract::eval::gold::{load_gold_set, GoldVideo};
use vts_extract::eval::metrics::normalize;
use vts_extract::eval::recipe::Screen;
use vts_extract::eval::score::score_bundle;
use vts_extract::eval::synth::shown_lines;
use vts_extract::eval::{metrics_from, run_eval, EvalOptions, Outcome, Unavailable};
use vts_extract::timeline::{Timeline, TimelineSegment};

fn gold_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("eval")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-eval-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn gold_set_has_at_least_ten_videos_covering_every_screen_kind() {
    let gold = load_gold_set(&gold_dir()).unwrap();

    assert!(gold.len() >= 10, "gold set has {} videos", gold.len());
    let mut kinds = BTreeSet::new();
    let mut continuous = 0usize;
    for video in &gold {
        assert!(
            video.steps.len() >= 2,
            "{}: needs two or more steps",
            video.id
        );
        assert!(
            video.recipe.narration.len() >= 2,
            "{}: needs narration",
            video.id
        );
        assert!(
            (20.0..=90.0).contains(&video.recipe.duration_secs),
            "{}: keep gold videos short",
            video.id
        );
        for still in &video.recipe.stills {
            kinds.insert(match still.screen {
                Screen::Terminal(_) => "terminal",
                Screen::Dialog(_) => "dialog",
                Screen::Slide(_) => "slide",
                Screen::Desktop(_) => "desktop",
            });
            continuous += usize::from(still.continuous);
        }
    }
    assert!(kinds.contains("terminal") && kinds.contains("dialog") && kinds.contains("slide"));
    assert!(continuous > 0, "at least one scrolling/streaming sequence");
    let commands: usize = gold.iter().map(GoldVideo::command_count).sum();
    assert!(commands >= 20, "only {commands} gold commands");
}

#[test]
fn every_gold_command_is_shown_on_screen_and_narrated_during_its_step() {
    for video in load_gold_set(&gold_dir()).unwrap() {
        let shown = shown_lines(&video.recipe);
        for step in &video.steps {
            for command in &step.commands {
                let on_screen = shown
                    .iter()
                    .filter(|(label, _)| label.as_deref() == Some(step.id.as_str()))
                    .any(|(_, lines)| lines.iter().any(|l| l.contains(command.as_str())));
                assert!(
                    on_screen,
                    "{}/{}: `{command}` never on screen",
                    video.id, step.id
                );
                let narrated = video.recipe.narration.iter().any(|cue| {
                    cue.t >= step.span.start_secs - 2.0
                        && cue.t <= step.span.end_secs
                        && normalize(&cue.text).contains(&normalize(command))
                });
                assert!(
                    narrated,
                    "{}/{}: `{command}` is not narrated",
                    video.id, step.id
                );
            }
        }
    }
}

#[test]
fn every_gold_procedure_compiles_as_a_valid_ir() {
    let dir = scratch("compile");
    let empty_bundle = dir.join("bundle");
    fs::create_dir_all(&empty_bundle).unwrap();
    for video in load_gold_set(&gold_dir()).unwrap() {
        assert_eq!(
            video.procedure.schema_version, SCHEMA_VERSION,
            "{}: gold IRs are written at the current schema",
            video.id
        );
        compile(&video.procedure, &empty_bundle, &dir.join(&video.id))
            .unwrap_or_else(|e| panic!("{}: {e:#}", video.id));
    }
}

/// Segments cut exactly at the recipe's change times, speech from its
/// narration cues: what a perfect extractor would produce.
fn perfect_extraction(video: &GoldVideo) -> (Timeline, Transcript) {
    let mut cuts = vec![0.0];
    cuts.extend(video.recipe.change_times());
    cuts.push(video.recipe.duration_secs);
    let segments = cuts
        .windows(2)
        .enumerate()
        .map(|(index, w)| TimelineSegment {
            index,
            start_secs: w[0],
            end_secs: w[1],
            motion_density: None,
            keyframe: None,
            interior_keyframes: vec![],
            skip: None,
            changed_region: None,
            cursor_presence: None,
            cursor: None,
            speech: vec![],
        })
        .collect();
    let transcript = Transcript {
        schema_version: 1,
        language: Some("en".into()),
        segments: video
            .recipe
            .narration
            .iter()
            .map(|cue| Segment {
                start_secs: cue.t,
                end_secs: cue.t + 3.0,
                text: cue.text.clone(),
                words: vec![],
            })
            .collect(),
    };
    let timeline = Timeline {
        schema_version: 1,
        duration_secs: video.recipe.duration_secs,
        language: Some("en".into()),
        transcript_source: None,
        chapters: vec![],
        skips: vec![],
        notes: vec![],
        segments,
    };
    (timeline, transcript)
}

#[test]
fn a_perfect_extraction_scores_every_extractor_metric_at_one() {
    for video in load_gold_set(&gold_dir()).unwrap() {
        let (timeline, transcript) = perfect_extraction(&video);

        let report = score_bundle(&video, &timeline, Some(&transcript));

        let c = &report.counts;
        assert_eq!(
            c.boundaries_matched, c.boundaries_gold,
            "{}: recall",
            video.id
        );
        assert_eq!(
            c.boundaries_matched, c.boundaries_predicted,
            "{}: precision",
            video.id
        );
        assert_eq!(c.steps_recovered, c.steps_gold, "{}: steps", video.id);
        assert_eq!(c.segments_clean, c.segments_total, "{}: clean", video.id);
        assert_eq!(c.commands_spoken, c.commands_gold, "{}: commands", video.id);
        let metrics = metrics_from(c, &Unavailable::default());
        let f1 = metrics.iter().find(|m| m.name == "boundary_f1").unwrap();
        assert!(matches!(f1.outcome, Outcome::Scored { value, .. } if (value - 1.0).abs() < 1e-9));
    }
}

#[test]
fn a_single_segment_timeline_scores_low_and_unavailable_inputs_carry_reasons() {
    let video = &load_gold_set(&gold_dir()).unwrap()[0];
    let (mut timeline, _) = perfect_extraction(video);
    timeline.segments.truncate(1);
    timeline.segments[0].end_secs = video.recipe.duration_secs;

    let report = score_bundle(video, &timeline, None);
    let unavailable = Unavailable {
        transcript: Some("no whisper".into()),
        packages: Some("no packages".into()),
        verification: Some("no packages; no agent".into()),
    };
    let metrics = metrics_from(&report.counts, &unavailable);

    let by_name = |name: &str| {
        metrics
            .iter()
            .find(|m| m.name == name)
            .unwrap()
            .outcome
            .clone()
    };
    assert_eq!(report.counts.boundaries_predicted, 0);
    assert!(matches!(by_name("boundary_recall"), Outcome::Scored { value, .. } if value == 0.0));
    assert!(matches!(
        by_name("boundary_precision"),
        Outcome::NotScored { .. }
    ));
    assert!(report.counts.steps_recovered < report.counts.steps_gold);
    assert_eq!(
        by_name("command_transcript_recall"),
        Outcome::NotScored {
            reason: "no whisper".into()
        }
    );
    assert_eq!(
        by_name("package_step_f1"),
        Outcome::NotScored {
            reason: "no packages".into()
        }
    );
    assert_eq!(
        by_name("verification_pass_rate"),
        Outcome::NotScored {
            reason: "no packages; no agent".into()
        }
    );
    let names: Vec<&str> = metrics.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names.len(), 12, "{names:?}");
}

#[test]
#[ignore = "generates a video, transcribes it with real whisper weights, scores it (vts-extract check --fix --yes first)"]
fn eval_runs_end_to_end_on_one_gold_video() {
    let first = load_gold_set(&gold_dir()).unwrap()[0].id.clone();
    let options = EvalOptions {
        gold_dir: gold_dir(),
        packages_dir: None,
        keep_dir: Some(scratch("e2e")),
        only: vec![first.clone()],
    };

    let report = run_eval(&options, &mut |line| println!("{line}")).unwrap();

    assert_eq!(report.videos.len(), 1);
    assert_eq!(report.videos[0].id, first);
    assert!(report.videos[0].segments >= 1);
    let recall = report.metric("boundary_recall").unwrap();
    assert!(
        matches!(recall.outcome, Outcome::Scored { .. }),
        "{recall:?}"
    );
    if cfg!(target_os = "macos") {
        let spoken = report.metric("command_transcript_recall").unwrap();
        assert!(
            matches!(spoken.outcome, Outcome::Scored { .. }),
            "{spoken:?}"
        );
    }
    assert!(matches!(
        report.metric("verification_pass_rate").unwrap().outcome,
        Outcome::NotScored { .. }
    ));
}
