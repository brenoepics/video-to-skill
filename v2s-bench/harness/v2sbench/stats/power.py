"""Sample size, ICC and effective sample size. Pre-registration section 8.1.

Section 8.1 requires N to be fixed *before* running and section 8.2's
stopping rule forbids extending a run after peeking, so the grid this
module recommends is chosen once, written into the pre-registration, and
lived with. That makes it part of the pre-registration rather than a
convenience.

Why simulate instead of using a closed-form power formula
---------------------------------------------------------
The declared decision rule is "the 95% cluster-bootstrap CI excludes 0 in
the hypothesised direction" (``analysis.verdict``), applied to a
two-level design with a ceiling at rate 1.0. Three features of that have
no clean analytic form: the between-task variance component (the dominant
term, and the whole reason for clustering); the bootstrap's own behaviour
at small N; and the ceiling, which shrinks the realised effect exactly
where section 5 says saturation would. Simulating the real rule on a
hierarchical generator captures all three, where an analytic
approximation would need a correction for each -- and every correction is
a place to flatter the design.

The two knobs, and which one matters
------------------------------------
``icc`` is the intra-class correlation of the outcome within a task, and
``rho`` is the correlation *between arms* within a (task, seed) cell.
Section 8.1 reports that rho is the higher-leverage knob by a wide
margin: at N=100/K=5 the MDE moves 15.0 -> 8.7pp as rho goes 0 -> 0.8,
but only 9.9 -> 11.9pp as icc goes 0.3 -> 0.9. That asymmetry is why
section 8.3 spends its effort on holding the pairing tight
(time-interleaved arms, identical container digests, byte-identical
prompts) rather than on adding trials: raising rho buys more than raising
K, and it is free.

The generator, stated so it can be disputed
-------------------------------------------
Per task, a baseline rate ``p_i ~ Beta`` with mean ``base_rate`` and
``Var(p_i) = icc * p(1-p)``, which is the exact beta-binomial identity
``icc = 1/(1 + a + b)`` -- so the simulated ICC is the same quantity
:func:`icc_estimate` measures from a pilot, not a loose analogue. Then
``n_seeds`` paired trials per task, with the two arms' outcomes drawn
through a Gaussian copula at correlation ``rho``. Treatment rate is
``min(1, p_i + delta)``: the ceiling is real and is left in.

Two honest caveats about rho. For binary marginals the realised Pearson
correlation is *attenuated* relative to the copula parameter, so ``rho``
here is a latent (tetrachoric-scale) correlation; at ``base_rate = 0.35``
and a 15pp lift, ``rho = 0.6`` realises a Bernoulli correlation near
0.50. And rho is assumed constant across tasks, which it will not be.
Both make this a model, to be replaced by the pilot's measurement
(section 8.1: the first run is a pilot for exactly this reason).
"""

from __future__ import annotations

import math
import random
from collections.abc import Sequence
from dataclasses import dataclass

from .bootstrap import cluster_bootstrap_diff, norm_ppf

#: Pre-registration section 8.1 assumptions, so a caller that omits them
#: reproduces the published table rather than inventing a new baseline.
DEFAULT_BASE_RATE = 0.35
DEFAULT_ICC = 0.5
DEFAULT_RHO = 0.6

#: The declared grid: rows are N tasks, columns are K trials per task.
GRID_TASKS = (50, 75, 100, 150)
GRID_TRIALS = (1, 3, 5, 10)

#: Section 8.1's target and floor.
TARGET_GRID = (100, 3)
MINIMUM_GRID = (60, 3)

_BISECTION_STEPS = 7
_SEED_STRIDE = 1_000_003
_EPS = 1e-12


