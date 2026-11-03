"""McNemar's test on paired trial outcomes -- the pre-registered **robustness check**.

Not the primary test. Pre-registration section 8.2 makes the paired
sign-flip permutation test on per-task mean differences primary
(``permutation.paired_permutation``) and keeps exact McNemar "reported
alongside, never instead". The reason is measured, not aesthetic:
McNemar collapses each task to discordant trial counts, which discards
the within-task gradient, and simulated power at N=100 tasks / K=5
trials / 15pp lift is 0.97 for the paired-mean permutation test against
0.79 for McNemar on task majorities -- 0.77 against 0.46 at N=50.

It is kept, computed and published for two reasons. It is the test most
of the adjacent agent-benchmark literature reports, so a reader
comparing this benchmark with that literature needs the comparable
number; and it depends on the data in a different way from the
permutation test, so a disagreement between them is informative about
the data rather than about either test.

Why the *exact* form
--------------------
Under the null "the two arms are exchangeable on this task set", each
discordant pair is equally likely to fall either way, so the count ``b``
of treatment-wins is Binomial(b + c, 1/2). Section 8.2 fixes the exact
binomial form rather than the chi-square approximation because our
discordant counts will often be in the single digits, where the
approximation is unreliable (roughly ``b + c < 25``) -- and because
choosing the approximation after seeing small counts is exactly the
post-hoc latitude the pre-registration exists to remove.

The deeper caveat
-----------------
McNemar here is computed over *trials*, and trials within a task are
correlated, so its p-value is anti-conservative in precisely the way
section 8.2 warns about: it answers "are these trials exchangeable?"
when the question is "are these tasks exchangeable?". That is the second
reason it is not primary. Where it disagrees with the permutation test
and the cluster bootstrap, those are right and this is optimistic.
"""

from __future__ import annotations

import math
from collections.abc import Sequence
from fractions import Fraction

#: Returned when there are no discordant pairs at all.
NO_EVIDENCE_P = 1.0


def discordant(pairs: Sequence[tuple[bool, bool]]) -> tuple[int, int]:
    """Count discordant pairs as ``(b, c)``.

    ``b`` = treatment succeeded and reference failed.
    ``c`` = reference succeeded and treatment failed.

    Each element is one (task, seed) cell as ``(treatment_success,
    reference_success)``. Concordant pairs are counted by neither and are
    deliberately dropped -- see the module docstring.

    Only ``bool`` is accepted. Trials with ``status != "ok"`` have
    ``success = null`` in the record schema and must never reach this
    function: coercing an errored trial to ``False`` would silently
    convert a flaky container into evidence against an arm, which
    section 8's exclusion rule forbids. ``analysis.pair_trials`` drops
    them and reports their rate separately.
    """
    b = 0
    c = 0
    for index, pair in enumerate(pairs):
        if len(pair) != 2:
            raise ValueError(f"pair {index} must be a 2-tuple, got {pair!r}")
        treatment, reference = pair
        if not isinstance(treatment, bool) or not isinstance(reference, bool):
            raise ValueError(
                f"pair {index} must hold two bools, got {pair!r}; a trial with "
                "status != 'ok' has no success value and must be excluded, not coerced"
            )
        if treatment and not reference:
            b += 1
        elif reference and not treatment:
            c += 1
    return (b, c)


def mcnemar_exact(b: int, c: int) -> float:
    """Two-sided exact binomial p-value on the discordant counts.

    The null is ``B ~ Binomial(n=b+c, p=1/2)``. The two-sided p-value is
    the total null probability of every outcome at least as improbable as
    the observed one::

        p = sum{ P(X=j) : P(X=j) <= P(X=b) }

    Because the null distribution is symmetric and unimodal, that set is
    exactly ``{j <= m} u {j >= n-m}`` with ``m = min(b, c)``, so the sum
    collapses to ``min(1, 2 * P(X <= m))``. We evaluate it that way with
    ``math.comb`` and exact rational arithmetic, converting to float only
    at the end: the result is then correctly rounded and identical on
    every machine, which is what makes a published p-value auditable
    (section 7.4).

    ``b + c == 0`` returns 1.0. With no discordant pairs there is no
    evidence of any difference, and 1.0 is the honest p-value for "the
    data are exactly what the null predicts"; it is not a missing value.
    """
    if not isinstance(b, int) or isinstance(b, bool):
        raise ValueError(f"b must be an int, got {b!r}")
    if not isinstance(c, int) or isinstance(c, bool):
        raise ValueError(f"c must be an int, got {c!r}")
    if b < 0 or c < 0:
        raise ValueError(f"discordant counts must be non-negative, got b={b}, c={c}")

    n = b + c
    if n == 0:
        return NO_EVIDENCE_P

    m = min(b, c)
    tail = sum(math.comb(n, j) for j in range(m + 1))
    p = Fraction(2 * tail, 1 << n)
    return min(1.0, float(p))


def chi2_sf_1df(x: float) -> float:
    """Survival function of the chi-square distribution with 1 d.f.

    Needed because the standard library has no distribution functions and
    section 1's constraint forbids scipy. For a single degree of freedom
    there is no need for an incomplete-gamma routine: if ``X ~ chi2(1)``
    then ``X = Z^2`` for a standard normal ``Z``, so

        P(X > x) = P(|Z| > sqrt(x)) = erfc(sqrt(x / 2))

    and ``math.erfc`` is in the standard library and accurate to within a
    few ulps. This is exact in closed form, not an approximation.
    """
    if not math.isfinite(x):
        raise ValueError(f"x must be finite, got {x!r}")
    if x < 0.0:
        raise ValueError(f"chi-square statistic must be non-negative, got {x}")
    return math.erfc(math.sqrt(x / 2.0))


def mcnemar_chi2(b: int, c: int, continuity: bool = True) -> float:
    """Chi-square approximation to McNemar's test. **Not the primary test.**

    Reported alongside :func:`mcnemar_exact` for one purpose only: a
    reader comparing this benchmark against the large literature that
    uses the approximation can see both numbers and see the size of the
    gap. Nothing in this module feeds a verdict -- the exact form is
    already only a robustness check, and this is a robustness check on
    the robustness check.

    With ``continuity=True`` this is Edwards' continuity-corrected
    statistic ``(|b - c| - 1)^2 / (b + c)``, which is the usual default
    and is conservative relative to the uncorrected form. The corrected
    numerator is clamped at 0 so that ``b == c`` -- a perfect tie, the
    strongest possible agreement with the null -- cannot produce the
    positive statistic ``1 / (b + c)`` that the unclamped formula gives.

    ``b + c == 0`` returns 1.0, as in the exact test.
    """
    if not isinstance(b, int) or isinstance(b, bool):
        raise ValueError(f"b must be an int, got {b!r}")
    if not isinstance(c, int) or isinstance(c, bool):
        raise ValueError(f"c must be an int, got {c!r}")
    if b < 0 or c < 0:
        raise ValueError(f"discordant counts must be non-negative, got b={b}, c={c}")

    n = b + c
    if n == 0:
        return NO_EVIDENCE_P

    gap = float(abs(b - c))
    if continuity:
        gap = max(gap - 1.0, 0.0)
    return chi2_sf_1df(gap * gap / n)
