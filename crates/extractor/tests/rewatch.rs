//! Re-watch tools: frame-at and clip, verified against a fixture whose
//! per-second content (red → green → blue) is known by construction.

// Integration-test file: everything here is test code, where unwrap is fine.
// float_cmp: parsing tests compare exactly-representable literals on purpose.
#![allow(clippy::unwrap_used, clippy::float_cmp)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::bundle::ingest;
use vts_extract::compile::{compile, Evidence, ProcedureIr, SourceRef, Step};
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

fn basename(p: &Path) -> &str {
    p.file_name().and_then(|n| n.to_str()).unwrap()
}

#[test]
fn frame_at_exports_carry_timestamp_derived_basenames() {
    let dir = scratch("at-names");
    let bundle = color_bundle(&dir);
    let tc = toolchain();

    let a = frame_at(&bundle, &tc, "1").unwrap();
    let b = frame_at(&bundle, &tc, "4.5").unwrap();

    // Never "frame.jpg": the compiler flattens evidence by basename.
    assert_eq!(basename(&a.path), "at-000001000.jpg");
    assert_eq!(basename(&b.path), "at-000004500.jpg");
}

#[test]
fn clip_frames_carry_request_unique_basenames() {
    let dir = scratch("clip-names");
    let bundle = color_bundle(&dir);
    let tc = toolchain();

    let first = clip(&bundle, &tc, "2", "3", 2.0).unwrap().frames;
    let second = clip(&bundle, &tc, "2", "4", 2.0).unwrap().frames;

    assert_eq!(
        basename(&first[0].path),
        "clip-000002000-000003000-2000-f000.jpg"
    );
    // Overlapping requests must still never share a basename.
    let mut names: Vec<&str> = first
        .iter()
        .chain(&second)
        .map(|f| basename(&f.path))
        .collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), total, "clip basenames must be globally unique");
}

/// Minimal valid IR whose single step cites the given bundle-relative frames.
fn ir_citing(frames: Vec<String>) -> ProcedureIr {
    ProcedureIr {
        schema_version: 1,
        skill_name: "color-cuts".into(),
        title: "Color Cuts".into(),
        description: "Regression fixture: every cited frame must survive packaging.".into(),
        overview: "Fixture.".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: "colors.mp4".into(),
            title: None,
            duration_secs: 9.0,
        },
        steps: vec![Step {
            id: "01-watch-colors".into(),
            goal: "Cite mixed keyframe and re-watch evidence".into(),
            actions: "Watch the colors change.".into(),
            success_criteria: "All cited frames land in the package.".into(),
            confidence: "high".into(),
            caveats: None,
            evidence: frames
                .into_iter()
                .map(|frame| Evidence {
                    timestamp_secs: 1.0,
                    frame: Some(frame),
                    quote: None,
                    source: None,
                })
                .collect(),
            scripts: vec![],
            variants: vec![],
        }],
        artifacts: vec![],
        gaps: vec![],
        history: vec![],
    }
}

/// Regression for the real-world corruption: two re-watch frames plus a
/// keyframe must each land byte-for-byte in references/frames/.
#[test]
fn compiled_package_keeps_every_distinct_frame_byte_for_byte() {
    let dir = scratch("compile-mixed");
    let bundle = color_bundle(&dir);
    let tc = toolchain();
    fs::create_dir_all(bundle.join("frames")).unwrap();
    fs::write(bundle.join("frames/kf0001.jpg"), b"keyframe-bytes").unwrap();
    let red = frame_at(&bundle, &tc, "1").unwrap();
    let blue = frame_at(&bundle, &tc, "7").unwrap();

    let rel = |p: &Path| {
        p.strip_prefix(&bundle)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    };
    let ir = ir_citing(vec![
        "frames/kf0001.jpg".into(),
        rel(&red.path),
        rel(&blue.path),
    ]);
    let out = dir.join("skill");
    compile(&ir, &bundle, &out).unwrap();

    let packaged = out.join("references/frames");
    assert_eq!(
        fs::read(packaged.join("kf0001.jpg")).unwrap(),
        b"keyframe-bytes"
    );
    for exported in [&red, &blue] {
        assert_eq!(
            fs::read(packaged.join(basename(&exported.path))).unwrap(),
            fs::read(&exported.path).unwrap(),
            "re-watch frame must survive packaging byte-for-byte"
        );
    }
    assert_eq!(
        fs::read_dir(&packaged).unwrap().count(),
        3,
        "three distinct images expected"
    );
}
