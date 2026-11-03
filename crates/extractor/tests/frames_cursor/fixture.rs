//! A synthetic GUI walkthrough with a drawn cursor: a light window with
//! a toolbar and sidebar, an arrow cursor that sits still and jumps
//! between three places, a box that opens beside it after each of two
//! "clicks", and finally a cut to a dark slide with no cursor. Every
//! expected value in the tests follows from how this is drawn.

// Test support: unwrap is fine here.
#![allow(clippy::unwrap_used, dead_code)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::tools::Toolchain;

pub const NATIVE_W: usize = 1280;
pub const NATIVE_H: usize = 720;
pub const DURATION_SECS: u32 = 24;
/// Where the cursor's tip sits, and from when (each jump lands between
/// two one-second sampling ticks).
pub const CURSOR_AT: [((usize, usize), f64); 3] =
    [((200, 150), 0.0), ((600, 300), 4.5), ((900, 500), 9.5)];
/// A box opens beside the cursor at each "click": `(x, y, w, h, at)`.
pub const BOXES: [(usize, usize, usize, usize, f64); 2] =
    [(620, 330, 300, 160, 7.5), (700, 540, 400, 150, 12.5)];
/// A dark slide without a cursor replaces the window here.
pub const CUT_AT: f64 = 15.5;
/// The arrow sprite's size in native pixels.
pub const CURSOR_W: usize = 12;
pub const CURSOR_H: usize = 19;

pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-frames-cursor-tests")
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

/// The classic arrow pointer, tip at the top-left, as a polygon over a
/// 12x19 cell.
const ARROW: [(f64, f64); 7] = [
    (0.0, 0.0),
    (0.0, 16.0),
    (4.0, 12.0),
    (7.0, 18.0),
    (9.0, 17.0),
    (6.0, 11.0),
    (11.0, 11.0),
];

fn inside_arrow(x: usize, y: usize) -> bool {
    #[allow(clippy::cast_precision_loss)]
    let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
    let mut inside = false;
    for i in 0..ARROW.len() {
        let (x1, y1) = ARROW[i];
        let (x2, y2) = ARROW[(i + 1) % ARROW.len()];
        if (y1 > py) != (y2 > py) && px < x1 + (py - y1) * (x2 - x1) / (y2 - y1) {
            inside = !inside;
        }
    }
    inside
}

/// Write the cursor sprite as an RGBA PAM: black body, white one-pixel
/// outline, transparent elsewhere.
fn write_cursor_sprite(path: &Path) {
    let body: Vec<Vec<bool>> = (0..CURSOR_H)
        .map(|y| (0..CURSOR_W).map(|x| inside_arrow(x, y)).collect())
        .collect();
    let mut data = format!(
        "P7\nWIDTH {CURSOR_W}\nHEIGHT {CURSOR_H}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n"
    )
    .into_bytes();
    for y in 0..CURSOR_H {
        for x in 0..CURSOR_W {
            let outlined = (y.saturating_sub(1)..=(y + 1).min(CURSOR_H - 1))
                .any(|r| (x.saturating_sub(1)..=(x + 1).min(CURSOR_W - 1)).any(|c| body[r][c]));
            data.extend_from_slice(match (body[y][x], outlined) {
                (true, _) => &[0, 0, 0, 255],
                (false, true) => &[255, 255, 255, 255],
                _ => &[0, 0, 0, 0],
            });
        }
    }
    fs::write(path, data).unwrap();
}

/// Piecewise-constant ffmpeg expression for the cursor's x or y.
fn cursor_expr(coord: impl Fn((usize, usize)) -> usize) -> String {
    let mut expr = String::new();
    for (i, &(at, _)) in CURSOR_AT.iter().enumerate() {
        if let Some(&(_, next_from)) = CURSOR_AT.get(i + 1) {
            let _ = write!(expr, "if(lt(t\\,{next_from})\\,{}\\,", coord(at));
        } else {
            let _ = write!(expr, "{}", coord(at));
        }
    }
    expr + &")".repeat(CURSOR_AT.len() - 1)
}

/// A 720p30 h264 screencast of the walkthrough described above.
pub fn walkthrough_fixture(dir: &Path) -> PathBuf {
    let sprite = dir.join("cursor.pam");
    write_cursor_sprite(&sprite);
    // The window: toolbar, sidebar, three static text-like lines.
    let mut window = String::from(
        "[0:v]drawbox=x=0:y=0:w=1280:h=40:color=0xd0d0d0:t=fill,\
         drawbox=x=0:y=40:w=200:h=680:color=0xe0e0e0:t=fill,",
    );
    for row in 0..3 {
        let y = 80 + 30 * row;
        let w = 160 + 40 * row;
        let _ = write!(
            window,
            "drawbox=x=240:y={y}:w={w}:h=12:color=0x404040:t=fill,"
        );
    }
    for &(x, y, w, h, at) in &BOXES {
        let _ = write!(
            window,
            "drawbox=x={x}:y={y}:w={w}:h={h}:color=0x808080:t=fill:enable='gte(t\\,{at})',"
        );
    }
    window.push_str("format=rgba[win];");
    let cursor = format!(
        "[win][1:v]overlay=x='{}':y='{}':enable='lt(t\\,{CUT_AT})'[gui];",
        cursor_expr(|(x, _)| x),
        cursor_expr(|(_, y)| y)
    );
    let slide = "[2:v]drawbox=x=200:y=200:w=880:h=60:color=0xe0e0e0:t=fill[slide];";
    let filter = format!("{window}{cursor}{slide}[gui][slide]overlay=enable='gte(t\\,{CUT_AT})'");

    let out = dir.join("walkthrough.mp4");
    let res = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
        .arg(format!(
            "color=c=0xf0f0f0:s={NATIVE_W}x{NATIVE_H}:r=30:d={DURATION_SECS}"
        ))
        .args(["-loop", "1", "-framerate", "30", "-i"])
        .arg(&sprite)
        .args(["-f", "lavfi", "-i"])
        .arg(format!(
            "color=c=0x202020:s={NATIVE_W}x{NATIVE_H}:r=30:d={DURATION_SECS}"
        ))
        .args(["-filter_complex", &filter])
        .args(["-t", &DURATION_SECS.to_string()])
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
