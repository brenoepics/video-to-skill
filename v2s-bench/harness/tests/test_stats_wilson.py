"""Wilson intervals, normalised gain, and the clustered beta-binomial HDI.

The expected values are not self-consistency checks. Three independent
authorities are used:

1. **Newcombe (1998)**, "Two-sided confidence intervals for the single
   proportion: comparison of seven methods", *Statistics in Medicine*
   17(8):857-872. His four worked examples (81/263, 15/148, 0/20, 1/29)
   are tabulated to 4 dp for method 3, the Wilson score interval without
   continuity correction. Those four are asserted below.
2. **scipy 1.18.1**, ``scipy.stats.binomtest(k, n).proportion_ci(
   confidence_level=0.95, method="wilson")``, evaluated once while
   writing these tests and pasted in as literals. Neither this module nor
   the library imports scipy -- that is the point of the constraint.
3. **The defining quadratic.** The Wilson bounds are the two roots of
   ``(n + z^2) p^2 - (2k + z^2) p + k^2/n = 0``. ``test_roots_of_the_
   defining_quadratic`` solves that with exact ``Fraction`` coefficients
   and a different arrangement of the algebra, so a typo in the closed
   form cannot pass.
"""

from __future__ import annotations

import math
from fractions import Fraction

import pytest

from v2sbench.stats.wilson import (
    Z_95,
    beta_binomial_hdi,
    harmonic_mean_bound,
    normalized_gain,
    rate,
    wilson_interval,
    wilson_lower,
)

# Newcombe (1998) Table I, method 3 (Wilson), quoted to 4 dp.
NEWCOMBE = {
    (81, 263): (0.2553, 0.3662),
    (15, 148): (0.0624, 0.1605),
    (0, 20): (0.0, 0.1611),
    (1, 29): (0.0061, 0.1718),
}

# scipy 1.18.1, binomtest(k, n).proportion_ci(0.95, method="wilson").
SCIPY_WILSON = {
    (0, 10): (0.0, 0.27753279986288926),
    (1, 10): (0.017876213095072896, 0.4041500267952385),
    (5, 10): (0.236593090512564, 0.7634069094874361),
    (9, 10): (0.5958499732047615, 0.9821237869049271),
    (10, 10): (0.7224672001371109, 1.0),
    (0, 20): (0.0, 0.16112515805281935),
    (81, 263): (0.2552885198782742, 0.36620957698280004),
    (15, 148): (0.06238639953073629, 0.16048724172330803),
    (1, 29): (0.006113214292762667, 0.17175521879320282),
    (50, 100): (0.4038315303659956, 0.5961684696340044),
    (2, 4): (0.15003898915214953, 0.8499610108478505),
    (12, 40): (0.18074845229746528, 0.45430018818144935),
    (0, 1): (0.0, 0.7934506856227626),
    (1, 1): (0.20654931437723745, 1.0),
}


@pytest.mark.parametrize(("key", "expected"), sorted(NEWCOMBE.items()))
def test_matches_newcombe_1998_table(key: tuple[int, int], expected: tuple[float, float]) -> None:
    lo, hi = wilson_interval(*key)
    assert round(lo, 4) == pytest.approx(expected[0], abs=5e-5)
    assert round(hi, 4) == pytest.approx(expected[1], abs=5e-5)


@pytest.mark.parametrize(("key", "expected"), sorted(SCIPY_WILSON.items()))
def test_matches_scipy_to_nine_decimals(
    key: tuple[int, int], expected: tuple[float, float]
) -> None:
    # Our Z_95 is the published convention 1.959963985, scipy uses
    # 1.9599639845400545. The 4.6e-10 difference in z moves a bound by
    # under 1e-10, so 1e-9 is a real agreement test, not a loose one.
    lo, hi = wilson_interval(*key)
    assert lo == pytest.approx(expected[0], abs=1e-9)
    assert hi == pytest.approx(expected[1], abs=1e-9)


@pytest.mark.parametrize(("k", "n"), [(1, 10), (5, 10), (81, 263), (12, 40), (3, 8), (7, 16)])
def test_roots_of_the_defining_quadratic(k: int, n: int) -> None:
    """Independent check: solve the score equation, not the closed form."""
    z2 = Fraction(Z_95).limit_denominator(10**12) ** 2
    a = Fraction(n) + z2
    b = -(2 * Fraction(k) + z2)
    c = Fraction(k, 1) ** 2 / Fraction(n)
    disc = math.sqrt(float(b * b - 4 * a * c))
    root_lo = (float(-b) - disc) / float(2 * a)
    root_hi = (float(-b) + disc) / float(2 * a)
    lo, hi = wilson_interval(k, n)
    assert lo == pytest.approx(root_lo, abs=1e-12)
    assert hi == pytest.approx(root_hi, abs=1e-12)


