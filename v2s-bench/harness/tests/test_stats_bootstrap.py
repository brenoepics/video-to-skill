"""The cluster bootstrap, its BCa corrections, and the normal quantile.

Authorities:

1. **scipy 1.18.1** for the normal quantile (``scipy.special.ndtri``) and
   for the BCa tail map (``ndtr``/``ndtri`` composed by hand per Efron &
   Tibshirani 1993 eq. 14.10), evaluated once while writing these tests
   and pasted in as literals.
2. **numpy 2.5.3** for the jackknife acceleration of the reference
   dataset, computed from Efron & Tibshirani eq. 14.15 and pinned below
   as ``D12_ACCELERATION``.
3. Separately, while writing these tests, the whole pipeline was checked
   against ``scipy.stats.bootstrap(..., method="BCa")`` by feeding
   *scipy's own* resample distribution through ``_bca_quantiles`` and
   ``_order_stat``: it reproduced scipy's published interval
   ``(0.150000, 0.416667)`` exactly. Running both on their own RNG
   streams does **not** agree to the last digit on this dataset, because
   its resample distribution is lumpy (atoms 1/60 apart) and the 2.5%
   point lands on a jump, so a sampling difference in ``z0`` moves the
   bound by exactly one atom. That is a property of the data, not a
   disagreement between the implementations.
"""

from __future__ import annotations

import math

import pytest

from v2sbench.stats.bootstrap import (
    BootstrapResult,
    _bca_quantiles,
    bootstrap_mean,
    cluster_bootstrap_diff,
    norm_cdf,
    norm_ppf,
    task_diffs,
)

# scipy.special.ndtri
NDTRI = {
    0.975: 1.959963984540054,
    0.95: 1.6448536269514722,
    0.5: 0.0,
    0.025: -1.9599639845400545,
    0.001: -3.090232306167813,
    0.9999: 3.719016485455709,
    0.0001: -3.7190164854556804,
    0.4: -0.2533471031357997,
    0.6: 0.2533471031357997,
    0.30853753872598694: -0.49999999999999994,
}

#: The 12 per-task differences used as the BCa reference dataset.
D12 = [0.0, 0.2, 0.2, 0.4, 0.0, 0.6, 0.2, 0.4, 0.8, 0.0, 0.2, 0.4]
#: numpy: sum((jm - jack)**3) / (6 * sum((jm - jack)**2)**1.5)
D12_ACCELERATION = 0.031106824305105238


def _pairs(wins: int, losses: int, ties: int) -> list[tuple[bool, bool]]:
    return (
        [(True, False)] * wins + [(False, True)] * losses + [(True, True)] * ties
    )


def test_normal_quantile_matches_scipy_to_machine_precision() -> None:
    for p, expected in NDTRI.items():
        assert norm_ppf(p) == pytest.approx(expected, abs=1e-12)


def test_normal_quantile_hits_the_published_z_values() -> None:
    # The two constants the pre-registration and BENCHMARK.md quote.
    assert norm_ppf(0.975) == pytest.approx(1.959963985, abs=1e-6)
    assert norm_ppf(0.95) == pytest.approx(1.644853627, abs=1e-6)
    # ...and far better than the 1e-6 the brief asks for.
    assert norm_ppf(0.975) == pytest.approx(1.959963985, abs=1e-9)


def test_normal_cdf_and_quantile_are_inverses() -> None:
    for p in (1e-9, 0.001, 0.02424, 0.02425, 0.2, 0.5, 0.8, 0.97576, 0.999, 1 - 1e-9):
        assert norm_cdf(norm_ppf(p)) == pytest.approx(p, rel=1e-11)


def test_normal_quantile_rejects_values_outside_the_open_unit_interval() -> None:
    for bad in (0.0, 1.0, -0.1, 1.1, math.nan):
        with pytest.raises(ValueError):
            norm_ppf(bad)


def test_bca_tail_map_matches_an_independently_computed_reference() -> None:
    # Construct a resample distribution with exactly 40 of 100 values
    # strictly below the point estimate, so z0 = ndtri(0.40) exactly.
    # With the dataset's acceleration pinned above, scipy's ndtr/ndtri
    # give alpha1 = 0.010060349254968322, alpha2 = 0.9393030160107421.
    point = math.fsum(D12) / len(D12)
    ordered = sorted([point - 1.0] * 40 + [point + 1.0] * 60)
    result = _bca_quantiles(point, ordered, D12, 0.025)
    assert not isinstance(result, str)
    assert result[0] == pytest.approx(0.010060349254968322, rel=1e-11)
    assert result[1] == pytest.approx(0.9393030160107421, rel=1e-11)


