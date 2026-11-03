"""Power, ICC and effective sample size (pre-registration sections 8.1, 8.2).

There is no external authority for a simulated MDE, so the tests here are
of three kinds: closed-form identities checked by hand (ICC on
hand-constructed clusters, the design-effect formula), structural
properties that must hold for any correct implementation (MDE falls as N
rises, falls as K rises, rises as rho falls), and a **calibration** check
on the decision rule itself.

The comparison against section 8.1's published table is deliberately
recorded as a *documented disagreement* rather than asserted as a match
-- see ``test_disagreement_with_the_published_table_is_documented``.

Runtime: everything here uses small ``resamples``/``trials``, which is
what those parameters exist for. The published table is regenerated with
``resamples=2000, trials=2000``.
"""

from __future__ import annotations

import math

import pytest

from v2sbench.stats.power import (
    DEFAULT_ICC,
    DEFAULT_RHO,
    GRID_TASKS,
    GRID_TRIALS,
    MINIMUM_GRID,
    TARGET_GRID,
    _simulate_power,
    effective_samples_per_trial,
    effective_trials,
    icc_estimate,
    mde_for_grid,
    power_table,
    render_markdown,
)

FAST = {"resamples": 60, "trials": 60}


def test_effective_samples_matches_the_worked_example_in_section_8_2() -> None:
    # "K=5 at ICC 0.83 is worth about 1.15 independent trials":
    # 5 / (1 + 4 * 0.83) = 5 / 4.32 = 1.1574...
    assert effective_samples_per_trial(0.83, 5) == pytest.approx(1 / 4.32, rel=1e-12)
    assert effective_trials(0.83, 5) == pytest.approx(1.1574074074074074, rel=1e-12)


def test_effective_samples_bounds() -> None:
    # Independent trials: no discount at all.
    assert effective_trials(0.0, 7) == pytest.approx(7.0)
    # Perfectly correlated: the whole task is one observation, whatever K.
    assert effective_trials(1.0, 7) == pytest.approx(1.0)
    # One trial is one trial regardless of ICC.
    for icc in (0.0, 0.3, 0.83, 1.0):
        assert effective_samples_per_trial(icc, 1) == 1.0
    # Monotone decreasing in both arguments.
    assert effective_trials(0.5, 10) < effective_trials(0.2, 10)
    assert effective_samples_per_trial(0.5, 10) < effective_samples_per_trial(0.5, 3)


def test_effective_samples_rejects_impossible_arguments() -> None:
    for bad_icc in (-0.1, 1.1, math.nan):
        with pytest.raises(ValueError):
            effective_samples_per_trial(bad_icc, 3)
    for bad_k in (0, -1, 2.5):
        with pytest.raises(ValueError):
            effective_samples_per_trial(0.5, bad_k)  # type: ignore[arg-type]


def test_icc_is_one_when_tasks_are_internally_unanimous() -> None:
    # Two tasks, [1,1] and [0,0]. Grand mean 0.5, group means 1 and 0.
    # SSB = 2*0.25 + 2*0.25 = 1.0, MSB = 1.0; SSW = 0, MSW = 0.
    # n0 = (4 - 8/4) / 1 = 2. ICC = (1 - 0) / (1 + 1*0) = 1.
    assert icc_estimate([[1, 1], [0, 0]]) == pytest.approx(1.0, abs=1e-12)
    assert icc_estimate([[1] * 5, [0] * 5, [1] * 5, [0] * 5]) == pytest.approx(1.0, abs=1e-12)


def test_icc_is_zero_when_tasks_are_indistinguishable() -> None:
    # Two tasks, both [1,0]. Group means equal, so SSB = 0, MSB = 0;
    # SSW = 1.0, MSW = 0.5. ICC = (0 - 0.5) / (0 + 0.5) = -1 -> clamped to 0.
    assert icc_estimate([[1, 0], [1, 0]]) == 0.0
    assert icc_estimate([[1, 0], [0, 1], [1, 0], [0, 1]]) == 0.0


def test_icc_for_a_hand_computable_intermediate_case() -> None:
    # Four tasks of 2 trials: [1,1], [1,0], [0,1], [0,0]. N=8, m=4.
    # Grand mean 0.5; group means 1, .5, .5, 0.
    # SSB = 2(.25) + 0 + 0 + 2(.25) = 1.0 -> MSB = 1/3.
    # SSW = 0 + .5 + .5 + 0 = 1.0 -> MSW = 1/4.
    # n0 = (8 - 16/8) / 3 = 2.
    # ICC = (1/3 - 1/4) / (1/3 + 1*(1/4)) = (1/12) / (7/12) = 1/7.
    assert icc_estimate([[1, 1], [1, 0], [0, 1], [0, 0]]) == pytest.approx(1 / 7, abs=1e-12)


