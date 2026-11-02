//! Voice activity detection keeps transcript timestamps in absolute media
//! time: speech after a long pause must land after the pause, for segments
//! and for words. Real whisper + Silero weights, so ignored by default;
//! run with `cargo test --test asr_vad -- --ignored`.

// The fixture is spoken by macOS `say`, so nothing here compiles elsewhere.
#![cfg(target_os = "macos")]
// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::asr::{extract_audio_track, whisper::WhisperEngine};
use vts_extract::bundle::{ingest, load_manifest, Transcript};
use vts_extract::deps::Env;
use vts_extract::tools::Toolchain;

const OPENING: &str = "Welcome to the tutorial.";
const CLOSING: &str = "Now open the terminal and run the build.";
const PAUSE_SECS: f64 = 40.0;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-asr-vad-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(cmd: &mut Command) {
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `say` speech, 40 s of digital silence, then different `say` speech,
/// muxed under a black video so the bundle pipeline accepts it.
fn pauses_fixture(dir: &Path) -> PathBuf {
    let opening = dir.join("opening.aiff");
    let closing = dir.join("closing.aiff");
    run(Command::new("say").arg("-o").arg(&opening).arg(OPENING));
    run(Command::new("say").arg("-o").arg(&closing).arg(CLOSING));
    let wav = dir.join("pauses.wav");
    run(Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(&opening)
        .args(["-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono", "-i"])
        .arg(&closing)
        .args([
            "-filter_complex",
            "[1]atrim=0:40[s];[0][s][2]concat=n=3:v=0:a=1",
            "-ar",
            "16000",
        ])
        .arg(&wav));
    let mp4 = dir.join("pauses.mp4");
    run(Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(["-f", "lavfi", "-i", "color=c=black:s=320x240:r=10", "-i"])
        .arg(&wav)
        .args(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
        .arg(&mp4));
    mp4
}

fn heard(transcript: &Transcript) -> String {
    transcript
        .segments
        .iter()
        .map(|s| s.text.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
#[ignore = "requires whisper + VAD model weights and macOS `say`"]
fn speech_after_a_long_pause_keeps_absolute_timestamps_with_vad() {
    let tools = Toolchain::resolve(&Env::from_system()).unwrap();
    let engine =
        WhisperEngine::from_toolchain(&tools).expect("run `vts-extract check --fix --yes` first");
    assert!(
        engine.vad_model().is_some(),
        "VAD weights must be installed"
    );

    let dir = scratch("pauses");
    let bundle = dir.join("bundle");
    ingest(&pauses_fixture(&dir), &bundle, &tools).unwrap();
    let duration = load_manifest(&bundle).unwrap().media.duration_secs;
    assert!(duration > PAUSE_SECS + 2.0, "fixture is {duration}s");

    let transcript = extract_audio_track(&bundle, &tools, &engine, &mut |_| {}).unwrap();

    let text = heard(&transcript);
    assert!(text.contains("tutorial"), "heard: {text}");
    assert!(text.contains("build"), "heard: {text}");
    let words: Vec<_> = transcript.segments.iter().flat_map(|s| &s.words).collect();
    assert!(!words.is_empty(), "expected word-level timestamps");
    assert!(
        words.iter().any(|w| w.start_secs < 5.0),
        "the opening is spoken at the start: {words:?}"
    );
    assert!(
        words.iter().any(|w| w.start_secs >= PAUSE_SECS),
        "the closing must land after the {PAUSE_SECS}s pause, not be shifted \
         into trimmed time: {words:?}"
    );
    let opening = transcript
        .segments
        .iter()
        .find(|s| s.text.to_lowercase().contains("tutorial"))
        .unwrap();
    let closing = transcript
        .segments
        .iter()
        .find(|s| s.text.to_lowercase().contains("build"))
        .unwrap();
    assert!(opening.end_secs < 5.0, "opening segment: {opening:?}");
    assert!(
        closing.start_secs >= PAUSE_SECS,
        "the closing must be its own segment after the pause, not merged \
         into the opening: {closing:?}"
    );
    assert!(
        closing.end_secs <= duration + 1.0,
        "timestamps stay within the media ({duration}s): {closing:?}"
    );
    assert!(
        words.windows(2).all(|w| w[0].start_secs <= w[1].start_secs),
        "word times must be monotonic: {words:?}"
    );
}
