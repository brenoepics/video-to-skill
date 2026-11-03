"""Cluster bootstrap over tasks -- the pre-registered primary interval.

Why cluster, and why the task is the cluster
--------------------------------------------
Pre-registration section 8 fixes the **unit of generalisation as the
task, not the trial**, and names treating trials as independent as "the
most common way agent benchmarks report intervals that are far too
narrow". The reason is mechanical: three seeds of the same task share a
tool, a checker, a video and a knowledge gap, so their outcomes are
strongly correlated. A trial-level interval divides by sqrt(n_trials)
when the information in the sample only grows with sqrt(n_tasks), and on
a 16 x 3 grid that is an interval roughly sqrt(3) too narrow -- enough to
turn an inconclusive result into a published claim.

So we resample **tasks** with replacement, carrying each task's trials
along unbroken, and recompute the paired difference each time. The
clustering is not a refinement; it is the whole reason this module
exists, and ``tests/test_stats_bootstrap.py`` contains a test whose only
job is to show the naive interval is narrower on data where it has no
right to be.

The point estimate, stated explicitly
-------------------------------------
With unequal trial counts per task there are two defensible paired
differences and they are not equal:

* ``point`` -- the unweighted mean over tasks of each task's within-task
  mean difference. Every task carries weight 1/n_tasks regardless of how
  many trials it got. **This is the figure the report publishes.**
* ``pooled_point`` -- the difference computed over all pairs at once,
  which weights each task by its trial count.

``point`` is primary because it is the quantity the resampling
distribution is built for: resampling tasks estimates the sampling error
of a task-level average, and the estimator has to match the unit of
generalisation or the interval does not cover the thing it brackets.
``pooled_point`` is also exposed, because when a task loses trials to
container errors (section 8's exclusion rule) the two diverge, and a
reader is entitled to see that the choice of weighting -- not the data --
moved the number. On a balanced grid they are identical.

Declared limitation: this interval is optimistic
-----------------------------------------------
Pre-registration section 8.2 states it next to the figure rather than
leaving it to be discovered: at around N = 100 tasks **both CLT intervals
and the plain bootstrap under-cover in this clustered setting** -- their
realised coverage is below the nominal 95% -- and reaching nominal
coverage there needed a clustered Bayesian model (Bowyer et al., ICML
2025, arXiv:2503.01747). BCa's bias and acceleration corrections reduce
the shortfall but do not remove it. So the published CI is narrower than
a correctly-covering interval would be, and a bound that sits just clear
of 0 should be read as weaker than its nominal level claims. We keep the
cluster BCa bootstrap because section 8.2 fixes it, and we publish the
caveat with it; ``wilson.beta_binomial_hdi`` offers a clustered posterior
interval for a *rate* as a secondary check, and is deliberately not
extended to the paired difference (see its docstring).

Randomness
----------
Every draw comes from a private ``random.Random(seed)``. The global RNG
is never touched, and ``random.Random.choices`` consumes exactly one
``random()`` per draw, so a given (data, seed, resamples) triple yields
byte-identical bounds on any machine and any Python >= 3.11.
"""

from __future__ import annotations

import math
import random
from collections.abc import Sequence
from dataclasses import dataclass

Pair = tuple[bool, bool]
Cluster = Sequence[Pair]

_SQRT_2 = math.sqrt(2.0)
_SQRT_2PI = math.sqrt(2.0 * math.pi)

# Acklam's rational approximation to the standard normal quantile.
_A = (-3.969683028665376e01, 2.209460984245205e02, -2.759285104469687e02,
      1.383577518672690e02, -3.066479806614716e01, 2.506628277459239e00)
_B = (-5.447609879822406e01, 1.615858368580409e02, -1.556989798598866e02,
      6.680131188771972e01, -1.328068155288572e01)
_C = (-7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e00,
      -2.549732539343734e00, 4.374664141464968e00, 2.938163982698783e00)
_D = (7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e00,
      3.754408661907416e00)
_P_LOW = 0.02425


def norm_cdf(x: float) -> float:
    """Standard normal CDF, in closed form via ``math.erfc``. Exact, not fitted."""
    return 0.5 * math.erfc(-x / _SQRT_2)