def effective_samples_per_trial(icc: float, k: int) -> float:
    """``1 / (1 + (k - 1) * icc)`` -- the design-effect reciprocal.

    Section 8.2 asks for this to be published because the headline trial
    count is otherwise read as independent evidence. K = 5 trials at an
    ICC of 0.83 is worth ``5 / (1 + 4 * 0.83) = 1.16`` independent trials
    (see :func:`effective_trials`): four fifths of the apparent sample is
    the same task answering again. Making that visible is the difference
    between "300 runs" and "60 tasks' worth of information".
    """
    if not math.isfinite(icc) or not 0.0 <= icc <= 1.0:
        raise ValueError(f"icc must lie in [0, 1], got {icc!r}")
    if not isinstance(k, int) or k < 1:
        raise ValueError(f"k must be a positive int, got {k!r}")
    return 1.0 / (1.0 + (k - 1) * icc)


def effective_trials(icc: float, k: int) -> float:
    """``k`` trials expressed as independent trials: ``k / (1 + (k-1) * icc)``."""
    return k * effective_samples_per_trial(icc, k)


def icc_estimate(clusters: Sequence[Sequence[float]]) -> float:
    """One-way random-effects ICC from per-task outcomes (ANOVA estimator).

    Takes one inner sequence per task holding that task's outcomes (0/1
    for task success). Uses the unequal-size ANOVA estimator::

        n0  = (N - sum(n_i^2) / N) / (m - 1)
        ICC = (MSB - MSW) / (MSB + (n0 - 1) * MSW)

    The ANOVA estimator rather than a likelihood fit because it is
    closed-form, needs no optimiser, and is auditable by hand from the
    two mean squares -- which matters when the number it produces decides
    how many runs the project pays for.

    Negative estimates are clamped to 0.0: the estimator can go negative
    when within-task variation exceeds between-task variation, and a
    negative intra-class correlation is not a meaningful quantity here,
    but note that the clamp is a floor rather than evidence of
    independence. Returns ``nan`` when every outcome is identical, where
    there is no variance to partition and the ratio is 0/0.
    """
    groups = [[float(v) for v in cluster] for cluster in clusters]
    groups = [g for g in groups if g]
    m = len(groups)
    if m < 2:
        raise ValueError(f"need at least 2 non-empty tasks to partition variance, got {m}")
    sizes = [len(g) for g in groups]
    total_n = sum(sizes)
    if total_n <= m:
        raise ValueError("need at least one task with 2+ trials to estimate within-task variance")

    grand = math.fsum(math.fsum(g) for g in groups) / total_n
    means = [math.fsum(g) / len(g) for g in groups]
    ss_between = math.fsum(n * (mean - grand) ** 2 for n, mean in zip(sizes, means, strict=True))
    ss_within = math.fsum(
        math.fsum((v - mean) ** 2 for v in g) for g, mean in zip(groups, means, strict=True)
    )
    ms_between = ss_between / (m - 1)
    ms_within = ss_within / (total_n - m)
    n0 = (total_n - math.fsum(n * n for n in sizes) / total_n) / (m - 1)
    denom = ms_between + (n0 - 1.0) * ms_within
    if denom == 0.0:
        return math.nan
    return min(max((ms_between - ms_within) / denom, 0.0), 1.0)


def _beta_params(mean: float, icc: float) -> tuple[float, float]:
    """Beta(a, b) with the given mean and ``Var = icc * mean * (1 - mean)``."""
    concentration = 1.0 / icc - 1.0
    return (mean * concentration, (1.0 - mean) * concentration)


def _simulate_power(
    delta: float,
    n_tasks: int,
    n_seeds: int,
    base_rate: float,
    icc: float,
    rho: float,
    resamples: int,
    trials: int,
    alpha: float,
    seed: int,
    method: str,
) -> float:
    """Fraction of simulated experiments whose CI lower bound clears 0.

    Experiment ``j`` always uses seed ``seed * stride + j``, for every
    ``delta`` the bisection visits -- common random numbers. The task
    rates and the latent normal draws are therefore shared across
    deltas and only the success thresholds move, which makes the
    estimated power a near-monotone, fully reproducible function of
    delta. That is what makes a 7-step bisection trustworthy at a few
    hundred replicates instead of needing tens of thousands.
    """
    beta = None if icc == 0.0 else _beta_params(base_rate, icc)
    mix = math.sqrt(max(1.0 - rho * rho, 0.0))
    rejections = 0
    for j in range(trials):
        rng = random.Random(seed * _SEED_STRIDE + j)
        clusters = []
        for _ in range(n_tasks):
            p = base_rate if beta is None else min(max(rng.betavariate(*beta), _EPS), 1 - _EPS)
            q = min(max(p + delta, _EPS), 1.0 - _EPS)
            t_cut, r_cut = norm_ppf(q), norm_ppf(p)
            pairs = []
            for _ in range(n_seeds):
                g1 = rng.gauss(0.0, 1.0)
                g2 = rng.gauss(0.0, 1.0)
                pairs.append((g1 < t_cut, rho * g1 + mix * g2 < r_cut))
            clusters.append(pairs)
        result = cluster_bootstrap_diff(
            clusters, resamples=resamples, seed=seed * _SEED_STRIDE + j, ci=1.0 - alpha,
            method=method,
        )
        if result.ci_lo > 0.0:
            rejections += 1
    return rejections / trials


