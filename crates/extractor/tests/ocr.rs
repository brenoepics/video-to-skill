//! The screen-text track: artifact shape, grep, and which keyframes the
//! track builder reads — platform-independent through a fake engine.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use vts_extract::bundle::{ingest, load_manifest};
use vts_extract::frames::{extract_frame_track, load_frame_track, FrameConfig};
use vts_extract::ocr::{
    self, extract_text_track, load_screen_text, save_screen_text, BBox, OcrEngine, Recognized,
    ScreenTextLine, ScreenTextTrack, SCREEN_TEXT_FILE,
};
use vts_extract::tools::Toolchain;

/// Two lines of terminal text rendered at 640x160 (see `fixtures/`).
const FIXTURE_PNG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/terminal.png");

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-ocr-tests")
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

/// Three seconds of the terminal fixture, then three seconds of solid
/// blue: two shots, text only in the first.
fn terminal_then_blue(dir: &Path) -> PathBuf {
    let out = dir.join("terminal.mp4");
    let res = Command::new("ffmpeg")
        .args(["-y", "-loop", "1", "-framerate", "10", "-t", "3", "-i"])
        .arg(FIXTURE_PNG)
        .args([
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=640x160:r=10:d=3",
            "-filter_complex",
            "[0:v]format=yuv420p[a];[1:v]format=yuv420p[b];[a][b]concat=n=2:v=1:a=0",
            "-c:v",
            "mpeg4",
            "-q:v",
            "2",
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "fixture generation failed: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    out
}

fn bundle_with_frames(dir: &Path) -> PathBuf {
    let media = terminal_then_blue(dir);
    let bundle = dir.join("bundle");
    ingest(&media, &bundle, &toolchain()).unwrap();
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();
    bundle
}

fn line(timestamp_secs: f64, frame: &str, text: &str) -> ScreenTextLine {
    ScreenTextLine {
        timestamp_secs,
        frame: frame.into(),
        bbox: BBox {
            x: 0.1,
            y: 0.2,
            w: 0.5,
            h: 0.1,
        },
        text: text.into(),
        confidence: Some(0.9),
        engine: "fake".into(),
    }
}

#[test]
fn screen_text_artifact_round_trips_through_disk() {
    let dir = scratch("roundtrip");
    let track = ScreenTextTrack {
        schema_version: 1,
        frames_read: 2,
        lines: vec![line(1.0, "frames/shot000.jpg", "cargo build --release")],
    };

    save_screen_text(&dir, &track).unwrap();

    assert!(dir.join(SCREEN_TEXT_FILE).is_file());
    assert_eq!(load_screen_text(&dir).unwrap(), track);
}

#[test]
fn grep_matches_case_insensitively_and_keeps_track_order() {
    let track = ScreenTextTrack {
        schema_version: 1,
        frames_read: 3,
        lines: vec![
            line(1.0, "frames/shot000.jpg", "$ cargo build --release"),
            line(4.0, "frames/shot001.jpg", "Compiling vts-extract"),
            line(9.0, "frames/shot002.jpg", "$ git push origin main"),
        ],
    };

    let hits = ocr::grep(&track, &ocr::compile_pattern("CARGO|push").unwrap());

    let texts: Vec<&str> = hits.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["$ cargo build --release", "$ git push origin main"]);
    assert!(
        ocr::compile_pattern("(").is_err(),
        "a malformed pattern is an error, never an empty result"
    );
}

#[test]
fn text_lines_are_tab_separated_records_the_agent_can_cut() {
    let l = line(754.5, "frames/shot012.jpg", "kubectl get pods");

    assert_eq!(
        ocr::format_line(&l),
        "t=754.500\tframes/shot012.jpg\tkubectl get pods"
    );
}

/// Records every image it is asked to read and answers with one line
/// naming that image, so tests can see which file the track chose.
struct FakeEngine {
    max_dimension: Option<u32>,
    reads: Mutex<Vec<PathBuf>>,
}

impl OcrEngine for FakeEngine {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn max_dimension(&self) -> Option<u32> {
        self.max_dimension
    }

    fn read(&self, image: &Path) -> anyhow::Result<Vec<Recognized>> {
        self.reads.lock().unwrap().push(image.to_path_buf());
        let name = image.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with("shot001") {
            return Ok(Vec::new());
        }
        Ok(vec![Recognized {
            bbox: BBox {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 0.5,
            },
            text: format!("read {name}"),
            confidence: Some(0.5),
        }])
    }
}

#[test]
fn the_track_reads_native_stills_and_reports_the_agent_facing_keyframe() {
    let dir = scratch("native");
    let bundle = bundle_with_frames(&dir);
    let engine = FakeEngine {
        max_dimension: None,
        reads: Mutex::new(Vec::new()),
    };

    let track = extract_text_track(&bundle, &engine).unwrap();

    assert_eq!(track.frames_read, 2, "both shots are read, text or not");
    let reads = engine.reads.lock().unwrap();
    assert!(
        reads
            .iter()
            .all(|p| p.to_string_lossy().contains("-native.jpg")),
        "native stills carry the most legible text: {reads:?}"
    );
    assert_eq!(track.lines.len(), 1);
    let only = &track.lines[0];
    assert_eq!(only.frame, "frames/shot000.jpg");
    assert_eq!(only.text, "read shot000-native.jpg");
    assert_eq!(only.engine, "fake");
    let keyframe = &load_frame_track(&bundle).unwrap().shots[0].keyframe;
    assert!((only.timestamp_secs - keyframe.timestamp_secs).abs() < 1e-9);
    assert_eq!(load_screen_text(&bundle).unwrap(), track);
}

#[test]
fn the_track_falls_back_to_bounded_keyframes_when_the_native_still_is_too_large() {
    let dir = scratch("bounded");
    let bundle = bundle_with_frames(&dir);
    // The media is 640 px wide; an engine capped at 320 cannot take it.
    let engine = FakeEngine {
        max_dimension: Some(320),
        reads: Mutex::new(Vec::new()),
    };

    let track = extract_text_track(&bundle, &engine).unwrap();

    let reads = engine.reads.lock().unwrap();
    assert!(
        reads
            .iter()
            .all(|p| !p.to_string_lossy().contains("native")),
        "{reads:?}"
    );
    assert_eq!(track.lines[0].text, "read shot000.jpg");
}

#[test]
fn a_missing_engine_is_recorded_in_the_manifest_and_leaves_no_artifact() {
    let dir = scratch("unavailable");
    let bundle = bundle_with_frames(&dir);
    let reason = "no local OCR engine on this platform";

    let note = ocr::record_unavailable(&bundle, reason).unwrap();
    ocr::record_unavailable(&bundle, reason).unwrap();

    assert_eq!(note, format!("screen_text: {reason}"));
    let notes = load_manifest(&bundle).unwrap().notes;
    assert_eq!(
        notes.iter().filter(|n| **n == note).count(),
        1,
        "re-running never duplicates the note: {notes:?}"
    );
    assert!(!bundle.join(SCREEN_TEXT_FILE).exists());
}
