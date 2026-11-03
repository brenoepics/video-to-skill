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
Pre-registration clause                    Implementation
=========================================  ==================================
8.2 primary test: sign-flip permutation    ``permutation.paired_permutation``
8.2 robustness check: exact McNemar        ``mcnemar.mcnemar_exact``
8.2 interval: cluster bootstrap, BCa       ``bootstrap.cluster_bootstrap_diff``
8.2 per-task/per-cell rates: Wilson        ``wilson.wilson_interval``
8.2 normalised gain                        ``wilson.normalized_gain``
8.2 estimated ICC, effective samples       ``power.icc_estimate``,
                                           ``power.effective_samples_per_trial``
8.2 Holm-Bonferroni over the ablations     ``multiplicity.holm_bonferroni``
8.2 difference-of-differences, no verdict  ``analysis.analyse_difference_of_differences``
8.1 power / sample size                    ``power.mde_for_grid``
5.3 exclusions (probe-declared, errored)   ``analysis.pair_trials``
H2 by headroom band                        ``analysis.analyse_strata``
=========================================  ==================================

A secondary clustered interval for a *rate* is in
``wilson.beta_binomial_hdi``; section 8.2's declared under-coverage of
the bootstrap interval is documented in ``bootstrap``'s module
docstring.

Nothing in this package reads a task checker, a skill, or anything else
that could make an estimator depend on the outcome it is estimating; it
consumes run records and the frozen declarations in ``v2sbench.arms``.
"""

from __future__ import annotations

from .analysis import (
    ArmSummary,
    CellSummary,
    ComparisonResult,
    DiffOfDiffResult,
    StratumResult,
    analyse_arms,
    analyse_cells,
    analyse_comparison,
    analyse_comparisons,
    analyse_difference_of_differences,
    analyse_strata,
    pair_trials,
    pair_trials_by_task,
    refuse_mixed,
    verdict,
)
from .bootstrap import (
    BootstrapResult,
    bootstrap_mean,
    cluster_bootstrap_diff,
    norm_cdf,
    norm_ppf,
    task_diffs,
)
from .mcnemar import chi2_sf_1df, discordant, mcnemar_chi2, mcnemar_exact
from .multiplicity import holm_bonferroni, holm_rejected
from .permutation import PermutationResult, paired_permutation
from .power import (
    PowerRow,
    effective_samples_per_trial,
    effective_trials,
    icc_estimate,
    mde_for_grid,
    power_table,
    render_markdown,
)
from .wilson import (
    Z_95,
    beta_binomial_hdi,
    harmonic_mean_bound,
    normalized_gain,
    rate,
    wilson_interval,
    wilson_lower,
)

__all__ = [
    "Z_95",
    "ArmSummary",
    "BootstrapResult",
    "CellSummary",
    "ComparisonResult",
    "DiffOfDiffResult",
    "PermutationResult",
    "PowerRow",
    "StratumResult",
    "analyse_arms",
    "analyse_cells",
    "analyse_comparison",
    "analyse_comparisons",
    "analyse_difference_of_differences",
    "analyse_strata",
    "beta_binomial_hdi",
    "bootstrap_mean",
    "chi2_sf_1df",
    "cluster_bootstrap_diff",
    "discordant",
    "effective_samples_per_trial",
    "effective_trials",
    "harmonic_mean_bound",
    "holm_bonferroni",
    "holm_rejected",
    "icc_estimate",
    "mcnemar_chi2",
    "mcnemar_exact",
    "mde_for_grid",
    "norm_cdf",
    "norm_ppf",
    "normalized_gain",
    "pair_trials",
    "pair_trials_by_task",
    "paired_permutation",
    "power_table",
    "rate",
    "refuse_mixed",
    "render_markdown",
    "task_diffs",
    "verdict",
    "wilson_interval",
    "wilson_lower",
]
