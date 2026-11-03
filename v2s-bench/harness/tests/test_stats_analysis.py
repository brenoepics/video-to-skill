"""The glue: pairing, exclusions, verdicts, Holm, and the report rows.

These tests are about *rules*, not arithmetic -- the arithmetic is pinned
against external authorities in the other four files. What is checked
here is that each rule the pre-registration fixed is the rule the code
applies: who gets paired with whom, what an errored trial does, which
comparisons pay for multiplicity, and that no verdict is ever written by
hand.
"""

from __future__ import annotations

from dataclasses import FrozenInstanceError

import pytest

from v2sbench.arms import COMPARISONS, Comparison
from v2sbench.stats.analysis import (
    DEFAULT_MIN_PAIRS,
    ComparisonResult,
    analyse_arms,
    analyse_cells,
    analyse_comparison,
    analyse_comparisons,
    analyse_difference_of_differences,
    analyse_strata,
    pair_trials,
    refuse_mixed,
    verdict,
)
from v2sbench.stats.multiplicity import holm_bonferroni, holm_rejected

H1 = next(c for c in COMPARISONS if c.hypothesis == "H1")
FAST = {"resamples": 400, "seed": 0}


def record(
    task: str,
    arm: str,
    seed: int,
    success: bool | None = True,
    status: str = "ok",
    run_id: str = "run-1",
    saturated: bool = False,
    tier: str = "T1",
    headroom: str = "high",
    discoverability: str = "poor",
    video_tier: str = "hermetic",
    is_mock: bool = False,
    **metrics: object,
) -> dict:
    return {
        "schema_version": 1,
        "trial_id": f"{run_id}:{task}:{arm}:{seed}",
        "run_id": run_id,
        "task": {
            "id": task,
            "tier": tier,
            "suite_version": "0.1.0",
            "discoverability": discoverability,
            "headroom_band": headroom,
            "saturated": saturated,
        },
        "arm": arm,
        "seed": seed,
        "outcome": {"status": status, "success": success if status == "ok" else None},
        "metrics": metrics,
        "video": {"tier": video_tier},
        "provenance": {
            "harness_version": "0.1.0",
            "model": "test-model-1",
            "started_at": "2026-01-01T00:00:00Z",
            "is_mock": is_mock,
        },
    }


def grid(pattern: dict[str, tuple[int, int]], arm_a: str = "A1", arm_b: str = "A0") -> list[dict]:
    """``{task: (treatment_wins, trials)}`` -> records for both arms."""
    out: list[dict] = []
    for task, (wins, trials) in pattern.items():
        for seed in range(trials):
            out.append(record(task, arm_a, seed, success=seed < wins))
            out.append(record(task, arm_b, seed, success=False))
    return out


# --------------------------------------------------------------- pairing


def test_pairing_requires_both_arms_in_the_same_cell() -> None:
    records = [
        record("t1", "A1", 0),
        record("t1", "A0", 0),
        record("t1", "A1", 1),  # no A0 at seed 1: dropped
        record("t1", "A0", 2),  # no A1 at seed 2: dropped
    ]
    assert pair_trials(records, "A1", "A0") == [[(True, True)]]


def test_pairing_does_not_cross_run_ids() -> None:
    # Section 9: re-running a frozen suite is a new row, never an edit.
    records = [
        record("t1", "A1", 0, run_id="run-1"),
        record("t1", "A0", 0, run_id="run-2"),
    ]
    assert pair_trials(records, "A1", "A0") == []


def test_errored_trials_are_excluded_and_never_counted_as_failures() -> None:
    records = [
        record("t1", "A1", 0),
        record("t1", "A0", 0),
        record("t1", "A1", 1, status="error", success=None),
        record("t1", "A0", 1),
    ]
    clusters = pair_trials(records, "A1", "A0")
    # One pair, not two: the errored cell is gone, not turned into a loss.
    assert clusters == [[(True, True)]]


@pytest.mark.parametrize("status", ["error", "timeout", "budget_exceeded", "refused"])
def test_every_non_ok_status_is_excluded_from_the_endpoint(status: str) -> None:
    records = [record("t1", "A1", 0, status=status, success=None), record("t1", "A0", 0)]
    assert pair_trials(records, "A1", "A0") == []
    arms = {a.id: a for a in analyse_arms(records)}
    assert arms["A1"].n == 0
    assert arms["A1"].error_rate == 1.0
    assert arms["A1"].status_counts == {status: 1}


