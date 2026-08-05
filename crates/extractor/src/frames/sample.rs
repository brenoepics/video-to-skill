//! Tiny-thumbnail sampling: one ffmpeg pass emits raw RGB 9x8
//! frames for the whole video — cheap enough to analyze hours of footage.

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
        .args(["-v", "error", "-i"])
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
    Ok(output
        .stdout
        .chunks_exact(TINY_RGB_LEN)
        .enumerate()
        .map(|(i, chunk)| SampledFrame {
            #[allow(clippy::cast_precision_loss)] // frame counts are small
            timestamp_secs: i as f64 / fps,
            pixels: chunk.to_vec(),
        })
        .collect())
}
