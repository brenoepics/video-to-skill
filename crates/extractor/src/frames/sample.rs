//! Tiny-thumbnail sampling: one ffmpeg pass emits raw RGB 9x8
//! frames for the whole video — cheap enough to analyze hours of footage.
//!
//! The decoder skips non-reference frames (`-skip_frame noref`): nothing
//! depends on them, so output stays artifact-free, and at one sample per
//! second the at-most-few-frames shift in which picture lands on a tick
//! is invisible to shot detection. It cuts the full-decode pass by
//! 1.2–1.6x on B-frame-heavy h264 and is ignored by decoders without it.

use std::path::Path;

use anyhow::{bail, Context, Result};

use super::analyze::{TINY_H, TINY_RGB_LEN, TINY_W};
use crate::tools::Toolchain;

pub struct SampledFrame {
    pub timestamp_secs: f64,
    pub pixels: Vec<u8>,
}

pub fn sample_tiny(tools: &Toolchain, media: &Path, fps: f64) -> Result<Vec<SampledFrame>> {
    let filter = format!("fps={fps},scale={TINY_W}x{TINY_H},format=rgb24");
    let output = std::process::Command::new(&tools.ffmpeg)
        .args(["-v", "error", "-skip_frame", "noref", "-i"])
        .arg(media)
        .args(["-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .context("running ffmpeg frame sampler")?;
    if !output.status.success() {
        bail!(
            "frame sampling failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if output.stdout.len() % TINY_RGB_LEN != 0 {
        bail!("frame sampler produced a truncated stream");
    }
    let (frames, _) = output.stdout.as_chunks::<TINY_RGB_LEN>();
    Ok(frames
        .iter()
        .enumerate()
        .map(|(i, chunk)| SampledFrame {
            #[allow(clippy::cast_precision_loss)] // frame counts are small
            timestamp_secs: i as f64 / fps,
            pixels: chunk.to_vec(),
        })
        .collect())
}
