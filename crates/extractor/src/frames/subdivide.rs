//! Periodic interior keyframes for long low-motion shots, so the
//! initial keyframe pass covers slow screencasts (typing barely moves
//! tiny thumbnails, leaving multi-minute shots behind one keyframe).
//! Pure planning: the caller exports the planned frames in one batch.

use super::sample::SampledFrame;
use super::{hash, FrameConfig, Keyframe};

/// Interior candidates closer than this to the shot's main keyframe
/// are duplicates and are skipped.
const MAIN_KEYFRAME_GAP_SECS: f64 = 1.0;

/// One shot's span plus what subdivision needs to hash and name frames.
pub struct ShotSpan<'a> {
    pub index: usize,
    pub start_secs: f64,
    pub end_secs: f64,
    /// Timestamp of the shot's main keyframe (dedup anchor).
    pub main_keyframe_secs: f64,
    /// The shot's sampled thumbnails, for stable interior hashes.
    pub samples: &'a [SampledFrame],
}

/// Plan width-bounded interior keyframes at `start + k * interval`
/// (k = 1, 2, ...) strictly inside the shot — no native variants.
/// Returns them in timestamp order with bundle-relative paths; empty
/// when the shot fits within one interval.
#[must_use]
pub fn plan_interior_keyframes(shot: &ShotSpan, config: &FrameConfig) -> Vec<Keyframe> {
    let interval = config.subdivision_interval_secs;
    let mut keyframes = Vec::new();
    if interval <= 0.0 {
        return keyframes;
    }
    let Some(last_sampled) = shot.samples.last().map(|f| f.timestamp_secs) else {
        return keyframes;
    };
    for k in 1u32.. {
        let timestamp = shot.start_secs + f64::from(k) * interval;
        // Stay strictly inside the shot and on sampled ground — past
        // the last sampled frame the media may have nothing to decode.
        if timestamp >= shot.end_secs || timestamp > last_sampled {
            break;
        }
        if (timestamp - shot.main_keyframe_secs).abs() <= MAIN_KEYFRAME_GAP_SECS {
            continue;
        }
        let rel = format!(
            "{}/shot{:03}-k{:02}.jpg",
            super::FRAMES_DIR,
            shot.index,
            keyframes.len() + 1
        );
        keyframes.push(Keyframe {
            timestamp_secs: timestamp,
            path: rel,
            native_path: None,
            hash: nearest_sample_hash(shot.samples, timestamp),
        });
    }
    keyframes
}

/// Perceptual hash of the sampled thumbnail nearest to `timestamp` —
/// a deterministic identity without a second decode pass.
fn nearest_sample_hash(samples: &[SampledFrame], timestamp: f64) -> String {
    samples
        .iter()
        .min_by(|a, b| {
            (a.timestamp_secs - timestamp)
                .abs()
                .total_cmp(&(b.timestamp_secs - timestamp).abs())
        })
        .map(|f| format!("{:016x}", hash::thumbnail_hash(f.luma())))
        .unwrap_or_default()
}
