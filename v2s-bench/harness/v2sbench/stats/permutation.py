"""Paired sign-flip permutation test on per-task mean differences. **Primary test.**

Why this replaced exact McNemar as primary
------------------------------------------
McNemar binarises each (task, seed) cell into win / loss / tie and then
counts discordant pairs. That throws away the *within-task gradient*: a
task where the treatment wins 5 trials out of 5 and a task where it wins
3 out of 5 contribute the same way once the task is collapsed, and a task
whose trials split evenly contributes nothing at all. The methodology
review measured the cost of discarding that information, at alpha = 0.05
and a 15 percentage-point lift:

    design                  paired-mean permutation   McNemar-on-majority
    N = 100 tasks, K = 5              0.97                   0.79
    N =  50 tasks, K = 5              0.77                   0.46

Half the power at N = 50 is not a refinement, it is the difference
between a run that can answer H1 and one that cannot, at identical cost.
So the primary test is now a sign-flip permutation test whose unit is the
task -- the same unit of generalisation the cluster bootstrap uses
(pre-registration section 8) -- applied to each task's *mean* difference
rather than to a binarised verdict. ``mcnemar.mcnemar_exact`` is kept and
reported as a **robustness check**, because it is the test most of the
adjacent literature uses and a reader should be able to see both.

Why a permutation test rather than a t-test on the task means
-------------------------------------------------------------
The per-task mean difference on K trials takes few distinct values
(``-1, -2/3, ..., 1`` at K = 3), is bounded, and is heavily tied at zero.
Nothing about that distribution is normal at N = 50. The sign-flip null
-- "within a task, which arm the difference favours is a coin flip" --
is exactly the null the paired design licenses, it needs no distributional
assumption, and with N <= 20 tasks it can be enumerated completely, so
the p-value is not an estimate at all.

Exactness
---------
With ``n <= exact_max_tasks`` the test enumerates all ``2^n`` sign
vectors. It does so by dynamic programming over subset sums of the
rational task differences, not by visiting ``2^n`` vectors, so the exact
path costs milliseconds rather than minutes and is reached far more often
than a naive implementation would allow. The enumerated p-value is a
ratio of exact integers and is identical on every machine.
"""

from __future__ import annotations

import math
import random
from collections.abc import Sequence
from dataclasses import dataclass
from fractions import Fraction

ALTERNATIVES = ("two-sided", "greater", "less")

#: Rational resolution used to turn an input difference into an exact
#: fraction for enumeration. Any difference this harness produces is
#: ``net_successes / n_trials`` with ``n_trials`` far below this, so the
#: conversion is exact in every real case.
_MAX_DENOMINATOR = 1_000_000

#: Caps on the exact path. Beyond these the state space of the dynamic
#: program stops being cheaper than sampling, so we sample and say so.
_MAX_SCALE = 10**7
_MAX_STATES = 2_000_000


@dataclass(frozen=True)
class PermutationResult:
    """A p-value plus everything needed to reproduce or dispute it."""

    p_value: float
    statistic: float
    alternative: str
    mode: str
    n_tasks: int
    seed: int
    resamples: int | None = None
    sign_vectors: int | None = None
    note: str | None = None


def _exact_counts(scaled: list[int], alternative: str) -> tuple[int, int]:
    """Count sign vectors at least as extreme as observed, and the total.

    For sign vector ``e``, the scaled statistic is
    ``S(e) = sum e_i s_i = T - 2 * u`` where ``T = sum s_i`` and ``u`` is
    the subset sum over the flipped tasks. So the whole enumeration
    reduces to the multiset of subset sums of ``s``, which a dictionary
    DP produces in ``O(n * distinct_sums)`` with exact integer counts.
    """
    total = sum(scaled)
    counts: dict[int, int] = {0: 1}
    for value in scaled:
        updated: dict[int, int] = {}
        for subset_sum, multiplicity in counts.items():
            updated[subset_sum] = updated.get(subset_sum, 0) + multiplicity
            shifted = subset_sum + value
            updated[shifted] = updated.get(shifted, 0) + multiplicity
        counts = updated
        if len(counts) > _MAX_STATES:
            raise OverflowError("subset-sum state space too large")

    extreme = 0
    for subset_sum, multiplicity in counts.items():
        stat = total - 2 * subset_sum
        if alternative == "two-sided":
            hit = abs(stat) >= abs(total)
        elif alternative == "greater":
            hit = stat >= total
        else:
            hit = stat <= total
        if hit:
            extreme += multiplicity
    return extreme, 1 << len(scaled)


