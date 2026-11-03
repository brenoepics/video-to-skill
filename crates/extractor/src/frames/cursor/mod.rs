//! Where the mouse pointer is, sample by sample, and what follows from
//! it: how much of each screen state had a pointer on screen (a screen
//! recording, as opposed to slides or a camera — VideoAgentTrek's
//! ScreenFilter keeps stretches with a pointer in 80 % of frames), and
//! native crops around the pointer just before and after each state
//! change (GUI Narrator's cursor-centred crops, which lifted action
//! narration from 20.8 to 38.6).
//!
//! The pointer is tracked on a second, mid-resolution decode of the
//! video ([`MID_W`]x[`MID_H`] gray): at thumbnail size a pointer is a
//! fraction of a pixel. See [`tracker`] for the rule.

pub mod arrow;
pub mod blobs;
pub mod evidence;
mod pass;
pub mod tracker;

use std::ops::Range;
use std::path::Path;

use anyhow::Result;

use self::tracker::{Fix, Kind, Tracker};
use super::analyze::State;
use crate::tools::Toolchain;

/// Size of the gray plane the pointer is tracked on: a 1x pointer on a
/// 1080p recording is about six pixels tall here.
pub const MID_W: usize = 640;
pub const MID_H: usize = 360;

/// One optional fix per sample, aligned with the thumbnail samples.
pub struct CursorTrack {
    pub fixes: Vec<Option<Fix>>,
    native: (u32, u32),
}

impl CursorTrack {
    /// Share of the samples in `range` with a pointer on screen; samples
    /// beyond the track count as without one.
    #[must_use]
    pub fn presence(&self, range: Range<usize>) -> f64 {
        if range.is_empty() {
            return 0.0;
        }
        let seen = range
            .clone()
            .filter(|&i| self.fixes.get(i).is_some_and(Option::is_some))
            .count();
        #[allow(clippy::cast_precision_loss)] // sample counts are small
        {
            seen as f64 / range.len() as f64
        }
    }

    /// The pointer's tip at sample `index`, in native pixels.
    #[must_use]
    pub fn native_tip(&self, index: usize) -> Option<(u32, u32)> {
        let (x, y) = self.fixes.get(index).copied().flatten()?.tip();
        let scale = |v: usize, native: u32, mid: usize| {
            u32::try_from(u64::from(native) * v as u64 / mid as u64).unwrap_or(native)
        };
        Some((
            scale(x, self.native.0, MID_W).min(self.native.0 - 1),
            scale(y, self.native.1, MID_H).min(self.native.1 - 1),
        ))
    }
}

/// Track the pointer through `media` at `fps` samples per second (the
/// thumbnail rate, so sample indices line up), on a `native`-sized frame.
pub fn track_cursor(
    tools: &Toolchain,
    media: &Path,
    fps: f64,
    native: (u32, u32),
    states: &[State],
) -> Result<CursorTrack> {
    let mut tracker = Tracker::new(MID_W);
    pass::stream_gray(tools, media, fps, &mut |frame| tracker.observe(frame))?;
    let mut fixes = tracker.finish();
    let starts: Vec<usize> = states.iter().map(|s| s.start).collect();
    backfill(&mut fixes, &starts);
    Ok(CursorTrack { fixes, native })
}

/// A pointer seen leaving a place had been there: fill the fix-less
/// samples before a departure, back to the start of its screen state
/// (`starts`, ascending), with that place.
pub fn backfill(fixes: &mut [Option<Fix>], starts: &[usize]) {
    for (i, &start) in starts.iter().enumerate() {
        let len = fixes.len();
        let end = starts.get(i + 1).copied().unwrap_or(len);
        let span = &mut fixes[start.min(len)..end.min(len)];
        let Some(first) = span.iter().position(Option::is_some) else {
            continue;
        };
        if let Some(departure) = span[first].filter(|f| f.kind == Kind::Departure) {
            for fix in &mut span[..first] {
                *fix = Some(Fix {
                    kind: Kind::Backfilled,
                    ..departure
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
