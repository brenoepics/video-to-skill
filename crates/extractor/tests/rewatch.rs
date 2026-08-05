//! Re-watch tools: frame-at and clip, verified against a fixture whose
//! per-second content (red → green → blue) is known by construction.

// Integration-test file: everything here is test code, where unwrap is fine.
// float_cmp: parsing tests compare exactly-representable literals on purpose.
#![allow(clippy::unwrap_used, clippy::float_cmp)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::bundle::ingest;
use vts_extract::rewatch::{clip, frame_at, parse_timestamp};
use vts_extract::tools::Toolchain;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-rewatch-tests")
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

/// red 0-3s, green 3-6s, blue 6-9s.
fn color_bundle(dir: &Path) -> PathBuf {
    let fixture = dir.join("colors.mp4");
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
            "color=c=lime:s=320x240:r=10:d=3",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x240:r=10:d=3",
            "-filter_complex",
            "[0][1][2]concat=n=3",
            "-c:v",
            "mpeg4",
        ])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(res.status.success());
    let bundle = dir.join("bundle");
    ingest(&fixture, &bundle, &toolchain()).unwrap();
    bundle
}

/// Dominant RGB channel of an exported JPEG (via ffmpeg 1x1 downscale).
fn dominant_channel(jpg: &Path) -> usize {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(jpg)
        .args([
            "-vf",
            "scale=1x1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .unwrap();
    let px = &out.stdout[..3];
    (0..3).max_by_key(|&i| px[i]).unwrap()
}

#[test]
fn timestamps_parse_in_all_supported_forms() {
    assert_eq!(parse_timestamp("90").unwrap(), 90.0);
    assert_eq!(parse_timestamp("90.5").unwrap(), 90.5);
    assert_eq!(parse_timestamp("12:34").unwrap(), 754.0);
    assert_eq!(parse_timestamp("2:03.5").unwrap(), 123.5);
    assert_eq!(parse_timestamp("1:02:03").unwrap(), 3723.0);
}

#[test]
fn malformed_timestamps_are_rejected() {
    for bad in ["", "abc", "1:2:3:4", "-5", "12:", ":30", "1:xx"] {
        assert!(parse_timestamp(bad).is_err(), "accepted {bad:?}");
    }
}

#[test]
fn frame_at_returns_the_frame_at_that_moment_from_source_media() {
    let dir = scratch("frame-at");
    let bundle = color_bundle(&dir);
    let tc = toolchain();

    let red = frame_at(&bundle, &tc, "1").unwrap();
    let green = frame_at(&bundle, &tc, "4.5").unwrap();
    let blue = frame_at(&bundle, &tc, "0:07").unwrap();

    assert_eq!(dominant_channel(&red.path), 0, "1s should be red");
    assert_eq!(dominant_channel(&green.path), 1, "4.5s should be green");
    assert_eq!(dominant_channel(&blue.path), 2, "7s should be blue");
    assert!((green.timestamp_secs - 4.5).abs() < f64::EPSILON);
    // Namespaced under the bundle's scratch area, disposable.
    assert!(red.path.starts_with(bundle.join("rewatch")));
}

#[test]
fn frame_at_rejects_out_of_range_timestamps() {
    let dir = scratch("range");
    let bundle = color_bundle(&dir);

    let err = frame_at(&bundle, &toolchain(), "99").unwrap_err();

    assert!(err.to_string().contains("beyond"), "got: {err}");
}

#[test]
fn clip_samples_the_range_at_the_requested_fps() {
    let dir = scratch("clip");
    let bundle = color_bundle(&dir);

    let result = clip(&bundle, &toolchain(), "2", "5", 2.0).unwrap();

    assert!(!result.truncated);
    assert_eq!(result.frames.len(), 6, "3s at 2fps");
    assert!((result.frames[0].timestamp_secs - 2.0).abs() < f64::EPSILON);
    assert!((result.frames[1].timestamp_secs - 2.5).abs() < f64::EPSILON);
    for frame in &result.frames {
        assert!(frame.path.is_file());
    }
    // Spans the cut: first frame red, last frame green.
    assert_eq!(dominant_channel(&result.frames[0].path), 0);
    assert_eq!(dominant_channel(&result.frames.last().unwrap().path), 1);
}

#[test]
fn clip_caps_the_frame_count_and_flags_truncation() {
    let dir = scratch("cap");
    let bundle = color_bundle(&dir);

    let result = clip(&bundle, &toolchain(), "0", "9", 30.0).unwrap();

    assert!(result.truncated, "270 requested frames must hit the cap");
    assert_eq!(result.frames.len(), 60, "documented cap");
}
