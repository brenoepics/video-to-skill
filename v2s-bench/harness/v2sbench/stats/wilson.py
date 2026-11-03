"""Wilson score intervals for a binomial proportion.

Why Wilson and not the textbook normal interval
------------------------------------------------
Every proportion this harness publishes is computed from a small number
of trials, and many of them sit near 0 or 1 (a saturated task, a task
nobody passes, an arm that never violates policy). The Wald interval
``p +- z*sqrt(p(1-p)/n)`` is actively wrong in exactly that regime: its
width collapses to zero at ``k == 0`` and ``k == n``, so it would claim
certainty from the one shape of evidence that carries least of it. The
Wilson score interval is the inversion of the score test, so it stays
inside [0, 1], keeps a sensible width at the boundaries, and has far
better mean coverage at small ``n`` (Brown, Cai & DasGupta 2001, who
recommend it as the default for ``n < 40``).

The decisive reason, though, is parity: ``vts-extract eval`` in the
parent project already reports "every proportion carries its Wilson 95%
lower bound" (see ``BENCHMARK.md``), and gates CI on that bound. A
benchmark that reported the same kind of quantity with a different
estimator would produce two incomparable sets of numbers for the same
repository. Pre-registration section 8 therefore fixes Wilson for all
per-task and per-arm rates.

Definition (Wilson 1927). The interval is the set of ``p`` for which the
score statistic does not reject, i.e. the two roots of::

    (k/n - p)^2 = z^2 * p(1-p)/n

which rearranges to the quadratic ``(n + z^2) p^2 - (2k + z^2) p + k^2/n = 0``.
``tests/test_stats_wilson.py`` solves that quadratic independently and
checks this module against it, so the closed form below cannot drift
without a test failing.
"""

from __future__ import annotations

import math
from collections.abc import Sequence

#: Two-sided 95% standard normal quantile, i.e. Phi^-1(0.975).
#: Hard-coded rather than computed so that every figure in the report is
#: bit-identical across machines and Python builds. The exact value is
#: 1.9599639845400545; the project's published convention writes it to
#: ten digits, and the difference moves a bound by <1e-10.
Z_95 = 1.959963985

#: One-sided 95% quantile, Phi^-1(0.95). Offered because a *lower bound*
#: at 95% confidence is one-sided by nature; the parent project's gate
#: nevertheless uses the two-sided Z_95 lower limit, so that stays the
#: default and this constant is here for callers who want the other
#: convention and say so.
Z_90 = 1.644853627


def wilson_interval(successes: int, n: int, z: float = Z_95) -> tuple[float, float]:
    """Wilson score interval for ``successes`` out of ``n``.

    Returns ``(lo, hi)``, both in [0, 1].

    ``n == 0`` returns ``(0.0, 1.0)``: the vacuous interval. With no
    trials, no proportion is excluded, and that is the honest answer --
    the alternative conventions (raising, or returning ``nan``) either
    break report generation for a cell that was simply never run, or
    propagate a value that renders as a blank where a reader would read
    the blank as "zero". The report's ``n`` field is what tells a reader
    there is no evidence here; the interval must not pretend otherwise.

    ``k == 0`` and ``k == n`` use the closed forms the algebra collapses
    to, because there the centre and the half-width are equal::

        k = 0:  (0,              z^2 / (n + z^2))
        k = n:  (n / (n + z^2),  1)

    Evaluating the general expression there instead leaves a residue of a
    few times 1e-17 on the bound that should be exactly 0 -- small, but
    this module's output is published and recomputed, and a lower bound
    that prints as 5.6e-17 or fails an ``== 0.0`` check is a defect in a
    figure. Both forms are hand-checkable: 0/20 at z=1.96 gives
    3.8415 / 23.8415 = 0.16113.
    """
    if not isinstance(successes, int) or isinstance(successes, bool):
        raise ValueError(f"successes must be an int, got {successes!r}")
    if not isinstance(n, int) or isinstance(n, bool):
        raise ValueError(f"n must be an int, got {n!r}")
    if n < 0:
        raise ValueError(f"n must be non-negative, got {n}")
    if not 0 <= successes <= n:
        raise ValueError(f"successes must satisfy 0 <= k <= n, got k={successes}, n={n}")
    if not math.isfinite(z) or z <= 0:
        raise ValueError(f"z must be a positive finite float, got {z!r}")

    if n == 0:
        return (0.0, 1.0)

    nf = float(n)
    z2 = z * z
    denom = nf + z2
    if successes == 0:
        return (0.0, z2 / denom)
    if successes == n:
        return (nf / denom, 1.0)

    k = float(successes)
    centre = (k + z2 / 2.0) / denom
    half = (z / denom) * math.sqrt(k * (nf - k) / nf + z2 / 4.0)
    lo = centre - half
    hi = centre + half
    # Guard against floating-point excursions of a few ulps only; the
    # mathematics already confines the interval to [0, 1].
    return (min(max(lo, 0.0), 1.0), min(max(hi, 0.0), 1.0))


