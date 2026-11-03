//! Apple Vision text recognition (`VNRecognizeTextRequest`), macOS only.
//!
//! Accurate recognition with language correction off: correction
//! rewrites CLI tokens into prose (measured on a terminal fixture:
//! `--release` → `-- release`, `app:1.2.3` → `app: 1.2.3 .`), which is
//! exactly the text the agent needs verbatim. Apple documents
//! `customWords` as ignored while correction is off; the vocabulary is
//! still passed so it applies the day correction is turned on.

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_foundation::{NSArray, NSDictionary, NSString, NSURL};
use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRequestTextRecognitionLevel};

use super::{BBox, OcrEngine, Recognized};

pub struct AppleVision {
    custom_words: Vec<String>,
}

impl AppleVision {
    #[must_use]
    pub fn new(custom_words: Vec<String>) -> AppleVision {
        AppleVision { custom_words }
    }

    fn request(&self) -> Retained<VNRecognizeTextRequest> {
        let request = VNRecognizeTextRequest::new();
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        request.setUsesLanguageCorrection(false);
        let words: Vec<Retained<NSString>> = self
            .custom_words
            .iter()
            .map(|word| NSString::from_str(word))
            .collect();
        request.setCustomWords(&NSArray::from_retained_slice(&words));
        request
    }
}

impl OcrEngine for AppleVision {
    fn name(&self) -> &'static str {
        "apple-vision"
    }

    fn read(&self, image: &Path) -> Result<Vec<Recognized>> {
        let absolute = image
            .canonicalize()
            .with_context(|| format!("no image at {}", image.display()))?;
        let url = NSURL::fileURLWithPath_isDirectory(
            &NSString::from_str(&absolute.to_string_lossy()),
            false,
        );
        // SAFETY: a file URL and an empty options dictionary are valid
        // arguments; the handler owns no other state.
        let handler = unsafe {
            VNImageRequestHandler::initWithURL_options(
                VNImageRequestHandler::alloc(),
                &url,
                &NSDictionary::new(),
            )
        };
        let request = self.request();
        let requests = NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(
            request.clone(),
        ))]);
        handler.performRequests_error(&requests).map_err(|err| {
            anyhow!(
                "Vision text recognition failed: {}",
                err.localizedDescription()
            )
        })?;
        let Some(observations) = request.results() else {
            return Ok(Vec::new());
        };
        let mut lines = Vec::new();
        for observation in &*observations {
            let Some(top) = observation.topCandidates(1).firstObject() else {
                continue;
            };
            // SAFETY: a recognised-text observation always carries a box.
            let rect = unsafe { observation.boundingBox() };
            // Vision's box is normalised with its origin at the bottom
            // left; the track's origin is the top left.
            let bbox = BBox {
                x: rect.origin.x,
                y: 1.0 - rect.origin.y - rect.size.height,
                w: rect.size.width,
                h: rect.size.height,
            }
            .rounded();
            lines.push(Recognized {
                bbox,
                text: top.string().to_string(),
                confidence: Some(top.confidence()),
            });
        }
        Ok(lines)
    }
}
