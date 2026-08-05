//! The transcript artifact: timestamped speech, stored as
//! `transcript.json` inside the bundle.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const TRANSCRIPT_FILE: &str = "transcript.json";

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

pub fn load_transcript(bundle_dir: &Path) -> Result<Transcript> {
    let path = bundle_dir.join(TRANSCRIPT_FILE);
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("no transcript at {}", path.display()))?;
    serde_json::from_str(&raw).context("transcript artifact is not valid")
}

pub fn save_transcript(bundle_dir: &Path, transcript: &Transcript) -> Result<()> {
    fs::create_dir_all(bundle_dir)?;
    let tmp = bundle_dir.join(format!("{TRANSCRIPT_FILE}.tmp"));
    fs::write(&tmp, serde_json::to_string_pretty(transcript)?)?;
    fs::rename(&tmp, bundle_dir.join(TRANSCRIPT_FILE))?;
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
    }
}
