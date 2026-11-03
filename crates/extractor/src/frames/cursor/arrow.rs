//! The arrow pointer as a template rendered at runtime, and a masked
//! correlation score that says how much a patch of a gray plane looks
//! like it. No downloaded model: the classic arrow (a dark body with a
//! light one-pixel outline, or the reverse) is a polygon every desktop
//! draws much the same way, so a few sizes of it cover the recordings
//! this tool sees. Other pointers (an I-beam, a hand) are not matched.

/// The classic arrow pointer polygon over a 12x19 cell, tip at the
/// top-left.
const ARROW: [(f64, f64); 7] = [
    (0.0, 0.0),
    (0.0, 16.0),
    (4.0, 12.0),
    (7.0, 18.0),
    (9.0, 17.0),
    (6.0, 11.0),
    (11.0, 11.0),
];
const CELL_W: f64 = 12.0;
const CELL_H: f64 = 19.0;

/// A rendered arrow: `width`x`height` pixels, each body, outline, or
/// neither (the ring of outline pixels surrounds the body).
#[derive(Debug, Clone)]
pub struct Template {
    pub width: usize,
    pub height: usize,
    body: Vec<bool>,
    outline: Vec<bool>,
}

impl Template {
    /// The arrow with a body `body_height` pixels tall plus its outline.
    #[must_use]
    pub fn render(body_height: usize) -> Template {
        #[allow(clippy::cast_precision_loss)] // small pixel counts
        let scale = body_height as f64 / CELL_H;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // ceil of a small positive
        let body_width = (CELL_W * scale).ceil() as usize;
        let (width, height) = (body_width + 2, body_height + 2);
        // Each pixel is sampled at four points so a small render keeps
        // the arrow's thin parts.
        let body: Vec<bool> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| {
                x >= 1 && y >= 1 && {
                    #[allow(clippy::cast_precision_loss)] // small pixel counts
                    let (px, py) = ((x - 1) as f64, (y - 1) as f64);
                    let hits = [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
                        .iter()
                        .filter(|(dx, dy)| inside_arrow((px + dx) / scale, (py + dy) / scale))
                        .count();
                    hits >= 2
                }
            })
            .collect();
        let outline = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| {
                !body[y * width + x]
                    && (y.saturating_sub(1)..=(y + 1).min(height - 1)).any(|r| {
                        (x.saturating_sub(1)..=(x + 1).min(width - 1)).any(|c| body[r * width + c])
                    })
            })
            .collect();
        Template {
            width,
            height,
            body,
            outline,
        }
    }

    /// How much the patch of `plane` (a `plane_width`-wide gray plane)
    /// with this template's top-left at `(x, y)` looks like an arrow:
    /// the absolute normalised correlation between the template's
    /// body/outline split and the patch's gray levels over those pixels
    /// (0 when the patch is flat or the template falls off the plane).
    #[must_use]
    pub fn score(&self, plane: &[u8], plane_width: usize, x: usize, y: usize) -> f64 {
        let plane_height = plane.len() / plane_width;
        if x + self.width > plane_width || y + self.height > plane_height {
            return 0.0;
        }
        let (mut n_body, mut sum_body, mut n_out, mut sum_out, mut squares) =
            (0u32, 0u64, 0u32, 0u64, 0u64);
        for row in 0..self.height {
            for col in 0..self.width {
                let i = row * self.width + col;
                let v = u64::from(plane[(y + row) * plane_width + x + col]);
                if self.body[i] {
                    n_body += 1;
                    sum_body += v;
                } else if self.outline[i] {
                    n_out += 1;
                    sum_out += v;
                } else {
                    continue;
                }
                squares += v * v;
            }
        }
        let n = f64::from(n_body + n_out);
        #[allow(clippy::cast_precision_loss)] // sums over a few hundred bytes
        let (sum_body, sum_out, squares) = (sum_body as f64, sum_out as f64, squares as f64);
        let mean = (sum_body + sum_out) / n;
        let var = squares / n - mean * mean;
        if var <= f64::EPSILON {
            return 0.0;
        }
        // Correlation with a binary template (1 on the outline, 0 on the
        // body): the gap between the two means, scaled by both spreads.
        let (nb, no) = (f64::from(n_body), f64::from(n_out));
        let gap = sum_out / no - sum_body / nb;
        (gap * (nb * no).sqrt() / (n * var.sqrt())).abs()
    }
}

