//! `WebVTT` captions: cue timings with styling tags and the common HTML
//! entities removed. `<HH:MM:SS.mmm>` tags inside a cue (how YouTube
//! times words in automatic tracks) become word timing when present.

use crate::bundle::{Segment, Word};

pub fn parse(raw: &str) -> Vec<Segment> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut segments = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some((start, end)) = timing_line(lines[i].trim_start_matches('\u{feff}')) else {
            i += 1;
            continue;
        };
        i += 1;
        let mut body = Vec::new();
        while i < lines.len() && !lines[i].trim().is_empty() {
            body.push(lines[i]);
            i += 1;
        }
        let (text, words) = cue_text(&body.join("\n"), start, end);
        if !text.is_empty() {
            segments.push(Segment {
                start_secs: start,
                end_secs: end,
                text,
                words,
            });
        }
    }
    segments
}

/// `00:00:01.000 --> 00:00:03.500 align:start` → the two times.
fn timing_line(line: &str) -> Option<(f64, f64)> {
    let (start, rest) = line.split_once("-->")?;
    let end = rest.split_whitespace().next()?;
    Some((timestamp(start.trim())?, timestamp(end)?))
}

/// `HH:MM:SS.mmm` or `MM:SS.mmm`; a comma before the millis is tolerated.
fn timestamp(field: &str) -> Option<f64> {
    let parts: Vec<&str> = field.split(':').collect();
    let (hours, mins, secs) = match parts.as_slice() {
        [m, s] => ("0", *m, *s),
        [h, m, s] => (*h, *m, *s),
        _ => return None,
    };
    let hours: f64 = hours.parse().ok()?;
    let mins: f64 = mins.parse().ok()?;
    let secs: f64 = secs.replace(',', ".").parse().ok()?;
    Some(hours * 3600.0 + mins * 60.0 + secs)
}

/// Strip tags; a `<timestamp>` tag starts a new timed run. Returns the
/// plain text and, when any timed run existed, one word per token.
fn cue_text(body: &str, start: f64, end: f64) -> (String, Vec<Word>) {
    let mut runs: Vec<(f64, String)> = vec![(start, String::new())];
    let mut timed = false;
    let mut rest = body;
    while let Some(open) = rest.find('<') {
        runs_push(&mut runs, &rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            rest = &rest[open + 1..];
            continue;
        };
        let tag = &rest[open + 1..open + close];
        if let Some(at) = timestamp(tag) {
            timed = true;
            runs.push((at, String::new()));
        }
        rest = &rest[open + close + 1..];
    }
    runs_push(&mut runs, rest);

    let text = runs
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if !timed {
        return (text, Vec::new());
    }
    let mut words = Vec::new();
    for (i, (at, run)) in runs.iter().enumerate() {
        let run_end = runs.get(i + 1).map_or(end, |(next, _)| *next);
        for token in run.split_whitespace() {
            words.push(Word {
                start_secs: *at,
                end_secs: run_end,
                text: token.to_string(),
            });
        }
    }
    (text, words)
}

fn runs_push(runs: &mut [(f64, String)], raw: &str) {
    if let Some((_, text)) = runs.last_mut() {
        text.push_str(&decode_entities(raw));
    }
}

fn decode_entities(raw: &str) -> String {
    raw.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::{parse, timestamp};

    #[test]
    fn timestamps_accept_both_vtt_shapes() {
        assert_eq!(timestamp("00:01:05.250"), Some(65.25));
        assert_eq!(timestamp("01:05.250"), Some(65.25));
        assert_eq!(timestamp("00:00:02,500"), Some(2.5));
        assert_eq!(timestamp("garbage"), None);
    }

    #[test]
    fn youtube_style_word_tags_become_word_timing() {
        let raw = "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\n\
                   first<00:00:01.500><c> second</c><00:00:02.200><c> third</c>\n";

        let segments = parse(raw);

        assert_eq!(segments[0].text, "first second third");
        let words: Vec<(&str, f64, f64)> = segments[0]
            .words
            .iter()
            .map(|w| (w.text.as_str(), w.start_secs, w.end_secs))
            .collect();
        assert_eq!(
            words,
            vec![
                ("first", 1.0, 1.5),
                ("second", 1.5, 2.2),
                ("third", 2.2, 3.0)
            ]
        );
    }

    #[test]
    fn style_and_note_blocks_and_identifiers_are_skipped() {
        let raw = "WEBVTT\n\nSTYLE\n::cue { color: red }\n\nNOTE hi\n\n\
                   intro\n00:00:00.000 --> 00:00:01.000\n<v Speaker>Hi</v>\n\n";

        let segments = parse(raw);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hi");
        assert_eq!(segments[0].words, vec![]);
    }
}
