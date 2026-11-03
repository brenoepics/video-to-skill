//! Pure frame analysis: screen-state segmentation, perceptual hashes
//! and motion density over the sampled thumbnails. No I/O; unit-tested.
//!
//! A screen *state* is a stretch of the recording that shows one
//! result — a terminal after its command printed, a dialog once open.
//! Following GIFdroid (Feng & Chen 2022) a state ends where the
//! structural similarity to the previous sample drops, and the new
//! state's evidence frame is the first sample after which the picture
//! holds still for [`STEADY_SECS`]: the settled result, not the
//! transition. The drop is measured per window ([`super::ssim`]): a
//! sample has changed when at least [`CHANGE_FRACTION`] of the screen's
//! windows look structurally different ([`WINDOW_CHANGE`]), which sees a
//! new text line but not a cursor or a moving pointer. Three refinements
//! for real tutorials: windows that keep changing (a facecam, a spinner,
//! a playing video) are *live* and never count; a sample that has
//! drifted from its state's evidence frame by [`DRIFT_RATIO`] times
//! that much (slow typing adds up) also ends the state; and a change the strip matcher
//! explains as a vertical scroll ([`super::scroll`]) is folded into the
//! running state instead of ending it.

use super::sample::{SampledFrame, THUMB_H, THUMB_W};
use super::scroll::scroll_shift;
use super::ssim::{window_ssims, WINDOWS};

/// Per-window SSIM below which that patch of the screen has changed.
pub const WINDOW_CHANGE: f64 = 0.7;
/// Fraction of (non-live) windows that must change between consecutive
/// samples for the screen to be in a new state.
pub const CHANGE_FRACTION: f64 = 0.02;
/// A steady sample that differs from its state's evidence frame by
/// this many times the change fraction has drifted into a new state
/// (small edits that added up).
pub const DRIFT_RATIO: f64 = 2.0;
/// A window that changed in at least [`LIVE_MIN`] of [`LIVE_SPAN`]
/// consecutive transitions is animating; it stays ignored for
/// [`LIVE_MEMORY`] transitions after it last was, so a presenter who
/// pauses does not start a state when they move again.
pub const LIVE_SPAN: usize = 5;
pub const LIVE_MIN: usize = 3;
pub const LIVE_MEMORY: usize = 10;
/// How long the picture must hold still before a sample counts as the
/// settled result of a change (Video2Action's steady-state rule).
pub const STEADY_SECS: f64 = 1.0;

/// Thumbnail dimensions of the perceptual hash (9 wide for dHash's 8
/// horizontal comparisons per row, 8 tall).
pub const HASH_W: usize = 9;
pub const HASH_H: usize = 8;

/// One screen state, as sample indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    /// First sample that differs from the previous state.
    pub start: usize,
    /// The sample that shows the settled result — the keyframe.
    pub evidence: usize,
}

/// Tunables for [`segment_states`].
#[derive(Debug, Clone, Copy)]
pub struct StateRule {
    pub change_fraction: f64,
    /// Consecutive steady sample-to-sample transitions required after a
    /// change before a sample is the evidence frame.
    pub steady_samples: usize,
}

/// Split sampled frames into screen states; always at least one state
/// when there are frames.
#[must_use]
pub fn segment_states(frames: &[SampledFrame], rule: &StateRule) -> Vec<State> {
    let transitions = Transitions::of(frames);
    let mut states = Vec::new();
    let mut start = 0usize;
    while start < frames.len() {
        let evidence = transitions.first_steady(start, rule);
        let end = transitions.state_end(frames, evidence, rule);
        states.push(State { start, evidence });
        start = end;
    }
    states
}

/// Which windows changed at each sample-to-sample transition
/// (`changed[i]` is sample `i-1` → `i`; `changed[0]` is empty), and
/// which were animating just before it.
struct Transitions {
    changed: Vec<Vec<bool>>,
    animating: Vec<Vec<bool>>,
}

impl Transitions {
    fn of(frames: &[SampledFrame]) -> Transitions {
        let changed: Vec<Vec<bool>> = (0..frames.len())
            .map(|i| match i {
                0 => Vec::new(),
                _ => changed_windows(&frames[i - 1], &frames[i]),
            })
            .collect();
        let animating = (0..frames.len())
            .map(|i| animating_before(&changed, i))
            .collect();
        Transitions { changed, animating }
    }

    fn len(&self) -> usize {
        self.changed.len()
    }

    /// Windows to ignore at transition `i`: animating before it or
    /// before any of the [`LIVE_MEMORY`] transitions leading up to it.
    fn live(&self, i: usize) -> Vec<bool> {
        let mut live = vec![false; WINDOWS];
        for k in i.saturating_sub(LIVE_MEMORY)..=i {
            for (live, &hit) in live.iter_mut().zip(&self.animating[k]) {
                *live |= hit;
            }
        }
        live
    }

    /// Did the screen change at transition `i`, live windows aside?
    fn changed_at(&self, i: usize, rule: &StateRule) -> bool {
        fraction(&self.changed[i], &self.live(i)) >= rule.change_fraction
    }