def test_probe_excluded_tasks_are_in_the_full_suite_by_default() -> None:
    # Section 5.3: the full suite is primary and no task is dropped for
    # failing to separate the arms.
    records = [
        *grid({"t1": (2, 2)}),
        record("sat", "A1", 0, saturated=True, headroom="saturated"),
        record("sat", "A0", 0, saturated=True, headroom="saturated"),
    ]
    assert len(pair_trials(records, "A1", "A0")) == 2
    assert len(pair_trials(records, "A1", "A0", include_saturated=False)) == 1


def test_duplicate_cells_are_refused_rather_than_double_counted() -> None:
    records = [record("t1", "A1", 0), record("t1", "A1", 0), record("t1", "A0", 0)]
    with pytest.raises(ValueError, match="duplicate ok trial"):
        pair_trials(records, "A1", "A0")


def test_pairing_rejects_a_comparison_of_an_arm_with_itself() -> None:
    with pytest.raises(ValueError, match="must differ"):
        pair_trials([], "A1", "A1")


def test_pairing_is_ordered_by_task_then_run_then_seed() -> None:
    records = [
        *grid({"zz": (1, 2), "aa": (0, 2)}),
        record("aa", "A1", 9, run_id="run-0"),
        record("aa", "A0", 9, run_id="run-0"),
    ]
    clusters = pair_trials(records, "A1", "A0")
    assert len(clusters) == 2
    assert len(clusters[0]) == 3  # "aa" first, with its run-0 pair included


def test_pairing_rejects_records_missing_identifiers() -> None:
    bad = record("t1", "A1", 0)
    bad["seed"] = "zero"
    with pytest.raises(ValueError, match="non-integer seed"):
        pair_trials([bad, record("t1", "A0", 0)], "A1", "A0")


def test_an_ok_trial_with_no_boolean_outcome_is_an_error() -> None:
    bad = record("t1", "A1", 0)
    bad["outcome"] = {"status": "ok", "success": None}
    with pytest.raises(ValueError, match="must record a boolean"):
        pair_trials([bad, record("t1", "A0", 0)], "A1", "A0")


# --------------------------------------------------------------- refuse_mixed


def test_refuse_mixed_blocks_mock_plus_measured() -> None:
    records = [record("t1", "A1", 0, is_mock=True), record("t1", "A0", 0, is_mock=False)]
    with pytest.raises(ValueError, match="refusing to aggregate"):
        refuse_mixed(records)


def test_refuse_mixed_allows_a_uniform_set_and_reports_which() -> None:
    assert refuse_mixed([record("t1", "A1", 0, is_mock=True)]) is True
    assert refuse_mixed([record("t1", "A1", 0)]) is False
    assert refuse_mixed([]) is False


def test_refuse_mixed_treats_a_missing_flag_as_measured() -> None:
    plain = record("t1", "A0", 0)
    del plain["provenance"]["is_mock"]
    with pytest.raises(ValueError):
        refuse_mixed([plain, record("t1", "A1", 0, is_mock=True)])


# --------------------------------------------------------------- verdicts


def test_verdict_covers_all_four_branches() -> None:
    n = DEFAULT_MIN_PAIRS
    assert verdict(n, 5, 0.05, 0.30) == "supported"
    assert verdict(n, 5, -0.30, -0.05) == "not-supported"
    assert verdict(n, 5, -0.10, 0.20) == "inconclusive"
    assert verdict(n - 1, 5, 0.05, 0.30) == "insufficient-data"
    assert verdict(n, 1, 0.05, 0.30) == "insufficient-data"
    assert verdict(n, 5, 0.05, 0.30, arms_present=False) == "insufficient-data"
    assert verdict(n, 5, None, None) == "insufficient-data"


def test_a_bound_exactly_on_zero_is_inconclusive_not_supported() -> None:
    assert verdict(50, 5, 0.0, 0.3) == "inconclusive"
    assert verdict(50, 5, -0.3, 0.0) == "inconclusive"


def test_verdict_is_computed_from_the_data_not_stored() -> None:
    wins = analyse_comparison(grid({f"t{i}": (5, 5) for i in range(8)}), H1, **FAST)
    assert wins.verdict == "supported"
    assert wins.ci_lo > 0

    # The baseline wins on every task (with some spread), so H1 fails in
    # the direction section 10 commits to publishing.
    baseline_better = grid({"t0": (5, 5), "t1": (4, 5), "t2": (5, 5), "t3": (3, 5),
                            "t4": (4, 5), "t5": (5, 5), "t6": (4, 5), "t7": (3, 5)},
                           arm_a="A0", arm_b="A1")
    losses = analyse_comparison(baseline_better, H1, **FAST)
    assert losses.verdict == "not-supported"
    assert losses.ci_hi < 0

    flat = analyse_comparison(grid({f"t{i}": (2, 4) for i in range(8)}), H1, **FAST)
    assert flat.verdict in ("supported", "inconclusive")

    thin = analyse_comparison(grid({"t1": (1, 1)}), H1, **FAST)
    assert thin.verdict == "insufficient-data"


