"""From immutable run records to the report's figures. Pre-registration section 8.

Every rule here was fixed before any trial existed. They live in this
module rather than in the report builder so that each one is testable in
isolation:

* **Pairing** is by ``(task, run_id, seed)`` and needs *both* arms to have
  an ``ok`` trial in that exact cell. A half-present cell is dropped,
  never completed with a guess.
* **Errored trials** carry ``success = null`` and are excluded from the
  endpoint with their rate published per arm (section 8.2). They are
  never coerced to failure: a flaky container is not evidence about an
  arm, and counting it as a loss would penalise whichever arm happened to
  run while the host was unhealthy.
* **Exclusions are declared on the probe, never on the outcome**
  (section 5.3). The full suite is the primary figure and is produced by
  default; the probe-filtered subset is the *same* code path with
  ``include_saturated=False``, and both are reported with the full-suite
  number first. No task is ever dropped for failing to separate the arms.
* **Primary test** is the paired sign-flip permutation test on per-task
  mean differences; **exact McNemar is a robustness check** reported
  beside it, never instead (section 8.2).
* **Verdicts are computed** by :func:`verdict` from the CI, never written
  by hand -- and are deliberately *not* emitted for a
  difference-of-differences (section 8.2).
"""

from __future__ import annotations

import math
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass, replace

from ..arms import BY_ID, COMPARISONS, Comparison
from .bootstrap import bootstrap_mean, cluster_bootstrap_diff, task_diffs
from .mcnemar import discordant, mcnemar_exact
from .multiplicity import METHOD as HOLM_METHOD
from .multiplicity import holm_bonferroni
from .permutation import paired_permutation
from .power import effective_trials, icc_estimate
from .wilson import normalized_gain, rate, wilson_interval

Record = Mapping[str, object]
Pairs = list[list[tuple[bool, bool]]]

#: Below this many usable pairs a comparison reports ``insufficient-data``
#: instead of a verdict. Section 8 fixes no such threshold, so this is a
#: *declared* default, not a pre-registered one: the verdict rule has to
#: be total, and a CI from three pairs that happens to exclude 0 is not
#: support. Record the value used beside the figure.
DEFAULT_MIN_PAIRS = 10

#: A cluster bootstrap over one task cannot estimate between-task
#: variance -- the only thing it exists to estimate.
MIN_TASKS = 2

P_METHOD = "paired-sign-flip-permutation"
P_ROBUSTNESS = "mcnemar-exact"
CI_PREFIX = "cluster-bootstrap-tasks"
DIMENSIONS = ("headroom_band", "discoverability", "tier", "video_tier")
_ORDER: dict[str, tuple[str, ...]] = {
    "headroom_band": ("high", "medium", "low", "saturated", "unknown"),
    "discoverability": ("good", "poor", "unknown"),
    "video_tier": ("hermetic", "real", "unknown"),
}


@dataclass(frozen=True)
class ComparisonResult:
    """One row of the report's ``comparisons`` array."""

    hypothesis: str
    treatment: str
    reference: str
    verdict: str
    claim: str = ""
    fails_if: str = ""
    suite: str = "full"
    n_pairs: int = 0
    n_tasks: int = 0
    diff: float | None = None
    ci_lo: float | None = None
    ci_hi: float | None = None
    ci_method: str = "none"
    treatment_rate: float | None = None
    reference_rate: float | None = None
    normalized_gain: float | None = None
    pooled_diff: float | None = None
    p_value: float | None = None
    p_adjusted: float | None = None
    p_method: str = P_METHOD
    permutation_mode: str | None = None
    discordant_b: int | None = None
    discordant_c: int | None = None
    p_mcnemar: float | None = None
    p_robustness_method: str = P_ROBUSTNESS
    corrected: bool = False
    resamples: int = 0
    seed: int = 0
    fallback_reason: str | None = None


@dataclass(frozen=True)
class DiffOfDiffResult:
    """A contrast between two ablation differences. Estimate only.

    Section 8.2: this carries a ``sqrt(2)`` variance penalty relative to
    either difference alone, so it is reported as an estimate with an
    interval and nothing else. There is no ``p_value`` and no ``verdict``
    field here -- absent, not left empty -- because the cheapest way to
    manufacture a finding is to test every pair of ablations and report
    whichever cleared.
    """

    label: str
    left: str
    right: str
    estimate: float | None
    ci_lo: float | None
    ci_hi: float | None
    ci_method: str
    n_tasks: int
    note: str = (
        "Estimate only: a difference-of-differences carries a sqrt(2) penalty, "
        "so no p-value or verdict is emitted (preregistration section 8.2)."
    )