def norm_ppf(p: float) -> float:
    """Standard normal quantile (inverse CDF).

    BCa needs Phi and Phi^-1 and the standard library has neither, while
    section 1 forbids scipy. Phi is closed-form from ``erfc``; its
    inverse is not, so this is Acklam's rational approximation (relative
    error < 1.15e-9) followed by **one Halley step** using the exact CDF::

        e = Phi(x) - p ;  u = e * sqrt(2 pi) * exp(x^2 / 2)
        x <- x - u / (1 + x u / 2)

    The refinement costs one ``erfc`` call and takes the result to full
    double precision, which matters because the published CI bound is
    read off a quantile of this function and we would rather not explain
    a 1e-9 wobble in a figure. ``tests/test_stats_bootstrap.py`` pins it
    against known quantiles to 1e-12.
    """
    if not math.isfinite(p) or not 0.0 < p < 1.0:
        raise ValueError(f"p must lie strictly inside (0, 1), got {p!r}")
    if p < _P_LOW:
        q = math.sqrt(-2.0 * math.log(p))
        x = (((((_C[0] * q + _C[1]) * q + _C[2]) * q + _C[3]) * q + _C[4]) * q + _C[5]) / (
            (((_D[0] * q + _D[1]) * q + _D[2]) * q + _D[3]) * q + 1.0)
    elif p > 1.0 - _P_LOW:
        q = math.sqrt(-2.0 * math.log(1.0 - p))
        x = -(((((_C[0] * q + _C[1]) * q + _C[2]) * q + _C[3]) * q + _C[4]) * q + _C[5]) / (
            (((_D[0] * q + _D[1]) * q + _D[2]) * q + _D[3]) * q + 1.0)
    else:
        q = p - 0.5
        r = q * q
        x = (((((_A[0] * r + _A[1]) * r + _A[2]) * r + _A[3]) * r + _A[4]) * r + _A[5]) * q / (
            ((((_B[0] * r + _B[1]) * r + _B[2]) * r + _B[3]) * r + _B[4]) * r + 1.0)
    err = norm_cdf(x) - p
    u = err * _SQRT_2PI * math.exp(x * x / 2.0)
    return x - u / (1.0 + x * u / 2.0)


@dataclass(frozen=True)
class BootstrapResult:
    """One interval, with everything needed to recompute it.

    ``fallback_reason`` is non-None exactly when BCa was requested and
    could not be formed; ``ci_method`` then reads ``percentile`` and the
    reason is published rather than hidden, because "the interval you are
    reading is not the pre-registered one" is information a reader needs.
    """

    point: float
    ci_lo: float
    ci_hi: float
    ci_method: str
    resamples: int
    seed: int
    n_tasks: int
    n_pairs: int
    pooled_point: float
    ci: float = 0.95
    fallback_reason: str | None = None


def task_diffs(clusters: Sequence[Cluster]) -> tuple[list[float], int, float]:
    """Per-task mean paired difference, total pair count, pooled difference.

    Public because it is the *single* definition of the per-task paired
    difference in this package. ``analysis.py`` needs the same quantity
    to report a point estimate when there are too few tasks to bootstrap,
    and two independent definitions of a published number is exactly the
    kind of drift section 7.4's recomputation promise is meant to rule
    out.
    """
    if len(clusters) == 0:
        raise ValueError("clusters must be non-empty; nothing to resample")
    diffs: list[float] = []
    total_pairs = 0
    pooled_num = 0
    for t_index, cluster in enumerate(clusters):
        if len(cluster) == 0:
            raise ValueError(f"cluster {t_index} has no pairs; drop empty tasks before resampling")
        net = 0
        for p_index, pair in enumerate(cluster):
            if len(pair) != 2:
                raise ValueError(f"cluster {t_index} pair {p_index} must be a 2-tuple")
            treatment, reference = pair
            if not isinstance(treatment, bool) or not isinstance(reference, bool):
                raise ValueError(
                    f"cluster {t_index} pair {p_index} must hold two bools, got {pair!r}"
                )
            net += int(treatment) - int(reference)
        diffs.append(net / len(cluster))
        pooled_num += net
        total_pairs += len(cluster)
    return diffs, total_pairs, pooled_num / total_pairs


def _order_stat(ordered: list[float], q: float) -> float:
    """Nearest-rank quantile: the ceil(q*B)-th smallest resample statistic.

    No interpolation, so the bound is always a value the bootstrap
    actually produced. Clamping at the ends means a BCa quantile pushed
    outside (0, 1) degrades to the extreme resample rather than raising.
    """
    size = len(ordered)
    index = int(math.ceil(q * size)) - 1
    return ordered[min(max(index, 0), size - 1)]


