//! The frame track: screen states (recorded as `shots`), one evidence
//! keyframe per state, and a motion-density signal, written to
//! `frames.json` in the bundle.
//!
//! A state is a stretch of the recording that shows one result; its
//! keyframe is the first sample after the change where the picture
//! holds still — the terminal after its output, the dialog once open —
//! rather than the midpoint of the stretch. See [`analyze`] for the
//! rule and [`scroll`] for why scrolling is not a state change.

pub mod analyze;
pub mod export;
mod plan;
mod sample;
mod scroll;
mod ssim;
mod subdivide;

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
    /// Fraction (0–1) of the screen's SSIM windows that must change
    /// between samples for the screen to be in a new state.
    pub change_fraction: f64,
    /// How long the picture must hold still after a change before a
    /// sample is that state's evidence keyframe.
    pub steady_secs: f64,
    /// Width bound for agent-readable keyframes (native stills keep
    /// full resolution).
    pub max_keyframe_width: u32,
    /// Shots longer than this gain periodic interior keyframes, so
    /// slow screencasts stay covered by the initial pass.
    pub subdivision_interval_secs: f64,
}

impl Default for FrameConfig {
    fn default() -> FrameConfig {
        FrameConfig {
            sample_fps: 1.0,
            change_fraction: analyze::CHANGE_FRACTION,
            steady_secs: analyze::STEADY_SECS,
            max_keyframe_width: 1024,
            subdivision_interval_secs: 15.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameTrack {
    pub schema_version: u32,
    pub shots: Vec<Shot>,
}

/// One screen state (the field keeps its historical name `shots`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    pub index: usize,
    /// First sampled second that differs from the previous state.
    pub start_secs: f64,
    pub end_secs: f64,
    /// Mean frame-to-frame change inside the shot, 0.0–1.0. High values
    /// flag demos/action worth denser inspection.
    pub motion_density: f64,
    /// The state's evidence frame: the first steady second after the
    /// change that started it.
    pub keyframe: Keyframe,
    /// Periodic keyframes inside long shots (one per subdivision
    /// interval), width-bounded, no native variants.
    #[serde(default)]
    pub interior_keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub timestamp_secs: f64,
    /// Bundle-relative path of the width-bounded JPEG.
    pub path: String,
    /// Bundle-relative path of the native-resolution still of the same
    /// moment, for reading on-screen text.
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

    let sampled = sample::sample_thumbnails(tools, &media, config.sample_fps)?;
    let rule = analyze::StateRule {
        change_fraction: config.change_fraction,
        steady_samples: steady_samples(config),
    };
    let states = analyze::segment_states(&sampled, &rule);

    let frames_dir = bundle_dir.join(FRAMES_DIR);
    fs::create_dir_all(&frames_dir)?;

    // Plan every frame first, then export them all in one concurrent
    // batch: exports are independent and dominate this stage's time.
    let (shots, requests) = plan::plan_shots(
        &states,
        &sampled,
        manifest.media.duration_secs,
        bundle_dir,
        config,
    );
    export::export_frames(tools, &media, &requests)?;

    let track = FrameTrack {
        schema_version: 1,
        shots,
    };
    let path = bundle_dir.join(FRAMES_FILE);
    crate::fsx::write_atomic(&path, &serde_json::to_vec_pretty(&track)?)?;
    manifest.tracks.frames = Some(FRAMES_FILE.to_string());
    save_manifest(bundle_dir, &manifest)?;
    Ok(track)
}

/// Steady sample-to-sample transitions required after a change: the
/// steady duration in samples, at least one.
fn steady_samples(config: &FrameConfig) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // tiny, non-negative
    {
        (config.steady_secs * config.sample_fps).round().max(1.0) as usize
    }
}
