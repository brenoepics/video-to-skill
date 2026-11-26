//! Audio-track extraction: demux to wav, transcribe, persist.
//!
//! The recognition engine is a seam (`TranscriptionEngine`) so the
//! pipeline is testable without model weights; `WhisperEngine` is the
//! production implementation.

mod demux;
pub mod dtw;
pub mod prompt;
pub mod segments;
pub mod vad;
pub mod whisper;

use std::path::Path;

use anyhow::Result;

use crate::bundle::{
    load_manifest, save_manifest, save_transcript, Segment, SourceInfo, Transcript, TRANSCRIPT_FILE,
};
use crate::tools::Toolchain;

/// What an engine hears in a wav file.
#[derive(Debug, Clone)]
pub struct EngineOutput {
    pub language: Option<String>,
    pub segments: Vec<Segment>,
}

/// What is known about the media before a word of it is heard — an
/// engine may prime recognition with it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hints {
    /// What the media is called: the title a URL ingest captured, else
    /// the file's stem.
    pub title: Option<String>,
}

impl Hints {
    #[must_use]
    pub fn for_source(source: &SourceInfo) -> Hints {
        let stem = || {
            Path::new(&source.media_path)
                .file_stem()
                .map(|stem| stem.to_string_lossy().replace('_', " "))
        };
        Hints {
            title: source.title.clone().or_else(stem),
        }
    }
}

/// Speech recognition seam. `progress` receives 0–100.
pub trait TranscriptionEngine {
    fn transcribe(
        &self,
        wav: &Path,
        hints: &Hints,
        progress: &mut dyn FnMut(u8),
    ) -> Result<EngineOutput>;
}

/// Produce the bundle's transcript artifact and record it in the manifest.
///
/// Defined outcomes for degenerate inputs: a video with no audio track
/// yields an empty transcript and a "no audio" manifest note (the engine
/// is never invoked); audio in which the engine finds no speech yields an
/// empty transcript and a "no speech" note.
pub fn extract_audio_track(
    bundle_dir: &Path,
    tools: &Toolchain,
    engine: &dyn TranscriptionEngine,
    progress: &mut dyn FnMut(u8),
) -> Result<Transcript> {
    let mut manifest = load_manifest(bundle_dir)?;

    let output = if manifest.media.has_audio {
        let wav = demux::to_wav(tools, Path::new(&manifest.source.media_path), bundle_dir)?;
        let output = engine.transcribe(&wav, &Hints::for_source(&manifest.source), progress)?;
        let _ = std::fs::remove_file(&wav);
        Some(output)
    } else {
        None
    };

    let transcript = Transcript {
        schema_version: 1,
        language: output.as_ref().and_then(|o| o.language.clone()),
        segments: output
            .as_ref()
            .map(|o| o.segments.clone())
            .unwrap_or_default(),
    };
    save_transcript(bundle_dir, &transcript)?;

    manifest.tracks.transcript = Some(TRANSCRIPT_FILE.to_string());
    manifest.media.language.clone_from(&transcript.language);
    match &output {
        None => push_note(&mut manifest.notes, "transcript: no audio track in source"),
        Some(o) if o.segments.is_empty() => {
            push_note(&mut manifest.notes, "transcript: no speech detected");
        }
        Some(_) => {}
    }
    save_manifest(bundle_dir, &manifest)?;
    Ok(transcript)
}

fn push_note(notes: &mut Vec<String>, note: &str) {
    if !notes.iter().any(|n| n == note) {
        notes.push(note.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(title: Option<&str>, media_path: &str) -> SourceInfo {
        SourceInfo {
            original: media_path.to_string(),
            media_path: media_path.to_string(),
            sha256: "ab".repeat(32),
            title: title.map(str::to_string),
        }
    }

    #[test]
    fn the_captured_title_names_the_media() {
        let hints = Hints::for_source(&source(Some("Bash in 100 Seconds"), "/dl/abc.mp4"));
        assert_eq!(hints.title.as_deref(), Some("Bash in 100 Seconds"));
    }

    #[test]
    fn a_local_file_is_named_by_its_stem() {
        let hints = Hints::for_source(&source(None, "/videos/Learn_Mac_Terminal.mov"));
        assert_eq!(hints.title.as_deref(), Some("Learn Mac Terminal"));
    }
}
