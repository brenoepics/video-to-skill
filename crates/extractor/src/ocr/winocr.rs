//! `Windows.Media.Ocr`, Windows only. Uses whichever OCR language pack
//! the user profile has (English on a stock install); reports no
//! confidence because the API exposes none.

use std::path::Path;

use anyhow::{Context, Result};
use windows::core::h;
use windows::Globalization::Language;
use windows::Graphics::Imaging::BitmapDecoder;
use windows::Media::Ocr::{OcrEngine as NativeEngine, OcrLine};
use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

use super::{BBox, OcrEngine, Recognized};

pub struct WindowsOcr;

impl WindowsOcr {
    /// An engine, if a language pack with OCR support is installed.
    pub fn available() -> Result<WindowsOcr> {
        native_engine().map(|_| WindowsOcr)
    }
}

fn native_engine() -> Result<NativeEngine> {
    NativeEngine::TryCreateFromUserProfileLanguages()
        .or_else(|_| NativeEngine::TryCreateFromLanguage(&Language::CreateLanguage(h!("en-US"))?))
        .context("no Windows OCR language pack is installed")
}

impl OcrEngine for WindowsOcr {
    fn name(&self) -> &'static str {
        "windows-ocr"
    }

    /// Windows OCR rejects images wider or taller than this (2600 px on
    /// current builds), so 4K stills fall back to the bounded keyframe.
    fn max_dimension(&self) -> Option<u32> {
        NativeEngine::MaxImageDimension().ok()
    }

    fn read(&self, image: &Path) -> Result<Vec<Recognized>> {
        let bytes =
            std::fs::read(image).with_context(|| format!("no image at {}", image.display()))?;
        let stream = InMemoryRandomAccessStream::new()?;
        let writer = DataWriter::CreateDataWriter(&stream)?;
        writer.WriteBytes(&bytes)?;
        writer.StoreAsync()?.join()?;
        writer.FlushAsync()?.join()?;
        writer.DetachStream()?;
        stream.Seek(0)?;
        let decoder = BitmapDecoder::CreateAsync(&stream)?.join()?;
        let bitmap = decoder.GetSoftwareBitmapAsync()?.join()?;
        let width = f64::from(bitmap.PixelWidth()?);
        let height = f64::from(bitmap.PixelHeight()?);
        // WinRT objects are agile; one engine per read keeps the worker
        // threads independent.
        let result = native_engine()?.RecognizeAsync(&bitmap)?.join()?;
        let mut lines = Vec::new();
        for line in result.Lines()? {
            lines.push(Recognized {
                bbox: line_bbox(&line, width, height)?,
                text: line.Text()?.to_string(),
                confidence: None,
            });
        }
        Ok(lines)
    }
}

/// The union of the line's word boxes, normalised.
fn line_bbox(line: &OcrLine, width: f64, height: f64) -> Result<BBox> {
    let mut edges = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for word in line.Words()? {
        let rect = word.BoundingRect()?;
        edges.0 = edges.0.min(f64::from(rect.X));
        edges.1 = edges.1.min(f64::from(rect.Y));
        edges.2 = edges.2.max(f64::from(rect.X + rect.Width));
        edges.3 = edges.3.max(f64::from(rect.Y + rect.Height));
    }
    if !edges.0.is_finite() {
        edges = (0.0, 0.0, 0.0, 0.0);
    }
    Ok(BBox::from_pixels(edges, width, height))
}
