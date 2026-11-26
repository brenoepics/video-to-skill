//! Creator captions as a transcript source. The platform's own caption
//! track is the author's text — verbatim where whisper would guess — but
//! YouTube serves creator cues with cue-level timing only, so whisper's
//! words are borrowed for timing when the cues carry none.

mod json3;
mod vtt;

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use super::{
    load_manifest, primary_language, save_manifest, save_transcript_from, Segment, Transcript,
    TranscriptSource, Word, TRANSCRIPT_FILE,
};

/// A whisper word that falls between two cues snaps to the nearer one
/// when it is this close; anything further off belongs to no cue.
const WORD_SNAP_SECS: f64 = 1.0;

/// Parsed creator captions: the manifest's track, read into segments.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionCues {
    /// Language tag of the track as the platform labels it.
    pub language: String,
    pub segments: Vec<Segment>,
}

impl CaptionCues {
    /// True when the cues came with their own per-word offsets.
    #[must_use]
    pub fn has_word_timing(&self) -> bool {
        self.segments.iter().any(|s| !s.words.is_empty())
    }
}

/// Read the creator-caption track the manifest records. `Ok(None)` when
/// the bundle has none.
pub fn load_captions(bundle_dir: &Path) -> Result<Option<CaptionCues>> {
    let manifest = load_manifest(bundle_dir)?;
    let Some(track) = manifest.platform.captions else {
        return Ok(None);
    };
    let path = bundle_dir.join(&track.path);
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("no caption file at {}", path.display()))?;
    let segments = match track.format.as_str() {
        "json3" => json3::parse(&raw)?,
        "vtt" => vtt::parse(&raw),
        other => bail!("unsupported caption format '{other}'"),
    };
    Ok(Some(CaptionCues {
        language: track.language,
        segments,
    }))
}

/// Make the creator captions the bundle's transcript and record that in
/// the manifest. `word_timing` is a whisper transcript whose words are
/// spread over the cues when the captions carry no word offsets of their
/// own; its detected language must agree with the captions', otherwise
/// the captions are refused (with a manifest note) and whatever
/// transcript exists stands.
pub fn extract_caption_track(
    bundle_dir: &Path,
    cues: &CaptionCues,
    word_timing: Option<&Transcript>,
) -> Result<Transcript> {
    let mut manifest = load_manifest(bundle_dir)?;
    let language = primary_language(&cues.language);
    if let Some(spoken) = word_timing.and_then(|t| t.language.as_deref()) {
        if !spoken.eq_ignore_ascii_case(language) {
            let note = format!(
                "transcript: creator captions are '{}' but the speech is '{spoken}'; kept whisper",
                cues.language
            );
            push_note(&mut manifest.notes, &note);
            save_manifest(bundle_dir, &manifest)?;
            bail!("{note}");
        }
    }

    let mut segments = cues.segments.clone();
    let timing_note = if cues.has_word_timing() {
        "transcript: creator captions with their own word timing"
    } else if let Some(whisper) = word_timing {
        fill_word_timing(&mut segments, &whisper.segments);
        "transcript: creator captions; word timing from whisper"
    } else {
        "transcript: creator captions carry cue-level timing only (whisper did not run)"
    };
    let transcript = Transcript {
        schema_version: 1,
        language: Some(language.to_ascii_lowercase()),
        segments,
    };
    save_transcript_from(bundle_dir, &transcript, TranscriptSource::CreatorCaptions)?;

    manifest.tracks.transcript = Some(TRANSCRIPT_FILE.to_string());
    manifest.media.language.clone_from(&transcript.language);
    push_note(&mut manifest.notes, timing_note);
    save_manifest(bundle_dir, &manifest)?;
    Ok(transcript)
}

/// Give each cue the whisper words whose midpoint falls inside it; a
/// word in the gap between cues goes to the nearer cue when the gap is
/// short. Word text stays as whisper heard it — the timing is what the
/// captions lack, the cue text is already the creator's.
fn fill_word_timing(cues: &mut [Segment], whisper: &[Segment]) {
    for cue in cues.iter_mut() {
        cue.words.clear();
    }
    for word in whisper.iter().flat_map(|s| &s.words) {
        let mid = f64::midpoint(word.start_secs, word.end_secs);
        // Cues are half-open: a word on the boundary is inside the later
        // one, which ranks before "just past the end" of the earlier one.
        let nearest = cues
            .iter_mut()
            .map(|cue| {
                let inside = mid >= cue.start_secs && mid < cue.end_secs;
                let gap = distance(mid, cue.start_secs, cue.end_secs);
                ((gap, u8::from(!inside)), cue)
            })
            .min_by(|a, b| a.0 .0.total_cmp(&b.0 .0).then(a.0 .1.cmp(&b.0 .1)));
        if let Some(((gap, _), cue)) = nearest {
            if gap <= WORD_SNAP_SECS {
                cue.words.push(Word::clone(word));
            }
        }
    }
}

fn distance(t: f64, start: f64, end: f64) -> f64 {
    if t < start {
        start - t
    } else if t >= end {
        t - end
    } else {
        0.0
    }
}

fn push_note(notes: &mut Vec<String>, note: &str) {
    if !notes.iter().any(|n| n == note) {
        notes.push(note.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(start: f64, end: f64) -> Segment {
        Segment {
            start_secs: start,
            end_secs: end,
            text: String::new(),
            words: vec![],
        }
    }

    fn word(start: f64, end: f64) -> Word {
        Word {
            start_secs: start,
            end_secs: end,
            text: "w".into(),
        }
    }

    #[test]
    fn words_go_to_the_cue_they_fall_in_or_snap_to_a_near_one() {
        let mut cues = vec![cue(1.0, 3.0), cue(5.0, 7.0)];
        let whisper = vec![Segment {
            start_secs: 0.0,
            end_secs: 10.0,
            text: String::new(),
            // inside cue 0 · gap, nearer cue 0 · gap, nearer cue 1 · far from both
            words: vec![
                word(1.5, 2.0),
                word(3.2, 3.6),
                word(4.4, 4.8),
                word(8.5, 9.0),
            ],
        }];

        fill_word_timing(&mut cues, &whisper);

        assert_eq!(cues[0].words.len(), 2);
        assert_eq!(cues[1].words.len(), 1);
        assert_eq!(cues[1].words[0].start_secs, 4.4);
    }

    #[test]
    fn a_word_on_the_boundary_belongs_to_the_later_cue() {
        let mut cues = vec![cue(0.0, 2.0), cue(2.0, 4.0)];
        let whisper = vec![Segment {
            start_secs: 0.0,
            end_secs: 4.0,
            text: String::new(),
            words: vec![word(1.9, 2.1)],
        }];

        fill_word_timing(&mut cues, &whisper);

        assert_eq!(cues[0].words, vec![]);
        assert_eq!(cues[1].words.len(), 1);
    }
}
