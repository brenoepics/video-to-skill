//! The platform OCR engines on text with a known rendering, and the
//! `text` subcommand the agent greps. Engine tests skip where the
//! platform has no engine (Linux) — the fake-engine suite in `ocr.rs`
//! covers the track there.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use vts_extract::bundle::{
    ingest, save_manifest, Manifest, MediaInfo, PlatformPriors, SourceInfo, Tracks,
};
use vts_extract::frames::{extract_frame_track, FrameConfig};
use vts_extract::ocr::{
    self, extract_text_track, save_screen_text, BBox, OcrEngine, Recognized, ScreenTextLine,
    ScreenTextTrack,
};
use vts_extract::tools::Toolchain;

const FIXTURE_PNG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/terminal.png");

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-ocr-engine-tests")
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

fn platform_engine() -> Option<Box<dyn OcrEngine>> {
    match ocr::platform_engine(&ocr::vocabulary::custom_words(Some("Fixture Walkthrough"))) {
        Ok(engine) => Some(engine),
        Err(no_engine) => {
            eprintln!("skipping: {no_engine}");
            None
        }
    }
}

/// OCR output folded for comparison: lowercase, single-spaced.
fn folded(texts: impl Iterator<Item = String>) -> String {
    texts
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[test]
fn the_platform_engine_reads_the_terminal_fixture() {
    let Some(engine) = platform_engine() else {
        return;
    };

    let lines = engine.read(Path::new(FIXTURE_PNG)).unwrap();

    let text = folded(lines.iter().map(|l| l.text.clone()));
    assert!(text.contains("cargo build --release"), "{lines:?}");
    assert!(text.contains("git push origin main"), "{lines:?}");
    for Recognized { bbox, .. } in &lines {
        let BBox { x, y, w, h } = *bbox;
        assert!(
            (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y),
            "normalised, top-left origin: {bbox:?}"
        );
        assert!(w > 0.0 && h > 0.0 && x + w <= 1.0 + 1e-6 && y + h <= 1.0 + 1e-6);
    }
    let first = lines.iter().find(|l| l.text.contains("cargo")).unwrap();
    let second = lines.iter().find(|l| l.text.contains("push")).unwrap();
    assert!(first.bbox.y < second.bbox.y, "y grows downwards");
}

/// A font file ffmpeg's drawtext can use on this machine, or `None`.
fn drawtext_font() -> Option<String> {
    let filters = Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .ok()?;
    if !String::from_utf8_lossy(&filters.stdout).contains(" drawtext ") {
        return None;
    }
    [
        "/System/Library/Fonts/Menlo.ttc",
        "C:/Windows/Fonts/consola.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ]
    .into_iter()
    .find(|candidate| Path::new(candidate).is_file())
    // drawtext parses ':' as an option separator, so the Windows drive
    // colon is escaped.
    .map(|font| font.replace(':', "\\:"))
}

#[test]
fn the_platform_engine_reads_text_rendered_by_ffmpeg_drawtext() {
    let Some(engine) = platform_engine() else {
        return;
    };
    let Some(font) = drawtext_font() else {
        eprintln!("skipping: ffmpeg has no drawtext filter or no known font here");
        return;
    };
    let dir = scratch("drawtext");
    let media = dir.join("drawtext.mp4");
    let filter = format!(
        "drawtext=fontfile={font}:text='kubectl get pods -n staging':fontsize=40:fontcolor=0xd4d4d4:x=40:y=100"
    );
    let res = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=0x1e1e1e:s=960x240:r=10:d=3",
        ])
        .args(["-vf", &filter, "-c:v", "mpeg4", "-q:v", "2"])
        .arg(&media)
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "fixture generation failed: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    let bundle = dir.join("bundle");
    ingest(&media, &bundle, &toolchain()).unwrap();
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    let track = extract_text_track(&bundle, engine.as_ref()).unwrap();

    let text = folded(track.lines.iter().map(|l| l.text.clone()));
    assert!(text.contains("kubectl get pods -n staging"), "{track:?}");
    assert_eq!(track.lines[0].frame, "frames/shot000.jpg");
    assert_eq!(track.lines[0].engine, engine.name());
}

fn two_line_track() -> ScreenTextTrack {
    let line = |t: f64, frame: &str, text: &str| ScreenTextLine {
        timestamp_secs: t,
        frame: frame.into(),
        bbox: BBox {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.1,
        },
        text: text.into(),
        confidence: None,
        engine: "fake".into(),
    };
    ScreenTextTrack {
        schema_version: 1,
        frames_read: 2,
        lines: vec![
            line(1.0, "frames/shot000.jpg", "$ cargo build --release"),
            line(4.0, "frames/shot001.jpg", "$ git push origin main"),
        ],
    }
}

fn text_command(bundle: &Path, grep: Option<&str>) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_vts-extract"));
    cmd.arg("text").arg("--bundle").arg(bundle);
    if let Some(pattern) = grep {
        cmd.args(["--grep", pattern]);
    }
    cmd.output().unwrap()
}

#[test]
fn the_text_subcommand_prints_matching_lines_in_track_order() {
    let dir = scratch("text-command");
    save_screen_text(&dir, &two_line_track()).unwrap();

    let filtered = text_command(&dir, Some("PUSH"));
    let all = text_command(&dir, None);

    assert!(filtered.status.success());
    assert_eq!(
        String::from_utf8_lossy(&filtered.stdout),
        "t=4.000\tframes/shot001.jpg\t$ git push origin main\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&all.stdout),
        "t=1.000\tframes/shot000.jpg\t$ cargo build --release\n\
         t=4.000\tframes/shot001.jpg\t$ git push origin main\n"
    );
}

#[test]
fn the_text_subcommand_explains_an_absent_track_from_the_manifest_note() {
    let dir = scratch("text-absent");
    let manifest = Manifest {
        schema_version: 1,
        source: SourceInfo {
            original: "v.mp4".into(),
            media_path: "/abs/v.mp4".into(),
            sha256: "ab".repeat(32),
            title: None,
        },
        media: MediaInfo {
            container: "mp4".into(),
            duration_secs: 1.0,
            width: 320,
            height: 240,
            fps: 10.0,
            has_audio: false,
            language: None,
        },
        tracks: Tracks::default(),
        platform: PlatformPriors::default(),
        notes: vec!["screen_text: no local OCR engine on this platform".into()],
    };
    save_manifest(&dir, &manifest).unwrap();

    let out = text_command(&dir, Some("cargo"));

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no screen text track"), "{stderr}");
    assert!(stderr.contains("no local OCR engine"), "{stderr}");
}
