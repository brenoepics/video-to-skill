//! Creator captions as the transcript: cue text is the creator's ground
//! truth; whisper only lends word timing when the captions lack it.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use vts_extract::bundle::{
    extract_caption_track, load_captions, load_manifest, load_transcript, load_transcript_source,
    save_manifest, save_transcript, CaptionTrack, Manifest, MediaInfo, PlatformPriors, Segment,
    SourceInfo, Tracks, Transcript, TranscriptSource, Word,
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-captions-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A bundle whose manifest points at a creator-caption file — no media
/// needed, the caption stage reads nothing else.
fn bundle_with_captions(dir: &Path, file: &str, language: &str, contents: &str) -> PathBuf {
    let bundle = dir.join("bundle");
    fs::create_dir_all(&bundle).unwrap();
    fs::write(bundle.join(file), contents).unwrap();
    let format = file.rsplit('.').next().unwrap().to_string();
    let manifest = Manifest {
        schema_version: 1,
        source: SourceInfo {
            original: "https://youtube.example/watch?v=c".into(),
            media_path: bundle.join("source.mp4").display().to_string(),
            sha256: "ab".repeat(32),
            title: Some("Captioned".into()),
        },
        media: MediaInfo {
            container: "mp4".into(),
            duration_secs: 10.0,
            width: 320,
            height: 240,
            fps: 10.0,
            has_audio: true,
            language: None,
        },
        tracks: Tracks::default(),
        platform: PlatformPriors {
            captions: Some(CaptionTrack {
                path: file.into(),
                language: language.into(),
                format,
                available_languages: vec![language.into()],
            }),
            chapters: vec![],
            skips: vec![],
        },
        notes: vec![],
    };
    save_manifest(&bundle, &manifest).unwrap();
    bundle
}

/// Creator captions exactly as YouTube serves them: one `utf8` segment
/// per cue, line breaks inside it, no per-word offsets.
const JSON3_CUE_LEVEL: &str = r#"{"wireMagic":"pb3","events":[
{"tStartMs":0,"dDurationMs":10000,"id":1,"wpWinPosId":1,"wsWinStyleId":1},
{"tStartMs":1200,"dDurationMs":2160,"segs":[{"utf8":"Welcome to the\ntutorial"}]},
{"tStartMs":5318,"dDurationMs":2656,"segs":[{"utf8":"run cargo build"}]},
{"tStartMs":8000,"dDurationMs":500,"segs":[{"utf8":"\n"}]}
]}"#;

fn word(start: f64, end: f64, text: &str) -> Word {
    Word {
        start_secs: start,
        end_secs: end,
        text: text.into(),
    }
}

fn whisper(language: &str, words: Vec<Word>) -> Transcript {
    Transcript {
        schema_version: 1,
        language: Some(language.into()),
        segments: vec![Segment {
            start_secs: 1.0,
            end_secs: 8.0,
            text: "welcome to the tutorial run cargo built".into(),
            words,
        }],
    }
}

#[test]
fn creator_captions_become_the_transcript_without_whisper() {
    let dir = scratch("cue-level");
    let bundle = bundle_with_captions(&dir, "source.en.json3", "en", JSON3_CUE_LEVEL);

    let cues = load_captions(&bundle).unwrap().expect("captions recorded");
    assert_eq!(cues.language, "en");
    assert!(
        !cues.has_word_timing(),
        "YouTube creator cues carry no word offsets"
    );
    let transcript = extract_caption_track(&bundle, &cues, None).unwrap();

    let texts: Vec<&str> = transcript
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(
        texts,
        vec!["Welcome to the tutorial", "run cargo build"],
        "line breaks become spaces, blank cues are dropped"
    );
    let times: Vec<(f64, f64)> = transcript
        .segments
        .iter()
        .map(|s| (s.start_secs, s.end_secs))
        .collect();
    assert_eq!(times, vec![(1.2, 3.36), (5.318, 7.974)]);
    assert!(transcript.segments.iter().all(|s| s.words.is_empty()));
    assert_eq!(transcript.language.as_deref(), Some("en"));
    assert_eq!(load_transcript(&bundle).unwrap(), transcript);
    assert_eq!(
        load_transcript_source(&bundle).unwrap(),
        TranscriptSource::CreatorCaptions
    );
    let manifest = load_manifest(&bundle).unwrap();
    assert_eq!(
        manifest.tracks.transcript.as_deref(),
        Some("transcript.json")
    );
    assert_eq!(manifest.media.language.as_deref(), Some("en"));
    assert!(
        manifest.notes.iter().any(|n| n.contains("cue-level")),
        "the missing word timing is noted: {:?}",
        manifest.notes
    );
}