def _sampled_p(
    diffs: list[float], resamples: int, seed: int, alternative: str, observed: float
) -> float:
    """Monte Carlo sign-flip p-value with the ``(1 + count) / (1 + B)`` estimator.

    The ``+1`` on both sides is not a fudge: the observed sign vector is
    itself one of the ``2^n`` equally likely vectors under the null, so
    including it is correct, and it also means a reported p-value can
    never be exactly 0 -- which it never is, and which would be
    indistinguishable from "we did not run enough resamples" if we let
    the naive ratio print 0.0000.
    """
    rng = random.Random(seed)
    n = len(diffs)
    count = 0
    for _ in range(resamples):
        bits = rng.getrandbits(n)
        stat = math.fsum(d if (bits >> i) & 1 else -d for i, d in enumerate(diffs))
        if alternative == "two-sided":
            # Tolerance so that a resample equal to the observed statistic
            # up to float noise counts as "at least as extreme", which it is.
            if abs(stat) >= abs(observed) - 1e-12 * max(1.0, abs(observed)):
                count += 1
        elif alternative == "greater":
            if stat >= observed - 1e-12 * max(1.0, abs(observed)):
                count += 1
        elif stat <= observed + 1e-12 * max(1.0, abs(observed)):
            count += 1
    return (1 + count) / (1 + resamples)


def paired_permutation(
    task_diffs: Sequence[float],
    resamples: int = 10000,
    seed: int = 0,
    alternative: str = "two-sided",
    exact_max_tasks: int = 20,
) -> PermutationResult:
    """Sign-flip permutation test on one mean difference per task.

    ``task_diffs`` holds one value per task: that task's mean paired
    difference (treatment minus reference), as produced by
    ``bootstrap.task_diffs``. One value per task, never one per trial --
    feeding this trial-level differences would reinstate exactly the
    independence assumption section 8 forbids.

    Returns a :class:`PermutationResult` whose ``mode`` says whether the
    p-value was enumerated (``exact-enumeration``) or sampled
    (``monte-carlo``), because "p = 0.031" means different things in the
    two cases and the report prints the distinction.
    """
    if alternative not in ALTERNATIVES:
        raise ValueError(f"alternative must be one of {ALTERNATIVES}, got {alternative!r}")
    if not isinstance(resamples, int) or resamples < 1:
        raise ValueError(f"resamples must be a positive int, got {resamples!r}")
    if not isinstance(seed, int) or isinstance(seed, bool):
        raise ValueError(f"seed must be an int, got {seed!r}")
    if not isinstance(exact_max_tasks, int) or exact_max_tasks < 0:
        raise ValueError(f"exact_max_tasks must be a non-negative int, got {exact_max_tasks!r}")

    diffs = [float(d) for d in task_diffs]
    if not diffs:
        raise ValueError("task_diffs is empty; a permutation test needs at least one task")
    for index, value in enumerate(diffs):
        if not math.isfinite(value):
            raise ValueError(f"task difference {index} is not finite: {value!r}")
    n = len(diffs)
    observed = math.fsum(diffs) / n

    note: str | None = None
    if n <= exact_max_tasks:
        fractions = [Fraction(d).limit_denominator(_MAX_DENOMINATOR) for d in diffs]
        scale = math.lcm(*(f.denominator for f in fractions))
        if scale <= _MAX_SCALE:
            try:
                extreme, total_vectors = _exact_counts(
                    [int(f * scale) for f in fractions], alternative
                )
            except OverflowError:
                note = "exact enumeration abandoned: subset-sum state space too large"
            else:
                return PermutationResult(
                    p_value=extreme / total_vectors,
                    statistic=observed,
                    alternative=alternative,
                    mode="exact-enumeration",
                    n_tasks=n,
                    seed=seed,
                    sign_vectors=total_vectors,
                )
        else:
            note = (
                f"exact enumeration skipped: task differences need a common denominator "
                f"of {scale}, above the {_MAX_SCALE} cap"
            )
    return PermutationResult(
        p_value=_sampled_p(diffs, resamples, seed, alternative, observed),
        statistic=observed,
        alternative=alternative,
        mode="monte-carlo",
        n_tasks=n,
        seed=seed,
        resamples=resamples,
        note=note,
    )
