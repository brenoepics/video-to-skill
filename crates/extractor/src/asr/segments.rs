//! From whisper's decoded segments to transcript segments: token texts
//! become words, and a decoded segment that runs across a pause VAD cut
//! out is split there, so no segment spans audio that was never decoded.

use super::vad::TimeMap;
use crate::bundle::{Segment, Word};

/// One decoded token, timestamps in decoded (trimmed) centiseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub t0: i64,
    pub t1: i64,
    /// Control tokens (`[_BEG_]`, `<|en|>`, timestamps) carry no text.
    pub special: bool,
}

/// A removed pause at least this long (centiseconds of original audio)
/// splits the decoded segment it falls inside. Shorter gaps are ordinary
/// phrasing, where whisper's own segmentation stands.
pub const SPLIT_PAUSE_CS: i64 = 100;

/// Transcript segments for one decoded segment, in absolute media time.
/// `text` is whisper's own text for it and `start_cs`/`end_cs` its decoded
/// bounds. Without a removed pause inside, the result is that segment
/// verbatim; otherwise one segment per stretch of continuous speech.
#[must_use]
pub fn segments_for(
    text: &str,
    start_cs: i64,
    end_cs: i64,
    tokens: &[Token],
    times: &TimeMap,
) -> Vec<Segment> {
    let pieces = split_at_pauses(tokens, times);
    if pieces.len() == 1 {
        return vec![Segment {
            start_secs: secs(times, start_cs),
            end_secs: secs(times, end_cs),
            text: text.trim().to_string(),
            words: words_of(tokens, times),
        }];
    }
    let last = pieces.len() - 1;
    pieces
        .into_iter()
        .enumerate()
        .filter_map(|(i, piece)| {
            let spoken: Vec<&Token> = piece.iter().filter(|t| !t.special).collect();
            let text = spoken.iter().map(|t| t.text.as_str()).collect::<String>();
            let text = text.trim().to_string();
            if text.is_empty() {
                return None;
            }
            let start = if i == 0 { start_cs } else { spoken.first()?.t0 };
            let end = if i == last { end_cs } else { spoken.last()?.t1 };
            Some(Segment {
                start_secs: secs(times, start),
                end_secs: secs(times, end),
                text,
                words: words_of(piece, times),
            })
        })
        .collect()
}

/// Runs of continuous speech: a new run starts at a spoken token that a
/// removed pause separates from the spoken token before it.
fn split_at_pauses<'a>(tokens: &'a [Token], times: &TimeMap) -> Vec<&'a [Token]> {
    let mut pieces = Vec::new();
    let mut piece_from = 0;
    let mut previous_end = None;
    for (i, token) in tokens.iter().enumerate() {
        if token.special {
            continue;
        }
        if let Some(end) = previous_end {
            if times.removed_between(end, token.t0) >= SPLIT_PAUSE_CS {
                pieces.push(&tokens[piece_from..i]);
                piece_from = i;
            }
        }
        previous_end = Some(token.t1);
    }
    pieces.push(&tokens[piece_from..]);
    pieces
}

/// Spoken tokens as words. Control tokens are skipped, and so is anything
/// bracket-shaped — the long-standing proxy for them.
fn words_of(tokens: &[Token], times: &TimeMap) -> Vec<Word> {
    tokens
        .iter()
        .filter(|t| !t.special && !t.text.starts_with('[') && !t.text.starts_with('<'))
        .map(|t| Word {
            start_secs: secs(times, t.t0),
            end_secs: secs(times, t.t1),
            text: t.text.clone(),
        })
        .collect()
}

/// A decoded centisecond timestamp as absolute media seconds.
// Timestamps are far below f64's 2^52 exact-integer range.
#[allow(clippy::cast_precision_loss)]
fn secs(times: &TimeMap, decoded_cs: i64) -> f64 {
    times.to_original(decoded_cs) as f64 / 100.0
}

#[cfg(test)]
// Expected times are exact centisecond quotients, so equality is exact.
#[allow(clippy::float_cmp)]
mod tests;