def test_a_missing_arm_is_insufficient_data_not_a_null_result() -> None:
    only_baseline = [record("t1", "A0", s) for s in range(20)]
    result = analyse_comparison(only_baseline, H1, **FAST)
    assert result.verdict == "insufficient-data"
    assert result.n_pairs == 0
    assert result.diff is None
    assert result.p_value is None


# --------------------------------------------------------------- comparisons


def test_comparison_result_carries_every_schema_field() -> None:
    result = analyse_comparison(grid({f"t{i}": (3, 5) for i in range(6)}), H1, **FAST)
    for field in (
        "hypothesis", "claim", "fails_if", "treatment", "reference", "n_pairs", "n_tasks",
        "diff", "ci_lo", "ci_hi", "ci_method", "discordant_b", "discordant_c", "p_value",
        "p_adjusted", "p_method", "verdict",
    ):
        assert hasattr(result, field), field
    assert result.n_pairs == 30
    assert result.n_tasks == 6
    assert result.ci_method.startswith("cluster-bootstrap-tasks")


def test_primary_p_is_the_permutation_test_and_mcnemar_is_reported_beside_it() -> None:
    result = analyse_comparison(grid({f"t{i}": (4, 5) for i in range(7)}), H1, **FAST)
    assert result.p_method == "paired-sign-flip-permutation"
    assert result.permutation_mode == "exact-enumeration"
    assert result.p_robustness_method == "mcnemar-exact"
    assert result.p_mcnemar is not None
    assert result.p_value is not None
    assert result.discordant_b == 28 and result.discordant_c == 0


def test_normalized_gain_is_reported_with_the_raw_difference() -> None:
    # A1 wins 2 of 4 trials per task, A0 never wins: diff 0.5, ref rate 0.
    result = analyse_comparison(grid({f"t{i}": (2, 4) for i in range(5)}), H1, **FAST)
    assert result.diff == pytest.approx(0.5)
    assert result.reference_rate == pytest.approx(0.0)
    assert result.normalized_gain == pytest.approx(0.5)


def test_holm_is_applied_to_exactly_the_flagged_comparisons() -> None:
    records: list[dict] = []
    for i in range(6):
        for arm in ("A0", "A1", "A2", "A3", "A5", "A6", "A7"):
            for seed in range(4):
                win = arm in ("A1", "A7") and seed < 3
                records.append(record(f"t{i}", arm, seed, success=win))
    results = analyse_comparisons(records, **FAST)
    by_hypothesis = {r.hypothesis: r for r in results}

    flagged = {c.hypothesis for c in COMPARISONS if c.corrected}
    assert "H1" not in flagged and len(flagged) >= 4
    for hypothesis, result in by_hypothesis.items():
        if hypothesis in flagged:
            assert result.p_adjusted is not None, hypothesis
            assert result.p_adjusted >= result.p_value
            assert "holm-bonferroni" in result.p_method
        else:
            assert result.p_adjusted is None, hypothesis
            assert result.p_method == "paired-sign-flip-permutation"


def test_the_holm_family_size_is_read_from_the_declarations() -> None:
    records: list[dict] = []
    for i in range(6):
        for arm in ("A0", "A1", "A2", "A3", "A5", "A6", "A7"):
            for seed in range(4):
                records.append(record(f"t{i}", arm, seed, success=(arm == "A1" and seed < 3)))
    results = analyse_comparisons(records, **FAST)
    expected = sum(1 for r in results if r.corrected and r.p_value is not None)
    for result in results:
        if result.corrected:
            assert f"m={expected}" in result.p_method


def test_comparisons_are_returned_in_declared_order() -> None:
    results = analyse_comparisons(grid({f"t{i}": (3, 4) for i in range(6)}), **FAST)
    assert [r.hypothesis for r in results] == [c.hypothesis for c in COMPARISONS]


def test_the_probe_filtered_subset_is_the_same_code_path() -> None:
    records = grid({f"t{i}": (3, 4) for i in range(6)})
    for rec in records:
        if rec["task"]["id"] == "t0":
            rec["task"]["saturated"] = True
    full = analyse_comparison(records, H1, suite="full", **FAST)
    filtered = analyse_comparison(records, H1, include_saturated=False,
                                  suite="probe-filtered", **FAST)
    assert full.suite == "full" and filtered.suite == "probe-filtered"
    assert full.n_tasks == 6 and filtered.n_tasks == 5


