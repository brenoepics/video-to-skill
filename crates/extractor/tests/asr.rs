//! Audio track: demux + transcription orchestration, engine mocked.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::asr::{extract_audio_track, EngineOutput, TranscriptionEngine};
use vts_extract::bundle::{ingest, load_manifest, load_transcript, Segment, Word};
use vts_extract::tools::Toolchain;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-asr-tests")
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

fn bundle_with_fixture(dir: &Path, audio: bool) -> PathBuf {
    let fixture = dir.join("fixture.mp4");
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc=duration=2:size=320x240:rate=10",
    ]);
    if audio {
        cmd.args(["-f", "lavfi", "-i", "sine=frequency=440:duration=2"]);
        cmd.args(["-c:a", "aac", "-shortest"]);
    }
    cmd.args(["-c:v", "mpeg4"]).arg(&fixture);
    assert!(cmd.output().unwrap().status.success());
    let bundle = dir.join("bundle");
    ingest(&fixture, &bundle, &toolchain()).unwrap();
    bundle
}

/// Canned engine; panics if speech recognition is attempted when it must not be.
struct FakeEngine {
    output: Option<EngineOutput>,
}

impl TranscriptionEngine for FakeEngine {
    fn transcribe(
        &self,
        _wav: &Path,
        progress: &mut dyn FnMut(u8),
    ) -> anyhow::Result<EngineOutput> {
        let output = self.output.as_ref().expect("engine must not be called");
        progress(10);
        progress(100);
        Ok(output.clone())
    }
}

fn speechful_output() -> EngineOutput {
    EngineOutput {
        language: Some("en".into()),
        segments: vec![
            Segment {
                start_secs: 0.0,
                end_secs: 1.0,
                text: "hello world".into(),
                words: vec![
                    Word {
                        start_secs: 0.0,
                        end_secs: 0.4,
                        text: "hello".into(),
                    },
                    Word {
                        start_secs: 0.5,
                        end_secs: 1.0,
                        text: "world".into(),
                    },
                ],
            },
            Segment {
                start_secs: 1.0,
                end_secs: 2.0,
                text: "from the test".into(),
                words: vec![],
            },
        ],
    }
}

#[test]
fn video_without_audio_yields_empty_transcript_with_note_and_no_engine_call() {
    let dir = scratch("no-audio");
    let bundle = bundle_with_fixture(&dir, false);
    let engine = FakeEngine { output: None };

    let transcript = extract_audio_track(&bundle, &toolchain(), &engine, &mut |_| {}).unwrap();

    assert!(transcript.segments.is_empty());
    let manifest = load_manifest(&bundle).unwrap();
    assert_eq!(
        manifest.tracks.transcript.as_deref(),
        Some("transcript.json")
    );
    assert!(
        manifest.notes.iter().any(|n| n.contains("no audio")),
        "notes: {:?}",
        manifest.notes
    );
    // The artifact on disk round-trips.
    assert_eq!(load_transcript(&bundle).unwrap(), transcript);
}

#[test]
fn engine_output_lands_in_transcript_and_language_in_manifest() {
    let dir = scratch("speech");
    let bundle = bundle_with_fixture(&dir, true);
    let engine = FakeEngine {
        output: Some(speechful_output()),
    };

    let transcript = extract_audio_track(&bundle, &toolchain(), &engine, &mut |_| {}).unwrap();

    assert_eq!(transcript.schema_version, 1);
    assert_eq!(transcript.language.as_deref(), Some("en"));
    assert_eq!(transcript.segments.len(), 2);
    assert_eq!(transcript.segments[0].words.len(), 2);
    assert_eq!(load_transcript(&bundle).unwrap(), transcript);
    let manifest = load_manifest(&bundle).unwrap();
    assert_eq!(manifest.media.language.as_deref(), Some("en"));
    assert_eq!(
        manifest.tracks.transcript.as_deref(),
        Some("transcript.json")
    );
}

#[test]
fn speechless_audio_gets_a_no_speech_note() {
    let dir = scratch("silence");
    let bundle = bundle_with_fixture(&dir, true);
    let engine = FakeEngine {
        output: Some(EngineOutput {
            language: None,
            segments: vec![],
        }),
    };

    let transcript = extract_audio_track(&bundle, &toolchain(), &engine, &mut |_| {}).unwrap();

    assert!(transcript.segments.is_empty());
    let manifest = load_manifest(&bundle).unwrap();
    assert!(
        manifest.notes.iter().any(|n| n.contains("no speech")),
        "notes: {:?}",
        manifest.notes
    );
}

/// Real end-to-end: macOS-generated speech through actual whisper weights.
/// Ignored by default (needs `say`, ffmpeg, and the bootstrapped model);
/// run with: cargo test --test asr -- --ignored
#[test]
#[ignore = "requires whisper model weights and macOS `say`"]
fn real_whisper_transcribes_generated_speech() {
    use vts_extract::asr::whisper::WhisperEngine;
    use vts_extract::deps::{probe, Env, Presence, Tool};

    let env = Env::from_system();
    let model = probe(&env)
        .into_iter()
        .find(|s| s.tool == Tool::WhisperModel && s.presence == Presence::Installed)
        .and_then(|s| s.location)
        .expect("run `vts-extract check --fix` first");

    let dir = scratch("real-whisper");
    let aiff = dir.join("speech.aiff");
    let status = Command::new("say")
        .args(["-o"])
        .arg(&aiff)
        .arg("Welcome to the tutorial. First, open the terminal.")
        .output()
        .unwrap();
    assert!(status.status.success());
    let fixture = dir.join("speech.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=320x240:r=10",
            "-i",
        ])
        .arg(&aiff)
        .args(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(status.status.success());
    let bundle = dir.join("bundle");
    ingest(&fixture, &bundle, &toolchain()).unwrap();

    let transcript = extract_audio_track(
        &bundle,
        &toolchain(),
        &WhisperEngine::new(model),
        &mut |_| {},
    )
    .unwrap();

    let heard = transcript
        .segments
        .iter()
        .map(|s| s.text.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(heard.contains("tutorial"), "heard: {heard}");
    assert!(heard.contains("terminal"), "heard: {heard}");
    assert_eq!(transcript.language.as_deref(), Some("en"));
    assert!(
        transcript.segments.iter().any(|s| !s.words.is_empty()),
        "expected word-level timestamps"
    );
}

#[test]
fn engine_progress_reaches_the_caller() {
    let dir = scratch("progress");
    let bundle = bundle_with_fixture(&dir, true);
    let engine = FakeEngine {
        output: Some(speechful_output()),
    };
    let mut seen: Vec<u8> = vec![];

    extract_audio_track(&bundle, &toolchain(), &engine, &mut |p| seen.push(p)).unwrap();

    assert_eq!(seen, vec![10, 100]);
}
