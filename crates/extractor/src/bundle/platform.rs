//! What the platform offered beside the media — free priors recorded at
//! ingest: the creator's own captions (the author's words, where they
//! exist), chapters (section boundaries, often one per step) and
//! `SponsorBlock` marks (ranges that waste inspection budget and yield
//! junk steps).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlatformPriors {
    /// The creator-caption track kept in the bundle, when the video has one.
    #[serde(default)]
    pub captions: Option<CaptionTrack>,
    /// Creator chapters in video order: section hints for the analysis.
    #[serde(default)]
    pub chapters: Vec<Chapter>,
    /// `SponsorBlock`-marked ranges (sponsor, intro, outro, selfpromo).
    /// The media is never cut, so every timestamp stays on the original
    /// timeline; analysis skips these ranges instead.
    #[serde(default)]
    pub skips: Vec<SkipRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptionTrack {
    /// Bundle-relative path of the subtitle file as yt-dlp wrote it.
    pub path: String,
    /// Language tag as the platform labels the track (e.g. `en`, `pt-BR`).
    pub language: String,
    /// Subtitle format: "json3" or "vtt".
    pub format: String,
    /// Every creator-caption language the platform offered, sorted.
    #[serde(default)]
    pub available_languages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub start_secs: f64,
    pub end_secs: f64,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkipRange {
    pub start_secs: f64,
    pub end_secs: f64,
    /// `SponsorBlock` category: sponsor, intro, outro or selfpromo.
    pub category: String,
}

/// The primary subtag of a language tag: `pt-BR` → `pt`, `en_US` → `en`.
/// Whisper reports languages this way, so the two sources compare.
#[must_use]
pub fn primary_language(tag: &str) -> &str {
    tag.split(['-', '_']).next().unwrap_or(tag)
}

#[cfg(test)]
mod tests {
    use super::primary_language;

    #[test]
    fn primary_language_drops_region_and_script() {
        assert_eq!(primary_language("pt-BR"), "pt");
        assert_eq!(primary_language("zh_Hans"), "zh");
        assert_eq!(primary_language("en"), "en");
    }
}