/// Point-in-polygon on the 12x19 arrow cell.
fn inside_arrow(px: f64, py: f64) -> bool {
    let mut inside = false;
    for i in 0..ARROW.len() {
        let (x1, y1) = ARROW[i];
        let (x2, y2) = ARROW[(i + 1) % ARROW.len()];
        if (y1 > py) != (y2 > py) && px < x1 + (py - y1) * (x2 - x1) / (y2 - y1) {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rendered_arrow_has_its_tip_at_the_top_left_and_an_outline_around_it() {
        let t = Template::render(19);
        assert_eq!((t.width, t.height), (14, 21));
        // The tip is sub-pixel: the first body pixel sits one row down
        // in the first body column, with outline pixels around it.
        let first = t.body.iter().position(|&b| b).unwrap();
        assert_eq!((first % t.width, first / t.width), (1, 2));
        assert!(t.outline[t.width + 1] && t.outline[2 * t.width] && !t.outline[0]);
        assert!(!t.body[2 * t.width + 12] && !t.outline[2 * t.width + 12]);
        let body = t.body.iter().filter(|&&b| b).count();
        let outline = t.outline.iter().filter(|&&b| b).count();
        assert!(
            body > 60 && outline > body / 2,
            "{body} body, {outline} outline"
        );
        // The smallest size the bank uses still has a recognisable body.
        let small = Template::render(6);
        assert!(small.body.iter().filter(|&&b| b).count() >= 10);
    }

    /// A 64x64 plane of `background` with a dark arrow of `height`
    /// (drawn from the same polygon, so the match is exact) at (x, y).
    fn plane_with_arrow(background: u8, height: usize, x: usize, y: usize) -> Vec<u8> {
        let t = Template::render(height);
        let mut plane = vec![background; 64 * 64];
        for row in 0..t.height {
            for col in 0..t.width {
                let i = row * t.width + col;
                if t.body[i] {
                    plane[(y + row) * 64 + x + col] = 20;
                } else if t.outline[i] {
                    plane[(y + row) * 64 + x + col] = 255;
                }
            }
        }
        plane
    }

    #[test]
    fn the_score_is_high_on_an_arrow_and_low_elsewhere() {
        let t = Template::render(10);
        let plane = plane_with_arrow(230, 10, 20, 15);
        assert!(t.score(&plane, 64, 20, 15) > 0.95);
        assert!(t.score(&plane, 64, 22, 17) < 0.6, "two pixels off");
        assert!(
            t.score(&plane, 64, 40, 40) < f64::EPSILON,
            "flat background"
        );
        assert!(t.score(&plane, 64, 60, 60) < f64::EPSILON, "off the plane");
    }

    #[test]
    fn a_light_arrow_with_a_dark_outline_scores_as_well() {
        let t = Template::render(10);
        let mut plane = plane_with_arrow(40, 10, 20, 15);
        for v in &mut plane {
            *v = 255 - *v;
        }
        assert!(t.score(&plane, 64, 20, 15) > 0.95);
    }

    #[test]
    fn a_nearby_size_still_scores_well_and_a_bar_does_not() {
        let plane = plane_with_arrow(230, 10, 20, 15);
        // Within about 20 % of the size the match holds; a bank of
        // sizes 15 % apart therefore always has one that fits.
        for height in [8, 9, 11, 12] {
            let score = Template::render(height).score(&plane, 64, 20, 15);
            assert!(score > 0.6, "height {height}: {score:.3}");
        }
        let mut bar = vec![230u8; 64 * 64];
        for y in 20..26 {
            bar[y * 64 + 10..y * 64 + 50].fill(20);
        }
        assert!(Template::render(10).score(&bar, 64, 10, 18) < 0.5);
    }
}
