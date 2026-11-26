//! Real whisper weights on macOS `say` speech: what the engine hears, and
//! how close its word timestamps land to where the words were spoken.
//! Ignored by default (needs `say`, ffmpeg, and the bootstrapped whisper +
//! VAD models); run with `cargo test --test asr_whisper -- --ignored`.

// The fixture is spoken by macOS `say`, so nothing here compiles elsewhere.
#![cfg(target_os = "macos")]
// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::asr::{extract_audio_track, whisper::WhisperEngine};
use vts_extract::bundle::{ingest, Word};
use vts_extract::deps::Env;
use vts_extract::tools::Toolchain;

/// Spoken one after another, each followed by `GAP_SECS` of silence. Each
/// opens with a word no other phrase contains, so the transcript's words
/// can be dealt back to the phrase that spoke them.
const PHRASES: [&str; 4] = [
    "Welcome to the tutorial.",
    "First, open the terminal.",
    "Then run the build.",
    "Finally, check the output.",
];
const GAP_SECS: f64 = 1.5;
/// How far a phrase's first word may start from the moment `say` spoke
/// it, and how far any word may stray outside its phrase. whisper's
/// heuristic lands the onsets within 0.13 s on this fixture; a DTW time
/// leaking across the cut pause misplaces a word by over a second.
const TOLERANCE_SECS: f64 = 0.25;

