//! Keyframe export: agent-readable JPEGs (bounded width) plus
//! native-resolution stills at shot boundaries.

use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::tools::Toolchain;

/// Export one frame at `timestamp` to `dest`. `max_width` bounds the
/// output (aspect preserved, never upscaled); `None` keeps native size.
pub fn export_frame(
    tools: &Toolchain,
    media: &Path,
    timestamp_secs: f64,
    dest: &Path,
    max_width: Option<u32>,
) -> Result<()> {
    let mut cmd = std::process::Command::new(&tools.ffmpeg);
    cmd.args(["-y", "-v", "error", "-ss", &format!("{timestamp_secs:.3}")])
        .arg("-i")
        .arg(media)
        .args(["-frames:v", "1"]);
    if let Some(width) = max_width {
        cmd.args(["-vf", &format!("scale='min({width},iw)':-2")]);
    }
    // -q:v 3: high-quality JPEG, ~100-300 KB at 1024px — the documented
    // per-frame size budget for agent consumption.
    cmd.args(["-q:v", "3"]).arg(dest);
    let output = cmd.output().context("running ffmpeg frame export")?;
    if !output.status.success() {
        bail!(
            "frame export at {timestamp_secs:.3}s failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
