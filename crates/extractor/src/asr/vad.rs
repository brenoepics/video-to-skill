//! Voice activity detection: keep only the speech in a sample buffer and
//! remember how to map times in the trimmed audio back to the original.
//!
//! whisper.cpp 1.8 can do this internally, but only through `whisper_full`
//! with its context-owned state; the state-based entry point whisper-rs
//! uses ignores the VAD flags, and even upstream leaves token timestamps
//! in trimmed time. Doing the splice here keeps both segment and word
//! timestamps exact, with the same layout upstream uses: speech regions
//! concatenated, a short silence between them, and a small overlap past
//! each region so words on a boundary are not clipped.

use std::path::Path;

use anyhow::{Context, Result};
use whisper_rs::{WhisperVadContext, WhisperVadContextParams, WhisperVadParams};

/// whisper.cpp's input rate; the demux step guarantees it.
pub const SAMPLE_RATE: usize = 16_000;
const SAMPLES_PER_CS: usize = SAMPLE_RATE / 100;
/// Silence inserted between two kept regions (upstream: 0.1 s).
const GAP_SAMPLES: usize = SAMPLE_RATE / 10;
/// Audio kept past a region's end so a word on the boundary survives
/// (upstream's default `samples_overlap`, 0.1 s).
const OVERLAP_SAMPLES: usize = SAMPLE_RATE / 10;

/// A stretch of speech, in centiseconds of the original audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechRegion {
    pub start_cs: i64,
    pub end_cs: i64,
}

/// Run the Silero model over `samples` with whisper.cpp's default VAD
/// parameters (0.5 threshold, 250 ms minimum speech, 100 ms minimum
/// silence, 30 ms padding).
pub fn detect_speech(model: &Path, samples: &[f32]) -> Result<Vec<SpeechRegion>> {
    let model = model
        .to_str()
        .context("the VAD model path is not valid UTF-8")?;
    let mut vad = WhisperVadContext::new(model, WhisperVadContextParams::default())
        .context("loading the VAD model")?;
    let regions = vad
        .segments_from_samples(WhisperVadParams::default(), samples)
        .context("running voice activity detection")?;
    Ok(regions
        .map(|r| SpeechRegion {
            start_cs: whole_cs(r.start),
            end_cs: whole_cs(r.end),
        })
        .collect())
}

// whisper.cpp hands back float centiseconds and truncates them itself.
#[allow(clippy::cast_possible_truncation)]
fn whole_cs(t: f32) -> i64 {
    t as i64
}

/// Map from a time in the trimmed audio back to the original media, both
/// in centiseconds. Kept speech maps linearly; a time inside the short
/// stand-in for a removed pause snaps to the nearer edge of that pause, so
/// nothing is ever placed in audio that was cut. The empty map is the
/// identity.
#[derive(Debug, Clone, Default)]
pub struct TimeMap {
    /// Contiguous, in trimmed-time order.
    spans: Vec<Span>,
}

#[derive(Debug, Clone, Copy)]
struct Span {
    trimmed: (i64, i64),
    original: (i64, i64),
    /// A stand-in for removed audio rather than kept speech.
    pause: bool,
}

impl TimeMap {
    #[must_use]
    pub fn identity() -> TimeMap {
        TimeMap::default()
    }

    /// Original time for `trimmed_cs`; clamps outside the mapped range.
    #[must_use]
    pub fn to_original(&self, trimmed_cs: i64) -> i64 {
        let (Some(first), Some(last)) = (self.spans.first(), self.spans.last()) else {
            return trimmed_cs;
        };
        if trimmed_cs <= first.trimmed.0 {
            return first.original.0;
        }
        if trimmed_cs >= last.trimmed.1 {
            return last.original.1;
        }
        let span = self.spans[self.spans.partition_point(|s| s.trimmed.1 < trimmed_cs)];
        let (t0, t1) = span.trimmed;
        let (o0, o1) = span.original;
        if span.pause {
            return if trimmed_cs - t0 < t1 - trimmed_cs {
                o0
            } else {
                o1
            };
        }
        if t1 == t0 {
            return o0;
        }
        o0 + (trimmed_cs - t0) * (o1 - o0) / (t1 - t0)
    }

    /// How much original audio (centiseconds) was cut between two trimmed
    /// times: zero within kept speech, a pause's length across one.
    #[must_use]
    pub fn removed_between(&self, earlier_cs: i64, later_cs: i64) -> i64 {
        let original = self.to_original(later_cs) - self.to_original(earlier_cs);
        (original - (later_cs - earlier_cs)).max(0)
    }

    fn span(&mut self, trimmed: (i64, i64), original: (i64, i64), pause: bool) {
        self.spans.push(Span {
            trimmed,
            original,
            pause,
        });
    }
}