# --------------------------------------------------- difference of differences


def test_difference_of_differences_is_an_estimate_with_no_verdict() -> None:
    records: list[dict] = []
    for i in range(8):
        for arm, wins in (("A1", 4), ("A2", 2), ("A3", 1)):
            for seed in range(4):
                records.append(record(f"t{i}", arm, seed, success=seed < wins))
    h3 = Comparison("H3", "A1", "A2", "pixels beat transcript", "CI includes 0", True)
    h4 = Comparison("H4", "A1", "A3", "verification helps", "CI includes 0", True)
    result = analyse_difference_of_differences(records, h3, h4, **FAST)

    assert not hasattr(result, "p_value")
    assert not hasattr(result, "verdict")
    assert "no p-value or verdict" in result.note
    # (A1-A2) = 0.5, (A1-A3) = 0.75, so the contrast is -0.25.
    assert result.estimate == pytest.approx(-0.25)
    assert result.ci_lo is not None and result.ci_hi is not None
    assert result.n_tasks == 8


def test_difference_of_differences_without_shared_tasks_is_empty() -> None:
    h3 = Comparison("H3", "A1", "A2", "c", "f", True)
    h4 = Comparison("H4", "A1", "A3", "c", "f", True)
    result = analyse_difference_of_differences(grid({"t1": (1, 2)}), h3, h4, **FAST)
    assert result.estimate is None
    assert result.ci_method == "none"


# --------------------------------------------------------------- report rows


def test_arm_summary_reports_rates_errors_and_clustering() -> None:
    records = [record(f"t{t}", "A1", s, success=(s == 0)) for t in range(4) for s in range(3)]
    records += [record("t0", "A1", 9, status="error", success=None)]
    records += [
        record(f"t{t}", "A0", s, success=False, turns=4, usd_cost=0.5, wall_secs=12.0,
               key_action_hit=(s == 0), policy_violations=["rm -rf"] if s == 1 else [])
        for t in range(4)
        for s in range(3)
    ]
    arms = {a.id: a for a in analyse_arms(records)}

    assert arms["A1"].n == 12 and arms["A1"].successes == 4
    assert arms["A1"].rate == pytest.approx(1 / 3)
    assert arms["A1"].wilson_lo < arms["A1"].rate < arms["A1"].wilson_hi
    assert arms["A1"].error_rate == pytest.approx(1 / 13)
    assert arms["A1"].status_counts == {"error": 1, "ok": 12}
    assert arms["A1"].icc is not None and arms["A1"].icc >= 0.0
    assert arms["A1"].mean_trials_per_task == pytest.approx(3.0)
    assert arms["A1"].effective_trials is not None

    assert arms["A0"].rate == 0.0
    assert arms["A0"].mean_turns == pytest.approx(4.0)
    assert arms["A0"].mean_usd == pytest.approx(0.5)
    assert arms["A0"].mean_wall_secs == pytest.approx(12.0)
    assert arms["A0"].policy_violation_rate == pytest.approx(1 / 3)
    assert arms["A0"].key_action_hit_rate == pytest.approx(1 / 3)
    assert arms["A0"].name == "baseline"


def test_arm_summary_carries_no_metric_means_when_nothing_was_recorded() -> None:
    arms = analyse_arms([record("t1", "A1", 0)])
    assert arms[0].mean_turns is None
    assert arms[0].key_action_hit_rate is None
    assert arms[0].icc is None


def test_cell_summary_is_the_task_by_arm_grid() -> None:
    records = grid({"t1": (2, 3), "t2": (0, 3)})
    records.append(record("t1", "A1", 7, status="timeout", success=None))
    cells = {(c.task, c.arm): c for c in analyse_cells(records)}
    assert set(cells) == {("t1", "A1"), ("t1", "A0"), ("t2", "A1"), ("t2", "A0")}
    assert cells[("t1", "A1")].successes == 2
    assert cells[("t1", "A1")].n == 3
    assert cells[("t1", "A1")].errors == 1
    assert cells[("t2", "A1")].rate == 0.0
    assert cells[("t2", "A0")].wilson_hi > 0.0


def test_cell_summary_reports_skill_citation() -> None:
    records = [record("t1", "A1", s, skill_cited=(s < 2)) for s in range(4)]
    cell = analyse_cells(records)[0]
    assert cell.skill_cited_rate == pytest.approx(0.5)


