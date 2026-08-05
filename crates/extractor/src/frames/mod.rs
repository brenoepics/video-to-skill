//! The frame track: shot boundaries, deduplicated keyframes, and a
//! motion-density signal, recorded as `frames.json` in the bundle.

pub mod analyze;
pub(crate) mod export;
mod sample;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::bundle::{load_manifest, save_manifest};
use crate::tools::Toolchain;

pub const FRAMES_FILE: &str = "frames.json";
const FRAMES_DIR: &str = "frames";

/// Tunables, all with tested defaults.
#[derive(Debug, Clone)]
pub struct FrameConfig {
    /// Thumbnails sampled per second for analysis.
    pub sample_fps: f64,
    /// Mean-absolute-difference (0–255) beyond which a frame starts a
    /// new shot.
    pub boundary_threshold: f64,
    /// Width bound for agent-readable keyframes (native stills keep
    /// full resolution).
    pub max_keyframe_width: u32,
}

impl Default for FrameConfig {
    fn default() -> FrameConfig {
        FrameConfig {
            sample_fps: 1.0,
            boundary_threshold: 25.0,
            max_keyframe_width: 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameTrack {
    pub schema_version: u32,
    pub shots: Vec<Shot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    pub index: usize,
    pub start_secs: f64,
    pub end_secs: f64,
    /// Mean frame-to-frame change inside the shot, 0.0–1.0. High values
    /// flag demos/action worth denser inspection.
    pub motion_density: f64,
    pub keyframe: Keyframe,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub timestamp_secs: f64,
    /// Bundle-relative path of the width-bounded JPEG.
    pub path: String,
    /// Bundle-relative path of the native-resolution still (at shot
    /// boundaries).
    pub native_path: Option<String>,
    /// 64-bit perceptual dhash, hex — stable identity across runs.
    pub hash: String,
}

pub fn load_frame_track(bundle_dir: &Path) -> Result<FrameTrack> {
    let raw = fs::read_to_string(bundle_dir.join(FRAMES_FILE))
        .with_context(|| format!("no frame track at {}", bundle_dir.display()))?;
    serde_json::from_str(&raw).context("frame track artifact is not valid")
}

/// Analyze the video's frames and produce the bundle's frame track.
pub fn extract_frame_track(
    bundle_dir: &Path,
    tools: &Toolchain,
    config: &FrameConfig,
) -> Result<FrameTrack> {
    let mut manifest = load_manifest(bundle_dir)?;
    let media = Path::new(&manifest.source.media_path).to_path_buf();

    let sampled = sample::sample_tiny(tools, &media, config.sample_fps)?;
    let pixels: Vec<Vec<u8>> = sampled.iter().map(|f| f.pixels.clone()).collect();
    let boundaries = analyze::segment(&pixels, config.boundary_threshold);

    let frames_dir = bundle_dir.join(FRAMES_DIR);
    fs::create_dir_all(&frames_dir)?;

    let mut shots = Vec::new();
    let mut start = 0usize;
    let ends: Vec<usize> = boundaries.iter().copied().chain([sampled.len()]).collect();
    for (index, &end) in ends.iter().enumerate() {
        if end <= start {
            continue;
        }
        let mid = usize::midpoint(start, end.saturating_sub(1));
        let timestamp = sampled[mid].timestamp_secs;
        let kf_rel = format!("{FRAMES_DIR}/shot{index:03}.jpg");
        let native_rel = format!("{FRAMES_DIR}/shot{index:03}-native.jpg");
        export::export_frame(
            tools,
            &media,
            timestamp,
            &bundle_dir.join(&kf_rel),
            Some(config.max_keyframe_width),
        )?;
        export::export_frame(
            tools,
            &media,
            sampled[start].timestamp_secs,
            &bundle_dir.join(&native_rel),
            None,
        )?;
        shots.push(Shot {
            index,
            start_secs: sampled[start].timestamp_secs,
            end_secs: end_time(&sampled, end, manifest.media.duration_secs),
            motion_density: motion_density(&pixels[start..end]),
            keyframe: Keyframe {
                timestamp_secs: timestamp,
                path: kf_rel,
                native_path: Some(native_rel),
                hash: format!(
                    "{:016x}",
                    analyze::dhash(&analyze::to_gray(&sampled[mid].pixels))
                ),
            },
        });
        start = end;
    }

    let track = FrameTrack {
        schema_version: 1,
        shots,
    };
    let tmp = bundle_dir.join(format!("{FRAMES_FILE}.tmp"));
    fs::write(&tmp, serde_json::to_string_pretty(&track)?)?;
    fs::rename(&tmp, bundle_dir.join(FRAMES_FILE))?;
    manifest.tracks.frames = Some(FRAMES_FILE.to_string());
    save_manifest(bundle_dir, &manifest)?;
    Ok(track)
}

fn end_time(sampled: &[sample::SampledFrame], end: usize, duration: f64) -> f64 {
    sampled.get(end).map_or(duration, |f| f.timestamp_secs)
}

/// Mean frame-to-frame change across a shot, normalized to 0.0–1.0.
fn motion_density(shot_pixels: &[Vec<u8>]) -> f64 {
    if shot_pixels.len() < 2 {
        return 0.0;
    }
    let total: f64 = shot_pixels
        .windows(2)
        .map(|w| analyze::mean_abs_diff(&w[0], &w[1]))
        .sum();
    #[allow(clippy::cast_precision_loss)] // frame counts are small
    {
        total / ((shot_pixels.len() - 1) as f64) / 255.0
    }
}
