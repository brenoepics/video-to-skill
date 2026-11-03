"""Holm-Bonferroni correction across the pre-registered ablation family.

Why correct at all, and why only the ablations
----------------------------------------------
Pre-registration section 8 applies Holm-Bonferroni "across the ... ablation
comparisons, with the uncorrected values also shown". The ablations
(H3-H6, and H8 as declared in ``arms.py``) all test the *same underlying
question* -- "which component of v2s is responsible?" -- against the same
treatment arm A1 on the same task set. Running five such tests at
alpha = 0.05 and reporting whichever clears gives roughly a 1-in-4 chance
of a spurious winner, and the component that came out significant would
then be written into a README as a feature that works.

H1 is deliberately **not** in the family. It is the single declared
primary endpoint, fixed before any data existed, and it is not selected
from a menu; correcting it would cost power to buy protection against a
multiplicity that does not exist. Membership is read off each
``Comparison.corrected`` flag in ``arms.py`` rather than hard-coded here,
so adding a hypothesis to the family is a one-line change in the
pre-registered file and cannot be done accidentally from the stats layer.

Why Holm rather than Bonferroni or Benjamini-Hochberg
-----------------------------------------------------
Holm is uniformly more powerful than plain Bonferroni and controls the
same quantity (the family-wise error rate) under no assumptions about
dependence -- which matters because these five comparisons share the arm
A1 and are therefore positively correlated. Benjamini-Hochberg would
control only the false discovery rate; for a small family of named,
individually-publishable claims ("verification buys nothing") a reader
cares about the error rate of the specific claim in front of them, not
about the expected proportion of errors across the family. The
uncorrected p-value is published next to the adjusted one either way, so
nothing is concealed by the choice.
"""

from __future__ import annotations

import math
from collections.abc import Mapping

#: The name of the procedure, for the report's ``p_method`` field.
METHOD = "holm-bonferroni"


def holm_bonferroni(pvalues: Mapping[str, float]) -> dict[str, float]:
    """Holm-adjusted p-values, keyed exactly as the input.

    With ``m`` hypotheses ordered ``p_(1) <= ... <= p_(m)``, the adjusted
    value for rank ``i`` (1-based) is::

        p~_(i) = max over j <= i of  min(1, (m - j + 1) * p_(j))

    The running maximum is not cosmetic: without it the adjusted sequence
    can decrease where the raw sequence increases, and a reader comparing
    two rows would see a larger raw p-value carrying a smaller adjusted
    one. Enforcing monotonicity makes the adjusted column safe to read
    as "reject every hypothesis whose adjusted p is <= alpha", which is
    the equivalent step-down rule.

    Returns a dict in the **input key order**, so the report's rows stay
    in the order the pre-registration declares them rather than in
    significance order -- sorting results by their own p-value is a
    small, standard way to make a table read better than the experiment
    was.

    An empty mapping returns an empty dict: a family of nothing needs no
    correction, and this is the normal case for a run whose ablation arms
    were not scheduled.
    """
    for key, value in pvalues.items():
        if not isinstance(key, str):
            raise ValueError(f"p-value keys must be str, got {key!r}")
        if not isinstance(value, (int, float)) or isinstance(value, bool):
            raise ValueError(f"p-value for {key!r} must be a float, got {value!r}")
        if not math.isfinite(value):
            raise ValueError(f"p-value for {key!r} must be finite, got {value!r}")
        if not 0.0 <= value <= 1.0:
            raise ValueError(f"p-value for {key!r} must lie in [0, 1], got {value!r}")

    m = len(pvalues)
    if m == 0:
        return {}

    # Ties are broken by key so the output is deterministic; with the
    # running maximum applied, tied p-values receive identical adjusted
    # values regardless of the order they are visited in.
    ranked = sorted(pvalues.items(), key=lambda item: (item[1], item[0]))
    adjusted: dict[str, float] = {}
    running = 0.0
    for rank, (key, raw) in enumerate(ranked):
        running = max(running, min(1.0, (m - rank) * raw))
        adjusted[key] = running
    return {key: adjusted[key] for key in pvalues}


def holm_rejected(pvalues: Mapping[str, float], alpha: float = 0.05) -> dict[str, bool]:
    """Which hypotheses the Holm step-down rule rejects at ``alpha``.

    Provided so that no caller re-derives the comparison from adjusted
    values and gets the boundary wrong: the rule is ``p~ <= alpha``, with
    the inequality non-strict. Note the harness's published verdicts come
    from the bootstrap CI, not from this function (see
    ``analysis.verdict``); rejection is reported, never substituted for
    the pre-registered decision rule.
    """
    if not math.isfinite(alpha) or not 0.0 < alpha < 1.0:
        raise ValueError(f"alpha must lie strictly inside (0, 1), got {alpha!r}")
    return {key: value <= alpha for key, value in holm_bonferroni(pvalues).items()}