@dataclass(frozen=True)
class ArmSummary:
    """One row of the report's ``arms`` array."""

    id: str
    name: str
    n: int
    successes: int
    rate: float
    wilson_lo: float
    wilson_hi: float
    error_rate: float
    isolates: str = ""
    description: str = ""
    mean_usd: float | None = None
    mean_turns: float | None = None
    mean_wall_secs: float | None = None
    policy_violation_rate: float | None = None
    key_action_hit_rate: float | None = None
    icc: float | None = None
    mean_trials_per_task: float | None = None
    effective_trials: float | None = None
    status_counts: Mapping[str, int] | None = None


@dataclass(frozen=True)
class CellSummary:
    """One cell of the report's task x arm grid."""

    task: str
    arm: str
    n: int
    successes: int
    errors: int
    rate: float
    wilson_lo: float
    wilson_hi: float
    mean_turns: float | None = None
    mean_usd: float | None = None
    key_action_hit_rate: float | None = None
    skill_cited_rate: float | None = None


@dataclass(frozen=True)
class StratumResult:
    """One row of the report's ``strata`` array (hypothesis H2)."""

    dimension: str
    value: str
    treatment: str
    reference: str
    n_pairs: int
    n_tasks: int
    diff: float | None = None
    ci_lo: float | None = None
    ci_hi: float | None = None
    ci_method: str = "none"


def _sub(record: Record, key: str) -> Mapping[str, object]:
    value = record.get(key)
    return value if isinstance(value, Mapping) else {}


def _status(record: Record) -> str:
    status = _sub(record, "outcome").get("status")
    return status if isinstance(status, str) else "missing"


def _success(record: Record) -> bool:
    value = _sub(record, "outcome").get("success")
    if not isinstance(value, bool):
        raise ValueError(
            f"record {record.get('trial_id')!r} has status 'ok' but success={value!r}; "
            "an ok trial must record a boolean outcome"
        )
    return value


def _saturated(record: Record) -> bool:
    """Probe-declared exclusion (section 5.3), as recorded by the harness."""
    task = _sub(record, "task")
    return bool(task.get("saturated")) or task.get("headroom_band") == "saturated"


def _mean(values: Iterable[float]) -> float | None:
    seq = list(values)
    return math.fsum(seq) / len(seq) if seq else None


def _metric_mean(records: Sequence[Record], key: str) -> float | None:
    return _mean(
        float(v)
        for r in records
        if isinstance(v := _sub(r, "metrics").get(key), (int, float)) and not isinstance(v, bool)
    )


def _flag_rate(records: Sequence[Record], key: str) -> float | None:
    flags = [v for r in records if isinstance(v := _sub(r, "metrics").get(key), bool)]
    return sum(flags) / len(flags) if flags else None


def refuse_mixed(records: Sequence[Record]) -> bool:
    """Raise if mock and measured records appear together; return True if all mock.

    Mock records come from the stub agent and hold invented outcomes.
    Aggregating even one into a figure publishes a fabricated number under
    a real run's provenance, which no later labelling undoes. The
    report's ``status.kind`` of ``mock`` covers the all-mock case, so the
    return value tells the builder which banner to render; the mixed case
    has no honest rendering and must stop the build.
    """
    mock = [bool(_sub(r, "provenance").get("is_mock", False)) for r in records]
    if any(mock) and not all(mock):
        examples = [r.get("trial_id") for r, m in zip(records, mock, strict=True) if m][:3]
        raise ValueError(
            f"refusing to aggregate {sum(mock)} mock record(s) with "
            f"{len(mock) - sum(mock)} measured record(s); e.g. {examples}. "
            "Mock and measured data are never combined into one figure."
        )
    return bool(mock) and all(mock)