def test_icc_is_undefined_when_nothing_varies() -> None:
    assert math.isnan(icc_estimate([[1, 1], [1, 1]]))


def test_icc_validates_its_input() -> None:
    with pytest.raises(ValueError, match="at least 2"):
        icc_estimate([[1, 0]])
    with pytest.raises(ValueError, match="within-task"):
        icc_estimate([[1], [0], [1]])


def test_mde_falls_as_the_number_of_tasks_rises() -> None:
    # The property that makes section 8.1's argument: only N reduces the
    # between-task term. Loose tolerance for simulation noise, fixed seed.
    mdes = [mde_for_grid(n, 3, seed=11, **FAST) for n in (20, 40, 80)]
    assert mdes[0] > mdes[1] > mdes[2]
    assert mdes[0] - mdes[2] > 0.03


def test_section_8_1s_variance_decomposition_in_both_regimes() -> None:
    """``Var = Vb/N + Vw/(N*K)``, checked by simulation at a fixed budget.

    Section 8.1's argument is that at a fixed budget ``B = N*K`` the
    second term is ``Vw/B`` and invariant to the split, so **only N
    reduces the first term**. Both halves of that are visible here:

    * With a constant treatment effect (``effect_sd=0``) there is almost
      no between-task variance in the paired difference, so ``Vb ~ 0``
      and the two splits of a 240-run budget come out equal -- exactly
      what the algebra predicts when the first term vanishes.
    * Once the effect varies across tasks (``effect_sd=0.31``), ``Vb > 0``
      and spending the budget on tasks wins by a wide margin.

    Which is also why section 8.1's own table cannot have been produced
    with ``effect_sd=0``: its K columns separate, so its model carries a
    non-zero ``Vb``.
    """
    flat_tasks = mde_for_grid(80, 3, seed=11, **FAST)
    flat_trials = mde_for_grid(20, 12, seed=11, **FAST)
    assert flat_tasks == pytest.approx(flat_trials, abs=0.02)

    varied_tasks = mde_for_grid(80, 3, effect_sd=0.31, seed=11, resamples=120, trials=120)
    varied_trials = mde_for_grid(20, 12, effect_sd=0.31, seed=11, resamples=120, trials=120)
    assert varied_tasks < varied_trials
    assert varied_trials - varied_tasks > 0.05


def test_more_trials_never_hurts() -> None:
    mdes = [mde_for_grid(40, k, seed=5, **FAST) for k in (1, 3, 10)]
    assert mdes[0] >= mdes[1] >= mdes[2]


def test_a_tighter_pairing_is_worth_more_than_more_trials() -> None:
    # Section 8.1: rho is the higher-leverage knob, which is why section
    # 8.3 spends its effort there.
    loose = mde_for_grid(60, 5, rho=0.0, seed=3, **FAST)
    tight = mde_for_grid(60, 5, rho=0.8, seed=3, **FAST)
    assert tight < loose


def test_icc_moves_the_mde_the_opposite_way_from_section_8_1() -> None:
    """A documented disagreement with section 8.1, not a bug.

    Section 8.1 reports the MDE rising from 9.9 to 11.9pp as ICC goes
    0.3 -> 0.9, i.e. a *design-effect* model in which ICC inflates the
    variance of the task-level mean while the marginal Bernoulli variance
    stays put. This generator is instead generative: ICC *is* the
    beta-binomial ICC, so raising it mechanically pushes the per-task
    rates towards 0 and 1, which shrinks the per-trial variance
    ``p(1-p)`` -- the dominant term here -- and the MDE *falls*.

    Both models are defensible and they are not the same model. This test
    pins the behaviour of the one that ships, with the direction asserted
    explicitly so that it is read as a known divergence rather than
    discovered as a surprise. The pilot (section 8.1) measures the real
    quantity and settles it.
    """
    low = mde_for_grid(60, 5, icc=0.3, seed=3, **FAST)
    high = mde_for_grid(60, 5, icc=0.9, seed=3, **FAST)
    assert high < low


