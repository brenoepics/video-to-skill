//! Step recovery by timestamp `IoU`. The extractor emits shots and states,
//! not steps, so a gold step counts as recovered when the segments the
//! extractor assigns to it (each segment goes to the step it overlaps
//! most) cover it with `IoU` at or above a threshold; a segment is clean
//! when its step was recovered and the segment stays inside that step,
//! give or take the boundary tolerance. Over-segmentation inside a step
//! is free; a segment straddling two steps is a false positive.

/// A closed time span in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub start_secs: f64,
    pub end_secs: f64,
}

impl Span {
    #[must_use]
    pub fn new(start_secs: f64, end_secs: f64) -> Span {
        Span {
            start_secs,
            end_secs,
        }
    }

    fn len(self) -> f64 {
        (self.end_secs - self.start_secs).max(0.0)
    }

    fn overlap(self, other: Span) -> f64 {
        (self.end_secs.min(other.end_secs) - self.start_secs.max(other.start_secs)).max(0.0)
    }
}

/// Counts behind step precision (`segments_clean / segments_total`) and
/// recall (`steps_recovered / steps_gold`). Segments that overlap no
/// gold step (an unlabelled intro) are left out of `segments_total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StepCounts {
    pub steps_gold: u64,
    pub steps_recovered: u64,
    pub segments_total: u64,
    pub segments_clean: u64,
}

/// Score `segments` (disjoint, in order) against gold `steps`.
#[must_use]
pub fn recover_steps(
    segments: &[Span],
    steps: &[Span],
    iou_threshold: f64,
    tolerance_secs: f64,
) -> StepCounts {
    let assigned: Vec<Option<usize>> = segments
        .iter()
        .map(|segment| best_step(*segment, steps))
        .collect();
    let recovered: Vec<bool> = steps
        .iter()
        .enumerate()
        .map(|(step_idx, step)| {
            let members = segments
                .iter()
                .zip(&assigned)
                .filter(|(_, a)| **a == Some(step_idx))
                .map(|(s, _)| *s);
            union_iou(members, *step) >= iou_threshold
        })
        .collect();
    let mut counts = StepCounts {
        steps_gold: steps.len() as u64,
        steps_recovered: recovered.iter().filter(|r| **r).count() as u64,
        ..StepCounts::default()
    };
    for (segment, step_idx) in segments.iter().zip(&assigned) {
        let Some(step_idx) = step_idx else {
            continue;
        };
        counts.segments_total += 1;
        let step = steps[*step_idx];
        let inside = segment.start_secs >= step.start_secs - tolerance_secs
            && segment.end_secs <= step.end_secs + tolerance_secs;
        if recovered[*step_idx] && inside {
            counts.segments_clean += 1;
        }
    }
    counts
}

/// The step a segment overlaps most; `None` when it overlaps none.
fn best_step(segment: Span, steps: &[Span]) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (idx, step) in steps.iter().enumerate() {
        let overlap = segment.overlap(*step);
        if overlap > 0.0 && best.is_none_or(|(_, b)| overlap > b) {
            best = Some((idx, overlap));
        }
    }
    best.map(|(idx, _)| idx)
}

/// `IoU` between the union of disjoint `members` and `step`.
fn union_iou(members: impl Iterator<Item = Span>, step: Span) -> f64 {
    let (mut union_len, mut inter_len) = (0.0, 0.0);
    for member in members {
        union_len += member.len();
        inter_len += member.overlap(step);
    }
    let denominator = union_len + step.len() - inter_len;
    if denominator <= 0.0 {
        0.0
    } else {
        inter_len / denominator
    }
}

#[cfg(test)]
mod tests {
    use super::{recover_steps, Span, StepCounts};

    fn spans(pairs: &[(f64, f64)]) -> Vec<Span> {
        pairs.iter().map(|(s, e)| Span::new(*s, *e)).collect()
    }

    #[test]
    fn a_perfect_segmentation_recovers_every_step_cleanly() {
        let steps = spans(&[(0.0, 10.0), (10.0, 20.0)]);
        let counts = recover_steps(&steps, &steps, 0.5, 2.0);
        assert_eq!(
            counts,
            StepCounts {
                steps_gold: 2,
                steps_recovered: 2,
                segments_total: 2,
                segments_clean: 2
            }
        );
    }

    #[test]
    fn over_segmentation_inside_a_step_is_not_penalised() {
        let steps = spans(&[(0.0, 10.0), (10.0, 20.0)]);
        let segments = spans(&[(0.0, 4.0), (4.0, 10.0), (10.0, 20.0)]);
        let counts = recover_steps(&segments, &steps, 0.5, 2.0);
        assert_eq!(counts.steps_recovered, 2);
        assert_eq!((counts.segments_total, counts.segments_clean), (3, 3));
    }

    #[test]
    fn a_segment_straddling_two_steps_is_a_false_positive() {
        let steps = spans(&[(0.0, 10.0), (10.0, 20.0)]);
        let segments = spans(&[(0.0, 13.0), (13.0, 20.0)]);
        let counts = recover_steps(&segments, &steps, 0.5, 2.0);
        assert_eq!(counts.steps_recovered, 2, "both steps still mostly covered");
        assert_eq!((counts.segments_total, counts.segments_clean), (2, 1));
    }

    #[test]
    fn a_missed_boundary_loses_the_smaller_step() {
        let steps = spans(&[(0.0, 8.0), (8.0, 20.0)]);
        let segments = spans(&[(0.0, 20.0)]);
        let counts = recover_steps(&segments, &steps, 0.5, 2.0);
        assert_eq!(counts.steps_recovered, 1);
        assert_eq!((counts.segments_total, counts.segments_clean), (1, 0));
    }

    #[test]
    fn a_boundary_within_tolerance_keeps_segments_clean() {
        let steps = spans(&[(0.0, 10.0), (10.0, 20.0)]);
        let segments = spans(&[(0.0, 11.0), (11.0, 20.0)]);
        let counts = recover_steps(&segments, &steps, 0.5, 2.0);
        assert_eq!((counts.segments_total, counts.segments_clean), (2, 2));
    }

    #[test]
    fn segments_outside_every_step_are_ignored() {
        let steps = spans(&[(5.0, 10.0)]);
        let segments = spans(&[(0.0, 5.0), (5.0, 10.0)]);
        let counts = recover_steps(&segments, &steps, 0.5, 2.0);
        assert_eq!((counts.segments_total, counts.segments_clean), (1, 1));
    }
}