def pair_trials_by_task(
    records: Sequence[Record],
    treatment: str,
    reference: str,
    include_saturated: bool = True,
) -> dict[str, list[tuple[bool, bool]]]:
    """Paired ``ok`` outcomes keyed by task id. See :func:`pair_trials`."""
    if treatment == reference:
        raise ValueError(f"treatment and reference must differ, both are {treatment!r}")

    cells: dict[tuple[str, str, int], dict[str, bool]] = {}
    for record in records:
        arm = record.get("arm")
        if arm not in (treatment, reference):
            continue
        if not include_saturated and _saturated(record):
            continue
        if _status(record) != "ok":
            continue
        task_id = _sub(record, "task").get("id")
        run_id = record.get("run_id")
        seed = record.get("seed")
        if not isinstance(task_id, str) or not isinstance(run_id, str):
            raise ValueError(f"record {record.get('trial_id')!r} lacks a task id or run_id")
        if not isinstance(seed, int) or isinstance(seed, bool):
            raise ValueError(f"record {record.get('trial_id')!r} has a non-integer seed {seed!r}")
        slot = cells.setdefault((task_id, run_id, seed), {})
        if arm in slot:
            raise ValueError(
                f"duplicate ok trial for task={task_id} run={run_id} seed={seed} arm={arm}; "
                "records are append-only and one cell must appear exactly once"
            )
        slot[str(arm)] = _success(record)

    clusters: dict[str, list[tuple[bool, bool]]] = {}
    for (task_id, _run, _seed), slot in sorted(cells.items()):
        if treatment in slot and reference in slot:
            clusters.setdefault(task_id, []).append((slot[treatment], slot[reference]))
    return clusters


def pair_trials(
    records: Sequence[Record],
    treatment: str,
    reference: str,
    include_saturated: bool = True,
) -> Pairs:
    """Group ``ok`` trials into one list of paired outcomes per task.

    ``run_id`` is in the pairing key because re-running a frozen suite is
    "a new row, never an edit" (section 9): two runs legitimately hold the
    same (task, seed), and pairing across them would invent comparisons
    never run against the same material.

    ``include_saturated`` defaults to **True** -- section 5.3 makes the
    full suite primary and forbids selection on the outcome. Pass
    ``False`` for the probe-filtered subset, reported second.

    Output order is by task id then ``(run_id, seed)``, so the
    bootstrap's draw sequence depends on the data, not on dict order.
    """
    clusters = pair_trials_by_task(records, treatment, reference, include_saturated)
    return [clusters[task_id] for task_id in sorted(clusters)]


def verdict(
    n_pairs: int,
    n_tasks: int,
    ci_lo: float | None,
    ci_hi: float | None,
    arms_present: bool = True,
    min_pairs: int = DEFAULT_MIN_PAIRS,
) -> str:
    """The pre-registered decision rule, in one place, applied to every row.

    ``insufficient-data`` when the evidence is too thin or an arm was
    never run; ``supported`` when the CI excludes 0 on the hypothesised
    side; ``not-supported`` when it excludes 0 on the *opposite* side (an
    outcome section 10 commits to publishing); ``inconclusive`` when the
    CI contains 0.

    Every comparison in ``arms.py`` is directional, treatment above
    reference, so the hypothesised direction is ``ci_lo > 0``. Reversing
    that would have to be declared in ``arms.py`` first; it is not a
    choice the stats layer may make after seeing data.
    """
    if not arms_present or n_pairs < min_pairs or n_tasks < MIN_TASKS:
        return "insufficient-data"
    if ci_lo is None or ci_hi is None:
        return "insufficient-data"
    if ci_lo > 0.0:
        return "supported"
    if ci_hi < 0.0:
        return "not-supported"
    return "inconclusive"


def _interval(
    clusters: Pairs, resamples: int, seed: int, ci: float, method: str
) -> tuple[int, int, float | None, float | None, float | None, float | None, str, str | None]:
    """Shared point-estimate-and-CI path for comparisons and strata."""
    if not clusters:
        return (0, 0, None, None, None, None, "none", None)
    diffs, n_pairs, pooled = task_diffs(clusters)
    point = math.fsum(diffs) / len(diffs)
    if len(diffs) < MIN_TASKS:
        return (n_pairs, len(diffs), point, pooled, None, None,
                f"none ({MIN_TASKS} tasks needed to resample tasks)", None)
    boot = cluster_bootstrap_diff(clusters, resamples=resamples, seed=seed, ci=ci, method=method)
    return (n_pairs, len(diffs), boot.point, boot.pooled_point, boot.ci_lo, boot.ci_hi,
            f"{CI_PREFIX}-{boot.ci_method}", boot.fallback_reason)


