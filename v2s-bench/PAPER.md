# Does a skill distilled from a tutorial video make an agent better?

**A registered protocol for v2s-bench v0.1**

> **This is a protocol, not a results paper.** It states a question, a
> design, and an analysis plan, and it was written before any trial was
> run. The results sections are empty on purpose. If you are looking for
> numbers, there are none yet, and anything presenting itself as a v2s-bench
> result without a link to a frozen [pre-registration](docs/preregistration.md)
> should be disbelieved.

---

## Abstract

[video-to-skill](https://github.com/brenoepics/video-to-skill) converts a
tutorial video into an installable agent skill: a procedure whose commands
are read from screen pixels rather than narration, with per-step timestamp
provenance, and whose steps are executed in an OS sandbox before the
package is badged. The project measures extraction *fidelity* against a
synthetic gold set. It does not measure whether the resulting skill changes
what an agent can do.

We specify a pre-registered, paired experiment over four task tiers —
purpose-built CLIs, real CLIs, GUI applications and games — comparing a
well-prompted baseline agent against the same agent holding a v2s skill,
with six further arms isolating pixel grounding, execution verification,
context length, raw-transcript equivalence, skill discovery, and a
hand-written ceiling. Task admission requires a declared knowledge gap, an
oracle solution that passes, and a null agent that fails. Exclusions are
declared on a cold knowledge probe rather than on observed separation
between arms.

The design is shaped by three findings from adjacent literature: that
skill-versus-nothing is a large and nearly uninformative effect; that
skill-versus-the-same-information-rawer is a single-digit effect, and is
therefore what sets our sample size; and that the dominant mechanism by
which skills help may be *procedural stabilisation* rather than supplying
missing facts — which gives our central moderator hypothesis a competing
prediction and makes the test informative rather than confirmatory.

## 1. The question

The project's existing measurement, `vts-extract eval`, scores boundary and
step F1 and command recall against a synthetic gold set of rendered
screencasts. It answers: *did the pipeline read the video correctly?*

It cannot answer: *does the skill make an agent better?* Those come apart
in at least three ways. The model may already know the procedure, so a
perfect extraction changes nothing. The environment may correct the agent
anyway, so instruction is redundant. Or the skill may be faithful but
written in a form an agent does not act on — found but not followed, or
never found at all.

v2s-bench measures the second question, and is built so that a negative
answer is visible and publishable.

## 2. What is already known

| # | Finding | Source | Verification |
|---|---|---|---|
| 1 | Curated skills raise average pass rate from **33.9% → 50.5% (+16.6pp)** over 87 tasks in 8 domains; per-configuration gains span **+4.1 to +25.7pp** across 18 model×harness configurations | SkillsBench, [arXiv:2602.12670](https://arxiv.org/abs/2602.12670) | abstract checked 2026-11-03 |
| 2 | Focused skills of **at most three modules outperform larger bundles**; smaller models with skills can match larger models without | ibid. | abstract checked |
| 3 | Skills beat Workflow Memory by **+6.06 points** in matched comparisons | "Demystifying Agent Skills", [arXiv:2608.14036](https://arxiv.org/abs/2608.14036) | abstract checked |
| 4 | **Procedural anchoring accounts for 65.7% of skill-use cases; explicit knowledge injection only 4.5%** — skills help by stabilising noisy execution more than by supplying missing facts | ibid. | abstract checked |
| 5 | Retrieval precision collapses from **29.6% to 3.3%** as the skill pool grows from 5 to 100 | ibid. | abstract checked |
| 6 | A published **null**: +8.9pp, 95% CI **[−6.8, +24.6]**, p=0.71, at **61× token overhead**; attributed to environments that already correct the agent ("feedback bandwidth") | Chacko et al., arXiv:2605.20023 | from literature review; **not independently re-checked** |
| 7 | Nobody controls for context length; length-matched baselines named as the closest work's own top limitation | SkillsBench §limitations | from review |
| 8 | Topically-adjacent *irrelevant* documents can **improve** performance, while random text helps least | arXiv:2411.19463 | from review |
| 9 | A **knowing–doing gap**: models answer environment quiz questions correctly and then act wrongly | BALROG, arXiv:2411.13543 App. F.5 | from review |
| 10 | Feeding a model the *paper* describing an environment beat every RL baseline with no interaction (Crafter 27.3 vs 14.5) | SPRING; cf. Read-and-Reward | from review |
| 11 | Oracle plans lift a model from **20.0% → 50.0%** on tasks built from YouTube tutorials | VideoGUI (NeurIPS 2024) Tab. 8 | from review |
| 12 | Harness changes swung the same model **62.7% → 99.9%** on the same tasks | ARC-AGI-3 | from review |
| 13 | **10–15%** of tasks in borrowed agent benchmarks are defective; **15.3%** of FAIL verdicts across five computer-use suites were wrong; verifier errors reach 100% relative, with suites passing empty responses | Epoch AI; arXiv:2607.28367; ABC, arXiv:2507.02825 | from review |
| 14 | At N≈100 tasks, CLT **and** plain bootstrap intervals **under-cover** in clustered agent evaluations | Bowyer et al., ICML 2025, arXiv:2503.01747 | from review |

> Rows marked "from review" come from a literature review conducted on
> 2026-11-03 and have not been re-checked against primary sources by the
> author of this document. They are load-bearing for the design, so they
> must be re-verified before any external publication. Rows 1–5 were
> checked directly.

### 2.1 What is new here

Not "skills help" — row 1 settles that. Specifically:

1. **A length-and-pipeline-matched placebo**, the control row 7 names as
   missing. Built as the real pipeline over the *wrong video* rather than
   as filler, because of row 8.
2. **A video-specific ablation** — pixels versus narration — which no
   general skill benchmark can run, because no other system reads commands
   off frames.
3. **An execution-verification ablation**: whether a verification badge
   changes downstream behaviour, not just confidence.
4. **Headroom calibration by direct probing**, turning row 9 from a
   confound into a declared stratification.
5. **A discovery-gap measurement**, motivated by row 5.
6. **Two competing mechanism accounts, tested against each other** (§4.2).

## 3. Design

Eight arms, differing *only* in the context material supplied. Identical
model, tools, sandbox, budget, retries and prompt; arms interleaved within
each (task, seed).

| Arm | Material | Isolates |
|---|---|---|
| A0 | nothing | reference |
| A1 | compiled, verified v2s skill, discovered by description | the product |
| A2 | skill built from a **frame-stripped** bundle | pixel grounding |
| A3 | same extraction, `verify` not run, badge stripped | execution verification |
| A4 | hand-written, grader-blind | the ceiling |
| A5 | real pipeline over a **different video**, same tool family | context length, provenance formatting, priming |
| A6 | the raw transcript, pasted | "why not just paste it?" |
| A7 | A1's skill, invoked explicitly | skill discovery |

A2 is a genuine control because the frames are *physically removed* from
the bundle it is compiled from — an instruction not to look at them would
not be a control.

Four tiers: **T1** purpose-built CLIs (zero contamination — the tools did
not exist before this benchmark); **T2** real pinned CLIs; **T3** GUI
applications under Xvfb; **T4** open-source games. Full specification,
admission criteria and integrity gates:
[docs/preregistration.md](docs/preregistration.md).

## 4. Hypotheses

### 4.1 Primary and ablation

| # | Claim | Contrast | Fails if |
|---|---|---|---|
| H1 | A v2s skill raises task success | A1 − A0 | CI includes 0 |
| H3 | Pixels beat narration | A1 − A2 | CI includes 0 |
| H4 | Execution verification contributes downstream | A1 − A3 | CI includes 0 |
| H5 | The benefit is not a length/priming artifact | A1 − A5 | placebo recovers most of the gain |
| H6 | A compiled skill beats pasting the transcript | A1 − A6 | CI includes 0 |
| H7 | v2s does not degrade safety | violation rate | A1 violates more than A0 |
| H8 | There is a discovery gap | A7 − A1 | CI includes 0 |

H1 is expected to be positive and is **not** the headline. H6 is the
comparison this work will be judged on, and row 3 and row 6 together
suggest it is a single-digit effect — smaller than the detectable effect of
any grid we can afford, which §5 addresses directly rather than hiding.

### 4.2 Two mechanisms, opposite predictions

The interesting disagreement in the literature is *why* skills help.

- **H2a — knowledge-gap account.** Skills supply facts the model lacks, so
  the effect concentrates where the cold probe score is low. This is the
  account v2s's own design implies: read the exact command off the pixels,
  because the model does not know it.
- **H2b — procedural-anchoring account.** Skills stabilise execution on
  long, noisy trajectories, largely independently of whether the model knew
  the facts. Row 4 puts anchoring at 65.7% of cases and knowledge injection
  at 4.5%, which predicts **H2a will largely fail**.

They are separated by stratifying the A1 − A0 effect on two axes measured
in advance: cold probe score (`probe_known`) and trajectory length/noise in
the baseline arm. H2a predicts the effect loads on the first; H2b on the
second. Secondary endpoints distinguish them further: anchoring should show
up as fewer turns, higher `first_try` and lower variance even where success
is unchanged.

Reported as **per-stratum effects with intervals, not as an interaction
test**. With three or four bands of a handful of tasks each, an interaction
statistic has no power worth quoting, and quoting one anyway would dress a
coin flip as an inference.

This matters for the project, not just the paper. If H2b holds, the
headline feature is less "we read the exact command off the pixels" and
more "we hand the agent a stable procedure" — and the roadmap implied by
that is different.

### 4.3 A caveat we can state now

Row 5 reports retrieval precision collapsing as the skill pool grows. Our
trials install **one** skill, the best possible case for ambient discovery.
So A1 is an **upper bound** on real-world discovery, where a user has
dozens of installed skills, and H8's gap should be read as a floor on the
real gap. We will say so next to the number.

## 5. Analysis plan

Fixed in advance; implemented in `harness/v2sbench/stats/`, standard
library only so that `v2sbench verify-results` can re-derive every
published figure without a scientific stack.

- **Unit of generalisation: the task**, not the trial. Clustered standard
  errors run up to ~3× the naive ones in this setting.
- **Primary test:** paired **sign-flip permutation** on per-task mean
  differences — exact for N ≤ 20, sampled above. Simulated power at
  N=100/K=5/15pp is 0.97 against 0.79 for McNemar on task majorities.
- **Robustness check:** exact McNemar on paired trial outcomes, reported
  alongside, never instead.
- **Interval:** cluster bootstrap over tasks, BCa, 10 000 resamples —
  **declared as optimistic** (row 14).
- Also reported: Wilson intervals per cell; normalised gain
  `(p_t − p_r)/(1 − p_r)`; estimated ICC; effective samples per trial
  `1/(1 + (K−1)·ICC)`.
- **Multiplicity:** Holm–Bonferroni across {H3, H4, H5, H6, H8}; H1 is not
  in the family; uncorrected values also shown.
- Difference-of-differences between ablations is reported as an estimate
  only — no p-value, no verdict.

**Sample size.** Variance is `σ_b²/N + σ_w²/(N·K)`; at fixed budget `B=N·K`
the second term is `σ_w²/B` and invariant to the split, so only N helps.
Target **N=100, K=3**; minimum defensible N=60, K=3. K=3 rather than K=1
because K=1 cannot detect a flaky task or estimate our own ICC.

The detectable effect at that grid depends on a modelling choice we are
explicit about. Our own simulator, assuming the treatment helps every task
by the same amount, gives ≈**7.0pp**. The literature-derived model, which
carries between-task *effect heterogeneity*, gives ≈**11.7pp**. We plan
against the pessimistic figure, because heterogeneity is not hypothetical
here: published per-configuration gains for the same intervention span
+4.1 to +25.7pp (§2, row 1). Both tables, the diagnosis of why they
disagree — and a test pinning the disagreement so it cannot be closed by
tuning the generator — are in
[the pre-registration §8.1](docs/preregistration.md).

All of it is conditional on assumed ICC and ρ, so **the first run is a
pilot** — 15 tasks × 10 trials × {A0, A1} ≈ 300 trials — after which the
table is regenerated from measured values and recorded as an amendment.

**Cost is a first-class result.** Row 6 measured +8.9pp at 61× token
overhead. A skill that helps slightly and costs 60× is a different product
decision from one that helps slightly and is free, so tokens and dated
dollars are reported per arm.

## 6. Results

_Empty by design. Nothing has been run._

### 6.1 Integrity gates

Verified on the tier-1 suite (12 tasks) as of 2026-11-03. These are
statements about the apparatus, not results.

| Gate | Status |
|---|---|
| Oracle passes on every task | **12/12** |
| Null agent scores zero on every checker | **0 tasks passed** |
| Checker absent from the workspace while the agent runs | **asserted by test** |
| Narration never speaks over itself | **13/13 recipes** |
| Hermetic videos render byte-identically (same machine, same TTS) | **verified** |
| Every declared arm can be recorded and validated | **asserted by test** |
| Published figures recompute from raw records | **verified** |
| No material contains grader vocabulary | pending materials |

The end-to-end chain has been exercised on the real suite with the stub
agent: 24 trials, a report correctly refusing to describe itself as a
measurement, and `verify-results` reproducing every figure.

### 6.2 Headroom

_Pending a full `v2sbench probe` sweep: per-task cold probe score, band,
and the saturation list._

One result from a two-task smoke test is worth recording now, because it
limits what the probe can do. Both tasks scored `probe_known = 0.00`
(0/3) — including the one we expect to be *easiest*, whose tool documents
its own trap in `--help`.

That is not a surprise, it is arithmetic: tier 1's tools were invented for
this benchmark, so no model can know their flags, and `probe_known` is
**≈0 by construction across the whole tier**. Two consequences:

1. The saturation exclusion rule (§5.4) can never fire in tier 1, and the
   headroom banding there is degenerate — every task is "high headroom".
2. What actually predicts tier-1 difficulty is `discoverability`: whether
   the procedure can be recovered from on-box documentation. That is a
   different axis from prior knowledge, and in tier 1 it is the only one
   that varies.

The probe earns its place in tiers 2–4, where the tools are real and the
model's knowledge is genuinely partial. Reporting a probe score of 0 for
every tier-1 task as though it were a measured finding would be
misleading; it is a property of the design.

### 6.3 Primary and ablation contrasts

_Pending. One row per hypothesis in §4.1, each with n, paired difference,
95% CI, permutation p, McNemar p, Holm-adjusted p, and a computed verdict._

### 6.4 Mechanism

_Pending. A1 − A0 stratified by probe score and by baseline trajectory
length, with the secondary stability endpoints, testing H2a against H2b._

## 7. What would change our minds

Declared in advance, so that a null is a result rather than a disappointment.

- If **H1 fails on T1** — the uncontaminated tier, where information flow
  is easiest to demonstrate — the central claim has failed, and the results
  page will say so in its first paragraph.
- If **H5 fails**, the measured benefit is not attributable to what the
  video taught.
- If **H3 and H6 both fail**, v2s's advantage is not distinguishable from
  transcript summarisation, and this benchmark does not support the
  pixel-reading claim in the project README.
- If **H4 fails**, execution verification does not measurably change
  downstream success — a negative result about one of the project's
  headline features.
- If the effect appears **only** where probe scores are low and feedback
  bandwidth is low, the honest conclusion is narrow: v2s pays off for
  unfamiliar, silently-failing tools, not in general.
- If **H2b beats H2a**, the project's stated rationale for reading pixels
  is not the reason its skills help.

## 8. Threats to validity

Fifteen, with mitigations and residual risk, in
[docs/preregistration.md §11](docs/preregistration.md). The four we regard
as most serious:

1. **We built the tool we are grading.** Mitigated by pre-registration,
   grader-blind materials, a real placebo, ablations designed so a null is
   visible, released raw records, and a third-party-runnable suite. A
   mitigation, not a refutation.
2. **Task selection.** The domains are ours. Admission criteria are fixed
   in advance, probes published, and no task is ever dropped for failing to
   separate the arms.
3. **Hermetic-video optimism.** Rendered screencasts are cleaner than real
   ones; the tiers are never averaged.
4. **Single-configuration over-reading.** Row 1 shows per-configuration
   gains spanning +4.1 to +25.7pp. A one-model result does not generalise,
   and v0.1 may ship with one model.

## 9. Reproduction

```bash
cd v2s-bench/harness && uv sync && python3 -m v2sbench.cli doctor
```

Every published figure recomputes from the committed raw records:

```bash
python3 -m v2sbench.cli verify-results
```

Silence means the arithmetic checks out; otherwise it prints each
disagreement. Full instructions: [README](README.md).

## References

Compiled 2026-11-03. Rows 1–5 of §2 were checked against their arXiv
abstracts on that date; the remainder come from a literature review and
**must be re-verified against primary sources before external
publication**. Identifiers, figures and venue attributions in the
unverified set may be wrong.

- SkillsBench: *Benchmarking How Well Agent Skills Work Across Diverse Tasks*. arXiv:2602.12670.
- *Demystifying Agent Skills: Why They Work — Until They Don't*. arXiv:2608.14036.
- Chacko et al. arXiv:2605.20023.
- BALROG. arXiv:2411.13543.
- *Benchmarking Agentic Capabilities* / verifier-error analysis. arXiv:2507.02825.
- Computer-use verdict audit. arXiv:2607.28367.
- Bowyer et al., *Position: Don't Use the CLT in LLM Evals*. ICML 2025. arXiv:2503.01747.
- Irrelevant-context effects. arXiv:2411.19463.
- SPRING; Read-and-Reward; VideoGUI (NeurIPS 2024); ARC-AGI-3; Epoch AI OSWorld analysis.