def test_bca_acceleration_sign_shows_up_in_the_tail_map() -> None:
    # A positive acceleration (this dataset: +0.0311) pushes both tail
    # probabilities up relative to the plain percentile alphas.
    point = math.fsum(D12) / len(D12)
    ordered = sorted([point - 1.0] * 50 + [point + 1.0] * 50)
    alpha1, alpha2 = _bca_quantiles(point, ordered, D12, 0.025)  # type: ignore[misc]
    assert D12_ACCELERATION > 0
    assert alpha1 > 0.025
    assert alpha2 > 0.975


def test_identical_task_differences_give_a_degenerate_interval() -> None:
    # Every task shifts by exactly +0.4, so every resample mean is 0.4 and
    # there is no sampling variation to report. BCa is undefined here and
    # the fall back is recorded rather than hidden.
    clusters = [_pairs(2, 0, 3)] * 8
    result = cluster_bootstrap_diff(clusters, resamples=500, seed=0)
    assert result.point == pytest.approx(0.4, abs=1e-15)
    assert result.ci_lo == pytest.approx(0.4, abs=1e-12)
    assert result.ci_hi == pytest.approx(0.4, abs=1e-12)
    assert result.ci_method == "percentile"
    assert result.fallback_reason is not None
    assert "bca-undefined" in result.fallback_reason


def test_treatment_always_winning_gives_an_interval_strictly_above_zero() -> None:
    clusters = [
        _pairs(5, 0, 0), _pairs(3, 0, 2), _pairs(4, 0, 1), _pairs(2, 0, 3),
        _pairs(5, 0, 0), _pairs(1, 0, 4), _pairs(3, 0, 2), _pairs(4, 0, 1),
    ]
    result = cluster_bootstrap_diff(clusters, resamples=2000, seed=0)
    assert result.ci_lo > 0.0
    assert result.point > 0.5
    assert result.ci_hi <= 1.0


def test_no_difference_gives_an_interval_containing_zero() -> None:
    clusters = [
        _pairs(1, 1, 3), _pairs(2, 2, 1), _pairs(0, 0, 5), _pairs(2, 1, 2),
        _pairs(1, 2, 2), _pairs(3, 3, 0), _pairs(0, 1, 4), _pairs(1, 0, 4),
    ]
    result = cluster_bootstrap_diff(clusters, resamples=2000, seed=0)
    assert result.ci_lo <= 0.0 <= result.ci_hi


def test_treatment_always_losing_gives_an_interval_strictly_below_zero() -> None:
    clusters = [_pairs(0, 4, 1), _pairs(0, 3, 2), _pairs(0, 5, 0), _pairs(0, 2, 3)] * 2
    result = cluster_bootstrap_diff(clusters, resamples=2000, seed=0)
    assert result.ci_hi < 0.0


def test_is_bit_identical_for_a_given_seed() -> None:
    clusters = [_pairs(3, 1, 1), _pairs(1, 1, 3), _pairs(4, 0, 1), _pairs(0, 2, 3)] * 3
    first = cluster_bootstrap_diff(clusters, resamples=1000, seed=42)
    again = cluster_bootstrap_diff(clusters, resamples=1000, seed=42)
    assert first == again
    assert isinstance(first, BootstrapResult)


def test_a_different_seed_moves_the_bounds_but_not_the_conclusion() -> None:
    clusters = [
        _pairs(3, 1, 1), _pairs(1, 1, 3), _pairs(4, 0, 1), _pairs(0, 2, 3),
        _pairs(2, 0, 3), _pairs(5, 0, 0), _pairs(0, 0, 5), _pairs(1, 2, 2),
        _pairs(2, 1, 2), _pairs(3, 0, 2), _pairs(0, 1, 4), _pairs(4, 1, 0),
        _pairs(1, 0, 4), _pairs(2, 2, 1), _pairs(3, 2, 0), _pairs(0, 3, 2),
        _pairs(5, 0, 0), _pairs(1, 1, 3), _pairs(2, 0, 3), _pairs(0, 0, 5),
    ]
    results = [cluster_bootstrap_diff(clusters, resamples=1000, seed=s) for s in (1, 2, 3, 4)]
    # The estimate is a function of the data alone; only the interval is sampled.
    assert len({result.point for result in results}) == 1
    # The resample draws really do differ between seeds.
    assert len({(result.ci_lo, result.ci_hi) for result in results}) > 1
    # ...but every interval overlaps every other: sampling noise, not disagreement.
    assert max(r.ci_lo for r in results) < min(r.ci_hi for r in results)
    assert max(r.ci_lo for r in results) - min(r.ci_lo for r in results) < 0.05


