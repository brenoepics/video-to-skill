//! Which engine this build has: Apple Vision on macOS, Windows OCR on
//! Windows, none elsewhere (a `RapidOCR` feature is the planned
//! cross-platform option).

use std::fmt;

use super::OcrEngine;

/// Why no engine could be built here — recorded in the manifest so the
/// agent knows to read the keyframes directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoEngine {
    pub reason: String,
}

impl fmt::Display for NoEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for NoEngine {}

/// The platform's engine, seeded with `custom_words` where it takes a
/// vocabulary (Apple Vision; Windows OCR has no such input).
#[cfg(target_os = "macos")]
pub fn platform_engine(custom_words: &[String]) -> Result<Box<dyn OcrEngine>, NoEngine> {
    Ok(Box::new(super::vision::AppleVision::new(
        custom_words.to_vec(),
    )))
}

#[cfg(windows)]
pub fn platform_engine(_custom_words: &[String]) -> Result<Box<dyn OcrEngine>, NoEngine> {
    super::winocr::WindowsOcr::available()
        .map(|engine| Box::new(engine) as Box<dyn OcrEngine>)
        .map_err(|err| NoEngine {
            reason: format!("Windows OCR is unavailable ({err:#}) — read the keyframes directly"),
        })
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn platform_engine(_custom_words: &[String]) -> Result<Box<dyn OcrEngine>, NoEngine> {
    Err(NoEngine {
        reason: format!(
            "no local OCR engine on {} (Apple Vision on macOS, Windows OCR on Windows) — \
             read the keyframes directly",
            std::env::consts::OS
        ),
    })
}
