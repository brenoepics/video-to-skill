//! Structural similarity (SSIM, Wang et al. 2004) per window of two
//! sampled frames: 1.0 where a patch is identical, falling as its
//! luminance, contrast or structure diverge. Unlike a pixel difference
//! it is insensitive to codec noise and brightness drift yet sensitive
//! to a new line of text, which is why GUI-recording state detectors
//! (GIFdroid) use it.
//!
//! Windows are 8x8, stepped 4 pixels (half overlap) — the common fast
//! variant of the paper's 11x11 Gaussian window; constants use its
//! K1 = 0.01, K2 = 0.03 at an 8-bit dynamic range. Luma windows carry
//! the structure (text, panes, cursors); the chroma planes' windows are
//! included so an isoluminant cut — ffmpeg's red (#ff0000) and green
//! (#008000) differ in luma by ~1/255 — is seen too.

use super::sample::{SampledFrame, CHROMA_H, CHROMA_W, THUMB_H, THUMB_W};

const WINDOW: usize = 8;
const STEP: usize = 4;
const C1: f64 = (0.01 * 255.0) * (0.01 * 255.0);
const C2: f64 = (0.03 * 255.0) * (0.03 * 255.0);

const fn windows_in(width: usize, height: usize) -> usize {
    ((width - WINDOW) / STEP + 1) * ((height - WINDOW) / STEP + 1)
}

/// Windows per frame: luma first, then U, then V.
pub const WINDOWS: usize = windows_in(THUMB_W, THUMB_H) + 2 * windows_in(CHROMA_W, CHROMA_H);

/// SSIM of every window of `a` against the same window of `b`, in
/// a fixed order ([`WINDOWS`] values).
#[must_use]
pub fn window_ssims(a: &SampledFrame, b: &SampledFrame) -> Vec<f64> {
    let mut out = Vec::with_capacity(WINDOWS);
    plane_windows(a.luma(), b.luma(), THUMB_W, THUMB_H, &mut out);
    let (au, av) = a.chroma();
    let (bu, bv) = b.chroma();
    plane_windows(au, bu, CHROMA_W, CHROMA_H, &mut out);
    plane_windows(av, bv, CHROMA_W, CHROMA_H, &mut out);
    out
}

/// Mean SSIM over all windows of two `width`x`height` planes.
#[cfg(test)]
fn ssim(a: &[u8], b: &[u8], width: usize, height: usize) -> f64 {
    let mut values = Vec::with_capacity(windows_in(width, height));
    plane_windows(a, b, width, height, &mut values);
    #[allow(clippy::cast_precision_loss)] // window counts are small
    {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn plane_windows(a: &[u8], b: &[u8], width: usize, height: usize, out: &mut Vec<f64>) {
    for top in (0..=height - WINDOW).step_by(STEP) {
        for left in (0..=width - WINDOW).step_by(STEP) {
            out.push(window_ssim(a, b, width, top, left));
        }
    }
}

fn window_ssim(a: &[u8], b: &[u8], width: usize, top: usize, left: usize) -> f64 {
    let (mut total_a, mut total_b, mut squares_a, mut squares_b, mut cross) =
        (0u32, 0u32, 0u32, 0u32, 0u32);
    for row in top..top + WINDOW {
        let base = row * width + left;
        for (&x, &y) in a[base..base + WINDOW].iter().zip(&b[base..base + WINDOW]) {
            let (x, y) = (u32::from(x), u32::from(y));
            total_a += x;
            total_b += y;
            squares_a += x * x;
            squares_b += y * y;
            cross += x * y;
        }
    }
    #[allow(clippy::cast_precision_loss)] // 64 pixels
    let n = (WINDOW * WINDOW) as f64;
    let mean_a = f64::from(total_a) / n;
    let mean_b = f64::from(total_b) / n;
    let var_a = f64::from(squares_a) / n - mean_a * mean_a;
    let var_b = f64::from(squares_b) / n - mean_b * mean_b;
    let cov = f64::from(cross) / n - mean_a * mean_b;
    ((2.0 * mean_a * mean_b + C1) * (2.0 * cov + C2))
        / ((mean_a * mean_a + mean_b * mean_b + C1) * (var_a + var_b + C2))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `width`x`height` plane whose pixel (x, y) is `f(x, y)` mod 256.
    fn plane(width: usize, height: usize, f: impl Fn(usize, usize) -> usize) -> Vec<u8> {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| u8::try_from(f(x, y) % 256).unwrap())
            .collect()
    }

    #[test]
    fn identical_planes_are_fully_similar() {
        let a = plane(32, 16, |x, y| x * 7 + y * 13);
        assert!((ssim(&a, &a, 32, 16) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_small_brightness_drift_barely_registers() {
        let a = plane(32, 16, |x, y| (x * 7 + y * 13) % 200);
        let b: Vec<u8> = a.iter().map(|v| v + 4).collect();
        assert!(ssim(&a, &b, 32, 16) > 0.98);
    }

    #[test]
    fn a_new_bright_line_changes_its_own_windows_and_no_others() {
        let dark = plane(64, 32, |_, _| 20);
        let with_line = plane(64, 32, |x, y| {
            if (10..12).contains(&y) && x < 50 {
                200
            } else {
                20
            }
        });
        let mut windows = Vec::new();
        plane_windows(&dark, &with_line, 64, 32, &mut windows);
        let changed = windows.iter().filter(|&&s| s < 0.7).count();
        // Rows 10-11 lie in windows with tops 4 and 8 (two window rows);
        // columns 0..50 in windows with lefts 0..=48 (13 columns).
        assert_eq!(changed, 2 * 13, "{windows:?}");
        assert!(windows.iter().filter(|&&s| s >= 0.7).all(|&s| s > 0.999));
    }

    #[test]
    fn unrelated_pictures_are_dissimilar() {
        let a = plane(64, 32, |x, _| x * 4);
        let b = plane(64, 32, |_, y| y * 8);
        assert!(ssim(&a, &b, 64, 32) < 0.3);
    }

    #[test]
    fn an_isoluminant_cut_shows_up_in_the_chroma_windows() {
        // Two flat frames with the same luma but opposite chroma.
        let luma = vec![76u8; THUMB_W * THUMB_H];
        let mut red = SampledFrame::from_luma(0.0, &luma);
        let mut green = SampledFrame::from_luma(1.0, &luma);
        red.set_chroma_for_test(90, 240);
        green.set_chroma_for_test(54, 34);
        assert!(window_ssims(&red, &red).iter().all(|&s| s > 0.999));
        let changed = window_ssims(&red, &green)
            .iter()
            .filter(|&&s| s < 0.7)
            .count();
        assert!(changed * 10 > WINDOWS, "{changed} of {WINDOWS} windows");
        assert_eq!(WINDOWS, 819 + 2 * 190);
    }
}
