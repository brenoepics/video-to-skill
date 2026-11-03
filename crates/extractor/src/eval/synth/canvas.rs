//! An RGB raster with just enough drawing to lay out a screen: fills,
//! rectangles, and bitmap text at an integer scale, written out as a
//! binary PPM that ffmpeg's image demuxer reads directly.

use super::font::{glyph, GLYPH_H, GLYPH_W};

pub type Rgb = [u8; 3];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pixels: Vec<u8>,
}

impl Canvas {
    #[must_use]
    pub fn new(width: u32, height: u32, background: Rgb) -> Canvas {
        let count = (width as usize) * (height as usize);
        Canvas {
            width,
            height,
            pixels: background.repeat(count),
        }
    }

    /// Fill the rectangle at (`x`, `y`) of `w` by `h` pixels, clipped.
    pub fn rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: Rgb) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + i32::try_from(w).unwrap_or(i32::MAX)).min(self.width_i32());
        let y1 = (y + i32::try_from(h).unwrap_or(i32::MAX)).min(self.height_i32());
        for py in y0..y1 {
            for px in x0..x1 {
                self.put(px, py, color);
            }
        }
    }

    /// Draw `text` with its top-left at (`x`, `y`), each glyph pixel
    /// `scale` canvas pixels wide; one glyph column of spacing between
    /// characters.
    pub fn text(&mut self, x: i32, y: i32, scale: u32, color: Rgb, text: &str) {
        let advance = i32::try_from((GLYPH_W + 1) * scale).unwrap_or(i32::MAX);
        let step = i32::try_from(scale).unwrap_or(i32::MAX);
        for (idx, c) in text.chars().enumerate() {
            let origin_x = x + advance * i32::try_from(idx).unwrap_or(i32::MAX);
            for (row, bits) in glyph(c).iter().enumerate() {
                for col in 0..GLYPH_W {
                    if bits & (0x10 >> col) != 0 {
                        let px = origin_x + step * i32::try_from(col).unwrap_or(0);
                        let py = y + step * i32::try_from(row).unwrap_or(0);
                        self.rect(px, py, scale, scale, color);
                    }
                }
            }
        }
    }

    /// Width in pixels `text` occupies at `scale`.
    #[must_use]
    pub fn text_width(text: &str, scale: u32) -> u32 {
        let chars = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
        chars.saturating_mul((GLYPH_W + 1) * scale)
    }

    /// Line height for text at `scale`, with breathing room.
    #[must_use]
    pub fn line_height(scale: u32) -> u32 {
        (GLYPH_H + 2) * scale
    }

    /// Binary PPM (P6) bytes.
    #[must_use]
    pub fn to_ppm(&self) -> Vec<u8> {
        let mut out = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        out.extend_from_slice(&self.pixels);
        out
    }

    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Rgb {
        let at = ((y * self.width + x) as usize) * 3;
        [self.pixels[at], self.pixels[at + 1], self.pixels[at + 2]]
    }

    fn put(&mut self, x: i32, y: i32, color: Rgb) {
        let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
            return;
        };
        if x >= self.width || y >= self.height {
            return;
        }
        let at = ((y * self.width + x) as usize) * 3;
        self.pixels[at..at + 3].copy_from_slice(&color);
    }

    fn width_i32(&self) -> i32 {
        i32::try_from(self.width).unwrap_or(i32::MAX)
    }

    fn height_i32(&self) -> i32 {
        i32::try_from(self.height).unwrap_or(i32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::Canvas;

    #[test]
    fn ppm_carries_a_header_and_every_pixel() {
        let canvas = Canvas::new(4, 2, [1, 2, 3]);
        let ppm = canvas.to_ppm();
        assert!(ppm.starts_with(b"P6\n4 2\n255\n"));
        assert_eq!(ppm.len(), "P6\n4 2\n255\n".len() + 4 * 2 * 3);
        assert_eq!(canvas.pixel(3, 1), [1, 2, 3]);
    }

    #[test]
    fn rectangles_are_clipped_to_the_canvas() {
        let mut canvas = Canvas::new(4, 4, [0, 0, 0]);
        canvas.rect(-2, -2, 4, 4, [9, 9, 9]);
        assert_eq!(canvas.pixel(1, 1), [9, 9, 9]);
        assert_eq!(canvas.pixel(2, 2), [0, 0, 0]);
    }

    #[test]
    fn text_lights_glyph_pixels_at_the_requested_scale() {
        let mut canvas = Canvas::new(40, 20, [0, 0, 0]);
        canvas.text(0, 0, 2, [255, 255, 255], "|");
        // The bar glyph is the middle column (bit 2): x = 2*2..2*3.
        assert_eq!(canvas.pixel(4, 0), [255, 255, 255]);
        assert_eq!(canvas.pixel(5, 13), [255, 255, 255]);
        assert_eq!(canvas.pixel(0, 0), [0, 0, 0]);
        assert_eq!(Canvas::text_width("ab", 2), 24);
        assert_eq!(Canvas::line_height(4), 36);
    }
}
