//! Agent re-watch tools: export exact frames from the source media so
//! the orchestrating agent can look closer at any moment.
//!
//! Outputs land in the bundle's `rewatch/` scratch area, one directory
//! per request — safe to delete wholesale without harming the bundle.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::bundle::load_manifest;
use crate::frames::export::export_frame;
use crate::tools::Toolchain;

const REWATCH_DIR: &str = "rewatch";
/// Width bound for exported frames (same agent-readable budget as
/// keyframes).
const MAX_WIDTH: u32 = 1024;
/// Hard cap on frames per clip request — keeps a careless wide request
/// from flooding the agent's context.
pub const CLIP_FRAME_CAP: usize = 60;

#[derive(Debug)]
pub struct ExportedFrame {
    pub timestamp_secs: f64,
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct ClipResult {
    pub frames: Vec<ExportedFrame>,
    /// True when the request exceeded [`CLIP_FRAME_CAP`] and was cut off.
    pub truncated: bool,
}

/// Parse "90", "90.5", "MM:SS(.f)", or "HH:MM:SS(.f)" into seconds.
pub fn parse_timestamp(raw: &str) -> Result<f64> {
    let parts: Vec<&str> = raw.split(':').collect();
    let parse_part = |p: &str, name: &str| -> Result<f64> {
        if p.is_empty() {
            bail!("bad timestamp '{raw}': empty {name}");
        }
        let v: f64 = p
            .parse()
            .map_err(|_| anyhow::anyhow!("bad timestamp '{raw}': '{p}' is not a number"))?;
        if v < 0.0 {
            bail!("bad timestamp '{raw}': negative values are not allowed");
        }
        Ok(v)
    };
    match parts.as_slice() {
        [secs] => parse_part(secs, "seconds"),
        [mins, secs] => Ok(parse_part(mins, "minutes")? * 60.0 + parse_part(secs, "seconds")?),
        [hours, mins, secs] => Ok(parse_part(hours, "hours")? * 3600.0
            + parse_part(mins, "minutes")? * 60.0
            + parse_part(secs, "seconds")?),
        _ => bail!("bad timestamp '{raw}': use seconds, MM:SS, or HH:MM:SS"),
    }
}

/// Export the frame at `timestamp` from the source media.
pub fn frame_at(bundle_dir: &Path, tools: &Toolchain, timestamp: &str) -> Result<ExportedFrame> {
    let manifest = load_manifest(bundle_dir)?;
    let ts = parse_timestamp(timestamp)?;
    if ts > manifest.media.duration_secs {
        bail!(
            "timestamp {ts:.3}s is beyond the video's end ({:.3}s)",
            manifest.media.duration_secs
        );
    }
    let dir = request_dir(bundle_dir, &format!("at-{:09}", millis(ts)))?;
    let path = dir.join("frame.jpg");
    export_frame(
        tools,
        Path::new(&manifest.source.media_path),
        ts,
        &path,
        Some(MAX_WIDTH),
    )?;
    Ok(ExportedFrame {
        timestamp_secs: ts,
        path,
    })
}

/// Densely sample `start..end` at `fps` from the source media.
pub fn clip(
    bundle_dir: &Path,
    tools: &Toolchain,
    start: &str,
    end: &str,
    fps: f64,
) -> Result<ClipResult> {
    let manifest = load_manifest(bundle_dir)?;
    let start = parse_timestamp(start)?;
    let end = parse_timestamp(end)?;
    if end <= start {
        bail!("clip end ({end:.3}s) must be after its start ({start:.3}s)");
    }
    if start > manifest.media.duration_secs {
        bail!(
            "clip start {start:.3}s is beyond the video's end ({:.3}s)",
            manifest.media.duration_secs
        );
    }
    // NaN fails this comparison too, which is exactly what we want.
    if fps <= 0.0 || fps.is_nan() {
        bail!("clip fps must be positive");
    }
    let end = end.min(manifest.media.duration_secs);

    // Half-open range [start, end): adjacent clip requests never
    // duplicate their boundary frame.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let requested = (((end - start) * fps).ceil() as usize).max(1);
    let truncated = requested > CLIP_FRAME_CAP;
    let count = requested.min(CLIP_FRAME_CAP);

    let dir = request_dir(
        bundle_dir,
        &format!(
            "clip-{:09}-{:09}-{}",
            millis(start),
            millis(end),
            millis(fps)
        ),
    )?;
    let media = Path::new(&manifest.source.media_path);
    let mut frames = Vec::with_capacity(count);
    for i in 0..count {
        #[allow(clippy::cast_precision_loss)] // counts are ≤ CLIP_FRAME_CAP
        let ts = start + i as f64 / fps;
        let path = dir.join(format!("f{i:03}.jpg"));
        export_frame(tools, media, ts, &path, Some(MAX_WIDTH))?;
        frames.push(ExportedFrame {
            timestamp_secs: ts,
            path,
        });
    }
    Ok(ClipResult { frames, truncated })
}

fn request_dir(bundle_dir: &Path, name: &str) -> Result<PathBuf> {
    let dir = bundle_dir.join(REWATCH_DIR).join(name);
    fs::create_dir_all(&dir).context("creating rewatch scratch dir")?;
    Ok(dir)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn millis(v: f64) -> u64 {
    (v.max(0.0) * 1000.0) as u64
}
