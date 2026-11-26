//! The screen-text track: on-screen text read from evidence keyframes
//! by a local OCR engine, recorded as `screen_text.json` in the bundle.
//!
//! OCR is a hint with a confidence, never evidence: the agent greps it
//! to find the frame that shows a command, then reads the pixels. The
//! engine is a seam (`OcrEngine`) so the track is testable without a
//! platform engine; `platform_engine` picks the real one.

mod engines;
mod track;
#[cfg(target_os = "macos")]
mod vision;
pub mod vocabulary;
#[cfg(windows)]
mod winocr;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

pub use engines::{platform_engine, NoEngine};
pub use track::{extract_text_track, record_unavailable};

pub const SCREEN_TEXT_FILE: &str = "screen_text.json";

/// Everything one engine read off the bundle's evidence keyframes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenTextTrack {
    pub schema_version: u32,
    /// Keyframes the engine looked at, text or not — so an empty
    /// `lines` means "nothing legible", not "never read".
    pub frames_read: usize,
    /// In keyframe order, each frame's lines in the engine's reading
    /// order.
    pub lines: Vec<ScreenTextLine>,
}

/// One line of recognised text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenTextLine {
    pub timestamp_secs: f64,
    /// Bundle-relative path of the agent-readable keyframe the text
    /// belongs to (the engine may have read its native-resolution
    /// still; the bounding box applies to both).
    pub frame: String,
    pub bbox: BBox,
    pub text: String,
    /// Engine-reported 0.0–1.0, or `None` when the engine gives none.
    pub confidence: Option<f32>,
    pub engine: String,
}

/// A text line's extent as fractions of the image, origin top-left —
/// the same box fits the keyframe and its native still.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl BBox {
    /// From pixel edges in an image of `width` × `height`, rounded to
    /// four decimals so the artifact stays compact.
    #[must_use]
    pub fn from_pixels(
        (left, top, right, bottom): (f64, f64, f64, f64),
        width: f64,
        height: f64,
    ) -> BBox {
        BBox {
            x: left / width,
            y: top / height,
            w: (right - left) / width,
            h: (bottom - top) / height,
        }
        .rounded()
    }

    #[must_use]
    pub fn rounded(self) -> BBox {
        let round = |v: f64| (v * 10_000.0).round() / 10_000.0;
        BBox {
            x: round(self.x),
            y: round(self.y),
            w: round(self.w),
            h: round(self.h),
        }
    }
}

/// What an engine reads in one image.
#[derive(Debug, Clone, PartialEq)]
pub struct Recognized {
    pub bbox: BBox,
    pub text: String,
    pub confidence: Option<f32>,
}

/// Text recognition seam. Engines are shared across worker threads, so
/// per-call state lives inside `read`.
pub trait OcrEngine: Sync {
    /// Stable identifier recorded on every line, e.g. `apple-vision`.
    fn name(&self) -> &'static str;

    /// Largest image side the engine accepts, when it has a limit; the
    /// track then reads a smaller export instead of the native still.
    fn max_dimension(&self) -> Option<u32> {
        None
    }

    /// Every line of text in the image at `image`, in reading order.
    fn read(&self, image: &Path) -> Result<Vec<Recognized>>;
}

pub fn load_screen_text(bundle_dir: &Path) -> Result<ScreenTextTrack> {
    let raw = fs::read_to_string(bundle_dir.join(SCREEN_TEXT_FILE))
        .with_context(|| format!("no screen text track at {}", bundle_dir.display()))?;
    serde_json::from_str(&raw).context("screen text artifact is not valid")
}

pub fn save_screen_text(bundle_dir: &Path, track: &ScreenTextTrack) -> Result<()> {
    fs::create_dir_all(bundle_dir)?;
    let path = bundle_dir.join(SCREEN_TEXT_FILE);
    crate::fsx::write_atomic(&path, &serde_json::to_vec_pretty(track)?)?;
    Ok(())
}

/// Compile a `--grep` pattern. Matching is case-insensitive because OCR
/// case is the least reliable thing about it.
pub fn compile_pattern(pattern: &str) -> Result<Regex> {
    Regex::new(&format!("(?i){pattern}")).with_context(|| format!("bad --grep pattern '{pattern}'"))
}

/// Lines whose text matches `pattern`, in track order.
#[must_use]
pub fn grep<'a>(track: &'a ScreenTextTrack, pattern: &Regex) -> Vec<&'a ScreenTextLine> {
    track
        .lines
        .iter()
        .filter(|line| pattern.is_match(&line.text))
        .collect()
}

/// `t=<secs>\t<frame>\t<text>` — the same shape `frame-at` and `clip`
/// print, so the agent can cut it the same way.
#[must_use]
pub fn format_line(line: &ScreenTextLine) -> String {
    format!(
        "t={:.3}\t{}\t{}",
        line.timestamp_secs, line.frame, line.text
    )
}

#[cfg(test)]
mod tests {
    use super::BBox;

    #[test]
    fn pixel_boxes_normalise_against_the_image_and_round_to_four_decimals() {
        let bbox = BBox::from_pixels((100.0, 50.0, 400.0, 80.0), 1920.0, 1080.0);

        assert_eq!(
            bbox,
            BBox {
                x: 0.0521,
                y: 0.0463,
                w: 0.1563,
                h: 0.0278,
            }
        );
    }
}
