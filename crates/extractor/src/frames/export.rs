//! Keyframe export: agent-readable JPEGs (bounded width), native-
//! resolution stills, and native-resolution crops of one rectangle.
//!
//! One ffmpeg process exports one frame; a single export is dominated by
//! process startup plus the decode from the preceding keyframe, so
//! batches run several processes at once. Every frame is still produced
//! by the same command, so batched output is byte-identical to lone
//! exports and independent of scheduling.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use anyhow::{bail, Context, Result};

use crate::tools::Toolchain;

/// Upper bound on concurrent ffmpeg processes: beyond this, 4K decodes
/// contend for memory bandwidth and the batch gets slower, not faster.
const MAX_WORKERS: usize = 8;

/// A rectangle of the native frame, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// What an export keeps of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// The whole frame, width-bounded (aspect preserved, never upscaled).
    Bounded { max_width: u32 },
    /// The whole frame at native size.
    Native,
    /// One rectangle of the frame at native size.
    Crop(CropRect),
}

/// One frame to export: the moment, where it goes, and what to keep.
#[derive(Debug, Clone)]
pub struct FrameRequest {
    pub timestamp_secs: f64,
    pub dest: PathBuf,
    pub output: Output,
}

impl FrameRequest {
    /// An agent-readable export, width-bounded to `max_width`.
    #[must_use]
    pub fn bounded(timestamp_secs: f64, dest: PathBuf, max_width: u32) -> FrameRequest {
        FrameRequest {
            timestamp_secs,
            dest,
            output: Output::Bounded { max_width },
        }
    }

    /// A native-resolution still.
    #[must_use]
    pub fn native(timestamp_secs: f64, dest: PathBuf) -> FrameRequest {
        FrameRequest {
            timestamp_secs,
            dest,
            output: Output::Native,
        }
    }

    /// A native-resolution crop of `rect`.
    #[must_use]
    pub fn crop(timestamp_secs: f64, dest: PathBuf, rect: CropRect) -> FrameRequest {
        FrameRequest {
            timestamp_secs,
            dest,
            output: Output::Crop(rect),
        }
    }

    /// The ffmpeg video filter that produces this output, if any.
    fn filter(&self) -> Option<String> {
        match self.output {
            Output::Bounded { max_width } => Some(format!("scale='min({max_width},iw)':-2")),
            Output::Native => None,
            Output::Crop(CropRect {
                x,
                y,
                width,
                height,
            }) => Some(format!("crop={width}:{height}:{x}:{y}")),
        }
    }
}

/// Export one frame as the request describes.
pub fn export_frame(tools: &Toolchain, media: &Path, request: &FrameRequest) -> Result<()> {
    let timestamp_secs = request.timestamp_secs;
    let mut cmd = std::process::Command::new(&tools.ffmpeg);
    cmd.args(["-y", "-v", "error", "-ss", &format!("{timestamp_secs:.3}")])
        .arg("-i")
        .arg(media)
        .args(["-frames:v", "1"]);
    if let Some(filter) = request.filter() {
        cmd.args(["-vf", &filter]);
    }
    // -q:v 3: high-quality JPEG, ~100-300 KB at 1024px — the documented
    // per-frame size budget for agent consumption.
    cmd.args(["-q:v", "3"]).arg(&request.dest);
    let output = cmd.output().context("running ffmpeg frame export")?;
    if !output.status.success() {
        bail!(
            "frame export at {timestamp_secs:.3}s failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Export every request, several at a time. Any failure fails the batch;
/// the error reported is the earliest failing request in input order, so
/// messages don't depend on which process finished first.
pub fn export_frames(tools: &Toolchain, media: &Path, requests: &[FrameRequest]) -> Result<()> {
    let parallelism = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
    let workers = worker_count(parallelism, requests.len());
    let next = AtomicUsize::new(0);
    let failures: Mutex<Vec<(usize, anyhow::Error)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(request) = requests.get(index) else {
                    break;
                };
                if let Err(err) = export_frame(tools, media, request) {
                    failures
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((index, err));
                }
            });
        }
    });
    let mut failures = failures
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    failures.sort_by_key(|(index, _)| *index);
    match failures.into_iter().next() {
        Some((_, err)) => Err(err),
        None => Ok(()),
    }
}

/// Concurrent ffmpeg processes for a batch: one per two logical cores
/// (each ffmpeg already threads its own decode), at least one when there
/// is work, never more than the requests, capped at [`MAX_WORKERS`].
#[must_use]
pub fn worker_count(parallelism: usize, requests: usize) -> usize {
    (parallelism / 2).clamp(1, MAX_WORKERS).min(requests)
}
