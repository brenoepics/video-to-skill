//! Perceptual hash of a sampled thumbnail: a 64-bit difference hash
//! that gives a keyframe a stable identity across runs.

use super::sample::{THUMB_H, THUMB_W};

/// Thumbnail dimensions of the perceptual hash (9 wide for dHash's 8
/// horizontal comparisons per row, 8 tall).
pub const HASH_W: usize = 9;
pub const HASH_H: usize = 8;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_hash_sees_gradients_and_ignores_brightness() {
        let flat_dim = vec![10u8; THUMB_W * THUMB_H];
        let flat_bright = vec![200u8; THUMB_W * THUMB_H];
        let ramp: Vec<u8> = (0..THUMB_H)
            .flat_map(|_| (0..THUMB_W).map(|x| u8::try_from(255 - x).unwrap()))
            .collect();
        assert_eq!(thumbnail_hash(&flat_dim), thumbnail_hash(&flat_bright));
        assert_ne!(thumbnail_hash(&ramp), thumbnail_hash(&flat_dim));
        assert_eq!(thumbnail_hash(&ramp), 0xffff_ffff_ffff_ffff);
    }
}