def _rates(clusters: Pairs) -> tuple[float | None, float | None, float | None]:
    """Pooled treatment rate, reference rate and normalised gain."""
    pairs = [pair for cluster in clusters for pair in cluster]
    if not pairs:
        return (None, None, None)
    p_t = sum(1 for t, _ in pairs if t) / len(pairs)
    p_r = sum(1 for _, r in pairs if r) / len(pairs)
    gain = None if p_r == 1.0 else normalized_gain(p_t, p_r)
    return (p_t, p_r, gain)


def analyse_comparison(
    records: Sequence[Record],
    comparison: Comparison,
    resamples: int = 10000,
    seed: int = 0,
    ci: float = 0.95,
    method: str = "bca",
    min_pairs: int = DEFAULT_MIN_PAIRS,
    include_saturated: bool = True,
    suite: str = "full",
) -> ComparisonResult:
    """Analyse one pre-registered comparison: permutation p primary,
    exact McNemar beside it as the robustness check (section 8.2).
    ``p_adjusted`` is filled in by :func:`analyse_comparisons`, the only
    function that knows the whole Holm family."""
    arms_seen = {r.get("arm") for r in records}
    present = comparison.treatment in arms_seen and comparison.reference in arms_seen
    clusters = pair_trials(records, comparison.treatment, comparison.reference, include_saturated)
    n_pairs, n_tasks, diff, pooled, lo, hi, ci_method, fallback = _interval(
        clusters, resamples, seed, ci, method
    )
    flat = [pair for cluster in clusters for pair in cluster]
    b, c = discordant(flat)
    perm = (
        paired_permutation(task_diffs(clusters)[0], resamples=resamples, seed=seed)
        if clusters
        else None
    )
    p_t, p_r, gain = _rates(clusters)
    return ComparisonResult(
        hypothesis=comparison.hypothesis,
        treatment=comparison.treatment,
        reference=comparison.reference,
        verdict=verdict(n_pairs, n_tasks, lo, hi, arms_present=present, min_pairs=min_pairs),
        claim=comparison.claim,
        fails_if=comparison.fails_if,
        suite=suite,
        n_pairs=n_pairs,
        n_tasks=n_tasks,
        diff=diff,
        ci_lo=lo,
        ci_hi=hi,
        ci_method=ci_method,
        treatment_rate=p_t,
        reference_rate=p_r,
        normalized_gain=gain,
        pooled_diff=pooled,
        p_value=perm.p_value if perm else None,
        permutation_mode=perm.mode if perm else None,
        discordant_b=b if flat else None,
        discordant_c=c if flat else None,
        p_mcnemar=mcnemar_exact(b, c) if flat else None,
        corrected=comparison.corrected,
        resamples=resamples,
        seed=seed,
        fallback_reason=fallback,
    )


def analyse_comparisons(
    records: Sequence[Record],
    comparisons: Sequence[Comparison] = COMPARISONS,
    resamples: int = 10000,
    seed: int = 0,
    ci: float = 0.95,
    method: str = "bca",
    min_pairs: int = DEFAULT_MIN_PAIRS,
    include_saturated: bool = True,
    suite: str = "full",
) -> list[ComparisonResult]:
    """Analyse every declared comparison, then Holm-correct the flagged family.

    Membership is ``Comparison.corrected`` and nothing else, so H1 -- the
    single declared primary endpoint -- keeps its uncorrected p-value
    while the ablation family pays for being a family. The family is
    iterated from ``COMPARISONS``, never hard-coded, so adding a
    hypothesis is a one-line change in the pre-registered file. The family
    size actually used goes into ``p_method``.
    """
    results = [
        analyse_comparison(
            records, c, resamples, seed, ci, method, min_pairs, include_saturated, suite
        )
        for c in comparisons
    ]
    family = {r.hypothesis: r.p_value for r in results if r.corrected and r.p_value is not None}
    adjusted = holm_bonferroni(family)
    labelled = f"{P_METHOD}; {HOLM_METHOD}(m={len(family)})"
    return [
        replace(r, p_adjusted=adjusted[r.hypothesis], p_method=labelled)
        if r.hypothesis in adjusted
        else r
        for r in results
    ]


