//! The extraction bundle: one directory holding everything derived from a
//! video. `manifest.json` is the root document; later pipeline stages add
//! their artifacts beside it and record them in the manifest's track slots.

mod ingest;
mod ingest_url;
mod probe_media;
mod transcript;

pub use ingest::{ingest, IngestReport};
pub use ingest_url::{ingest_any, DownloadedVideo, VideoDownloader, YtdlpDownloader};
pub use transcript::{
    load_transcript, save_transcript, Segment, Transcript, Word, TRANSCRIPT_FILE,
};

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const MANIFEST_FILE: &str = "manifest.json";

/// Root document of a bundle. Schema is versioned so later releases can
/// migrate old bundles instead of misreading them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub source: SourceInfo,
    pub media: MediaInfo,
    /// Artifacts other stages have produced, as bundle-relative paths.
    #[serde(default)]
    pub tracks: Tracks,
    /// User-readable notes about extraction outcomes (e.g. "no audio track").
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Original path or URL as the user supplied it.
    pub original: String,
    /// Absolute path of the media file being extracted from.
    pub media_path: String,
    /// sha256 of the media file — identity for idempotency checks.
    pub sha256: String,
    /// Human title when known (URL ingest fills this in).
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    pub container: String,
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
    /// Auto-detected spoken language, filled by the audio track stage.
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tracks {
    pub transcript: Option<String>,
    pub frames: Option<String>,
    pub timeline: Option<String>,
}

/// Load the manifest of an existing bundle.
pub fn load_manifest(bundle_dir: &Path) -> Result<Manifest> {
    let path = bundle_dir.join(MANIFEST_FILE);
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("no bundle manifest at {}", path.display()))?;
    serde_json::from_str(&raw).context("bundle manifest is not valid — was it hand-edited?")
}

/// Write the manifest atomically (temp file + rename) so a crash can't
/// leave a half-written manifest behind.
pub fn save_manifest(bundle_dir: &Path, manifest: &Manifest) -> Result<()> {
    fs::create_dir_all(bundle_dir)?;
    let tmp = bundle_dir.join(format!("{MANIFEST_FILE}.tmp"));
    fs::write(&tmp, serde_json::to_string_pretty(manifest)?)?;
    fs::rename(&tmp, bundle_dir.join(MANIFEST_FILE))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_through_disk() {
        let dir = std::env::temp_dir()
            .join("vts-manifest-roundtrip")
            .join(std::process::id().to_string());
        let manifest = Manifest {
            schema_version: 1,
            source: SourceInfo {
                original: "./v.mp4".into(),
                media_path: "/abs/v.mp4".into(),
                sha256: "ab".repeat(32),
                title: Some("Fixture".into()),
            },
            media: MediaInfo {
                container: "mp4".into(),
                duration_secs: 2.0,
                width: 320,
                height: 240,
                fps: 10.0,
                has_audio: true,
                language: None,
            },
            tracks: Tracks::default(),
            notes: vec!["a note".into()],
        };

        save_manifest(&dir, &manifest).unwrap();

        assert_eq!(load_manifest(&dir).unwrap(), manifest);
    }
}
