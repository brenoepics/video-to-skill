//! Shot detection on B-frame h264: the thumbnail sampler skips
//! non-reference frames, so this pins that boundaries, keyframes and
//! `frames.json` stay correct and deterministic on such content.
//! Skipped (with a note) where ffmpeg has no libx264 encoder.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::bundle::ingest;
use vts_extract::frames::{extract_frame_track, FrameConfig, FRAMES_FILE};
use vts_extract::tools::Toolchain;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-frames-bframes-tests")
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

fn has_libx264() -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(" libx264 "))
}

/// Three solid colors, 3 s each, as h264 with three B-frames between
/// references and a short GOP — the shape of a real screen recording.
fn three_cut_bframe_fixture(dir: &Path) -> PathBuf {
    let out = dir.join("cuts-bframes.mp4");
    let res = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(["-f", "lavfi", "-i", "color=c=red:s=320x240:r=30:d=3"])
        .args(["-f", "lavfi", "-i", "color=c=green:s=320x240:r=30:d=3"])
        .args(["-f", "lavfi", "-i", "color=c=blue:s=320x240:r=30:d=3"])
        .args(["-filter_complex", "[0][1][2]concat=n=3"])
        .args([
            "-c:v", "libx264", "-preset", "veryfast", "-bf", "3", "-g", "30",
        ])
        .args(["-pix_fmt", "yuv420p"])
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

#[test]
fn b_frame_content_yields_the_same_shots_deterministically() {
    if !has_libx264() {
        eprintln!("skipping: ffmpeg has no libx264 encoder");
        return;
    }
    let dir = scratch("cuts");
    let video = three_cut_bframe_fixture(&dir);
    let bundle = dir.join("bundle");
    ingest(&video, &bundle, &toolchain()).unwrap();

    let track = extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();
    let first = fs::read(bundle.join(FRAMES_FILE)).unwrap();
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();
    let second = fs::read(bundle.join(FRAMES_FILE)).unwrap();

    assert_eq!(track.shots.len(), 3, "one shot per solid color");
    assert!((track.shots[1].start_secs - 3.0).abs() <= 1.0);
    assert!((track.shots[2].start_secs - 6.0).abs() <= 1.0);
    for shot in &track.shots {
        assert!(bundle.join(&shot.keyframe.path).is_file());
    }
    assert_eq!(first, second, "frames.json must be byte-identical");
}
