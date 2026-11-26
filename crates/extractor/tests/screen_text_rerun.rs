//! Re-extracting into an existing bundle: the screen text track is the
//! current run's or absent, never a leftover from an earlier run. An
//! earlier run's track is planted, since only macOS and Windows have an
//! engine to build one.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use vts_extract::bundle::load_manifest;
use vts_extract::ocr::{
    self, load_screen_text, save_screen_text, BBox, ScreenTextLine, ScreenTextTrack,
    SCREEN_TEXT_FILE,
};

const STALE: &str = "left over from an earlier run";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-screen-text-rerun")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn vts(data_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vts-extract"))
        .args(args)
        .env("VTS_DATA_DIR", data_dir)
        .output()
        .unwrap()
}

fn extract(data_dir: &Path, media: &Path, bundle: &Path) {
    let out = vts(
        data_dir,
        &[
            "extract",
            media.to_str().unwrap(),
            "--out",
            bundle.to_str().unwrap(),
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Four seconds of a test pattern.
fn short_video(dir: &Path) -> PathBuf {
    let media = dir.join("pattern.mp4");
    let res = Command::new("ffmpeg")
        .args(["-y", "-f", "lavfi", "-i"])
        .arg("testsrc=size=320x240:rate=10:duration=4")
        .args(["-c:v", "mpeg4", "-q:v", "5"])
        .arg(&media)
        .output()
        .unwrap();
    assert!(res.status.success());
    media
}

/// The track an earlier run with an engine would have left.
fn earlier_track() -> ScreenTextTrack {
    ScreenTextTrack {
        schema_version: 1,
        frames_read: 1,
        lines: vec![ScreenTextLine {
            timestamp_secs: 1.0,
            frame: "frames/shot000.jpg".to_string(),
            bbox: BBox {
                x: 0.1,
                y: 0.1,
                w: 0.5,
                h: 0.1,
            },
            text: STALE.to_string(),
            confidence: Some(0.9),
            engine: "earlier-engine".to_string(),
        }],
    }
}

#[test]
fn a_rerun_never_serves_an_earlier_runs_screen_text() {
    let dir = scratch("rerun");
    let data = dir.join("data");
    let media = short_video(&dir);
    let bundle = dir.join("bundle");
    extract(&data, &media, &bundle);
    save_screen_text(&bundle, &earlier_track()).unwrap();

    extract(&data, &media, &bundle);

    if let Ok(track) = load_screen_text(&bundle) {
        assert!(
            track.lines.iter().all(|line| line.text != STALE),
            "stale screen text: {track:?}"
        );
    }
    // Fails when the run built no track; either way, nothing stale.
    let text = vts(&data, &["text", bundle.to_str().unwrap(), "--grep", STALE]);
    assert!(!String::from_utf8_lossy(&text.stdout).contains(STALE));
    if ocr::platform_engine(&[]).is_err() {
        assert!(!bundle.join(SCREEN_TEXT_FILE).exists());
        let notes = load_manifest(&bundle).unwrap().notes;
        let reasons = notes.iter().filter(|n| n.starts_with("screen_text: "));
        assert_eq!(reasons.count(), 1, "{notes:?}");
    }
}