def wilson_lower(successes: int, n: int, z: float = Z_95) -> float:
    """Lower Wilson bound only -- the quantity the parent project gates on.

    Reporting a lower bound rather than a point estimate is what keeps a
    small suite honest: 16 tasks x 3 seeds give a bound well below the
    observed rate, so a figure cannot look strong merely by being
    measured on few trials (``BENCHMARK.md``, "Gating").
    """
    return wilson_interval(successes, n, z)[0]


def harmonic_mean_bound(p_lo: float, r_lo: float) -> float:
    """Combine a precision bound and a recall bound into an F1 bound.

    This exists purely for parity with ``vts-extract eval``, which states
    that "an F1 row's bound is the harmonic mean of its precision and
    recall bounds" (``BENCHMARK.md``). Keeping the same rule here means a
    v2s-bench F1 row and a vts-extract F1 row can be read side by side.

    A caveat a reviewer should know, stated rather than hidden: the
    harmonic mean of two individually-valid lower bounds is **not** a
    rigorous 95% lower bound on F1, because the two counts are not
    independent and no joint coverage argument is made. It is a
    conservative, monotone summary that lies between the two inputs and
    is pulled towards the smaller of them -- which is the behaviour
    wanted from an F1 bound, since F1 is itself a harmonic mean. We use
    it only where the parent project already does, and never as the
    interval behind a hypothesis test -- those use the cluster bootstrap
    (``bootstrap.py``) instead.

    Returns 0.0 when either bound is 0.0, which is the limit of the
    harmonic mean rather than a special case.
    """
    for name, value in (("p_lo", p_lo), ("r_lo", r_lo)):
        if not math.isfinite(value):
            raise ValueError(f"{name} must be finite, got {value!r}")
        if not 0.0 <= value <= 1.0:
            raise ValueError(f"{name} must lie in [0, 1], got {value!r}")
    if p_lo <= 0.0 or r_lo <= 0.0:
        return 0.0
    return 2.0 * p_lo * r_lo / (p_lo + r_lo)


def rate(successes: int, n: int) -> float:
    """Point estimate ``k / n``, with ``n == 0`` mapping to 0.0.

    Separated out so that no caller has to repeat the zero-division
    guard, and so the report's ``rate`` field has exactly one definition.
    """
    if n < 0:
        raise ValueError(f"n must be non-negative, got {n}")
    if not 0 <= successes <= max(n, 0):
        raise ValueError(f"successes must satisfy 0 <= k <= n, got k={successes}, n={n}")
    return 0.0 if n == 0 else successes / n


def normalized_gain(p_treat: float, p_ref: float) -> float:
    """Fraction of the *available* headroom the treatment closed.

    ``(p_treat - p_ref) / (1 - p_ref)``: the convention in the adjacent
    skill-benchmark literature, and reported alongside the raw difference
    per pre-registration section 8.2. The reason is a ceiling effect. An
    arm that moves 0.90 -> 0.95 has closed half of everything that was
    left to close, while an arm that moves 0.40 -> 0.45 has closed a
    twelfth of it; the raw difference calls both "+5 points" and makes
    the first look like the weaker result. On a suite deliberately built
    to avoid saturation (section 5) most tasks sit far from the ceiling,
    so the two agree -- but the tasks where they disagree are exactly the
    ones a reader would otherwise misread.

    Negative values are returned as computed: a treatment that loses
    ground has closed a negative share of the headroom, and clamping
    that to 0 would hide a result section 10 commits to publishing.

    ``p_ref == 1`` raises. There is no headroom left to normalise by, so
    the quantity is undefined rather than infinite, and a caller must
    decide what to print instead of receiving a ``inf`` that renders as
    a number.
    """
    for name, value in (("p_treat", p_treat), ("p_ref", p_ref)):
        if not math.isfinite(value) or not 0.0 <= value <= 1.0:
            raise ValueError(f"{name} must be a finite rate in [0, 1], got {value!r}")
    if p_ref == 1.0:
        raise ValueError(
            "normalized gain is undefined when the reference rate is 1.0: there is no "
            "headroom to normalise by. Report the raw difference for this cell."
        )
    return (p_treat - p_ref) / (1.0 - p_ref)


