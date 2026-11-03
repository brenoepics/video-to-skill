"""Exact McNemar (the robustness check), its chi-square foil, and chi2 sf.

Authorities:

1. **Hand arithmetic**, shown in the comments. The two-sided exact
   binomial p at p = 1/2 is ``min(1, 2 * P(X <= min(b, c)))`` over
   ``n = b + c``, and every expected value below is written out as a
   ratio of integers over ``2^n`` so a reader can check it without
   running anything.
2. **scipy 1.18.1**: ``scipy.stats.binomtest(b, b + c, 0.5,
   alternative="two-sided").pvalue`` and ``scipy.stats.chi2.sf(x, 1)``,
   evaluated once while writing these tests and pasted in as literals.
"""

from __future__ import annotations

import math

import pytest

from v2sbench.stats.mcnemar import chi2_sf_1df, discordant, mcnemar_chi2, mcnemar_exact

# b, c -> two-sided exact p. Hand derivations:
#   (8,1): n=9,  m=1. 2*(C(9,0)+C(9,1))/2^9   = 2*10/512      = 0.0390625
#   (9,0): n=9,  m=0. 2*C(9,0)/2^9            = 2/512          = 0.00390625
#   (4,0): n=4,  m=0. 2*C(4,0)/2^4            = 2/16           = 0.125
#   (6,1): n=7,  m=1. 2*(1+7)/2^7             = 16/128         = 0.125
#   (2,0): n=2,  m=0. 2*1/4                   = 0.5
#   (3,1): n=4,  m=1. 2*(1+4)/16              = 10/16          = 0.625
#   (10,2): n=12, m=2. 2*(1+12+66)/4096       = 158/4096       = 0.038574218750
#   (12,3): n=15, m=3. 2*(1+15+105+455)/32768 = 1152/32768     = 0.03515625
#   (13,4): n=17, m=4. 2*(1+17+136+680+2380)/131072 = 6428/131072 = 0.049041748046875
#   (15,5): n=20, m=5. 2*(1+20+190+1140+4845+15504)/2^20 = 43400/1048576 = 0.04138946533203125
#   (5,5): n=10, m=5. 2*P(X<=5) > 1 -> capped at 1.0 (the null's own centre)
#   (1,1): n=2,  m=1. 2*(1+2)/4 = 1.5 -> capped at 1.0
EXACT = {
    (8, 1): 0.0390625,
    (1, 8): 0.0390625,
    (9, 0): 0.00390625,
    (4, 0): 0.125,
    (6, 1): 0.125,
    (2, 0): 0.5,
    (0, 2): 0.5,
    (3, 1): 0.625,
    (10, 2): 0.03857421875,
    (12, 3): 0.03515625,
    (13, 4): 0.049041748046875,
    (15, 5): 0.04138946533203125,
    (20, 8): 0.03569813817739487,
    (30, 15): 0.035697803555194696,
    (5, 5): 1.0,
    (1, 1): 1.0,
    (2, 2): 1.0,
    (1, 0): 1.0,
    (0, 1): 1.0,
}

# scipy.stats.chi2.sf(x, 1)
CHI2_SF = {
    0.0: 1.0,
    0.1: 0.7518296340458492,
    0.5: 0.47950012218695337,
    1.0: 0.31731050786291115,
    2.0: 0.15729920705028105,
    3.841458820694124: 0.04999999999999989,
    5.0: 0.025347318677468325,
    6.635: 0.009999419574042536,
    10.0: 0.001565402258002549,
    100.0: 1.5239706048320995e-23,
}


@pytest.mark.parametrize(("counts", "expected"), sorted(EXACT.items()))
def test_exact_matches_hand_arithmetic_and_scipy(
    counts: tuple[int, int], expected: float
) -> None:
    assert mcnemar_exact(*counts) == pytest.approx(expected, rel=1e-15, abs=1e-18)


def test_exact_is_symmetric_in_b_and_c() -> None:
    # The two-sided test cannot know which arm we called "treatment".
    for b, c in [(8, 1), (13, 4), (30, 15), (0, 7), (3, 3)]:
        assert mcnemar_exact(b, c) == mcnemar_exact(c, b)


def test_no_discordant_pairs_is_p_equals_one() -> None:
    assert mcnemar_exact(0, 0) == 1.0
    assert mcnemar_chi2(0, 0) == 1.0


