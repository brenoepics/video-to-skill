//! Platform priors in the timeline: chapters as section hints,
//! `SponsorBlock` ranges as skips, and which source the speech came from.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::asr::{extract_audio_track, EngineOutput, TranscriptionEngine};
use vts_extract::bundle::{
    extract_caption_track, ingest, load_captions, load_manifest, save_manifest, CaptionTrack,
    Chapter, Segment, SkipRange, TranscriptSource,
};
use vts_extract::frames::{extract_frame_track, FrameConfig};
use vts_extract::timeline::{assemble, load_timeline, TIMELINE_FILE};
use vts_extract::tools::Toolchain;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-timeline-priors-tests")
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
        whisper_vad: None,
    }
}

/// Two solid scenes (0-3s red, 3-6s blue) with an audio track, with the
/// frame track already extracted: two shots, two timeline segments.
fn two_shot_bundle(dir: &Path) -> PathBuf {
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
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();
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

fn chapter(start: f64, end: f64, title: &str) -> Chapter {
    Chapter {
        start_secs: start,
        end_secs: end,
        title: title.into(),
    }
}

fn skip(start: f64, end: f64, category: &str) -> SkipRange {
    SkipRange {
        start_secs: start,
        end_secs: end,
        category: category.into(),
    }
}

#[test]
fn chapters_and_skips_reach_the_timeline_and_flag_mostly_skipped_segments() {
    let dir = scratch("priors");
    let bundle = two_shot_bundle(&dir);
    let mut manifest = load_manifest(&bundle).unwrap();
    manifest.platform.chapters = vec![chapter(0.0, 3.0, "Intro"), chapter(3.0, 6.0, "Install")];
    // Half a second of the first shot and all of the second are sponsor.
    manifest.platform.skips = vec![skip(2.5, 6.0, "sponsor")];
    save_manifest(&bundle, &manifest).unwrap();
    let engine = CannedEngine(vec![seg(0.5, 2.0, "welcome"), seg(4.0, 5.0, "buy this")]);
    extract_audio_track(&bundle, &toolchain(), &engine, &mut |_| {}).unwrap();

    let timeline = assemble(&bundle).unwrap();

    assert_eq!(timeline.chapters, manifest.platform.chapters);
    assert_eq!(timeline.skips, manifest.platform.skips);
    assert_eq!(timeline.segments.len(), 2);
    assert_eq!(
        timeline.segments[0].skip, None,
        "a segment that is mostly content is kept"
    );
    assert_eq!(
        timeline.segments[1].skip.as_deref(),
        Some("sponsor"),
        "a segment that lies mostly inside a SponsorBlock range is skippable"
    );
    assert_eq!(
        timeline.segments[1].speech.len(),
        1,
        "its speech is still there for the record"
    );
    assert_eq!(timeline.transcript_source, Some(TranscriptSource::Whisper));
    assert_eq!(load_timeline(&bundle).unwrap(), timeline);
}

#[test]
fn a_bundle_without_priors_yields_empty_hints_and_no_skip_keys() {
    let dir = scratch("no-priors");
    let bundle = two_shot_bundle(&dir);

    let timeline = assemble(&bundle).unwrap();

    assert_eq!(timeline.chapters, vec![]);
    assert_eq!(timeline.skips, vec![]);
    assert!(timeline.segments.iter().all(|s| s.skip.is_none()));
    assert_eq!(timeline.transcript_source, None);
    let raw = fs::read_to_string(bundle.join(TIMELINE_FILE)).unwrap();
    assert!(
        !raw.contains("\"skip\""),
        "token-lean: no skip key on segments that are not skippable"
    );
    assert!(raw.contains("\"chapters\": []"), "{raw}");
}

#[test]
fn a_creator_caption_transcript_is_named_as_the_timeline_source() {
    let dir = scratch("captions-source");
    let bundle = two_shot_bundle(&dir);
    fs::write(
        bundle.join("source.en.vtt"),
        "WEBVTT\n\n00:00:00.500 --> 00:00:02.000\nwelcome to the tutorial\n",
    )
    .unwrap();
    let mut manifest = load_manifest(&bundle).unwrap();
    manifest.platform.captions = Some(CaptionTrack {
        path: "source.en.vtt".into(),
        language: "en".into(),
        format: "vtt".into(),
        available_languages: vec!["en".into()],
    });
    save_manifest(&bundle, &manifest).unwrap();
    let cues = load_captions(&bundle).unwrap().unwrap();
    extract_caption_track(&bundle, &cues, None).unwrap();

    let timeline = assemble(&bundle).unwrap();

    assert_eq!(
        timeline.transcript_source,
        Some(TranscriptSource::CreatorCaptions)
    );
    assert_eq!(timeline.language.as_deref(), Some("en"));
    assert_eq!(
        timeline.segments[0].speech[0].text,
        "welcome to the tutorial"
    );
}
