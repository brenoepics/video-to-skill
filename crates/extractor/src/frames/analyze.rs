//! Pure frame analysis: perceptual signatures and shot segmentation.
//! Operates on tiny RGB thumbnails; no I/O, fully unit-tested.
//!
//! Boundary detection compares RGB (not grayscale) so isoluminant cuts —
//! e.g. ffmpeg's red (#ff0000) and green (#008000), whose lumas differ by
//! ~1/255 — are still seen. Hashing uses derived luma, which is standard.

/// Thumbnail dimensions the sampler produces (9 wide for dHash's 8
/// horizontal comparisons per row, 8 tall).
pub const TINY_W: usize = 9;
pub const TINY_H: usize = 8;
/// Bytes per sampled RGB thumbnail.
pub const TINY_RGB_LEN: usize = TINY_W * TINY_H * 3;

/// BT.601 luma of an rgb24 thumbnail.
#[must_use]
pub fn to_gray(rgb: &[u8]) -> Vec<u8> {
    rgb.chunks_exact(3)
        .map(|px| {
            let y =
                (299 * u32::from(px[0]) + 587 * u32::from(px[1]) + 114 * u32::from(px[2])) / 1000;
            #[allow(clippy::cast_possible_truncation)] // result is ≤ 255
            {
                y as u8
            }
        })
        .collect()
}

/// 64-bit difference hash over a grayscale thumbnail: each bit compares
/// horizontal neighbours.
#[must_use]
pub fn dhash(gray: &[u8]) -> u64 {
    let mut hash = 0u64;
    let mut bit = 0;
    for row in 0..TINY_H {
        for col in 0..TINY_W - 1 {
            if gray[row * TINY_W + col] > gray[row * TINY_W + col + 1] {
                hash |= 1 << bit;
            }
            bit += 1;
        }
    }
    hash
}

/// Mean absolute byte difference, 0.0–255.0 — works on RGB or gray.
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

/// Segment a frame sequence into shots; returns boundary indices.
///
/// A boundary candidate fires when a frame drifts beyond `threshold` from
/// either its predecessor or the current shot's anchor; consecutive
/// candidates (a gradual fade) coalesce into a single boundary.
#[must_use]
pub fn segment(frames: &[Vec<u8>], threshold: f64) -> Vec<usize> {
    let mut candidates = Vec::new();
    let Some(mut anchor) = frames.first() else {
        return vec![];
    };
    for i in 1..frames.len() {
        let from_prev = mean_abs_diff(&frames[i], &frames[i - 1]);
        let from_anchor = mean_abs_diff(&frames[i], anchor);
        if from_prev > threshold || from_anchor > threshold {
            candidates.push(i);
            anchor = &frames[i];
        }
    }
    // Coalesce runs of adjacent candidates into a single boundary (the
    // index where the transition settles).
    let mut boundaries: Vec<usize> = Vec::new();
    for idx in candidates {
        match boundaries.last_mut() {
            Some(last) if idx == *last + 1 => *last = idx,
            _ => boundaries.push(idx),
        }
    }
    boundaries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_gray(value: u8) -> Vec<u8> {
        vec![value; TINY_W * TINY_H]
    }

    fn flat_rgb(r: u8, g: u8, b: u8) -> Vec<u8> {
        [r, g, b].repeat(TINY_W * TINY_H)
    }

    #[test]
    fn dhash_sees_gradients_and_ignores_brightness() {
        let mut ramp = Vec::with_capacity(TINY_W * TINY_H);
        for row in 0..TINY_H {
            for col in 0..TINY_W {
                // Decreasing left-to-right so left > right comparisons fire.
                #[allow(clippy::cast_possible_truncation)]
                ramp.push(((TINY_W - col) * 20 + row) as u8);
            }
        }
        assert_eq!(dhash(&flat_gray(10)), dhash(&flat_gray(200)));
        assert_ne!(dhash(&ramp), dhash(&flat_gray(10)));
    }

    #[test]
    fn isoluminant_color_change_is_still_a_boundary() {
        // ffmpeg red vs green: nearly equal luma, very different RGB.
        let red = flat_rgb(255, 0, 0);
        let green = flat_rgb(0, 128, 0);
        assert!(mean_abs_diff(&red, &green) > 100.0);
        let luma_gap = mean_abs_diff(&to_gray(&red), &to_gray(&green));
        assert!(luma_gap < 5.0, "gray would have missed it: {luma_gap}");
    }

    #[test]
    fn hard_changes_produce_one_boundary_each() {
        let frames = vec![
            flat_gray(20),
            flat_gray(20),
            flat_gray(120),
            flat_gray(120),
            flat_gray(220),
            flat_gray(220),
        ];
        assert_eq!(segment(&frames, 25.0), vec![2, 4]);
    }

    #[test]
    fn a_gradual_drift_coalesces_into_one_boundary() {
        let frames = vec![
            flat_gray(20),
            flat_gray(20),
            flat_gray(60),
            flat_gray(100),
            flat_gray(140),
            flat_gray(140),
        ];
        assert_eq!(segment(&frames, 25.0).len(), 1);
    }

    #[test]
    fn static_content_has_no_boundaries() {
        let frames = vec![flat_gray(90); 6];
        assert!(segment(&frames, 25.0).is_empty());
    }
}
