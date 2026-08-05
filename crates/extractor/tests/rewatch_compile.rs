//! Packaging regression: re-watch frames cited as step evidence must
//! survive skill compilation byte-for-byte alongside keyframes.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::bundle::ingest;
use vts_extract::compile::{compile, Evidence, ProcedureIr, SourceRef, Step};
use vts_extract::rewatch::frame_at;
use vts_extract::tools::Toolchain;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-rewatch-compile-tests")
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

fn basename(p: &Path) -> &str {
    p.file_name().and_then(|n| n.to_str()).unwrap()
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
