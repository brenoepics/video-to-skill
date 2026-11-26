//! Scroll detection by strip template matching (CAPdroid, Feng et al.
//! 2023): the earlier frame's luma is cut into horizontal strips and
//! each strip is looked for in the later frame at every vertical
//! offset. When most textured strips are found again at one common
//! non-zero offset the content merely moved — a scroll, not a new
//! screen state. A fast scroll that carries most of the screen away
//! finds too few strips and counts as a change, which is what a reader
//! wants: mostly new content deserves a new keyframe.
//!
//! At thumbnail scale a scroll rarely lands on a whole row, and a
//! two-row text line compared a third of a row off its true position
//! looks less like itself than like the next line down — whose only
//! difference is its length. So both frames are smoothed vertically
//! (a three-row box) and offsets are searched in half rows against a
//! vertically interpolated later frame; a residual misalignment then
//! costs far less than a different line's length does.

use super::sample::{THUMB_H, THUMB_W};

/// Strip height in thumbnail rows (~2.5 text lines at 1080p).
const STRIP_ROWS: usize = 6;
/// Largest offset searched, in rows: 40% of the screen. Beyond that a
/// scroll reveals mostly new content and is a state change anyway.
const MAX_SHIFT: usize = 36;
/// A strip whose brightest and darkest pixels differ less than this
/// is background: it matches anywhere and casts no vote.
const MIN_CONTRAST: u8 = 32;
/// A strip counts as found where its mean absolute difference (0–255)
/// to the later frame drops below this.
const LOCATE_MAD: u32 = 8;
/// The same bound as a summed difference over one strip.
const LOCATE_TOTAL: u32 = LOCATE_MAD * STRIP_PIXELS;
#[allow(clippy::cast_possible_truncation)] // a strip is 960 pixels
const STRIP_PIXELS: u32 = (STRIP_ROWS * THUMB_W) as u32;
/// Fraction of textured strips that must be found at one offset.
const AGREEMENT: f64 = 0.6;
/// Never call a scroll on fewer agreeing strips than this.
const MIN_VOTES: usize = 3;
/// Offsets within this many half rows of a candidate agree with it.
const TOLERANCE: usize = 2;

/// The vertical shift (rows, positive = content moved up) that explains
/// `later` as a scrolled `earlier`, or `None` when it is not a scroll.
#[must_use]
pub fn scroll_shift(earlier: &[u8], later: &[u8]) -> Option<isize> {
    let earlier = smooth_rows(earlier);
    let later = half_rows(&smooth_rows(later));
    let mut textured = 0usize;
    let mut found = Vec::new();
    for top in (0..=THUMB_H - STRIP_ROWS).step_by(STRIP_ROWS) {
        let strip = &earlier[top * THUMB_W..(top + STRIP_ROWS) * THUMB_W];
        if contrast(strip) < MIN_CONTRAST {
            continue;
        }
        textured += 1;
        if let Some(shift) = locate(strip, &later, top) {
            found.push(shift);
        }
    }
    let (shift, votes) = best_cluster(&found)?;
    #[allow(clippy::cast_precision_loss)] // strip counts are tiny
    let agreed = votes as f64 >= AGREEMENT * textured as f64;
    (votes >= MIN_VOTES && agreed && shift != 0).then_some(shift / 2)
}

fn contrast(strip: &[u8]) -> u8 {
    let max = strip.iter().copied().max().unwrap_or(0);
    let min = strip.iter().copied().min().unwrap_or(0);
    max - min
}

/// The plane with each row replaced by the mean of itself and its two
/// neighbours (edge rows use what they have).
fn smooth_rows(plane: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(plane.len());
    for row in 0..THUMB_H {
        let above = row.saturating_sub(1);
        let below = (row + 1).min(THUMB_H - 1);
        for col in 0..THUMB_W {
            let sum = u16::from(plane[above * THUMB_W + col])
                + u16::from(plane[row * THUMB_W + col])
                + u16::from(plane[below * THUMB_W + col]);
            #[allow(clippy::cast_possible_truncation)] // a mean of bytes is a byte
            out.push((sum / 3) as u8);
        }
    }
    out
}