def test_decision_rule_calibration_at_the_null() -> None:
    """The declared under-coverage (section 8.2), measured.

    At delta = 0 the CI rule should fire on 2.5% of experiments (a 95%
    two-sided interval, rejected on one side). It fires slightly more
    often, which is the anti-conservatism section 8.2 declares; this test
    pins the size of it so a regression in the bootstrap shows up as a
    calibration failure rather than as a quietly better-looking result.
    """
    rate = _simulate_power(0.0, 60, 3, 0.35, DEFAULT_ICC, DEFAULT_RHO, 0.0, 400, 400, 0.05, 7,
                           "percentile")
    assert 0.01 < rate < 0.07


def test_an_undetectable_grid_returns_nan_rather_than_a_number() -> None:
    # Two tasks at a base rate of 0.95: even a +1.0 shift cannot reliably
    # clear 0, and a table must say "n/a" rather than print something.
    assert math.isnan(mde_for_grid(2, 1, base_rate=0.95, power=0.95, resamples=40, trials=40))


def test_mde_is_deterministic_for_a_given_seed() -> None:
    first = mde_for_grid(40, 3, seed=19, **FAST)
    again = mde_for_grid(40, 3, seed=19, **FAST)
    assert first == again


def test_disagreement_with_the_published_table_is_documented() -> None:
    """Section 8.1's table and this simulation do not agree, by design.

    Section 8.1 publishes 11.7pp at N=100/K=3. This generator, with a
    *constant* additive treatment effect (``effect_sd=0``), returns a
    materially smaller number, because the published table carries a
    between-task effect-heterogeneity term that a constant effect does
    not: fitting ``Var = Vb + Vw/K`` to section 8.1's K=1 and K=10
    columns implies ``Vb ~ 0.096`` (an effect sd near 0.31) against the
    ~0.0012 this generator produces from the ceiling alone.

    The test asserts the *direction and rough size* of the disagreement
    rather than a match, so that nobody later "fixes" it by tuning the
    generator. ``power.py``'s module docstring carries the full account.
    """
    optimistic = mde_for_grid(100, 3, seed=2, resamples=120, trials=120)
    assert optimistic < 0.117
    heterogeneous = mde_for_grid(100, 3, effect_sd=0.31, seed=2, resamples=120, trials=120)
    assert heterogeneous > optimistic
    # Adding the missing term closes most, but not all, of the gap.
    assert abs(heterogeneous - 0.117) < abs(optimistic - 0.117)


def test_power_table_covers_the_declared_grid() -> None:
    rows = power_table(grids=[(20, 1), (20, 3)], resamples=40, trials=40)
    assert [(r.n_tasks, r.n_seeds) for r in rows] == [(20, 1), (20, 3)]
    assert rows[0].n_pairs == 20
    assert rows[1].n_pairs == 60
    assert rows[0].effective_trials == pytest.approx(1.0)
    assert rows[1].effective_trials == pytest.approx(effective_trials(DEFAULT_ICC, 3))
    # Every row must share one set of assumptions.
    assert len({(r.icc, r.rho, r.base_rate, r.trials, r.resamples, r.seed) for r in rows}) == 1


def test_declared_grid_constants_match_the_preregistration() -> None:
    assert GRID_TASKS == (50, 75, 100, 150)
    assert GRID_TRIALS == (1, 3, 5, 10)
    assert TARGET_GRID == (100, 3)
    assert MINIMUM_GRID == (60, 3)
    assert (DEFAULT_ICC, DEFAULT_RHO) == (0.5, 0.6)


def test_render_markdown_is_a_table_a_reader_can_check() -> None:
    rows = power_table(grids=[(20, 1), (20, 3)], resamples=40, trials=40)
    text = render_markdown(rows, target=0.10)
    assert "| N tasks | K=1 | K=3 | effective trials |" in text
    assert "| 20 |" in text
    assert "ICC=0.5" in text and "rho=0.6" in text
    assert "effect sd 0.0" in text
    assert "80% power" in text
    assert render_markdown([]) == "_No power rows._\n"


@pytest.mark.parametrize(
    "kwargs",
    [
        {"n_tasks": 1},
        {"n_seeds": 0},
        {"base_rate": 1.0},
        {"base_rate": 0.0},
        {"icc": 1.0},
        {"rho": 1.0},
        {"effect_sd": -0.1},
        {"power": 0.0},
        {"alpha": 1.0},
        {"trials": 0},
    ],
)
def test_rejects_impossible_designs(kwargs: dict) -> None:
    base = {"n_tasks": 10, "n_seeds": 3, "resamples": 10, "trials": 2}
    with pytest.raises(ValueError):
        mde_for_grid(**{**base, **kwargs})
