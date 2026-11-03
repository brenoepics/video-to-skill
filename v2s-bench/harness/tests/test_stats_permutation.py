"""The paired sign-flip permutation test -- the pre-registered primary test.

Authorities:

1. **scipy 1.18.1** ``scipy.stats.permutation_test((d,), np.mean,
   permutation_type="samples", alternative=..., n_resamples=np.inf)``,
   which is the same exact sign-flip test, evaluated once while writing
   these tests and pasted in as literals.
2. **Independent brute-force enumeration** over ``itertools.product``
   with exact ``Fraction`` arithmetic (``_brute``). The library reaches
   the same answer by a dynamic program over subset sums, so the two
   share no code path: a bug in the DP cannot hide.
3. **Hand arithmetic** for the closed cases: with all task differences
   equal and non-zero, only the all-plus and all-minus sign vectors are
   as extreme as observed, so two-sided ``p = 2 / 2^n`` exactly.
"""

from __future__ import annotations

import itertools
import math
from collections.abc import Sequence
from fractions import Fraction

import pytest

from v2sbench.stats.permutation import paired_permutation

# scipy, exact sign-flip, n_resamples=inf.
SCIPY_EXACT = {
    ("ones3", "two-sided"): 0.25,
    ("ones3", "greater"): 0.125,
    ("ones3", "less"): 1.0,
    ("ones5", "two-sided"): 0.0625,
    ("ones5", "greater"): 0.03125,
    ("mixed5", "two-sided"): 0.1875,
    ("zeros4", "two-sided"): 1.0,
    ("thirds6", "two-sided"): 0.25,
    ("thirds6", "less"): 0.96875,
    ("k3_10", "two-sided"): 0.046875,
    ("k3_10", "greater"): 0.0234375,
}
DATA: dict[str, list[float]] = {
    "ones3": [1.0, 1.0, 1.0],
    "ones5": [1.0] * 5,
    "mixed5": [0.5, 1.0, 1.0, 1.0, -0.5],
    "zeros4": [0.0, 0.0, 0.0, 0.0],
    "thirds6": [1 / 3, 1 / 3, 2 / 3, 0.0, -1 / 3, 1.0],
    "k3_10": [1 / 3, 0.0, 2 / 3, 1 / 3, -1 / 3, 1.0, 0.0, 1 / 3, 2 / 3, 1 / 3],
}


def _brute(diffs: Sequence[float], alternative: str) -> float:
    """Enumerate all 2^n sign vectors directly, in exact rational arithmetic."""
    fractions = [Fraction(x).limit_denominator(10**6) for x in diffs]
    observed = sum(fractions)
    hits = 0
    for signs in itertools.product((1, -1), repeat=len(fractions)):
        total = sum(s * f for s, f in zip(signs, fractions, strict=True))
        if alternative == "two-sided":
            hits += abs(total) >= abs(observed)
        elif alternative == "greater":
            hits += total >= observed
        else:
            hits += total <= observed
    return hits / 2 ** len(fractions)


@pytest.mark.parametrize(("key", "expected"), sorted(SCIPY_EXACT.items()))
def test_matches_scipys_exact_sign_flip_test(key: tuple[str, str], expected: float) -> None:
    name, alternative = key
    result = paired_permutation(DATA[name], alternative=alternative)
    assert result.mode == "exact-enumeration"
    assert result.p_value == pytest.approx(expected, rel=1e-15, abs=1e-18)


@pytest.mark.parametrize("name", sorted(DATA))
@pytest.mark.parametrize("alternative", ["two-sided", "greater", "less"])
def test_matches_independent_brute_force_enumeration(name: str, alternative: str) -> None:
    mine = paired_permutation(DATA[name], alternative=alternative)
    assert mine.mode == "exact-enumeration"
    assert mine.p_value == pytest.approx(_brute(DATA[name], alternative), rel=1e-15, abs=1e-18)


@pytest.mark.parametrize("n", [1, 2, 3, 5, 8, 13, 20])
def test_all_equal_differences_give_two_over_two_to_the_n(n: int) -> None:
    # Only the all-plus and all-minus vectors reach |S| >= |S_obs|.
    result = paired_permutation([0.75] * n)
    assert result.mode == "exact-enumeration"
    assert result.sign_vectors == 2**n
    assert result.p_value == pytest.approx(2 / 2**n, rel=1e-15)


