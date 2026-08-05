//! Audio-track extraction: demux to wav, transcribe, persist.
//!
//! The recognition engine is a seam (`TranscriptionEngine`) so the
//! pipeline is testable without model weights; `WhisperEngine` is the
//! production implementation.

mod demux;
pub mod whisper;

use std::path::Path;

use anyhow::Result;

use crate::bundle::{
    load_manifest, save_manifest, save_transcript, Segment, Transcript, TRANSCRIPT_FILE,
};
use crate::tools::Toolchain;

/// What an engine hears in a wav file.
#[derive(Debug, Clone)]
pub struct EngineOutput {
    pub language: Option<String>,
    pub segments: Vec<Segment>,
}

/// Speech recognition seam. `progress` receives 0–100.
pub trait TranscriptionEngine {
    fn transcribe(&self, wav: &Path, progress: &mut dyn FnMut(u8)) -> Result<EngineOutput>;
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
        let output = engine.transcribe(&wav, progress)?;
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
