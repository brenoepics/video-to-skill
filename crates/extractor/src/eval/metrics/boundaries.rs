//! Boundary matching: predicted segment starts against the gold visual
//! change times, within a tolerance. Each prediction matches at most one
//! gold boundary and vice versa, nearest pairs first.

/// Counts behind boundary precision (`matched / predicted`) and recall
/// (`matched / gold`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BoundaryCounts {
    pub predicted: u64,
    pub gold: u64,
    pub matched: u64,
}

/// Match `predicted` to `gold` times, one-to-one, when they lie within
/// `tolerance_secs` of each other.
#[must_use]
pub fn match_boundaries(predicted: &[f64], gold: &[f64], tolerance_secs: f64) -> BoundaryCounts {
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (p_idx, &p) in predicted.iter().enumerate() {
        for (g_idx, &g) in gold.iter().enumerate() {
            let distance = (p - g).abs();
            if distance <= tolerance_secs {
                pairs.push((distance, p_idx, g_idx));
            }
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut predicted_used = vec![false; predicted.len()];
    let mut gold_used = vec![false; gold.len()];
    let mut matched = 0u64;
    for (_, p_idx, g_idx) in pairs {
        if !predicted_used[p_idx] && !gold_used[g_idx] {
            predicted_used[p_idx] = true;
            gold_used[g_idx] = true;
            matched += 1;
        }
    }
    BoundaryCounts {
        predicted: predicted.len() as u64,
        gold: gold.len() as u64,
        matched,
    }
}

#[cfg(test)]
mod tests {
    use super::{match_boundaries, BoundaryCounts};

    #[test]
    fn exact_and_near_hits_both_count_once() {
        let counts = match_boundaries(&[3.0, 7.4, 20.0], &[3.0, 6.0, 12.0], 1.5);
        assert_eq!(
            counts,
            BoundaryCounts {
                predicted: 3,
                gold: 3,
                matched: 2
            }
        );
    }

    #[test]
    fn two_predictions_near_one_gold_boundary_match_only_once() {
        let counts = match_boundaries(&[5.0, 6.0], &[5.5], 1.0);
        assert_eq!(counts.matched, 1);
        assert_eq!(counts.predicted, 2);
    }

    #[test]
    fn nearest_pairs_win_when_predictions_compete() {
        // 4.0 is nearer to gold 4.2 than 5.0 is; 5.0 then takes gold 5.9.
        let counts = match_boundaries(&[4.0, 5.0], &[4.2, 5.9], 1.0);
        assert_eq!(counts.matched, 2);
    }

    #[test]
    fn outside_tolerance_never_matches() {
        let counts = match_boundaries(&[10.0], &[12.5], 2.0);
        assert_eq!(counts.matched, 0);
    }

    #[test]
    fn empty_inputs_yield_zero_counts() {
        assert_eq!(match_boundaries(&[], &[], 1.0), BoundaryCounts::default());
    }
}