def test_boundaries_are_exact_identities_not_clamps() -> None:
    # For k = 0 the centre and half-width coincide algebraically, so the
    # lower bound is exactly 0.0; likewise hi == 1.0 at k == n.
    for n in (1, 5, 10, 263):
        assert wilson_interval(0, n)[0] == 0.0
        assert wilson_interval(n, n)[1] == 1.0
    # ...and the interval is never degenerate there, unlike Wald.
    assert wilson_interval(0, 10)[1] > 0.27
    assert wilson_interval(10, 10)[0] < 0.73


def test_zero_trials_returns_the_vacuous_interval() -> None:
    assert wilson_interval(0, 0) == (0.0, 1.0)
    assert wilson_lower(0, 0) == 0.0
    assert rate(0, 0) == 0.0


def test_wider_interval_for_a_wider_z() -> None:
    narrow = wilson_interval(5, 10, z=1.0)
    wide = wilson_interval(5, 10, z=2.5)
    assert wide[0] < narrow[0] < narrow[1] < wide[1]


def test_lower_bound_rises_with_n_at_a_fixed_rate() -> None:
    # The gating property the parent project relies on: more evidence at
    # the same rate buys a higher lower bound.
    bounds = [wilson_lower(k, 2 * k) for k in (5, 25, 125, 625)]
    assert bounds == sorted(bounds)
    assert bounds[0] < 0.3 < bounds[-1] < 0.5


@pytest.mark.parametrize(
    ("args", "kwargs"),
    [
        ((-1, 10), {}),
        ((11, 10), {}),
        ((1, -1), {}),
        ((True, 10), {}),
        ((1.5, 10), {}),
        ((1, 10), {"z": 0.0}),
        ((1, 10), {"z": math.nan}),
    ],
)
def test_rejects_impossible_inputs(args: tuple, kwargs: dict) -> None:
    with pytest.raises(ValueError):
        wilson_interval(*args, **kwargs)


def test_harmonic_mean_bound_matches_the_parent_projects_rule() -> None:
    # vts-extract eval: an F1 row's bound is the harmonic mean of its
    # precision and recall bounds. 2*0.8*0.6/(0.8+0.6) = 0.6857142857...
    assert harmonic_mean_bound(0.8, 0.6) == pytest.approx(0.6857142857142857, abs=1e-15)
    assert harmonic_mean_bound(0.5, 0.5) == pytest.approx(0.5, abs=1e-15)
    # Lies between its inputs, pulled towards the smaller one (it is a
    # harmonic mean, not a minimum), and collapses to 0 when either does.
    assert 0.1 < harmonic_mean_bound(0.9, 0.1) < 0.2
    assert harmonic_mean_bound(0.9, 0.1) < (0.9 + 0.1) / 2
    assert harmonic_mean_bound(0.0, 0.9) == 0.0
    assert harmonic_mean_bound(0.9, 0.0) == 0.0
    with pytest.raises(ValueError):
        harmonic_mean_bound(1.2, 0.5)


def test_normalized_gain_separates_a_ceiling_from_a_weak_result() -> None:
    # Both are "+5 points"; the first closed half the available headroom.
    assert normalized_gain(0.95, 0.90) == pytest.approx(0.5, abs=1e-12)
    assert normalized_gain(0.45, 0.40) == pytest.approx(0.08333333333333333, abs=1e-12)
    assert normalized_gain(0.40, 0.40) == 0.0
    # A loss is reported as a loss, not clamped.
    assert normalized_gain(0.30, 0.40) == pytest.approx(-1.0 / 6.0, abs=1e-12)
    with pytest.raises(ValueError, match="no headroom"):
        normalized_gain(1.0, 1.0)
    with pytest.raises(ValueError):
        normalized_gain(1.1, 0.4)


def test_beta_binomial_hdi_agrees_with_wilson_when_tasks_are_homogeneous() -> None:
    # 20 tasks, 2/3 each: no between-task spread, so the clustered
    # posterior should land close to the pooled Wilson interval.
    lo, hi = beta_binomial_hdi([(2, 3)] * 20)
    w_lo, w_hi = wilson_interval(40, 60)
    assert lo == pytest.approx(w_lo, abs=0.03)
    assert hi == pytest.approx(w_hi, abs=0.03)


def test_beta_binomial_hdi_is_much_wider_when_tasks_disagree() -> None:
    # Same pooled rate (30/60), but every task is all-or-nothing: the
    # evidence is really 20 task-level coin flips, not 60 trials. The
    # pooled Wilson interval cannot see that; this one can.
    clustered = beta_binomial_hdi([(3, 3)] * 10 + [(0, 3)] * 10)
    pooled = wilson_interval(30, 60)
    assert clustered[1] - clustered[0] > 1.5 * (pooled[1] - pooled[0])
    assert clustered[0] < pooled[0] and clustered[1] > pooled[1]


def test_beta_binomial_hdi_validates_inputs() -> None:
    with pytest.raises(ValueError):
        beta_binomial_hdi([])
    with pytest.raises(ValueError):
        beta_binomial_hdi([(4, 3)])
    with pytest.raises(ValueError):
        beta_binomial_hdi([(1, 3)], credible=1.0)
