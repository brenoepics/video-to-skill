//! The transcript artifact: timestamped speech, stored as
//! `transcript.json` inside the bundle, together with where it came from.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const TRANSCRIPT_FILE: &str = "transcript.json";

/// Where a transcript's text came from. Creator captions are the
/// author's own words; whisper may mishear technical terms.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranscriptSource {
    CreatorCaptions,
    #[default]
    Whisper,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub schema_version: u32,
    /// Auto-detected ISO language code, when speech was found.
    pub language: Option<String>,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start_secs: f64,
    pub end_secs: f64,
    pub text: String,
    /// Word-level timing (token-level from whisper; empty when unavailable).
    #[serde(default)]
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub start_secs: f64,
    pub end_secs: f64,
    pub text: String,
}

/// On-disk shape: the transcript plus its provenance. Recognition
/// engines build a bare [`Transcript`]; the source rides beside it in
/// the file, and a file written before sources existed reads as whisper.
#[derive(Serialize)]
struct StoredTranscript<'a> {
    #[serde(flatten)]
    transcript: &'a Transcript,
    source: TranscriptSource,
}

#[derive(Deserialize)]
struct LoadedTranscript {
    #[serde(flatten)]
    transcript: Transcript,
    #[serde(default)]
    source: TranscriptSource,
}

fn read(bundle_dir: &Path) -> Result<LoadedTranscript> {
    let path = bundle_dir.join(TRANSCRIPT_FILE);
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("no transcript at {}", path.display()))?;
    serde_json::from_str(&raw).context("transcript artifact is not valid")
}

pub fn load_transcript(bundle_dir: &Path) -> Result<Transcript> {
    read(bundle_dir).map(|stored| stored.transcript)
}

/// Which source produced the bundle's transcript.
pub fn load_transcript_source(bundle_dir: &Path) -> Result<TranscriptSource> {
    read(bundle_dir).map(|stored| stored.source)
}

/// Persist an engine transcript; its source is recorded as whisper.
/// Captions go through [`save_transcript_from`].
pub fn save_transcript(bundle_dir: &Path, transcript: &Transcript) -> Result<()> {
    save_transcript_from(bundle_dir, transcript, TranscriptSource::Whisper)
}

/// Persist a transcript together with where its text came from.
pub fn save_transcript_from(
    bundle_dir: &Path,
    transcript: &Transcript,
    source: TranscriptSource,
) -> Result<()> {
    fs::create_dir_all(bundle_dir)?;
    let path = bundle_dir.join(TRANSCRIPT_FILE);
    let stored = StoredTranscript { transcript, source };
    crate::fsx::write_atomic(&path, &serde_json::to_vec_pretty(&stored)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_round_trips_through_disk() {
        let dir = std::env::temp_dir()
            .join("vts-transcript-roundtrip")
            .join(std::process::id().to_string());
        let transcript = Transcript {
            schema_version: 1,
            language: Some("en".into()),
            segments: vec![Segment {
                start_secs: 0.5,
                end_secs: 2.25,
                text: "hello".into(),
                words: vec![Word {
                    start_secs: 0.5,
                    end_secs: 1.0,
                    text: "hello".into(),
                }],
            }],
        };

        save_transcript(&dir, &transcript).unwrap();

        assert_eq!(load_transcript(&dir).unwrap(), transcript);
        assert_eq!(
            load_transcript_source(&dir).unwrap(),
            TranscriptSource::Whisper
        );
    }

    #[test]
    fn a_transcript_saved_before_sources_existed_reads_back_as_whisper() {
        let dir = std::env::temp_dir()
            .join("vts-transcript-legacy")
            .join(std::process::id().to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(TRANSCRIPT_FILE),
            r#"{"schema_version":1,"language":"en","segments":[]}"#,
        )
        .unwrap();

        assert_eq!(
            load_transcript_source(&dir).unwrap(),
            TranscriptSource::Whisper
        );
        assert_eq!(load_transcript(&dir).unwrap().segments, vec![]);
    }

    #[test]
    fn the_source_is_written_as_a_kebab_case_key() {
        let dir = std::env::temp_dir()
            .join("vts-transcript-source")
            .join(std::process::id().to_string());
        let transcript = Transcript {
            schema_version: 1,
            language: None,
            segments: vec![],
        };

        save_transcript_from(&dir, &transcript, TranscriptSource::CreatorCaptions).unwrap();

        let raw = fs::read_to_string(dir.join(TRANSCRIPT_FILE)).unwrap();
        assert!(raw.contains("\"source\": \"creator-captions\""), "{raw}");
        assert_eq!(
            load_transcript_source(&dir).unwrap(),
            TranscriptSource::CreatorCaptions
        );
    }
}