def test_exact_is_computed_in_rational_arithmetic_so_it_survives_large_n() -> None:
    # 2**n overflows a float well before n = 2000; exact integers do not.
    p = mcnemar_exact(1200, 800)
    assert 0.0 < p < 1e-15
    assert math.isfinite(p)
    # The naive float route really does blow up at this size, which is
    # why the implementation converts a Fraction at the very end.
    with pytest.raises(OverflowError):
        float(2 ** 2000) and None


def test_exact_never_exceeds_one_even_at_the_null_centre() -> None:
    for n in range(0, 40):
        for b in range(n + 1):
            assert 0.0 < mcnemar_exact(b, n - b) <= 1.0


@pytest.mark.parametrize(("x", "expected"), sorted(CHI2_SF.items()))
def test_chi2_survival_matches_scipy(x: float, expected: float) -> None:
    assert chi2_sf_1df(x) == pytest.approx(expected, rel=1e-13)


def test_chi2_survival_at_the_five_percent_critical_value() -> None:
    # The textbook 1-df critical value; this is the closed form's anchor.
    assert chi2_sf_1df(3.841458820694124) == pytest.approx(0.05, abs=1e-15)


@pytest.mark.parametrize(
    ("b", "c", "corrected", "uncorrected"),
    [
        # scipy.stats.chi2.sf((|b-c|-1)^2/(b+c), 1) and .sf((b-c)^2/(b+c), 1)
        (8, 1, 0.04550026389635857, 0.019630657257290702),
        (10, 2, 0.04330814281079206, 0.020921335337794035),
        (20, 8, 0.03763531378731436, 0.02334220201289086),
        (13, 4, 0.052345063273163295, 0.029049022161940597),
    ],
)
def test_chi2_approximation_matches_scipy(
    b: int, c: int, corrected: float, uncorrected: float
) -> None:
    assert mcnemar_chi2(b, c, continuity=True) == pytest.approx(corrected, rel=1e-12)
    assert mcnemar_chi2(b, c, continuity=False) == pytest.approx(uncorrected, rel=1e-12)


def test_the_approximation_is_optimistic_where_counts_are_small() -> None:
    # The documented reason section 8.2 fixes the exact form: at b+c=9 the
    # uncorrected approximation reports p=0.0196 where the truth is 0.0391.
    assert mcnemar_chi2(8, 1, continuity=False) < mcnemar_exact(8, 1) / 1.9
    # The continuity correction mostly repairs that, but not reliably.
    assert mcnemar_chi2(8, 1, continuity=True) > mcnemar_exact(8, 1)


def test_tied_counts_do_not_produce_a_positive_corrected_statistic() -> None:
    # Unclamped, (|b-c|-1)^2/(b+c) would be 1/(b+c) at b == c, reporting
    # evidence from the data that agrees with the null most perfectly.
    assert mcnemar_chi2(7, 7, continuity=True) == 1.0
    assert mcnemar_chi2(7, 7, continuity=False) == 1.0


def test_discordant_counts_only_the_cells_that_carry_information() -> None:
    pairs = [
        (True, False),  # b
        (True, False),  # b
        (False, True),  # c
        (True, True),  # concordant, ignored
        (False, False),  # concordant, ignored
    ]
    assert discordant(pairs) == (2, 1)
    assert discordant([]) == (0, 0)


def test_discordant_refuses_non_boolean_outcomes() -> None:
    # A trial with status != "ok" has success = null. Coercing it would
    # turn a flaky container into evidence against an arm.
    with pytest.raises(ValueError, match="status != 'ok'"):
        discordant([(True, None)])  # type: ignore[list-item]
    with pytest.raises(ValueError, match="status != 'ok'"):
        discordant([(1, 0)])  # type: ignore[list-item]
    with pytest.raises(ValueError, match="2-tuple"):
        discordant([(True, False, True)])  # type: ignore[list-item]


@pytest.mark.parametrize(("b", "c"), [(-1, 0), (0, -1)])
def test_rejects_negative_counts(b: int, c: int) -> None:
    with pytest.raises(ValueError):
        mcnemar_exact(b, c)
    with pytest.raises(ValueError):
        mcnemar_chi2(b, c)


def test_rejects_non_integer_counts() -> None:
    with pytest.raises(ValueError):
        mcnemar_exact(1.5, 2)  # type: ignore[arg-type]
    with pytest.raises(ValueError):
        mcnemar_exact(True, 2)  # type: ignore[arg-type]


def test_chi2_sf_rejects_a_negative_statistic() -> None:
    with pytest.raises(ValueError):
        chi2_sf_1df(-1.0)
    with pytest.raises(ValueError):
        chi2_sf_1df(math.inf)