@pytest.mark.parametrize(
    ("dimension", "expected"),
    [
        ("headroom_band", ["high", "low"]),
        ("discoverability", ["good", "poor"]),
        ("tier", ["T1", "T2"]),
        ("video_tier", ["hermetic", "real"]),
    ],
)
def test_strata_are_grouped_and_ordered_canonically(
    dimension: str, expected: list[str]
) -> None:
    records: list[dict] = []
    variants = {
        "headroom_band": [{"headroom": "low"}, {"headroom": "high"}],
        "discoverability": [{"discoverability": "good"}, {"discoverability": "poor"}],
        "tier": [{"tier": "T2"}, {"tier": "T1"}],
        "video_tier": [{"video_tier": "real"}, {"video_tier": "hermetic"}],
    }[dimension]
    for index, kwargs in enumerate(variants):
        for task in range(3):
            for seed in range(3):
                records.append(record(f"s{index}t{task}", "A1", seed, success=True, **kwargs))
                records.append(record(f"s{index}t{task}", "A0", seed, success=False, **kwargs))
    strata = analyse_strata(records, dimension, "A1", "A0", **FAST)
    assert [s.value for s in strata] == expected
    for stratum in strata:
        assert stratum.n_tasks == 3
        assert stratum.n_pairs == 9
        assert stratum.diff == pytest.approx(1.0)


def test_a_stratum_with_one_task_gets_no_interval() -> None:
    records = grid({"t1": (2, 3)})
    strata = analyse_strata(records, "headroom_band", "A1", "A0", **FAST)
    assert strata[0].n_tasks == 1
    assert strata[0].diff is not None
    assert strata[0].ci_lo is None
    assert "tasks needed" in strata[0].ci_method


def test_strata_rejects_an_undeclared_dimension() -> None:
    with pytest.raises(ValueError, match="dimension must be one of"):
        analyse_strata([], "model_id", "A1", "A0")


def test_missing_stratum_values_become_unknown_rather_than_vanishing() -> None:
    records = grid({"t1": (1, 2), "t2": (1, 2)})
    for rec in records:
        del rec["task"]["headroom_band"]
    strata = analyse_strata(records, "headroom_band", "A1", "A0", **FAST)
    assert [s.value for s in strata] == ["unknown"]
    assert strata[0].n_tasks == 2


# --------------------------------------------------------------- Holm itself


def test_holm_worked_example() -> None:
    # m = 4. Sorted: 0.01, 0.02, 0.03, 0.04.
    #   rank 1: 4 * 0.01 = 0.04
    #   rank 2: 3 * 0.02 = 0.06
    #   rank 3: 2 * 0.03 = 0.06  -> running max keeps 0.06
    #   rank 4: 1 * 0.04 = 0.04  -> running max lifts it to 0.06
    adjusted = holm_bonferroni({"a": 0.01, "b": 0.02, "c": 0.03, "d": 0.04})
    assert adjusted == {
        "a": pytest.approx(0.04),
        "b": pytest.approx(0.06),
        "c": pytest.approx(0.06),
        "d": pytest.approx(0.06),
    }


def test_holm_is_monotone_and_order_preserving() -> None:
    raw = {"z": 0.5, "a": 0.001, "m": 0.02}
    adjusted = holm_bonferroni(raw)
    assert list(adjusted) == list(raw)  # input order, not significance order
    ordered = sorted(raw, key=lambda k: raw[k])
    values = [adjusted[k] for k in ordered]
    assert values == sorted(values)
    assert all(adjusted[k] >= raw[k] for k in raw)
    assert all(adjusted[k] <= 1.0 for k in raw)


def test_holm_single_hypothesis_is_unchanged_and_empty_family_is_free() -> None:
    assert holm_bonferroni({"only": 0.03}) == {"only": pytest.approx(0.03)}
    assert holm_bonferroni({}) == {}


def test_holm_rejection_uses_a_non_strict_boundary() -> None:
    assert holm_rejected({"a": 0.0125, "b": 0.5}, alpha=0.05) == {"a": True, "b": False}
    assert holm_rejected({"a": 0.025, "b": 0.5}, alpha=0.05)["a"] is True


def test_holm_validates_its_input() -> None:
    for bad in ({"a": 1.5}, {"a": -0.1}, {"a": float("nan")}, {"a": "0.05"}):
        with pytest.raises(ValueError):
            holm_bonferroni(bad)  # type: ignore[arg-type]


def test_comparison_result_is_frozen() -> None:
    # A published row must not be mutable after it is computed.
    result = ComparisonResult("H1", "A1", "A0", "inconclusive")
    with pytest.raises(FrozenInstanceError):
        result.verdict = "supported"  # type: ignore[misc]