/// One phrase as `say` spoke it: when it starts and how long it lasts.
struct Spoken {
    onset: f64,
    secs: f64,
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-asr-whisper-tests")
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

/// Seconds of audio in a wav file.
fn wav_secs(wav: &Path) -> f64 {
    let reader = hound::WavReader::open(wav).unwrap();
    f64::from(reader.duration()) / f64::from(reader.spec().sample_rate)
}

/// The phrases as `say` speaks them, each padded with `GAP_SECS` of
/// silence, concatenated and muxed under a black video. `say` begins
/// speaking at once, so a phrase starts where the padded phrases before
/// it end, and lasts its unpadded length.
fn phrases_fixture(dir: &Path) -> (PathBuf, Vec<Spoken>) {
    let mut spoken = Vec::new();
    let mut padded = Vec::new();
    let mut at = 0.0;
    for (i, phrase) in PHRASES.iter().enumerate() {
        let aiff = dir.join(format!("phrase-{i}.aiff"));
        run(Command::new("say").arg("-o").arg(&aiff).arg(phrase));
        let wav = dir.join(format!("phrase-{i}.wav"));
        run(Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-i"])
            .arg(&aiff)
            .args(["-ar", "16000", "-ac", "1", "-af"])
            .arg(format!("apad=pad_dur={GAP_SECS}"))
            .arg(&wav));
        let secs = wav_secs(&wav);
        spoken.push(Spoken {
            onset: at,
            secs: secs - GAP_SECS,
        });
        at += secs;
        padded.push(wav);
    }
    let speech = dir.join("speech.wav");
    let mut concat = Command::new("ffmpeg");
    concat.args(["-y", "-v", "error"]);
    for wav in &padded {
        concat.arg("-i").arg(wav);
    }
    let mut filter = String::new();
    for i in 0..padded.len() {
        write!(filter, "[{i}]").unwrap();
    }
    write!(filter, "concat=n={}:v=0:a=1", padded.len()).unwrap();
    run(concat.arg("-filter_complex").arg(&filter).arg(&speech));
    let mp4 = dir.join("speech.mp4");
    run(Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(["-f", "lavfi", "-i", "color=c=black:s=320x240:r=10", "-i"])
        .arg(&speech)
        .args(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
        .arg(&mp4));
    (mp4, spoken)
}

/// Where each phrase's first word sits in `words` — whisper may split a
/// word into sub-word tokens, so a token that opens the word counts.
fn phrase_starts(words: &[&Word]) -> Vec<usize> {
    let heard = |w: &Word| w.text.trim().to_lowercase();
    PHRASES
        .iter()
        .map(|phrase| {
            let first = phrase
                .split_whitespace()
                .next()
                .unwrap()
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            words
                .iter()
                .position(|w| heard(w) == first)
                .or_else(|| {
                    words
                        .iter()
                        .position(|w| heard(w).len() >= 3 && first.starts_with(&heard(w)))
                })
                .unwrap_or_else(|| panic!("{first:?} not heard: {words:?}"))
        })
        .collect()
}

#[test]
#[ignore = "requires whisper + VAD model weights and macOS `say`"]
fn real_whisper_hears_say_speech_and_times_its_words() {
    let tools = Toolchain::resolve(&Env::from_system()).unwrap();
    let engine =
        WhisperEngine::from_toolchain(&tools).expect("run `vts-extract check --fix --yes` first");
    assert!(
        engine.vad_model().is_some(),
        "VAD weights must be installed"
    );
    let dir = scratch("phrases");
    let (fixture, spoken) = phrases_fixture(&dir);
    let bundle = dir.join("bundle");
    ingest(&fixture, &bundle, &tools).unwrap();

    let transcript = extract_audio_track(&bundle, &tools, &engine, &mut |_| {}).unwrap();

    let heard = transcript
        .segments
        .iter()
        .map(|s| s.text.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(heard.contains("tutorial"), "heard: {heard}");
    assert!(heard.contains("terminal"), "heard: {heard}");
    assert_eq!(transcript.language.as_deref(), Some("en"));
    let words: Vec<&Word> = transcript.segments.iter().flat_map(|s| &s.words).collect();
    assert!(!words.is_empty(), "expected word-level timestamps");
    assert!(
        words.iter().all(|w| w.start_secs <= w.end_secs),
        "a word never ends before it starts: {words:?}"
    );
    assert!(
        words.windows(2).all(|w| w[0].start_secs <= w[1].start_secs),
        "word starts must be monotonic: {words:?}"
    );
    let starts = phrase_starts(&words);
    assert!(
        starts.windows(2).all(|s| s[0] < s[1]),
        "phrases are heard in order: {starts:?} in {words:?}"
    );
    let mut misplaced = Vec::new();
    for (i, (phrase, said)) in PHRASES.iter().zip(&spoken).enumerate() {
        let from = starts[i];
        let to = starts.get(i + 1).copied().unwrap_or(words.len());
        let first = words[from];
        let drift = first.start_secs - said.onset;
        println!(
            "{:>10} spoken at {:6.2}s, heard at {:6.2}s ({drift:+.2}s)",
            first.text.trim(),
            said.onset,
            first.start_secs
        );
        if drift.abs() > TOLERANCE_SECS {
            misplaced.push(format!(
                "{:?} spoken at {:.2}s, heard at {:.2}s",
                first.text.trim(),
                said.onset,
                first.start_secs
            ));
        }
        let (earliest, latest) = (
            said.onset - TOLERANCE_SECS,
            said.onset + said.secs + TOLERANCE_SECS,
        );
        // Punctuation carries no provenance, and whisper stamps a closing
        // "." with the segment's end, which VAD's remap can push past the
        // pause; only spoken words must stay inside their phrase.
        let spoken_words = words[from..to]
            .iter()
            .filter(|w| w.text.contains(char::is_alphanumeric));
        for word in spoken_words {
            if word.start_secs < earliest || word.start_secs > latest {
                misplaced.push(format!(
                    "{:?} heard at {:.2}s, outside {phrase:?} ({:.2}–{:.2}s)",
                    word.text.trim(),
                    word.start_secs,
                    said.onset,
                    said.onset + said.secs
                ));
            }
        }
    }
    assert!(
        misplaced.is_empty(),
        "words must land within {TOLERANCE_SECS}s of where `say` spoke them: {misplaced:#?}\nall words: {words:?}"
    );
}