def test_clustering_is_the_whole_point_the_naive_interval_is_too_narrow() -> None:
    """The test this module exists for.

    Ten tasks, two of which the treatment always wins and eight of which
    it never moves. Every trial within a task agrees with its task, so
    the 100 trials carry only 10 tasks' worth of information. Treating
    each trial as its own cluster -- which is exactly what a trial-level
    bootstrap does -- divides by sqrt(100) instead of sqrt(10) and
    reports an interval far too tight.
    """
    clusters = [_pairs(10, 0, 0)] * 2 + [_pairs(0, 0, 10)] * 8
    clustered = cluster_bootstrap_diff(clusters, resamples=4000, seed=0)
    # One singleton cluster per trial: the naive, wrong analysis.
    naive_clusters = [[pair] for cluster in clusters for pair in cluster]
    naive = cluster_bootstrap_diff(naive_clusters, resamples=4000, seed=0)

    assert clustered.point == pytest.approx(naive.point, abs=1e-12)
    assert clustered.n_tasks == 10
    assert naive.n_tasks == 100
    clustered_width = clustered.ci_hi - clustered.ci_lo
    naive_width = naive.ci_hi - naive.ci_lo
    assert clustered_width > 2.5 * naive_width
    # And the direction of the error is the dangerous one: the naive
    # interval excludes 0 here, the honest one does not.
    assert naive.ci_lo > 0.0
    assert clustered.ci_lo <= 0.0


def test_point_and_pooled_estimates_differ_when_trial_counts_differ() -> None:
    # Task A: 1 trial, treatment wins. Task B: 9 trials, no movement.
    # Task-level mean: (1.0 + 0.0) / 2 = 0.5. Pooled: 1/10 = 0.1.
    clusters = [_pairs(1, 0, 0), _pairs(0, 0, 9)]
    result = cluster_bootstrap_diff(clusters, resamples=200, seed=0)
    assert result.point == pytest.approx(0.5, abs=1e-15)
    assert result.pooled_point == pytest.approx(0.1, abs=1e-15)
    assert result.n_pairs == 10
    assert result.n_tasks == 2


def test_point_and_pooled_agree_on_a_balanced_grid() -> None:
    clusters = [_pairs(2, 1, 0), _pairs(1, 0, 2), _pairs(0, 0, 3), _pairs(3, 0, 0)]
    result = cluster_bootstrap_diff(clusters, resamples=200, seed=0)
    assert result.point == pytest.approx(result.pooled_point, abs=1e-15)


def test_task_diffs_is_the_single_definition_of_the_per_task_difference() -> None:
    diffs, n_pairs, pooled = task_diffs([_pairs(2, 1, 1), _pairs(0, 0, 2)])
    assert diffs == pytest.approx([0.25, 0.0])
    assert n_pairs == 6
    assert pooled == pytest.approx(1 / 6)


def test_percentile_method_can_be_requested_explicitly() -> None:
    clusters = [_pairs(3, 1, 1), _pairs(1, 1, 3), _pairs(4, 0, 1), _pairs(0, 2, 3)] * 3
    result = cluster_bootstrap_diff(clusters, resamples=1000, seed=0, method="percentile")
    assert result.ci_method == "percentile"
    assert result.fallback_reason is None


def test_a_narrower_confidence_level_gives_a_narrower_interval() -> None:
    clusters = [_pairs(3, 1, 1), _pairs(1, 1, 3), _pairs(4, 0, 1), _pairs(0, 2, 3)] * 3
    wide = cluster_bootstrap_diff(clusters, resamples=2000, seed=0, ci=0.99)
    narrow = cluster_bootstrap_diff(clusters, resamples=2000, seed=0, ci=0.80)
    assert wide.ci_hi - wide.ci_lo > narrow.ci_hi - narrow.ci_lo


def test_bootstrap_mean_is_the_shared_engine() -> None:
    point, lo, hi, method, reason = bootstrap_mean(D12, resamples=2000, seed=0)
    assert point == pytest.approx(math.fsum(D12) / 12, abs=1e-15)
    assert lo < point < hi
    assert method in ("bca", "percentile")
    assert reason is None or "bca-undefined" in reason


@pytest.mark.parametrize(
    ("clusters", "kwargs"),
    [
        ([], {}),
        ([[]], {}),
        ([[(True, False)]], {"resamples": 0}),
        ([[(True, False)]], {"ci": 0.0}),
        ([[(True, False)]], {"ci": 1.0}),
        ([[(True, False)]], {"method": "studentized"}),
        ([[(True, False)]], {"seed": 1.5}),
        ([[(1, 0)]], {}),
        ([[(True, False, True)]], {}),
    ],
)
def test_rejects_malformed_inputs(clusters: list, kwargs: dict) -> None:
    with pytest.raises(ValueError):
        cluster_bootstrap_diff(clusters, **kwargs)


def test_single_task_falls_back_with_a_reason() -> None:
    result = cluster_bootstrap_diff([_pairs(2, 1, 1)], resamples=100, seed=0)
    assert result.n_tasks == 1
    assert result.ci_method == "percentile"
    assert result.fallback_reason is not None