/// Concatenate the speech regions of `samples` — each extended by a short
/// overlap, separated by a short silence — and return that trimmed audio
/// with the map back to original time.
#[must_use]
pub fn keep_speech(samples: &[f32], regions: &[SpeechRegion]) -> (Vec<f32>, TimeMap) {
    let mut kept = Vec::new();
    let mut map = TimeMap::identity();
    let end_of_audio = samples.len().saturating_sub(1);
    for (i, region) in regions.iter().enumerate() {
        let next = regions.get(i + 1);
        let overlap = if next.is_some() { OVERLAP_SAMPLES } else { 0 };
        let start = to_samples(region.start_cs).min(end_of_audio);
        let end = (to_samples(region.end_cs) + overlap).min(end_of_audio);
        if end <= start {
            continue;
        }
        let kept_from = to_cs(kept.len());
        kept.extend_from_slice(&samples[start..end]);
        map.span(
            (kept_from, to_cs(kept.len())),
            (region.start_cs, region.end_cs),
            false,
        );
        if let Some(next) = next {
            let gap_from = to_cs(kept.len());
            kept.resize(kept.len() + GAP_SAMPLES, 0.0);
            map.span(
                (gap_from, to_cs(kept.len())),
                (region.end_cs, next.start_cs),
                true,
            );
        }
    }
    (kept, map)
}

fn to_samples(cs: i64) -> usize {
    usize::try_from(cs).unwrap_or(0) * SAMPLES_PER_CS
}

fn to_cs(samples: usize) -> i64 {
    i64::try_from(samples / SAMPLES_PER_CS).unwrap_or(i64::MAX)
}

#[cfg(test)]
// The splice copies samples verbatim, so float equality is exact here.
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn ramp(secs: usize) -> Vec<f32> {
        // Sample i carries the value i, so a splice is checkable by value.
        #[allow(clippy::cast_precision_loss)]
        (0..secs * SAMPLE_RATE).map(|i| i as f32).collect()
    }

    fn region(start_cs: i64, end_cs: i64) -> SpeechRegion {
        SpeechRegion { start_cs, end_cs }
    }

    #[test]
    fn identity_map_leaves_times_alone() {
        assert_eq!(TimeMap::identity().to_original(1234), 1234);
    }

    #[test]
    fn splice_keeps_speech_separated_by_a_short_silence() {
        let samples = ramp(50);

        let (kept, _) = keep_speech(&samples, &[region(100, 200), region(4200, 4300)]);

        // 1 s + 0.1 s overlap, 0.1 s silence, 1 s (last region: no overlap).
        assert_eq!(kept.len(), 17_600 + 1_600 + 16_000);
        assert_eq!(kept[0], samples[16_000], "first region starts at 1.00 s");
        assert!(
            kept[17_600..19_200].iter().all(|s| *s == 0.0),
            "the gap between regions is silence"
        );
        assert_eq!(kept[19_200], samples[672_000], "second region at 42.00 s");
    }

    #[test]
    fn trimmed_times_map_back_to_original_media_time() {
        let (_, map) = keep_speech(&ramp(50), &[region(100, 200), region(4200, 4300)]);

        assert_eq!(map.to_original(0), 100, "trimmed start is the first region");
        assert_eq!(map.to_original(55), 150, "halfway through region one");
        assert_eq!(map.to_original(110), 200, "end of region one");
        assert_eq!(map.to_original(120), 4200, "second region begins after it");
        assert_eq!(map.to_original(170), 4250);
        assert_eq!(map.to_original(220), 4300);
        assert_eq!(map.to_original(900), 4300, "past the end clamps to the end");
    }

    #[test]
    fn times_inside_a_removed_pause_snap_to_its_nearer_edge() {
        let (_, map) = keep_speech(&ramp(50), &[region(100, 200), region(4200, 4300)]);

        assert_eq!(map.to_original(113), 200, "early in the gap: end of speech");
        assert_eq!(map.to_original(117), 4200, "late in the gap: next speech");
        assert_eq!(map.to_original(115), 4200, "the midpoint rounds forward");
    }

    #[test]
    fn removed_between_measures_the_audio_cut_between_two_trimmed_times() {
        let (_, map) = keep_speech(&ramp(50), &[region(100, 200), region(4200, 4300)]);

        assert_eq!(map.removed_between(20, 60), 0, "nothing cut inside speech");
        assert_eq!(
            map.removed_between(100, 130),
            (4210 - 190) - 30,
            "crossing the pause: its length, less the 0.1 s left in its place"
        );
        assert_eq!(TimeMap::identity().removed_between(0, 5000), 0);
    }

    #[test]
    fn no_speech_keeps_nothing() {
        let (kept, map) = keep_speech(&ramp(2), &[]);

        assert!(kept.is_empty(), "no regions means nothing to decode");
        assert_eq!(map.to_original(0), 0);
    }

    #[test]
    fn regions_running_past_the_audio_are_clamped() {
        let (kept, map) = keep_speech(&ramp(1), &[region(50, 300)]);

        assert_eq!(kept.len(), 15_999 - 8_000);
        assert_eq!(map.to_original(0), 50);
        assert_eq!(map.to_original(49), 300);
    }
}