    /// The first sample at or after `from` followed by `steady_samples`
    /// transitions without change (a scroll is movement, so it counts
    /// as change). The last sample is steady by definition.
    fn first_steady(&self, from: usize, rule: &StateRule) -> usize {
        (from..self.len())
            .find(|&i| {
                (i + 1..self.len())
                    .take(rule.steady_samples)
                    .all(|j| !self.changed_at(j, rule))
            })
            .unwrap_or(self.len() - 1)
    }

    /// Walk from a state's evidence sample to the first sample that
    /// shows a different state: one that changed since the previous
    /// sample, or drifted from the anchor. A change the strip matcher
    /// explains as a scroll re-anchors the state instead of ending it.
    fn state_end(&self, frames: &[SampledFrame], evidence: usize, rule: &StateRule) -> usize {
        let mut anchor = evidence;
        for i in evidence + 1..self.len() {
            let (prev, cur) = (&frames[i - 1], &frames[i]);
            if self.changed_at(i, rule) {
                if scroll_shift(prev.luma(), cur.luma()).is_none() {
                    return i;
                }
                anchor = i;
                continue;
            }
            let drift = fraction(&changed_windows(&frames[anchor], cur), &self.live(i));
            if drift >= DRIFT_RATIO * rule.change_fraction {
                if scroll_shift(frames[anchor].luma(), cur.luma()).is_none() {
                    return i;
                }
                anchor = i;
            }
        }
        self.len()
    }
}

/// Windows that changed in at least [`LIVE_MIN`] of the [`LIVE_SPAN`]
/// transitions before transition `i`.
fn animating_before(changed: &[Vec<bool>], i: usize) -> Vec<bool> {
    let mut counts = vec![0usize; WINDOWS];
    // Transition 0 has nothing before it; `changed[0]` is the empty vec.
    let first = i.saturating_sub(LIVE_SPAN).clamp(1.min(i), i);
    for transition in &changed[first..i] {
        for (count, &hit) in counts.iter_mut().zip(transition) {
            *count += usize::from(hit);
        }
    }
    counts.into_iter().map(|c| c >= LIVE_MIN).collect()
}

fn changed_windows(a: &SampledFrame, b: &SampledFrame) -> Vec<bool> {
    window_ssims(a, b)
        .iter()
        .map(|&s| s < WINDOW_CHANGE)
        .collect()
}

/// Share of all windows that changed and are not live.
fn fraction(changed: &[bool], live: &[bool]) -> f64 {
    let fresh = changed
        .iter()
        .zip(live)
        .filter(|&(&changed, &live)| changed && !live)
        .count();
    #[allow(clippy::cast_precision_loss)] // window counts are small
    {
        fresh as f64 / WINDOWS as f64
    }
}

/// 64-bit difference hash of a luma thumbnail: box-downscaled to
/// [`HASH_W`]x[`HASH_H`], each bit compares horizontal neighbours.
#[must_use]
pub fn thumbnail_hash(luma: &[u8]) -> u64 {
    let gray = downscale(luma);
    let mut hash = 0u64;
    let mut bit = 0;
    for row in 0..HASH_H {
        for col in 0..HASH_W - 1 {
            if gray[row * HASH_W + col] > gray[row * HASH_W + col + 1] {
                hash |= 1 << bit;
            }
            bit += 1;
        }
    }
    hash
}

/// Box-average a [`THUMB_W`]x[`THUMB_H`] luma plane to the hash grid.
fn downscale(luma: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HASH_W * HASH_H);
    for row in 0..HASH_H {
        let (y0, y1) = (row * THUMB_H / HASH_H, (row + 1) * THUMB_H / HASH_H);
        for col in 0..HASH_W {
            let (x0, x1) = (col * THUMB_W / HASH_W, (col + 1) * THUMB_W / HASH_W);
            let total: u32 = (y0..y1)
                .flat_map(|y| luma[y * THUMB_W + x0..y * THUMB_W + x1].iter())
                .map(|&v| u32::from(v))
                .sum();
            #[allow(clippy::cast_possible_truncation)] // a mean of bytes is a byte
            out.push((total / ((y1 - y0) * (x1 - x0)) as u32) as u8);
        }
    }
    out
}

/// Mean absolute byte difference, 0.0–255.0.
#[must_use]
pub fn mean_abs_diff(a: &[u8], b: &[u8]) -> f64 {
    let total: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    #[allow(clippy::cast_precision_loss)] // sums are far below 2^52
    {
        total as f64 / a.len() as f64
    }
}

/// Mean sample-to-sample luma change across a state, normalized to
/// 0.0–1.0. High values flag demos/action worth denser inspection.
#[must_use]
pub fn motion_density(frames: &[SampledFrame]) -> f64 {
    if frames.len() < 2 {
        return 0.0;
    }
    let total: f64 = frames
        .windows(2)
        .map(|w| mean_abs_diff(w[0].luma(), w[1].luma()))
        .sum();
    #[allow(clippy::cast_precision_loss)] // frame counts are small
    {
        total / ((frames.len() - 1) as f64) / 255.0
    }
}

#[cfg(test)]
mod tests;
