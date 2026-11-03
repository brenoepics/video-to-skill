//! Pure: which `SponsorBlock` category, if any, a segment is mostly made of.

use crate::bundle::SkipRange;

/// The category covering the largest share of `[start, end)`, when the
/// skip ranges together cover more than half of it. Overlapping ranges
/// count once.
#[must_use]
pub fn skip_category(start: f64, end: f64, skips: &[SkipRange]) -> Option<String> {
    let length = end - start;
    if length <= 0.0 {
        return None;
    }
    let mut clipped: Vec<(f64, f64, &str)> = skips
        .iter()
        .map(|s| {
            (
                s.start_secs.max(start),
                s.end_secs.min(end),
                s.category.as_str(),
            )
        })
        .filter(|(a, b, _)| b > a)
        .collect();
    clipped.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut covered = 0.0;
    let mut reach = start;
    let mut best: Option<(&str, f64)> = None;
    for (a, b, category) in clipped {
        covered += (b - a.max(reach)).max(0.0);
        reach = reach.max(b);
        if best.is_none_or(|(_, share)| b - a > share) {
            best = Some((category, b - a));
        }
    }
    (covered > length / 2.0)
        .then_some(best)
        .flatten()
        .map(|(category, _)| category.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skip(start: f64, end: f64, category: &str) -> SkipRange {
        SkipRange {
            start_secs: start,
            end_secs: end,
            category: category.into(),
        }
    }

    #[test]
    fn a_segment_inside_a_skip_range_takes_its_category() {
        assert_eq!(
            skip_category(10.0, 20.0, &[skip(5.0, 25.0, "sponsor")]),
            Some("sponsor".into())
        );
    }

    #[test]
    fn exactly_half_is_not_mostly() {
        assert_eq!(skip_category(0.0, 10.0, &[skip(5.0, 10.0, "intro")]), None);
        assert_eq!(
            skip_category(0.0, 10.0, &[skip(4.9, 10.0, "intro")]),
            Some("intro".into())
        );
    }

    #[test]
    fn overlapping_ranges_do_not_double_count() {
        let skips = [skip(0.0, 3.0, "sponsor"), skip(1.0, 3.0, "selfpromo")];
        assert_eq!(skip_category(0.0, 10.0, &skips), None);
    }

    #[test]
    fn the_largest_share_names_the_category() {
        let skips = [skip(0.0, 2.0, "intro"), skip(2.0, 9.0, "sponsor")];
        assert_eq!(skip_category(0.0, 10.0, &skips), Some("sponsor".into()));
    }

    #[test]
    fn empty_or_degenerate_input_is_never_skipped() {
        assert_eq!(skip_category(0.0, 10.0, &[]), None);
        assert_eq!(skip_category(5.0, 5.0, &[skip(0.0, 10.0, "outro")]), None);
    }
}
