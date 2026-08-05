//! Pure alignment: assign each transcript segment to the shot it
//! overlaps most. No I/O; fully unit-tested.

use crate::bundle::Segment;

/// For each shot range, the indices of transcript segments assigned to
/// it. Every transcript segment is assigned to exactly one shot: the one
/// it overlaps longest (ties and zero-overlap stragglers go to the
/// nearest shot, earlier on ties).
#[must_use]
pub fn assign_speech(transcript: &[Segment], shots: &[(f64, f64)]) -> Vec<Vec<usize>> {
    let mut assigned: Vec<Vec<usize>> = vec![Vec::new(); shots.len()];
    if shots.is_empty() {
        return assigned;
    }
    for (seg_idx, seg) in transcript.iter().enumerate() {
        let mut best = 0usize;
        let mut best_overlap = f64::MIN;
        for (shot_idx, &(start, end)) in shots.iter().enumerate() {
            let overlap = overlap_len(seg.start_secs, seg.end_secs, start, end);
            let score = if overlap > 0.0 {
                overlap
            } else {
                // Straggler (e.g. speech past the sampled video end):
                // negative distance to the shot, so nearest wins.
                -distance(seg.start_secs, seg.end_secs, start, end)
            };
            if score > best_overlap {
                best_overlap = score;
                best = shot_idx;
            }
        }
        assigned[best].push(seg_idx);
    }
    assigned
}

fn overlap_len(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

fn distance(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    if a1 < b0 {
        b0 - a1
    } else if a0 > b1 {
        a0 - b1
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64) -> Segment {
        Segment {
            start_secs: start,
            end_secs: end,
            text: String::new(),
            words: vec![],
        }
    }

    #[test]
    fn contained_speech_goes_to_its_shot() {
        let shots = [(0.0, 3.0), (3.0, 6.0)];
        let out = assign_speech(&[seg(0.5, 2.0)], &shots);
        assert_eq!(out, vec![vec![0], vec![]]);
    }

    #[test]
    fn straddling_speech_goes_to_the_larger_overlap() {
        let shots = [(0.0, 3.0), (3.0, 6.0)];
        // 0.5s in shot 0, 1.9s in shot 1.
        let out = assign_speech(&[seg(2.5, 4.9)], &shots);
        assert_eq!(out, vec![vec![], vec![0]]);
    }

    #[test]
    fn exact_tie_prefers_the_earlier_shot() {
        let shots = [(0.0, 3.0), (3.0, 6.0)];
        let out = assign_speech(&[seg(2.0, 4.0)], &shots);
        assert_eq!(out, vec![vec![0], vec![]]);
    }

    #[test]
    fn speech_past_the_last_shot_clamps_to_it() {
        let shots = [(0.0, 3.0), (3.0, 6.0)];
        let out = assign_speech(&[seg(6.2, 7.0)], &shots);
        assert_eq!(out, vec![vec![], vec![0]]);
    }

    #[test]
    fn shots_without_speech_stay_empty_and_order_is_preserved() {
        let shots = [(0.0, 2.0), (2.0, 4.0), (4.0, 6.0)];
        let out = assign_speech(&[seg(4.1, 4.5), seg(4.6, 5.0)], &shots);
        assert_eq!(out, vec![vec![], vec![], vec![0, 1]]);
    }
}