/// The plane with a linearly interpolated row between each pair of
/// rows: `2 * THUMB_H - 1` rows, row `2k` being original row `k`.
fn half_rows(plane: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity((2 * THUMB_H - 1) * THUMB_W);
    for row in 0..THUMB_H {
        out.extend_from_slice(&plane[row * THUMB_W..(row + 1) * THUMB_W]);
        if row + 1 < THUMB_H {
            let (a, b) = (
                &plane[row * THUMB_W..(row + 1) * THUMB_W],
                &plane[(row + 1) * THUMB_W..(row + 2) * THUMB_W],
            );
            out.extend(a.iter().zip(b).map(|(&x, &y)| u8::midpoint(x, y)));
        }
    }
    out
}

/// Where `strip` (taken from row `top`) reappears in `later` (a
/// half-row plane): the offset with the smallest summed difference,
/// nearest-to-zero on ties, if that difference is small enough to be
/// a match. The returned shift is in half rows.
fn locate(strip: &[u8], later: &[u8], top: usize) -> Option<isize> {
    let lowest = 2 * top.saturating_sub(MAX_SHIFT);
    let highest = 2 * (top + MAX_SHIFT).min(THUMB_H - STRIP_ROWS);
    (lowest..=highest)
        .map(|target| {
            #[allow(clippy::cast_possible_wrap)] // rows are far below isize::MAX
            let shift = 2 * top as isize - target as isize;
            (strip_difference(strip, later, target), shift)
        })
        .min_by_key(|&(difference, shift)| (difference, shift.abs()))
        .and_then(|(difference, shift)| (difference < LOCATE_TOTAL).then_some(shift))
}

/// Summed absolute difference between `strip` and the rows of the
/// half-row plane `later` starting at half row `target`, stepping two.
fn strip_difference(strip: &[u8], later: &[u8], target: usize) -> u32 {
    (0..STRIP_ROWS)
        .flat_map(|row| {
            let base = (target + 2 * row) * THUMB_W;
            strip[row * THUMB_W..(row + 1) * THUMB_W]
                .iter()
                .zip(&later[base..base + THUMB_W])
                .map(|(&x, &y)| u32::from(x.abs_diff(y)))
        })
        .sum()
}

/// The offset most strips agree on (within [`TOLERANCE`]) and how many.
fn best_cluster(shifts: &[isize]) -> Option<(isize, usize)> {
    #[allow(clippy::cast_possible_wrap)] // a one-digit constant
    let tolerance = TOLERANCE as isize;
    shifts
        .iter()
        .map(|&center| {
            let votes = shifts
                .iter()
                .filter(|&&s| (s - center).abs() <= tolerance)
                .count();
            (center, votes)
        })
        .max_by_key(|&(center, votes)| (votes, std::cmp::Reverse(center.abs())))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terminal-like screen: text rows of varying length every 3 rows,
    /// drawn from `scroll` rows down the content (content row 0 is at
    /// screen row `-scroll`).
    fn screen(scroll: usize) -> Vec<u8> {
        let mut luma = vec![20u8; THUMB_W * THUMB_H];
        for row in 0..THUMB_H {
            let content_row = row + scroll;
            if !content_row.is_multiple_of(3) {
                continue;
            }
            let len = 40 + (content_row * 17) % 100;
            luma[row * THUMB_W..row * THUMB_W + len].fill(200);
        }
        luma
    }

    #[test]
    fn a_static_screen_is_not_a_scroll() {
        assert_eq!(scroll_shift(&screen(0), &screen(0)), None);
    }

    #[test]
    fn content_moving_up_a_few_rows_is_a_scroll() {
        assert_eq!(scroll_shift(&screen(0), &screen(6)), Some(6));
        assert_eq!(scroll_shift(&screen(6), &screen(0)), Some(-6));
    }

    #[test]
    fn a_new_line_on_an_otherwise_still_screen_is_not_a_scroll() {
        let before = screen(0);
        let mut after = before.clone();
        after[60 * THUMB_W..60 * THUMB_W + 90].fill(200);
        assert_eq!(scroll_shift(&before, &after), None);
    }

    #[test]
    fn a_scroll_that_replaces_most_of_the_screen_is_a_change() {
        assert_eq!(scroll_shift(&screen(0), &screen(75)), None);
    }

    #[test]
    fn a_blank_screen_casts_no_votes() {
        let blank = vec![20u8; THUMB_W * THUMB_H];
        assert_eq!(scroll_shift(&blank, &blank), None);
        assert_eq!(scroll_shift(&blank, &screen(3)), None);
    }
}
