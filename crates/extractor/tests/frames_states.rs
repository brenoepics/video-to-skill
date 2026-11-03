//! Evidence-frame placement on a synthetic screencast: every screen
//! change yields one state whose keyframe is the first steady second
//! after the change, a slow scroll is folded into its surrounding state,
//! and a hard cut is still a boundary. The expected values follow from
//! how the fixture is drawn, not from the implementation.
//! Skipped (with a note) where ffmpeg has no libx264 encoder.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::bundle::ingest;
use vts_extract::frames::{extract_frame_track, FrameConfig, FRAMES_FILE};
use vts_extract::tools::Toolchain;

/// Seconds at which a new "terminal line" is drawn (each lands between
/// two one-second sampling ticks, so the first steady second after it
/// is unambiguous).
const LINE_APPEARS_AT: [f64; 3] = [4.5, 9.5, 14.5];
/// The content scrolls up 400px between these seconds (80px/s).
const SCROLL_FROM: f64 = 20.0;
const SCROLL_TO: f64 = 25.0;
/// A different screen (a light dialog) replaces the terminal here.
const CUT_AT: f64 = 29.5;
const DURATION_SECS: u32 = 36;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-frames-states-tests")
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

/// One "text line": three word-like boxes whose widths vary per row so
/// rows are distinguishable, drawn from `enable` (always, when empty).
fn text_line(filter: &mut String, row: usize, enable: &str) {
    let y = 60 + 40 * row;
    let widths = [
        200 + (row * 37) % 300,
        120 + (row * 53) % 260,
        80 + (row * 71) % 400,
    ];
    let colors = ["0xd0d0d0", "0x8fd08f", "0xa0a0d0"];
    let mut x = 40;
    for (w, color) in widths.iter().zip(colors) {
        write!(
            filter,
            "drawbox=x={x}:y={y}:w={w}:h=24:color={color}:t=fill{enable},"
        )
        .unwrap();
        x += w + 20;
    }
}

/// A 1080p30 h264 screencast with B-frames: a dark terminal whose
/// history fills a canvas 400px taller than the screen; three lines
/// appear one by one, the content then scrolls up 400px over five
/// seconds, and finally a light dialog screen cuts in.
fn screencast_fixture(dir: &Path) -> PathBuf {
    let mut terminal = String::from("[0:v]");
    for row in 0..36 {
        let enable = match row {
            20 => format!(":enable='gte(t\\,{})'", LINE_APPEARS_AT[0]),
            21 => format!(":enable='gte(t\\,{})'", LINE_APPEARS_AT[1]),
            22 => format!(":enable='gte(t\\,{})'", LINE_APPEARS_AT[2]),
            _ => String::new(),
        };
        text_line(&mut terminal, row, &enable);
    }
    let speed = 400.0 / (SCROLL_TO - SCROLL_FROM);
    write!(
        terminal,
        "crop=1920:1080:0:'min(400\\,max(0\\,(t-{SCROLL_FROM})*{speed}))'[term];"
    )
    .unwrap();
    let dialog = "[1:v]drawbox=x=400:y=300:w=1120:h=480:color=0x404040:t=fill,\
                  drawbox=x=440:y=340:w=600:h=40:color=0xffffff:t=fill[dlg];";
    let filter = format!("{terminal}{dialog}[term][dlg]overlay=enable='gte(t\\,{CUT_AT})'");

    let out = dir.join("screencast.mp4");
    let res = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
        .arg(format!(
            "color=c=0x1e1e1e:s=1920x1480:r=30:d={DURATION_SECS}"
        ))
        .args(["-f", "lavfi", "-i"])
        .arg(format!(
            "color=c=0xf0f0f0:s=1920x1080:r=30:d={DURATION_SECS}"
        ))
        .args(["-filter_complex", &filter])
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
fn keyframes_land_on_the_first_steady_second_after_each_change() {
    if !has_libx264() {
        eprintln!("skipping: ffmpeg has no libx264 encoder");
        return;
    }
    let dir = scratch("screencast");
    let video = screencast_fixture(&dir);
    let bundle = dir.join("bundle");
    ingest(&video, &bundle, &toolchain()).unwrap();

    let track = extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    let keyframes: Vec<f64> = track
        .shots
        .iter()
        .map(|s| s.keyframe.timestamp_secs)
        .collect();
    // One state per appended line, its evidence frame being the first
    // whole second after the line appeared; the opening screen and the
    // hard cut are states too; the scroll adds none.
    let mut expected = vec![0.0];
    expected.extend(LINE_APPEARS_AT.iter().map(|t| t.ceil()));
    expected.push(CUT_AT.ceil());
    assert_eq!(keyframes, expected, "evidence timestamps: {keyframes:?}");

    let starts: Vec<f64> = track.shots.iter().map(|s| s.start_secs).collect();
    assert!(
        !starts
            .iter()
            .any(|&t| t > LINE_APPEARS_AT[2].ceil() && t < CUT_AT),
        "the scroll must not cut: {starts:?}"
    );
    let scrolled = &track.shots[3];
    assert!(scrolled.start_secs <= SCROLL_FROM && scrolled.end_secs >= SCROLL_TO + 1.0);
    assert!((track.shots[4].start_secs - CUT_AT.ceil()).abs() < f64::EPSILON);
    for shot in &track.shots {
        assert!(bundle.join(&shot.keyframe.path).is_file());
        assert!(bundle
            .join(shot.keyframe.native_path.as_ref().unwrap())
            .is_file());
    }

    // Same media, same track, byte for byte.
    let first = fs::read(bundle.join(FRAMES_FILE)).unwrap();
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();
    let second = fs::read(bundle.join(FRAMES_FILE)).unwrap();
    assert_eq!(first, second, "frames.json must be byte-identical");
}
