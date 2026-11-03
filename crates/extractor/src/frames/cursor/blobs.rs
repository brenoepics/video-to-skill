//! Temporal difference and connected components on a gray plane: the
//! small groups of pixels that changed between two samples are where a
//! cursor left and where it landed.

/// A connected group of set pixels: its bounding box (right and bottom
/// exclusive) and how many pixels it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blob {
    pub left: usize,
    pub top: usize,
    pub right: usize,
    pub bottom: usize,
    pub pixels: usize,
}

impl Blob {
    #[must_use]
    pub fn width(&self) -> usize {
        self.right - self.left
    }

    #[must_use]
    pub fn height(&self) -> usize {
        self.bottom - self.top
    }
}

/// Pixels where `a` and `b` differ by more than `threshold`.
#[must_use]
pub fn changed_pixels(a: &[u8], b: &[u8], threshold: u8) -> Vec<bool> {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| x.abs_diff(y) > threshold)
        .collect()
}

/// The 8-connected groups of set pixels on a `width`-wide grid, in scan
/// order of their first pixel.
#[must_use]
pub fn blobs(mask: &[bool], width: usize) -> Vec<Blob> {
    let height = mask.len() / width;
    let mut seen = vec![false; mask.len()];
    let mut found = Vec::new();
    let mut stack = Vec::new();
    for seed in (0..mask.len()).filter(|&i| mask[i]) {
        if seen[seed] {
            continue;
        }
        let mut blob = Blob {
            left: width,
            top: height,
            right: 0,
            bottom: 0,
            pixels: 0,
        };
        seen[seed] = true;
        stack.push(seed);
        while let Some(index) = stack.pop() {
            let (x, y) = (index % width, index / width);
            blob.left = blob.left.min(x);
            blob.right = blob.right.max(x + 1);
            blob.top = blob.top.min(y);
            blob.bottom = blob.bottom.max(y + 1);
            blob.pixels += 1;
            for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    let next = ny * width + nx;
                    if mask[next] && !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        found.push(blob);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&str]) -> (Vec<bool>, usize) {
        let width = rows[0].len();
        (
            rows.iter()
                .flat_map(|r| r.chars().map(|c| c == '#'))
                .collect(),
            width,
        )
    }

    #[test]
    fn changed_pixels_compares_against_a_threshold() {
        let a = [10u8, 100, 200, 50];
        let b = [20u8, 100, 100, 0];
        assert_eq!(changed_pixels(&a, &b, 15), vec![false, false, true, true]);
        assert_eq!(changed_pixels(&a, &b, 10), vec![false, false, true, true]);
        assert_eq!(changed_pixels(&a, &b, 9), vec![true, false, true, true]);
    }

    #[test]
    fn blobs_are_boxed_and_counted_with_diagonals_joined() {
        let (mask, width) = grid(&[
            "##......#.", //
            ".#.......#", //
            "...##.....", //
            "....#.....", //
        ]);
        let found = blobs(&mask, width);
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(
            found[0],
            Blob {
                left: 0,
                top: 0,
                right: 2,
                bottom: 2,
                pixels: 3
            }
        );
        assert_eq!((found[1].left, found[1].right, found[1].pixels), (8, 10, 2));
        assert_eq!(
            (found[2].width(), found[2].height(), found[2].pixels),
            (2, 2, 3)
        );
    }

    #[test]
    fn an_empty_mask_has_no_blobs() {
        assert_eq!(blobs(&[false; 12], 4), Vec::<Blob>::new());
    }
}
