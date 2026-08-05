//! Timeline assembly: transcript + shots merged into the aligned,
//! agent-facing digest — including the degraded one-track modes.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::asr::{extract_audio_track, EngineOutput, TranscriptionEngine};
use vts_extract::bundle::{ingest, load_manifest, Segment};
use vts_extract::frames::{extract_frame_track, FrameConfig};
use vts_extract::timeline::{assemble, load_timeline, TIMELINE_FILE};
use vts_extract::tools::Toolchain;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-timeline-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn toolchain() -> Toolchain {
    Toolchain {
        ffmpeg: PathBuf::from("ffmpeg"),
        ffprobe: PathBuf::from("ffprobe"),
        whisper_model: None,
    }
}

/// Two solid scenes (0-3s red, 3-6s blue) with an audio track.
fn two_scene_bundle(dir: &Path) -> PathBuf {
    let fixture = dir.join("scenes.mp4");
    let res = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=320x240:r=10:d=3",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x240:r=10:d=3",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=6",
            "-filter_complex",
            "[0][1]concat=n=2[v]",
            "-map",
            "[v]",
            "-map",
            "2:a",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    let bundle = dir.join("bundle");
    ingest(&fixture, &bundle, &toolchain()).unwrap();
    bundle
}

struct CannedEngine(Vec<Segment>);

impl TranscriptionEngine for CannedEngine {
    fn transcribe(
        &self,
        _wav: &Path,
        _progress: &mut dyn FnMut(u8),
    ) -> anyhow::Result<EngineOutput> {
        Ok(EngineOutput {
            language: Some("en".into()),
            segments: self.0.clone(),
        })
    }
}

fn seg(start: f64, end: f64, text: &str) -> Segment {
    Segment {
        start_secs: start,
        end_secs: end,
        text: text.into(),
        words: vec![],
    }
}

#[test]
fn speech_lands_in_the_shot_it_overlaps_most() {
    let dir = scratch("align");
    let bundle = two_scene_bundle(&dir);
    // First span sits in scene 1; second straddles the 3s cut but mostly
    // in scene 2; third is entirely in scene 2.
    let engine = CannedEngine(vec![
        seg(0.2, 1.8, "welcome to scene one"),
        seg(2.5, 4.9, "now we move to scene two"),
        seg(5.0, 5.9, "and wrap up"),
    ]);
    extract_audio_track(&bundle, &toolchain(), &engine, &mut |_| {}).unwrap();
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    let timeline = assemble(&bundle).unwrap();

    assert_eq!(timeline.schema_version, 1);
    assert_eq!(timeline.language.as_deref(), Some("en"));
    assert_eq!(timeline.segments.len(), 2, "one segment per shot");
    let texts = |i: usize| -> Vec<&str> {
        timeline.segments[i]
            .speech
            .iter()
            .map(|s| s.text.as_str())
            .collect()
    };
    assert_eq!(texts(0), vec!["welcome to scene one"]);
    assert_eq!(texts(1), vec!["now we move to scene two", "and wrap up"]);
    // Segments carry their keyframe references and motion signal.
    for segment in &timeline.segments {
        assert!(segment.keyframe.is_some());
        assert!(segment.motion_density.is_some());
    }
    assert_eq!(load_timeline(&bundle).unwrap(), timeline);
    let manifest = load_manifest(&bundle).unwrap();
    assert_eq!(manifest.tracks.timeline.as_deref(), Some(TIMELINE_FILE));
}

#[test]
fn missing_transcript_yields_frames_only_timeline_with_gap_note() {
    let dir = scratch("no-transcript");
    let bundle = two_scene_bundle(&dir);
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    let timeline = assemble(&bundle).unwrap();

    assert_eq!(timeline.segments.len(), 2);
    assert!(timeline.segments.iter().all(|s| s.speech.is_empty()));
    assert!(
        timeline.notes.iter().any(|n| n.contains("transcript")),
        "notes: {:?}",
        timeline.notes
    );
}

#[test]
fn missing_frames_yields_transcript_only_timeline_with_gap_note() {
    let dir = scratch("no-frames");
    let bundle = two_scene_bundle(&dir);
    let engine = CannedEngine(vec![seg(0.5, 2.0, "audio only run")]);
    extract_audio_track(&bundle, &toolchain(), &engine, &mut |_| {}).unwrap();

    let timeline = assemble(&bundle).unwrap();

    assert_eq!(timeline.segments.len(), 1, "one whole-video segment");
    assert!(timeline.segments[0].keyframe.is_none());
    assert_eq!(timeline.segments[0].speech.len(), 1);
    assert!(
        timeline.notes.iter().any(|n| n.contains("frame")),
        "notes: {:?}",
        timeline.notes
    );
}

#[test]
fn timeline_is_deterministic_across_runs() {
    let dir = scratch("determinism");
    let bundle = two_scene_bundle(&dir);
    let engine = CannedEngine(vec![seg(0.5, 2.0, "hello")]);
    extract_audio_track(&bundle, &toolchain(), &engine, &mut |_| {}).unwrap();
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    assemble(&bundle).unwrap();
    let first = fs::read(bundle.join(TIMELINE_FILE)).unwrap();
    assemble(&bundle).unwrap();
    let second = fs::read(bundle.join(TIMELINE_FILE)).unwrap();

    assert_eq!(first, second);
}
