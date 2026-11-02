use super::*;
use crate::asr::vad::{keep_speech, SpeechRegion, SAMPLE_RATE};

fn token(text: &str, t0: i64, t1: i64) -> Token {
    Token {
        text: text.into(),
        t0,
        t1,
        t_dtw: None,
        special: false,
    }
}

fn special(text: &str, t: i64) -> Token {
    Token {
        text: text.into(),
        t0: t,
        t1: t,
        t_dtw: None,
        special: true,
    }
}

/// Speech at 0–1.4 s and 41.4–43.3 s: trimmed, the second block starts
/// at 1.6 s, right after the 0.1 s stand-in for the pause.
fn pause_map() -> TimeMap {
    let samples = vec![0.0; 50 * SAMPLE_RATE];
    let regions = [
        SpeechRegion {
            start_cs: 0,
            end_cs: 140,
        },
        SpeechRegion {
            start_cs: 4140,
            end_cs: 4330,
        },
    ];
    keep_speech(&samples, &regions).1
}

fn across_the_pause() -> Vec<Token> {
    vec![
        special("[_BEG_]", 0),
        token(" Welcome", 0, 44),
        token(" to", 44, 56),
        token(" the", 56, 75),
        token(" tutorial", 75, 130),
        token(",", 155, 156),
        token(" now", 156, 170),
        token(" open", 170, 190),
        token(".", 340, 350),
        special("[_TT_350]", 350),
    ]
}

#[test]
fn a_decoded_segment_is_split_where_vad_removed_a_pause() {
    let text = " Welcome to the tutorial, now open.";

    let segments = segments_for(text, 0, 350, &across_the_pause(), &pause_map());

    assert_eq!(segments.len(), 2, "{segments:#?}");
    assert_eq!(segments[0].text, "Welcome to the tutorial");
    assert_eq!((segments[0].start_secs, segments[0].end_secs), (0.0, 1.21));
    assert_eq!(segments[0].words.len(), 4);
    assert_eq!(segments[1].text, ", now open.");
    assert_eq!(
        (segments[1].start_secs, segments[1].end_secs),
        (41.4, 43.3),
        "the second block begins after the pause and keeps the decoded end"
    );
    assert_eq!(segments[1].words[0].text, ",");
    assert_eq!(segments[1].words[0].start_secs, 41.4);
    assert_eq!(segments[1].words[3].text, ".");
}

#[test]
fn without_a_removed_pause_the_decoded_text_is_kept_verbatim() {
    let tokens = vec![
        special("<|en|>", 0),
        token(" Hello", 0, 30),
        token(" there", 30, 60),
        special("[_TT_60]", 60),
    ];

    let segments = segments_for("  Hello there ", 0, 60, &tokens, &pause_map());

    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].text, "Hello there");
    assert_eq!(segments[0].words.len(), 2, "control tokens are not words");
    assert_eq!(segments[0].words[1].text, " there");
}

#[test]
fn plain_whisper_never_splits_even_across_long_silences() {
    let tokens = vec![token(" one", 0, 50), token(" two", 4000, 4050)];

    let segments = segments_for(" one two", 0, 4050, &tokens, &TimeMap::identity());

    assert_eq!(
        segments.len(),
        1,
        "no VAD: whisper's own segmentation stands"
    );
    assert_eq!((segments[0].start_secs, segments[0].end_secs), (0.0, 40.5));
    assert_eq!(segments[0].words[1].start_secs, 40.0);
}

#[test]
fn a_split_piece_with_no_text_is_dropped() {
    let tokens = vec![
        token(" Welcome", 0, 130),
        special("[_TT_156]", 156),
        token(" ", 340, 350),
        special("[_TT_350]", 350),
    ];

    let segments = segments_for(" Welcome", 0, 350, &tokens, &pause_map());

    assert_eq!(segments.len(), 1, "{segments:#?}");
    assert_eq!(segments[0].text, "Welcome");
}

fn token_dtw(text: &str, t0: i64, t1: i64, t_dtw: i64) -> Token {
    Token {
        t_dtw: Some(t_dtw),
        ..token(text, t0, t1)
    }
}

/// Speech at 0–1.4 s and 1.9–4.0 s: a half-second pause, too short to
/// split a segment, removed between them.
fn short_pause_map() -> TimeMap {
    let samples = vec![0.0; 5 * SAMPLE_RATE];
    let regions = [
        SpeechRegion {
            start_cs: 0,
            end_cs: 140,
        },
        SpeechRegion {
            start_cs: 190,
            end_cs: 400,
        },
    ];
    keep_speech(&samples, &regions).1
}

#[test]
fn a_word_starts_where_the_previous_word_s_dtw_alignment_ends() {
    // whisper leaves t_dtw in decoded time, exactly like t0/t1: the
    // expected seconds are the remapped ones.
    let tokens = vec![
        token_dtw(" now", 156, 170, 165),
        token_dtw(" open", 170, 190, 176),
    ];

    let segments = segments_for(" now open", 156, 190, &tokens, &pause_map());

    let words = &segments[0].words;
    assert_eq!(
        (words[0].start_secs, words[0].end_secs),
        (41.4, 41.45),
        "the first word keeps whisper's start and ends at its alignment"
    );
    assert_eq!(
        (words[1].start_secs, words[1].end_secs),
        (41.45, 41.56),
        "trimmed 1.65 s is 0.05 s into the second speech region"
    );
}

#[test]
fn a_token_without_a_dtw_time_keeps_its_heuristic_bounds() {
    let tokens = vec![token(" now", 156, 170), token(" open", 170, 190)];

    let segments = segments_for(" now open", 156, 190, &tokens, &pause_map());

    let words = &segments[0].words;
    assert_eq!((words[1].start_secs, words[1].end_secs), (41.5, 41.7));
}

#[test]
fn a_word_never_ends_before_it_starts() {
    let tokens = vec![token_dtw(" now", 156, 170, 150)];

    let segments = segments_for(" now", 156, 170, &tokens, &pause_map());

    let word = &segments[0].words[0];
    assert_eq!((word.start_secs, word.end_secs), (41.4, 41.4));
}

#[test]
fn a_removed_pause_hands_the_next_word_back_to_whisper_s_onset() {
    // "tutorial" ends in the first region; "now" is spoken after the cut
    // pause, where whisper's heuristic sits on the onset at 1.9 s.
    let tokens = vec![
        token_dtw(" tutorial", 75, 130, 135),
        token_dtw(" now", 156, 170, 165),
    ];

    let segments = segments_for(" tutorial now", 75, 170, &tokens, &short_pause_map());

    assert_eq!(
        segments.len(),
        1,
        "half a second is no split: {segments:#?}"
    );
    let words = &segments[0].words;
    assert_eq!(words[0].end_secs, 1.26);
    assert_eq!(
        words[1].start_secs, 1.9,
        "not 1.26 s, inside the phrase before the pause"
    );
}