def _log_beta(x: float, y: float) -> float:
    return math.lgamma(x) + math.lgamma(y) - math.lgamma(x + y)


def beta_binomial_hdi(
    counts: Sequence[tuple[int, int]],
    credible: float = 0.95,
    mu_points: int = 121,
    icc_points: int = 61,
) -> tuple[float, float]:
    """Clustered posterior interval for one arm's success *rate*.

    A secondary interval, offered because a Wilson interval on pooled
    trials assumes the trials are independent and they are not: the per
    (task, arm) grid is exactly the clustered structure section 8.2
    warns about, so a pooled Wilson interval on an arm's overall rate is
    too narrow for the same reason a trial-level bootstrap is.

    The model is the one the design implies: ``k_i ~ Binomial(n_i, p_i)``
    with ``p_i ~ Beta`` across tasks, parameterised by the population
    mean ``mu`` and the intra-class correlation ``icc = 1/(1 + kappa)``
    where ``kappa = a + b``. The posterior is evaluated on a product grid
    over ``(mu, icc)`` with a uniform prior on both, normalised, then
    marginalised over ``icc``; the returned interval is the
    highest-density interval of that marginal, i.e. the shortest set of
    grid cells holding ``credible`` mass.

    Two choices a reviewer may reasonably dispute, so they are stated:
    the prior is uniform on ``icc`` rather than on ``kappa`` (uniform on
    kappa would push mass towards near-independence and narrow the
    interval, which is the self-serving direction), and the grid is
    finite, so the bounds are accurate to one grid step --
    ``1 / (mu_points - 1)`` -- not to machine precision. Increase
    ``mu_points`` to tighten that; the cost is linear.

    **This is not extended to the paired difference.** A beta-binomial is
    a model of a rate, not of a difference between two correlated arms;
    doing that properly needs a joint model carrying the inter-arm
    correlation, and the reported interval for a difference stays the
    cluster bootstrap (``bootstrap.cluster_bootstrap_diff``) with its
    declared under-coverage.
    """
    if not math.isfinite(credible) or not 0.0 < credible < 1.0:
        raise ValueError(f"credible must lie strictly inside (0, 1), got {credible!r}")
    if mu_points < 3 or icc_points < 2:
        raise ValueError(f"grid too coarse: mu_points={mu_points}, icc_points={icc_points}")
    observed = [(int(k), int(n)) for k, n in counts]
    if not observed:
        raise ValueError("counts is empty; nothing to infer from")
    for k, n in observed:
        if n <= 0 or not 0 <= k <= n:
            raise ValueError(f"invalid cluster count ({k}, {n}): need 0 <= k <= n and n > 0")

    mus = [(i + 0.5) / mu_points for i in range(mu_points)]
    iccs = [(j + 0.5) / icc_points for j in range(icc_points)]
    log_cells: list[tuple[float, int]] = []
    for j, icc in enumerate(iccs):
        kappa = 1.0 / icc - 1.0
        for i, mu in enumerate(mus):
            a = mu * kappa
            b = (1.0 - mu) * kappa
            base = _log_beta(a, b)
            total = math.fsum(_log_beta(k + a, n - k + b) - base for k, n in observed)
            log_cells.append((total, i))
        del j
    peak = max(value for value, _ in log_cells)
    marginal = [0.0] * mu_points
    for value, i in log_cells:
        marginal[i] += math.exp(value - peak)
    mass = math.fsum(marginal)
    marginal = [m / mass for m in marginal]

    ordered = sorted(range(mu_points), key=lambda i: marginal[i], reverse=True)
    acc = 0.0
    included: list[int] = []
    for i in ordered:
        included.append(i)
        acc += marginal[i]
        if acc >= credible:
            break
    half = 0.5 / mu_points
    return (max(mus[min(included)] - half, 0.0), min(mus[max(included)] + half, 1.0))