def analyse_difference_of_differences(
    records: Sequence[Record],
    left: Comparison,
    right: Comparison,
    resamples: int = 10000,
    seed: int = 0,
    ci: float = 0.95,
    method: str = "bca",
    include_saturated: bool = True,
) -> DiffOfDiffResult:
    """Contrast two ablation differences. **Estimate only, by design.**

    Returns ``(left.diff - right.diff)`` bootstrapped over the tasks the
    two contrasts share, resampling each task *once* and recomputing both
    differences on the same draw, so the correlation between the contrasts
    (they usually share an arm) is carried rather than assumed away.

    No p-value and no verdict, per section 8.2: the contrast carries a
    ``sqrt(2)`` penalty, and the set of ablation pairs is large enough
    that testing them all would find something. The intended use is to
    say "we cannot separate these two components at this N", with an
    interval showing how far from separable they are.
    """
    left_by_task = pair_trials_by_task(records, left.treatment, left.reference, include_saturated)
    right_by_task = pair_trials_by_task(
        records, right.treatment, right.reference, include_saturated
    )
    shared = sorted(set(left_by_task) & set(right_by_task))
    label = f"({left.hypothesis}) {left.treatment}-{left.reference} minus " \
            f"({right.hypothesis}) {right.treatment}-{right.reference}"
    if not shared:
        return DiffOfDiffResult(label, left.hypothesis, right.hypothesis,
                                None, None, None, "none", 0)
    values = [
        task_diffs([left_by_task[t]])[0][0] - task_diffs([right_by_task[t]])[0][0] for t in shared
    ]
    point = math.fsum(values) / len(values)
    if len(values) < MIN_TASKS:
        return DiffOfDiffResult(label, left.hypothesis, right.hypothesis, point, None, None,
                                f"none ({MIN_TASKS} tasks needed)", len(values))
    point, lo, hi, used, _reason = bootstrap_mean(values, resamples, seed, ci, method)
    return DiffOfDiffResult(label, left.hypothesis, right.hypothesis, point, lo, hi,
                            f"{CI_PREFIX}-{used}", len(values))


def analyse_arms(records: Sequence[Record]) -> list[ArmSummary]:
    """Per-arm rates, error rates, ICC and secondary means for ``arms``.

    ``error_rate`` counts *any* non-``ok`` status, not only ``error``:
    the schema leaves ``success`` null for ``timeout``,
    ``budget_exceeded`` and ``refused`` too, so none of them can enter
    the endpoint and a reader needs one number for "trials that produced
    no measurement". ``status_counts`` breaks it down so a timeout-heavy
    arm is not mistaken for a flaky one.

    ``icc`` and ``effective_trials`` are section 8.2's clustering
    disclosures. ``wilson_lo``/``wilson_hi`` ignore that clustering and
    are therefore too narrow for an arm-level rate; they are kept for
    parity with ``vts-extract eval``, and ``wilson.beta_binomial_hdi`` is
    the clustered alternative.

    Probe-excluded tasks are *included* here: section 5.3 excludes them
    from a subset of the endpoint, not from the description of the run.
    """
    by_arm: dict[str, list[Record]] = {}
    for record in records:
        arm = record.get("arm")
        if isinstance(arm, str):
            by_arm.setdefault(arm, []).append(record)

    summaries: list[ArmSummary] = []
    for arm_id in sorted(by_arm):
        trials = by_arm[arm_id]
        counts: dict[str, int] = {}
        for record in trials:
            counts[_status(record)] = counts.get(_status(record), 0) + 1
        ok = [r for r in trials if _status(r) == "ok"]
        successes = sum(1 for r in ok if _success(r))
        lo, hi = wilson_interval(successes, len(ok))
        arm = BY_ID.get(arm_id)
        violations = [bool(_sub(r, "metrics").get("policy_violations")) for r in ok]
        groups: dict[str, list[float]] = {}
        for record in ok:
            task_id = _sub(record, "task").get("id")
            if isinstance(task_id, str):
                groups.setdefault(task_id, []).append(1.0 if _success(record) else 0.0)
        icc = None
        mean_k = None
        eff = None
        if len(groups) >= 2 and sum(len(g) for g in groups.values()) > len(groups):
            icc = icc_estimate(list(groups.values()))
            mean_k = sum(len(g) for g in groups.values()) / len(groups)
            if not math.isnan(icc):
                eff = effective_trials(icc, max(round(mean_k), 1))
        summaries.append(
            ArmSummary(
                id=arm_id,
                name=arm.name if arm else arm_id,
                n=len(ok),
                successes=successes,
                rate=rate(successes, len(ok)),
                wilson_lo=lo,
                wilson_hi=hi,
                error_rate=(len(trials) - len(ok)) / len(trials) if trials else 0.0,
                isolates=arm.isolates if arm else "",
                description=arm.description if arm else "",
                mean_usd=_metric_mean(ok, "usd_cost"),
                mean_turns=_metric_mean(ok, "turns"),
                mean_wall_secs=_metric_mean(ok, "wall_secs"),
                policy_violation_rate=(sum(violations) / len(violations)) if violations else None,
                key_action_hit_rate=_flag_rate(ok, "key_action_hit"),
                icc=icc,
                mean_trials_per_task=mean_k,
                effective_trials=eff,
                status_counts=dict(sorted(counts.items())),
            )
        )
    return summaries


