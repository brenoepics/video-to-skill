//! YouTube's json3 caption format: `events` with `tStartMs`,
//! `dDurationMs` and `segs` of `utf8` text. A creator track carries one
//! segment per cue with line breaks inside it and no word offsets
//! (checked against a real file); an automatic track adds a `tOffsetMs`
//! per word, which is honoured whenever it appears.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::bundle::{Segment, Word};

#[derive(Deserialize)]
struct Document {
    #[serde(default)]
    events: Vec<Event>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    t_start_ms: u32,
    #[serde(default)]
    d_duration_ms: u32,
    /// Absent on window-definition events.
    segs: Option<Vec<Seg>>,
    /// Set on the rolling-display continuations of automatic tracks.
    #[serde(default)]
    a_append: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Seg {
    #[serde(default)]
    utf8: String,
    t_offset_ms: Option<u32>,
}

pub fn parse(raw: &str) -> Result<Vec<Segment>> {
    let doc: Document = serde_json::from_str(raw).context("caption file is not valid json3")?;
    let mut segments = Vec::new();
    for event in doc.events {
        let Some(segs) = event.segs else {
            continue;
        };
        if event.a_append == 1 {
            continue;
        }
        let text = normalize(segs.iter().map(|s| s.utf8.as_str()));
        if text.is_empty() {
            continue;
        }
        let start = event.t_start_ms;
        let end = start.saturating_add(event.d_duration_ms);
        let words = if segs.iter().any(|s| s.t_offset_ms.is_some()) {
            words_of(&segs, start, end)
        } else {
            Vec::new()
        };
        segments.push(Segment {
            start_secs: secs(start),
            end_secs: secs(end),
            text,
            words,
        });
    }
    Ok(segments)
}

/// Each timed segment is a word: it starts at its offset and ends where
/// the next segment starts, the last one at the event's end.
fn words_of(segs: &[Seg], start_ms: u32, end_ms: u32) -> Vec<Word> {
    let starts: Vec<u32> = segs
        .iter()
        .map(|s| start_ms.saturating_add(s.t_offset_ms.unwrap_or(0)))
        .collect();
    segs.iter()
        .enumerate()
        .filter_map(|(i, seg)| {
            let text = seg.utf8.trim();
            if text.is_empty() {
                return None;
            }
            let end = starts.get(i + 1).copied().unwrap_or(end_ms);
            Some(Word {
                start_secs: secs(starts[i]),
                end_secs: secs(end),
                text: text.to_string(),
            })
        })
        .collect()
}

/// Join the segments and fold every run of whitespace — including the
/// line breaks YouTube embeds — into one space.
fn normalize<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    parts
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn secs(ms: u32) -> f64 {
    f64::from(ms) / 1000.0
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn window_definitions_and_append_events_are_not_cues() {
        let raw = r#"{"events":[
            {"tStartMs":0,"dDurationMs":1000,"id":1,"wpWinPosId":1},
            {"tStartMs":100,"dDurationMs":500,"segs":[{"utf8":"one"}]},
            {"tStartMs":590,"dDurationMs":10,"aAppend":1,"segs":[{"utf8":"\n"}]}
        ]}"#;

        let segments = parse(raw).unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "one");
        assert_eq!((segments[0].start_secs, segments[0].end_secs), (0.1, 0.6));
    }

    #[test]
    fn whitespace_only_segments_are_not_words() {
        let raw = r#"{"events":[{"tStartMs":0,"dDurationMs":1000,"segs":[
            {"utf8":"a","tOffsetMs":0},{"utf8":"\n","tOffsetMs":400},{"utf8":"b","tOffsetMs":600}
        ]}]}"#;

        let segments = parse(raw).unwrap();

        let words: Vec<(&str, f64, f64)> = segments[0]
            .words
            .iter()
            .map(|w| (w.text.as_str(), w.start_secs, w.end_secs))
            .collect();
        assert_eq!(words, vec![("a", 0.0, 0.4), ("b", 0.6, 1.0)]);
        assert_eq!(segments[0].text, "a b");
    }

    #[test]
    fn a_non_json3_file_is_an_error_not_an_empty_transcript() {
        assert!(parse("WEBVTT").is_err());
    }
}