def test_all_zero_differences_give_p_equals_one() -> None:
    # Every sign vector is exactly as extreme as the observed one.
    assert paired_permutation([0.0] * 7).p_value == 1.0


def test_exact_path_is_used_up_to_the_declared_task_limit() -> None:
    # 20 tasks is 1,048,576 sign vectors; the subset-sum DP still
    # enumerates them exactly, which is the whole point of not looping.
    result = paired_permutation([1 / 3, 2 / 3, 0.0, -1 / 3] * 5)
    assert result.mode == "exact-enumeration"
    assert result.sign_vectors == 2**20
    assert result.n_tasks == 20


def test_above_the_limit_it_samples_and_says_so() -> None:
    result = paired_permutation([1 / 3] * 21, resamples=2000, seed=3)
    assert result.mode == "monte-carlo"
    assert result.resamples == 2000
    assert result.sign_vectors is None


def test_sampled_p_value_is_never_zero_and_is_bounded_below() -> None:
    # (1 + count) / (1 + B): a p-value of exactly 0 would be
    # indistinguishable from "too few resamples".
    result = paired_permutation([1.0] * 30, resamples=500, seed=1)
    assert result.p_value >= 1 / 501
    assert result.p_value > 0.0


def test_sampling_approximates_the_exact_answer() -> None:
    # 21 tasks all +1/3: exact two-sided p is 2/2^21 ~ 9.5e-7, far below
    # the sampling floor, so the sampled value must sit at the floor.
    sampled = paired_permutation([1 / 3] * 21, resamples=4000, seed=5)
    assert sampled.p_value == pytest.approx(1 / 4001, rel=1e-12)


def test_sampling_recovers_a_moderate_p_value() -> None:
    diffs = [1 / 3, 0.0, 2 / 3, 1 / 3, -1 / 3, 1.0, 0.0, 1 / 3, 2 / 3, 1 / 3]
    exact = paired_permutation(diffs).p_value
    sampled = paired_permutation(diffs, resamples=20000, seed=11, exact_max_tasks=0)
    assert sampled.mode == "monte-carlo"
    assert sampled.p_value == pytest.approx(exact, abs=0.01)


def test_is_deterministic_for_a_given_seed() -> None:
    # A deliberately weak signal, so the sampled p-value sits in the
    # interior and seed-to-seed variation is visible rather than clipped
    # against the 1/(1+B) floor.
    diffs = [0.2, -0.1, 0.4, 0.0, -0.3, 0.1] * 4
    first = paired_permutation(diffs, resamples=1000, seed=7, exact_max_tasks=0)
    again = paired_permutation(diffs, resamples=1000, seed=7, exact_max_tasks=0)
    other = paired_permutation(diffs, resamples=1000, seed=8, exact_max_tasks=0)
    assert first.p_value == again.p_value
    assert first.p_value != other.p_value
    assert first.p_value == pytest.approx(other.p_value, abs=0.05)


def test_one_sided_alternatives_partition_the_evidence() -> None:
    diffs = [0.4, 0.2, 0.6, -0.2, 0.8, 0.4, 0.0]
    greater = paired_permutation(diffs, alternative="greater").p_value
    less = paired_permutation(diffs, alternative="less").p_value
    two = paired_permutation(diffs, alternative="two-sided").p_value
    assert greater < less  # the data favour the treatment
    assert two == pytest.approx(2 * greater, rel=1e-12)


def test_the_statistic_reported_is_the_mean_task_difference() -> None:
    result = paired_permutation([0.0, 0.5, 1.0, 0.5])
    assert result.statistic == pytest.approx(0.5, rel=1e-15)
    assert result.n_tasks == 4


def test_a_single_task_can_never_reach_significance() -> None:
    # With one task there are two sign vectors and both are as extreme.
    assert paired_permutation([1.0]).p_value == 1.0


def test_rejects_bad_inputs() -> None:
    with pytest.raises(ValueError, match="empty"):
        paired_permutation([])
    with pytest.raises(ValueError, match="alternative"):
        paired_permutation([0.5], alternative="sideways")
    with pytest.raises(ValueError, match="resamples"):
        paired_permutation([0.5], resamples=0)
    with pytest.raises(ValueError, match="not finite"):
        paired_permutation([0.5, math.nan])
    with pytest.raises(ValueError, match="seed"):
        paired_permutation([0.5], seed=1.5)  # type: ignore[arg-type]
