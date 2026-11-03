"""Statistics for v2s-bench: the arithmetic behind every published figure.

**Standard library only.** No numpy, no scipy, no pandas -- not as an
aesthetic preference but as a requirement of the claim this benchmark
makes. Section 7.4 of the pre-registration promises that
``v2sbench verify-results`` recomputes every published figure from the
raw records "so a reader can audit the arithmetic". An audit that first
requires installing a scientific stack is one most readers will not
perform, and a figure nobody re-derives is a figure on trust. Everything
here is ``math``, ``random``, ``fractions`` and ``dataclasses``, so the
distribution functions BCa needs are written out in closed form
(``bootstrap.norm_ppf``, ``mcnemar.chi2_sf_1df``) where a reviewer can
check them against a table.

The second reason is reproducibility: pure-Python float arithmetic over a
fixed sequence of Mersenne Twister draws gives bit-identical bounds on
every machine, which a BLAS-backed library does not promise.

The pre-registered analysis plan (section 8) maps onto these modules as:

=========================================  ==================================
Section 8 clause                           Implementation
=========================================  ==================================
primary test: exact McNemar                ``mcnemar.mcnemar_exact``
primary interval: cluster bootstrap, BCa   ``bootstrap.cluster_bootstrap_diff``
per-task rates: Wilson                     ``wilson.wilson_interval``
Holm-Bonferroni over the ablations         ``multiplicity.holm_bonferroni``
H2 by headroom band                        ``analysis.analyse_strata``
exclusions (saturated, errored)            ``analysis.pair_trials``
power / sample size (section 8.1)          ``power.mde_for_grid``
=========================================  ==================================

Nothing in this package reads a task checker, a skill, or anything else
that could make an estimator depend on the outcome it is estimating; it
consumes run records and the frozen declarations in ``v2sbench.arms``.
"""

from __future__ import annotations

from .analysis import (
    ArmSummary,
    CellSummary,
    ComparisonResult,
    StratumResult,
    analyse_arms,
    analyse_cells,
    analyse_comparison,
    analyse_comparisons,
    analyse_strata,
    pair_trials,
    refuse_mixed,
    verdict,
)
from .bootstrap import BootstrapResult, cluster_bootstrap_diff, norm_cdf, norm_ppf
from .mcnemar import chi2_sf_1df, discordant, mcnemar_chi2, mcnemar_exact
from .multiplicity import holm_bonferroni, holm_rejected
from .power import PowerRow, mde_for_grid, power_table, render_markdown
from .wilson import Z_95, harmonic_mean_bound, rate, wilson_interval, wilson_lower

__all__ = [
    "Z_95",
    "ArmSummary",
    "BootstrapResult",
    "CellSummary",
    "ComparisonResult",
    "PowerRow",
    "StratumResult",
    "analyse_arms",
    "analyse_cells",
    "analyse_comparison",
    "analyse_comparisons",
    "analyse_strata",
    "chi2_sf_1df",
    "cluster_bootstrap_diff",
    "discordant",
    "harmonic_mean_bound",
    "holm_bonferroni",
    "holm_rejected",
    "mcnemar_chi2",
    "mcnemar_exact",
    "mde_for_grid",
    "norm_cdf",
    "norm_ppf",
    "pair_trials",
    "power_table",
    "rate",
    "refuse_mixed",
    "render_markdown",
    "verdict",
    "wilson_interval",
    "wilson_lower",
]
