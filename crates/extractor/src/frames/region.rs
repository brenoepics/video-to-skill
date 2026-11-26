//! The changed region: where on the screen a state differs from the
//! one before it, so the frame track can export that part of the
//! native frame beside the width-bounded keyframe. A 1080p frame is
//! downscaled before a model reads it and small code text goes
//! illegible; what changed is usually small (a terminal line, a
//! dialog), and a crop of it keeps native pixels.
//!
//! Following SeeAction (Zhou et al. 2025) the region is the bounding
//! box of the largest connected group of changed SSIM windows — a
//! pointer that moved elsewhere does not stretch the box across the
//! screen — padded for context and widened to a minimum size. Live
//! windows (a facecam) never count, and a change that covers most of
//! the screen, live or not, is a cut: the native still already shows it.

use super::export::CropRect;
use super::sample::{THUMB_H, THUMB_W};
use super::ssim::{LUMA_COLS, LUMA_ROWS, STEP, WINDOW};

/// An axis-aligned rectangle in thumbnail pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// Context kept around the changed windows, in thumbnail pixels (one
/// window step: ~24 px at 1080p).
pub const PAD: usize = STEP;
/// A region is never smaller than this: at 1080p, 480x144 px — about
/// five text lines of context around a one-line change.
pub const MIN_WIDTH: usize = THUMB_W / 4;
pub const MIN_HEIGHT: usize = 12;
/// A change of at least this share of the luma windows, or a padded
/// region covering that share of the screen, is the screen itself.
pub const WHOLE_SCREEN: f64 = 0.75;

/// The region of `changed` (per-window flags, luma first) that is not
/// `live`, or `None` when nothing changed or the whole screen did.
#[must_use]
pub fn changed_region(changed: &[bool], live: &[bool]) -> Option<Region> {
    let luma = &changed[..LUMA_COLS * LUMA_ROWS];
    #[allow(clippy::cast_precision_loss)] // window counts are small
    let changed_share = luma.iter().filter(|&&c| c).count() as f64 / luma.len() as f64;
    if changed_share >= WHOLE_SCREEN {
        return None;
    }
    let fresh: Vec<bool> = luma
        .iter()
        .zip(live)
        .map(|(&changed, &live)| changed && !live)
        .collect();
    let Bounds { rows, cols } = largest_component(&fresh)?;
    let (x, y) = (STEP * cols.0, STEP * rows.0);
    let region = grow(Region {
        x,
        y,
        width: STEP * cols.1 + WINDOW - x,
        height: STEP * rows.1 + WINDOW - y,
    });
    #[allow(clippy::cast_precision_loss)] // thumbnail areas are tiny
    let share = (region.width * region.height) as f64 / (THUMB_W * THUMB_H) as f64;
    (share < WHOLE_SCREEN).then_some(region)
}

impl Region {
    /// The same rectangle on a `width`x`height` native frame, widened
    /// to even edges (chroma alignment) and kept inside the frame.
    #[must_use]
    pub fn to_native(self, width: u32, height: u32) -> CropRect {
        let (x, right) = scale_span(self.x, self.x + self.width, THUMB_W, width);
        let (y, bottom) = scale_span(self.y, self.y + self.height, THUMB_H, height);
        CropRect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }
}

/// `[from, to)` thumbnail pixels as native pixels: the start rounded
/// down and the end up, both to even values, the end within `native`.
fn scale_span(from: usize, to: usize, thumb: usize, native: u32) -> (u32, u32) {
    let scale = |v: usize| u64::from(native) * v as u64 / thumb as u64;
    let begin = scale(from) & !1;
    let end = ((scale(to) + 1) & !1).min(u64::from(native));
    // Both lie within `native`, which is a u32.
    (
        u32::try_from(begin).unwrap_or(native),
        u32::try_from(end).unwrap_or(native),
    )
}

/// Inclusive `(first, last)` window rows and columns of a group.
struct Bounds {
    rows: (usize, usize),
    cols: (usize, usize),
}

/// Bounds of the largest 8-connected group of set windows (the first
/// found on a tie).
fn largest_component(fresh: &[bool]) -> Option<Bounds> {
    let mut seen = vec![false; fresh.len()];
    let mut best: Option<(usize, Bounds)> = None;
    for seed in (0..fresh.len()).filter(|&i| fresh[i]) {
        if seen[seed] {
            continue;
        }
        let mut size = 0usize;
        let mut bounds = Bounds {
            rows: (LUMA_ROWS, 0),
            cols: (LUMA_COLS, 0),
        };
        let mut stack = vec![seed];
        seen[seed] = true;
        while let Some(index) = stack.pop() {
            let (row, col) = (index / LUMA_COLS, index % LUMA_COLS);
            size += 1;
            bounds.rows = (bounds.rows.0.min(row), bounds.rows.1.max(row));
            bounds.cols = (bounds.cols.0.min(col), bounds.cols.1.max(col));
            for next in neighbours(row, col) {
                if fresh[next] && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        if best.as_ref().is_none_or(|(best_size, _)| size > *best_size) {
            best = Some((size, bounds));
        }
    }
    best.map(|(_, bounds)| bounds)
}

/// Indices of the up to eight windows around (row, col).
fn neighbours(row: usize, col: usize) -> impl Iterator<Item = usize> {
    let rows = row.saturating_sub(1)..=(row + 1).min(LUMA_ROWS - 1);
    rows.flat_map(move |r| {
        (col.saturating_sub(1)..=(col + 1).min(LUMA_COLS - 1)).map(move |c| r * LUMA_COLS + c)
    })
    .filter(move |&i| i != row * LUMA_COLS + col)
}

/// The region padded by [`PAD`] and widened to the minimum size around
/// its centre, kept inside the thumbnail.
fn grow(region: Region) -> Region {
    let (x, width) = span(region.x, region.width, MIN_WIDTH, THUMB_W);
    let (y, height) = span(region.y, region.height, MIN_HEIGHT, THUMB_H);
    Region {
        x,
        y,
        width,
        height,
    }
}

fn span(start: usize, len: usize, min: usize, limit: usize) -> (usize, usize) {
    let grown = (len + 2 * PAD).max(min).min(limit);
    let centre = start + len / 2;
    (centre.saturating_sub(grown / 2).min(limit - grown), grown)
}

#[cfg(test)]
mod tests;
