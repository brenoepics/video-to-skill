//! Confidence bounds for the proportions the eval reports. Gating on a
//! lower bound instead of a point estimate keeps a gold set of a few
//! hundred boundaries from flagging run-to-run jitter as a regression.

/// z for a two-sided 95 % interval.
const Z_95: f64 = 1.959_964;

/// A count of successes out of trials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Proportion {
    pub numerator: u64,
    pub denominator: u64,
}

impl Proportion {
    #[must_use]
    pub fn new(numerator: u64, denominator: u64) -> Proportion {
        Proportion {
            numerator,
            denominator,
        }
    }

    /// Point estimate; `None` with no trials.
    #[must_use]
    pub fn value(self) -> Option<f64> {
        (self.denominator > 0).then(|| as_f64(self.numerator) / as_f64(self.denominator))
    }

    /// Wilson score interval lower bound at 95 %; `None` with no trials.
    #[must_use]
    pub fn lower_bound(self) -> Option<f64> {
        let p = self.value()?;
        let n = as_f64(self.denominator);
        let z2 = Z_95 * Z_95;
        let centre = p + z2 / (2.0 * n);
        let spread = Z_95 * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
        Some(((centre - spread) / (1.0 + z2 / n)).max(0.0))
    }
}

/// Harmonic mean of a precision and a recall; zero when either is zero.
#[must_use]
pub fn f1(precision: f64, recall: f64) -> f64 {
    if precision + recall <= 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    }
}

#[allow(clippy::cast_precision_loss)] // counts stay far below 2^52
fn as_f64(count: u64) -> f64 {
    count as f64
}

#[cfg(test)]
mod tests {
    use super::{f1, Proportion};

    #[test]
    fn wilson_lower_bound_matches_the_textbook_value() {
        // 8 of 10 at 95 %: Wilson lower bound is 0.490 (Newcombe 1998).
        let bound = Proportion::new(8, 10).lower_bound().unwrap();
        assert!((bound - 0.490).abs() < 0.001, "got {bound}");
    }

    #[test]
    fn a_perfect_score_still_has_a_bound_below_one() {
        let bound = Proportion::new(50, 50).lower_bound().unwrap();
        assert!(bound > 0.9 && bound < 1.0, "got {bound}");
    }

    #[test]
    fn zero_successes_bound_at_zero_and_zero_trials_are_unscored() {
        assert_eq!(Proportion::new(0, 10).lower_bound(), Some(0.0));
        assert_eq!(Proportion::new(0, 0).lower_bound(), None);
        assert_eq!(Proportion::new(0, 0).value(), None);
    }

    #[test]
    fn more_trials_tighten_the_bound() {
        let few = Proportion::new(8, 10).lower_bound().unwrap();
        let many = Proportion::new(80, 100).lower_bound().unwrap();
        assert!(many > few, "{many} should exceed {few}");
    }

    #[test]
    fn f1_is_the_harmonic_mean_and_tolerates_zero() {
        assert!((f1(0.5, 1.0) - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(f1(0.0, 1.0), 0.0);
        assert_eq!(f1(0.0, 0.0), 0.0);
    }
}