def mde_for_grid(
    n_tasks: int,
    n_seeds: int,
    base_rate: float = DEFAULT_BASE_RATE,
    icc: float = DEFAULT_ICC,
    rho: float = DEFAULT_RHO,
    resamples: int = 200,
    trials: int = 150,
    alpha: float = 0.05,
    power: float = 0.8,
    seed: int = 0,
    method: str = "percentile",
) -> float:
    """Smallest paired difference detectable at ``power`` on this grid.

    ``alpha`` is the complement of the **two-sided** CI level, matching
    the published rule: the harness reports a 95% CI and declares support
    when it excludes 0 on the hypothesised side, so the realised
    one-sided type-I rate is ``alpha / 2``. That is the same convention
    section 8.1's table quotes as "alpha = .05", and it is stated because
    quoting an MDE at one-sided 0.05 while publishing a two-sided
    interval would understate the sample size needed by a wide margin.

    ``resamples`` and ``trials`` are the accuracy knobs; the defaults
    keep a full table inside a CI budget, not publication accuracy. At
    ``trials=150`` the Monte Carlo standard error of a power near 0.8 is
    about 0.033, which maps to roughly +-1.5pp of MDE. Regenerate the
    published table with ``resamples=2000, trials=2000`` and record those
    parameters beside it.

    ``method="percentile"`` for the inner bootstrap rather than the
    pre-registered BCa: at a few hundred resamples BCa's tail correction
    is dominated by its own noise. Pass ``method="bca"`` with larger
    ``resamples`` to check that rather than trusting it.

    Returns ``math.nan`` when even a difference of 1.0 misses ``power``
    -- the honest answer for a grid that cannot detect anything, and a
    value that will not quietly render as a number.
    """
    if not isinstance(n_tasks, int) or n_tasks < 2:
        raise ValueError(f"n_tasks must be an int >= 2 for a cluster bootstrap, got {n_tasks!r}")
    if not isinstance(n_seeds, int) or n_seeds < 1:
        raise ValueError(f"n_seeds must be a positive int, got {n_seeds!r}")
    if not math.isfinite(base_rate) or not 0.0 < base_rate < 1.0:
        raise ValueError(f"base_rate must lie strictly inside (0, 1), got {base_rate!r}")
    if not math.isfinite(icc) or not 0.0 <= icc < 1.0:
        raise ValueError(f"icc must lie in [0, 1), got {icc!r}")
    if not math.isfinite(rho) or not -1.0 < rho < 1.0:
        raise ValueError(f"rho must lie strictly inside (-1, 1), got {rho!r}")
    if not math.isfinite(power) or not 0.0 < power < 1.0:
        raise ValueError(f"power must lie strictly inside (0, 1), got {power!r}")
    if not math.isfinite(alpha) or not 0.0 < alpha < 1.0:
        raise ValueError(f"alpha must lie strictly inside (0, 1), got {alpha!r}")
    if not isinstance(trials, int) or trials < 1:
        raise ValueError(f"trials must be a positive int, got {trials!r}")

    def power_at(delta: float) -> float:
        return _simulate_power(
            delta, n_tasks, n_seeds, base_rate, icc, rho, resamples, trials, alpha, seed, method
        )

    lo, hi = 0.0, 1.0
    if power_at(hi) < power:
        return math.nan
    for _ in range(_BISECTION_STEPS):
        mid = (lo + hi) / 2.0
        if power_at(mid) >= power:
            hi = mid
        else:
            lo = mid
    return hi