def _bca_quantiles(
    point: float, ordered: list[float], diffs: list[float], alpha: float
) -> tuple[float, float] | str:
    """Bias-corrected and accelerated tail probabilities, or a fallback reason.

    ``z0`` corrects for the median bias of the resampling distribution and
    ``a`` (the jackknife acceleration over tasks, Efron & Tibshirani 1993
    eq. 14.15) corrects for the standard error of the estimate changing
    with its own value -- which it does here, because the variance of a
    difference of proportions depends on the proportions. Both terms are
    why section 8 asks for BCa rather than bare percentiles.
    """
    below = sum(1 for value in ordered if value < point)
    if below == 0 or below == len(ordered):
        return ("bca-undefined: no resample statistic lies strictly "
                f"{'below' if below == 0 else 'above'} the point estimate")
    z0 = norm_ppf(below / len(ordered))

    k = len(diffs)
    if k < 2:
        return "bca-undefined: jackknife acceleration needs at least 2 tasks"
    total = math.fsum(diffs)
    jack = [(total - d) / (k - 1) for d in diffs]
    jack_mean = math.fsum(jack) / k
    dev = [jack_mean - value for value in jack]
    num = math.fsum(d**3 for d in dev)
    den = 6.0 * math.fsum(d * d for d in dev) ** 1.5
    if den == 0.0:
        return "bca-undefined: jackknife spread is zero (every task has the same difference)"
    accel = num / den
    if not math.isfinite(accel):
        return f"bca-undefined: acceleration is not finite ({accel!r})"

    out: list[float] = []
    for z_tail in (norm_ppf(alpha), norm_ppf(1.0 - alpha)):
        shifted = z0 + z_tail
        denom = 1.0 - accel * shifted
        if denom <= 0.0:
            return f"bca-undefined: acceleration {accel:.6g} inverts the tail map"
        out.append(norm_cdf(z0 + shifted / denom))
    return (out[0], out[1])


def bootstrap_mean(
    values: Sequence[float],
    resamples: int = 10000,
    seed: int = 0,
    ci: float = 0.95,
    method: str = "bca",
) -> tuple[float, float, float, str, str | None]:
    """Bootstrap the mean of one value per cluster.

    Returns ``(point, ci_lo, ci_hi, method_used, fallback_reason)``.

    This is the whole resampling engine, taken out of
    :func:`cluster_bootstrap_diff` so that every interval this package
    publishes -- a paired difference, or a difference *of* differences
    (``analysis.analyse_difference_of_differences``) -- is bracketed by
    the same code with the same draw sequence and the same quantile
    convention. One resampler means one thing to audit.
    """
    if method not in ("bca", "percentile"):
        raise ValueError(f"method must be 'bca' or 'percentile', got {method!r}")
    if not isinstance(resamples, int) or resamples < 1:
        raise ValueError(f"resamples must be a positive int, got {resamples!r}")
    if not isinstance(seed, int) or isinstance(seed, bool):
        raise ValueError(f"seed must be an int, got {seed!r}")
    if not math.isfinite(ci) or not 0.0 < ci < 1.0:
        raise ValueError(f"ci must lie strictly inside (0, 1), got {ci!r}")
    series = [float(v) for v in values]
    if not series:
        raise ValueError("values must be non-empty; nothing to resample")
    for index, value in enumerate(series):
        if not math.isfinite(value):
            raise ValueError(f"cluster value {index} is not finite: {value!r}")

    k = len(series)
    point = math.fsum(series) / k
    rng = random.Random(seed)
    draw = rng.choices
    stats = [sum(draw(series, k=k)) / k for _ in range(resamples)]
    stats.sort()

    alpha = (1.0 - ci) / 2.0
    reason: str | None = None
    used = method
    quantiles: tuple[float, float] = (alpha, 1.0 - alpha)
    if method == "bca":
        outcome = _bca_quantiles(point, stats, series, alpha)
        if isinstance(outcome, str):
            reason = outcome
            used = "percentile"
        else:
            quantiles = outcome
    return (point, _order_stat(stats, quantiles[0]), _order_stat(stats, quantiles[1]), used, reason)


def cluster_bootstrap_diff(
    clusters: Sequence[Cluster],
    resamples: int = 10000,
    seed: int = 0,
    ci: float = 0.95,
    method: str = "bca",
) -> BootstrapResult:
    """Resample tasks with replacement; bracket the paired success-rate difference.

    ``clusters`` holds one inner sequence per task, each being that task's
    paired ``(treatment_success, reference_success)`` outcomes. Trials
    stay with their task: a task is drawn whole or not at all.

    ``method="bca"`` is the pre-registered choice (section 8.2);
    ``"percentile"`` is available and is what BCa degrades to when its
    correction terms are undefined, with the reason recorded on the
    result.
    """
    diffs, n_pairs, pooled = task_diffs(clusters)
    point, lo, hi, used, reason = bootstrap_mean(diffs, resamples, seed, ci, method)
    return BootstrapResult(
        point=point,
        ci_lo=lo,
        ci_hi=hi,
        ci_method=used,
        resamples=resamples,
        seed=seed,
        n_tasks=len(diffs),
        n_pairs=n_pairs,
        pooled_point=pooled,
        ci=ci,
        fallback_reason=reason,
    )