def analyse_cells(records: Sequence[Record]) -> list[CellSummary]:
    """The task x arm grid: one cell per combination actually run."""
    by_cell: dict[tuple[str, str], list[Record]] = {}
    for record in records:
        task_id = _sub(record, "task").get("id")
        arm = record.get("arm")
        if isinstance(task_id, str) and isinstance(arm, str):
            by_cell.setdefault((task_id, arm), []).append(record)

    cells: list[CellSummary] = []
    for task_id, arm_id in sorted(by_cell):
        trials = by_cell[(task_id, arm_id)]
        ok = [r for r in trials if _status(r) == "ok"]
        successes = sum(1 for r in ok if _success(r))
        lo, hi = wilson_interval(successes, len(ok))
        cells.append(
            CellSummary(
                task=task_id,
                arm=arm_id,
                n=len(ok),
                successes=successes,
                errors=len(trials) - len(ok),
                rate=rate(successes, len(ok)),
                wilson_lo=lo,
                wilson_hi=hi,
                mean_turns=_metric_mean(ok, "turns"),
                mean_usd=_metric_mean(ok, "usd_cost"),
                key_action_hit_rate=_flag_rate(ok, "key_action_hit"),
                skill_cited_rate=_flag_rate(ok, "skill_cited"),
            )
        )
    return cells


def _stratum_value(record: Record, dimension: str) -> str:
    value = (
        _sub(record, "video").get("tier")
        if dimension == "video_tier"
        else _sub(record, "task").get(dimension)
    )
    return value if isinstance(value, str) and value else "unknown"


def analyse_strata(
    records: Sequence[Record],
    dimension: str,
    treatment: str,
    reference: str,
    resamples: int = 10000,
    seed: int = 0,
    ci: float = 0.95,
    method: str = "bca",
    include_saturated: bool = True,
) -> list[StratumResult]:
    """Effect size within each stratum of one dimension -- hypothesis H2.

    We report the effect *within* each band rather than a band x arm
    interaction statistic: with three or four bands of a handful of tasks
    each, an interaction test has no power worth quoting, while the
    per-band intervals show the pattern (or its absence) directly and let
    a reader judge the overlap. A deliberate departure from a formal
    interaction test, labelled as such in the report.

    Each band is bootstrapped over its *own* tasks, so a band holding one
    task reports a point estimate and no interval, not a zero-width one.
    """
    if dimension not in DIMENSIONS:
        raise ValueError(f"dimension must be one of {DIMENSIONS}, got {dimension!r}")

    groups: dict[str, list[Record]] = {}
    for record in records:
        groups.setdefault(_stratum_value(record, dimension), []).append(record)

    order = _ORDER.get(dimension)
    keys = (
        [k for k in order if k in groups] + sorted(set(groups) - set(order))
        if order
        else sorted(groups)
    )
    out: list[StratumResult] = []
    for key in keys:
        clusters = pair_trials(groups[key], treatment, reference, include_saturated)
        n_pairs, n_tasks, diff, _pooled, lo, hi, ci_method, _fb = _interval(
            clusters, resamples, seed, ci, method
        )
        out.append(
            StratumResult(
                dimension=dimension,
                value=key,
                treatment=treatment,
                reference=reference,
                n_pairs=n_pairs,
                n_tasks=n_tasks,
                diff=diff,
                ci_lo=lo,
                ci_hi=hi,
                ci_method=ci_method,
            )
        )
    return out