@dataclass(frozen=True)
class PowerRow:
    """One (tasks x trials) cell of the power table, with its assumptions."""

    n_tasks: int
    n_seeds: int
    n_pairs: int
    base_rate: float
    icc: float
    rho: float
    mde: float
    effective_trials: float
    power: float
    alpha: float
    resamples: int
    trials: int
    seed: int


def power_table(
    grids: Sequence[tuple[int, int]] | None = None,
    base_rate: float = DEFAULT_BASE_RATE,
    icc: float = DEFAULT_ICC,
    rho: float = DEFAULT_RHO,
    power: float = 0.8,
    alpha: float = 0.05,
    resamples: int = 200,
    trials: int = 150,
    seed: int = 0,
    method: str = "percentile",
) -> list[PowerRow]:
    """One :class:`PowerRow` per ``(n_tasks, n_seeds)`` grid.

    ``grids=None`` emits section 8.1's declared grid -- ``GRID_TRIALS``
    within ``GRID_TASKS`` -- so the published table can be regenerated
    without restating it. Every row is produced with identical simulation
    parameters by construction: a table whose cells came from different
    settings is not a table.
    """
    if grids is None:
        grids = [(n, k) for n in GRID_TASKS for k in GRID_TRIALS]
    rows: list[PowerRow] = []
    for n_tasks, n_seeds in grids:
        rows.append(
            PowerRow(
                n_tasks=n_tasks,
                n_seeds=n_seeds,
                n_pairs=n_tasks * n_seeds,
                base_rate=base_rate,
                icc=icc,
                rho=rho,
                mde=mde_for_grid(
                    n_tasks, n_seeds, base_rate, icc, rho, resamples, trials, alpha,
                    power, seed, method,
                ),
                effective_trials=effective_trials(icc, n_seeds),
                power=power,
                alpha=alpha,
                resamples=resamples,
                trials=trials,
                seed=seed,
            )
        )
    return rows


def render_markdown(rows: Sequence[PowerRow], target: float = 0.10) -> str:
    """Render rows as the markdown table section 8.1 publishes.

    One row per task count and one column per trial count, matching the
    layout in the pre-registration so the two can be compared line by
    line. ``target`` marks section 8.1's "MDE <= 10pp" criterion.
    """
    if not rows:
        return "_No power rows._\n"
    head = rows[0]
    ks = sorted({row.n_seeds for row in rows})
    ns = sorted({row.n_tasks for row in rows})
    cell = {(row.n_tasks, row.n_seeds): row for row in rows}
    lines = [
        f"MDE in percentage points (alpha={head.alpha}, {head.power:.0%} power, "
        f"base rate {head.base_rate}, ICC={head.icc}, rho={head.rho}; "
        f"{head.trials} simulated experiments x {head.resamples} resamples, seed {head.seed}).",
        "",
        "| N tasks | " + " | ".join(f"K={k}" for k in ks) + " | effective trials |",
        "| --- | " + " | ".join("---" for _ in ks) + " | --- |",
    ]
    for n in ns:
        cells = []
        for k in ks:
            row = cell.get((n, k))
            if row is None:
                cells.append("-")
            elif math.isnan(row.mde):
                cells.append("n/a")
            else:
                mark = "**" if row.mde <= target else ""
                cells.append(f"{mark}{row.mde * 100:.1f}{mark}")
        eff = [cell[(n, k)].effective_trials for k in ks if (n, k) in cell]
        lines.append(
            f"| {n} | " + " | ".join(cells) + f" | {max(eff):.2f} (K={ks[-1]}) |"
            if eff
            else f"| {n} | " + " | ".join(cells) + " | - |"
        )
    lines.append("")
    lines.append(f"Bold: meets the section 8.1 criterion of MDE <= {target:.0%}.")
    return "\n".join(lines) + "\n"
