//! The synthetic screencast shared by the frame-track fixture tests: a
//! dark terminal whose lines appear one by one, then scroll, then a
//! light dialog cuts in. Every expected value in the tests follows from
//! how this fixture is drawn.

// Test support: unwrap is fine here.
#![allow(clippy::unwrap_used, dead_code)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::tools::Toolchain;

/// Seconds at which a new "terminal line" is drawn (each lands between
/// two one-second sampling ticks, so the first steady second after it
/// is unambiguous).
pub const LINE_APPEARS_AT: [f64; 3] = [4.5, 9.5, 14.5];
/// The content scrolls up 400px between these seconds (80px/s).
pub const SCROLL_FROM: f64 = 20.0;
pub const SCROLL_TO: f64 = 25.0;
/// A different screen (a light dialog) replaces the terminal here.
pub const CUT_AT: f64 = 29.5;
pub const DURATION_SECS: u32 = 36;

pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-frames-states-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn toolchain() -> Toolchain {
    Toolchain {
        ffmpeg: PathBuf::from("ffmpeg"),
        ffprobe: PathBuf::from("ffprobe"),
        whisper_model: None,
        whisper_vad: None,
    }
}

pub fn has_libx264() -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(" libx264 "))
}

/// Height of one text line's boxes, in pixels of the 1080p canvas.
pub const LINE_HEIGHT: usize = 24;
/// Left edge of every text line.
const LINE_LEFT: usize = 40;
/// Gap between a line's word boxes.
const WORD_GAP: usize = 20;

/// The widths of a text line's three word-like boxes; they vary per
/// row so rows are distinguishable.
fn word_widths(row: usize) -> [usize; 3] {
    [
        200 + (row * 37) % 300,
        120 + (row * 53) % 260,
        80 + (row * 71) % 400,
    ]
}

/// Where text line `row` is drawn on the 1080p canvas, before any
/// scroll: `(left, right, top, bottom)` in pixels, right and bottom
/// exclusive.
pub fn line_extent(row: usize) -> (usize, usize, usize, usize) {
    let top = 60 + 40 * row;
    let widths = word_widths(row);
    let right = LINE_LEFT + widths.iter().sum::<usize>() + WORD_GAP * (widths.len() - 1);
    (LINE_LEFT, right, top, top + LINE_HEIGHT)
}

/// One "text line": three word-like boxes drawn from `enable` (always,
/// when empty).
fn text_line(filter: &mut String, row: usize, enable: &str) {
    let (_, _, y, _) = line_extent(row);
    let colors = ["0xd0d0d0", "0x8fd08f", "0xa0a0d0"];
    let mut x = LINE_LEFT;
    for (w, color) in word_widths(row).iter().zip(colors) {
        write!(
            filter,
            "drawbox=x={x}:y={y}:w={w}:h={LINE_HEIGHT}:color={color}:t=fill{enable},"
        )
        .unwrap();
        x += w + WORD_GAP;
    }
}

/// A 1080p30 h264 screencast with B-frames: a dark terminal whose
/// history fills a canvas 400px taller than the screen; three lines
/// appear one by one, the content then scrolls up 400px over five
/// seconds, and finally a light dialog screen cuts in.
pub fn screencast_fixture(dir: &Path) -> PathBuf {
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