#[test]
fn whisper_words_fill_in_timing_the_captions_lack() {
    let dir = scratch("word-timing");
    let bundle = bundle_with_captions(&dir, "source.en.json3", "en", JSON3_CUE_LEVEL);
    let cues = load_captions(&bundle).unwrap().unwrap();
    let whisper = whisper(
        "en",
        vec![
            word(1.21, 1.6, " Welcome"),
            word(1.6, 1.75, " to"),
            word(3.0, 3.5, " the"),
            word(3.5, 4.2, " tutorial"),
            word(5.4, 5.7, " run"),
            word(5.7, 6.4, " cargo"),
            word(6.4, 7.0, " built"),
            word(9.5, 9.7, " um"),
        ],
    );

    let transcript = extract_caption_track(&bundle, &cues, Some(&whisper)).unwrap();

    assert_eq!(transcript.segments[0].text, "Welcome to the tutorial");
    let first: Vec<&str> = transcript.segments[0]
        .words
        .iter()
        .map(|w| w.text.as_str())
        .collect();
    assert_eq!(
        first,
        vec![" Welcome", " to", " the", " tutorial"],
        "a word belongs to the cue its midpoint falls in; one in the gap \
         between cues snaps to the nearer cue"
    );
    let second: Vec<&str> = transcript.segments[1]
        .words
        .iter()
        .map(|w| w.text.as_str())
        .collect();
    assert_eq!(
        second,
        vec![" run", " cargo", " built"],
        "a word far from every cue is dropped"
    );
    assert_eq!(
        load_transcript_source(&bundle).unwrap(),
        TranscriptSource::CreatorCaptions
    );
    let manifest = load_manifest(&bundle).unwrap();
    assert!(
        manifest
            .notes
            .iter()
            .any(|n| n.contains("word timing from whisper")),
        "{:?}",
        manifest.notes
    );
}

#[test]
fn json3_word_offsets_make_whisper_unnecessary() {
    let dir = scratch("json3-words");
    let json3 = r#"{"events":[
{"tStartMs":1000,"dDurationMs":2000,"segs":[{"utf8":"Hello","tOffsetMs":0},{"utf8":" world","tOffsetMs":600}]}
]}"#;
    let bundle = bundle_with_captions(&dir, "source.en.json3", "en", json3);

    let cues = load_captions(&bundle).unwrap().unwrap();

    assert!(cues.has_word_timing());
    let words = &cues.segments[0].words;
    assert_eq!(words.len(), 2);
    assert_eq!((words[0].start_secs, words[0].end_secs), (1.0, 1.6));
    assert_eq!((words[1].start_secs, words[1].end_secs), (1.6, 3.0));
    assert_eq!(words[1].text, "world");
    assert_eq!(cues.segments[0].text, "Hello world");
}

#[test]
fn vtt_cues_are_parsed_with_tags_and_entities_removed() {
    let dir = scratch("vtt");
    let vtt = "WEBVTT\nKind: captions\nLanguage: pt-BR\n\nNOTE made by hand\n\n\
               1\n00:00:00.500 --> 00:00:01.800 align:start position:0%\n\
               Olá &amp; <c.colorE5E5E5>bem-vindo</c>\n\n\
               00:01.800 --> 00:03.000\nsegunda linha\nterceira\n\n";
    let bundle = bundle_with_captions(&dir, "source.pt-BR.vtt", "pt-BR", vtt);

    let cues = load_captions(&bundle).unwrap().unwrap();
    let transcript = extract_caption_track(&bundle, &cues, None).unwrap();

    let texts: Vec<&str> = transcript
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(texts, vec!["Olá & bem-vindo", "segunda linha terceira"]);
    assert_eq!(transcript.segments[0].start_secs, 0.5);
    assert_eq!(transcript.segments[1].end_secs, 3.0);
    assert_eq!(
        transcript.language.as_deref(),
        Some("pt"),
        "the transcript keeps whisper-style primary language codes"
    );
}

#[test]
fn captions_in_another_language_than_the_speech_are_refused() {
    let dir = scratch("mismatch");
    let bundle = bundle_with_captions(&dir, "source.en.json3", "en", JSON3_CUE_LEVEL);
    let cues = load_captions(&bundle).unwrap().unwrap();
    let whisper = whisper("pt", vec![word(1.2, 1.6, " Bem-vindo")]);
    save_transcript(&bundle, &whisper).unwrap();

    let err = extract_caption_track(&bundle, &cues, Some(&whisper)).unwrap_err();

    assert!(err.to_string().contains("pt"), "{err}");
    assert_eq!(
        load_transcript(&bundle).unwrap(),
        whisper,
        "whisper's transcript stands"
    );
    assert_eq!(
        load_transcript_source(&bundle).unwrap(),
        TranscriptSource::Whisper
    );
    let manifest = load_manifest(&bundle).unwrap();
    assert!(
        manifest.notes.iter().any(|n| n.contains("kept whisper")),
        "{:?}",
        manifest.notes
    );
}

#[test]
fn a_bundle_without_captions_loads_none() {
    let dir = scratch("none");
    let bundle = bundle_with_captions(&dir, "source.en.json3", "en", JSON3_CUE_LEVEL);
    let mut manifest = load_manifest(&bundle).unwrap();
    manifest.platform.captions = None;
    save_manifest(&bundle, &manifest).unwrap();

    assert!(load_captions(&bundle).unwrap().is_none());
}
