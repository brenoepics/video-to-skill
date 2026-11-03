# Pre-registration — v2s-bench v0.1

**Status:** DRAFT, not yet frozen. **Frozen on:** _(unset)_ · **Commit:** _(unset)_

Written and committed **before** any trial is run, and not edited after the
freeze commit except by appended, dated amendments (§13). Its purpose is to
make the result falsifiable: the hypotheses, the task set, the exclusion
rule, the primary endpoint and the statistical test are fixed while the
outcome is still unknown.

If you are reading a results page that does not link to a *frozen* copy of
this file, treat the numbers as exploratory.

---

## 1. What is being tested

[video-to-skill](https://github.com/brenoepics/video-to-skill) (v2s) turns a
tutorial video into an installable agent skill: a procedure with commands
read off screen pixels, per-step `[t=MM:SS]` provenance, and steps verified
by execution in an OS sandbox.

The repository already measures whether the *extraction* is faithful —
`vts-extract eval` scores boundary/step F1 and command recall against a
synthetic gold set (`BENCHMARK.md`). That is an **intrinsic** measure: it
asks whether the pipeline read the video correctly.

v2s-bench asks the **extrinsic** question, which nothing in the project
currently answers:

> Does an agent holding a v2s-generated skill complete real tasks more
> often than the same agent with a good prompt and no skill — and which
> component of v2s is responsible?

Being faithful is not the same as being useful. A pipeline can score well
on extraction and change no outcome, because the model already knew the
procedure, or because the environment corrects the agent anyway, or because
the skill is written in a way an agent does not act on.

## 2. What is already known, and what is actually new here

This is not an unexplored question, and the benchmark is designed around
what the adjacent literature already settles.

| Finding | Source | Consequence for this design |
|---|---|---|
| Skills vs. no skills on containerised tasks: 33.9% → 50.5% (+16.6pp), 87 tasks, 18 model×harness configs, 3 trials/cell | SkillsBench, arXiv:2602.12670 | Our H1 is *expected* to be positive. On its own it is close to uninformative, so it is not the headline. |
| Same source information, varying only the packaging: skill 61.9% / raw 59.1% / workflow-memory 55.9%, paired CI [+0.76, +11.36] | "Demystifying Agent Skills", arXiv:2608.14036 | The honest effect is **single-digit points**, not 17. This sets the sample size and makes A6 the comparison that matters. |
| A published **null**: +8.9pp, 95% CI [−6.8, +24.6], p=0.71, at 61× token overhead. "Feedback-bandwidth hypothesis": skills matter less when the environment already corrects the agent | Chacko et al., arXiv:2605.20023 | We stratify by **feedback bandwidth** (§5.2) and pre-commit to publishing a null (§12). |
| Nobody controls for context length; SkillsBench names length-matched baselines as its own top limitation | arXiv:2602.12670 §limitations | The placebo arm (§3, A5) is our cheapest novel contribution. |
| Topically-adjacent *irrelevant* documents can **improve** performance as a "magic word", while random text helps least | arXiv:2411.19463 | A random-noise placebo is the *wrong* placebo. Ours runs the real pipeline on the wrong video (§3.3). |
| Compact skills +18.9pp vs comprehensive documentation +5.7pp; 2–3 skills +20.0pp vs 4+ skills +5.2pp | arXiv:2602.12670 | Skill length and step count are recorded as covariates and tested as moderators, not assumed away. |
| LLMs answer NetHack quiz questions correctly and then act wrongly in-game — a "knowing–doing gap" | BALROG, arXiv:2411.13543 App. F.5 | Knowing the fact and doing the procedure are different measurements, so we measure both (§5). |
| Feeding a model the *paper* describing an environment beat every RL baseline with zero interaction (Crafter 27.3 vs 14.5) | SPRING; cf. Read-and-Reward for Atari manuals | A known-large effect we can calibrate the harness against (§4.3). |
| Oracle plans lift GPI-4o from 20.0% → 50.0% on tasks built from YouTube tutorials | VideoGUI (NeurIPS 2024) Tab. 8 | An external estimate of the ceiling for a video-derived procedure (our A4). |
| Harness changes swung the same model from 62.7% → 99.9% on the same tasks | ARC-AGI-3 | The harness is content-hashed and pinned exactly like the model (§9.2). |
| 10–15% of tasks in borrowed agent benchmarks are defective; 15.3% of FAIL verdicts across five computer-use benchmarks were wrong; verifier errors reach 100% relative (suites that pass empty responses, trivial agents scoring 38%) | Epoch AI; arXiv:2607.28367; ABC, arXiv:2507.02825 | Every task ships an **oracle** that must pass and a **trivial-agent** gate that must fail (§6). |
| Per-config gains span +4.1 to +25.7pp across model×harness configs | arXiv:2602.12670 | A single-config result does not generalise; multi-model replication is in scope (§8.4). |

**What is new here**, then, is not "skills help". It is:

1. A **length-and-pipeline-matched placebo** — the first control for the
   "any extra context helps" confound that the closest prior work names as
   its own chief limitation.
2. A **video-specific ablation**: pixels vs. narration (A2), which no
   general skill benchmark can run because no other system reads commands
   off frames.
3. An **execution-verification ablation** (A3) — testing whether a
   verification badge changes downstream behaviour, not just confidence.
4. **Headroom calibration** by direct knowledge probes, turning the
   knowing–doing gap into a declared stratification instead of a confound.
5. A **discovery gap** measurement (A7): whether a skill is even triggered.

## 3. Arms

All arms share the same model, tool access, sandbox, turn/time budget,
retry policy and task prompt. **The only difference is the context
material supplied.** Arms run paired: every (task, seed) is attempted by
every arm in the run's arm set, and arm order is **interleaved** within a
seed (§8.3).

| id | name | Context supplied | Isolates |
|---|---|---|---|
| `A0` | baseline | Nothing beyond the task prompt | — (reference) |
| `A1` | v2s-full | The compiled, executed-and-verified v2s skill, discovered from its description | the product as shipped |
| `A2` | transcript-only | A skill from the same pipeline with frame inspection withheld — ASR only | pixel grounding |
| `A3` | v2s-unverified | The same extraction with `verify`/repair not run and the badge stripped | execution verification |
| `A4` | human-expert | A grader-blind, hand-written skill from the same video | the ceiling |
| `A5` | placebo | The real v2s pipeline run on a *different* video from the same tool family | context length, provenance formatting, and "a procedure exists" priming |
| `A6` | raw-transcript | The raw ASR transcript pasted into context | "why not just paste the transcript?" |
| `A7` | v2s-forced | A1's skill, invoked explicitly instead of discovered | skill discovery |

### 3.1 The baseline must be strong

A weak baseline is the standard way to manufacture a benchmark win. Three
rules, enforced in `harness/v2sbench/arms.py` and the runner:

1. **Same tools.** Every arm may read `--help`, `man`, READMEs and the
   filesystem, and may run exploratory commands. The baseline is allowed to
   *discover* the procedure. We do not win by blindfolding it.
2. **A real prompt**, committed at `tasks/_prompts/baseline.md`, identical
   across arms, instructing the agent to inspect available documentation
   before acting.
3. **Matched budget and retries.** Identical `max_turns`, wall-clock and
   attempt counts. Retrying alone raises measured accuracy, so an arm with
   more attempts is not a comparison.

### 3.2 Arm-material authoring is grader-blind

Skills for `A1`–`A5` are produced by a process that **cannot read the
task's checker**. Enforced structurally:

- Checkers live in `tasks/<id>/check/`, oracles in `tasks/<id>/oracle/`,
  materials in `artifacts/skills/<id>/`.
- Skill generation runs with `check/` and `oracle/` absent from its
  workspace.
- `v2sbench validate --blind` rejects any material containing a declared
  forbidden literal, a checker/oracle filename, or a mined string literal
  from either (`harness/v2sbench/materials.py`). Without this rule a skill
  stops being a procedure and becomes a hidden answer key.
- `A4`'s author records the blindness claim, with name and date, in
  `artifacts/skills/<id>/human-skill/AUTHORING.md`.

### 3.3 Why the placebo is a real skill about the wrong thing

A length-matched block of random or irrelevant text is the obvious
placebo and the wrong one: topically-adjacent irrelevant documents have
been measured to *improve* task performance, while random text helps least,
so random noise would understate the confound it is supposed to bound.

`A5` therefore runs the **real v2s pipeline on a different video from the
same tool family** — genuine extraction, genuine `[t=MM:SS]` citations, a
genuine verification badge, a procedure that does not solve this task. It
matches A1 on length, register, formatting and provenance decoration, and
differs only in whether the content is the right content. If A5 recovers
most of A1's gain over A0, the effect was never about what the video
taught.

## 4. Task suite

Four tiers, easiest to hardest. Each task is a directory with `task.yaml`,
a `check/` directory of execution-based assertions, an `oracle/` solution,
and a declared **knowledge gap**.

| Tier | Name | Environment | Why it is here | n (target) |
|---|---|---|---|---|
| **T1** | novel-cli | Purpose-built CLI tools shipped here, Docker | **Zero contamination** — the tools did not exist before this benchmark. Measures whether information can flow video → skill → action at all. | 16 |
| **T2** | real-cli | Real pinned open-source CLIs/TUIs, Docker | External validity on tools people use, where the model has partial knowledge. | 14 |
| **T3** | gui | Real GUI apps under Xvfb in Docker | The regime v2s targets — screencasts of graphical apps. | 8 |
| **T4** | game | Open-source games, headless, seeded | Long-horizon and knowledge-dense, where tutorial video is abundant. | 4 |

Counts are targets, frozen at the freeze commit. §8.1 sets the sample size
the primary endpoint actually requires, which is larger than this; v0.1
publishes a pilot and states its power honestly rather than pretending a
42-task suite settles a 5pp question.

### 4.1 Tier 1 is the internal-validity anchor

Benchmarking v2s on `git init` or `:q!` measures nothing: every current
model knows those cold, the baseline scores ~100%, and no skill can move
it. So T1 invents its own tools — small, real, working CLIs installed in
the task container with deliberately non-guessable interfaces (a token read
from a file, an argument order that matters, a two-phase commit, a
subcommand whose obvious name does not exist). A screencast of a real
session is rendered deterministically (§7.1) and fed to v2s. The agent
drives the real tool; only the tool's provenance is synthetic.

Half the T1 tools ship **good `--help`** and half ship **poor or terse
documentation**, declared as `discoverability: good|poor`. We expect v2s to
matter little in the first group and more in the second, and that split is
more useful to someone deciding whether to run v2s than any aggregate.

### 4.2 Environments are Docker-only, by necessity

The two best-known desktop suites boot a QEMU VM *inside* the container and
silently fall back to software emulation when `/dev/kvm` is absent — they
"work" and are uselessly slow. Standard CI runners offer no reliable nested
virtualisation and ~14GB of disk. So T3 is container-native: real
applications under Xvfb, plus browser tasks driven through Playwright. A
hypervisor-backed desktop tier is explicitly out of scope for v0.1 rather
than attempted badly.

### 4.3 A calibration rung

Published work fed a model the *paper* describing an environment and beat
every reinforcement-learning baseline with no interaction at all — a large,
independently reported effect of documentation on agent behaviour. One T4
task reproduces that setup as a **harness calibration check**: if our
harness cannot recover a known-large effect, our null results are not
evidence of anything. It is reported as a harness diagnostic, never as a
v2s result.

## 5. Headroom, and the knowing–doing gap

### 5.1 Probes

Each task's knowledge gap is probed directly before the main run: the key
fact is asked of the model as a plain question, 10 times, no tools, no
skill (`v2sbench probe`). The pass rate is `probe_known`.

Tasks are stratified into **headroom bands** (high / medium / low) from
`probe_known`, and H2 is tested across bands. Publishing the probes is also
the honest disclosure that *any* skill system's effect size depends on what
the model already knew — and that a benchmark can be made to show almost
any effect by choosing tasks on that axis.

The probe is also a direct measurement of the knowing–doing gap: a task
with high `probe_known` and low baseline success is one the model *can
state* and *cannot do*.

### 5.2 Feedback bandwidth

The published null attributes its result to the environment already
correcting the agent: when a tool emits a clear error, trial-and-error
substitutes for instruction. Each task therefore declares
`feedback_bandwidth: high|low` — whether a wrong attempt produces a
diagnostic error (high) or silently produces a plausible-looking wrong
result (low). We expect the effect to concentrate in `low`, and we test it.

### 5.2a Two accounts of why a skill would help

Published work that opens up the mechanism reports **procedural anchoring
in 65.7% of skill-use cases against explicit knowledge injection in 4.5%**
— that is, skills mostly help by stabilising noisy execution, not by
supplying missing facts. That directly contradicts the account v2s's own
design implies ("read the exact command off the pixels, because the model
does not know it"), and it predicts **H2a will largely fail**.

Both accounts are therefore pre-registered as competing hypotheses (§2,
H2a/H2b) and stratified on axes fixed before any arm runs: the cold probe
score, and baseline trajectory length. The secondary endpoints separate
them further — anchoring should show as fewer turns, higher `first_try` and
lower variance even where success is unchanged.

This is the result most likely to be useful to the project regardless of
sign: if anchoring dominates, the headline feature is the stable procedure
rather than the pixel-exact command, and the roadmap that follows differs.

### 5.2b One installed skill is the best case for discovery

The same work reports retrieval precision falling from **29.6% to 3.3%** as
the skill pool grows from 5 to 100 skills. Our trials install exactly one
skill, so A1 measures ambient discovery under the most favourable possible
conditions, and H8's measured gap is a **floor** on the gap a user with
dozens of installed skills would see. Every A1 and H8 figure carries that
caveat.

### 5.3 Exclusion is declared on the probe, never on the outcome

The closest prior work excludes "tasks with no measurable separation
between conditions as low-signal." **That is selection on the outcome and
it mechanically inflates the reported lift** — plausibly part of why
+16.6pp over-predicts a later null of +8.9pp.

So: the only pre-declared exclusion is `probe_known ≥ 0.9` **and**
`feedback_bandwidth = high`, both measured before any arm is run, and the
primary endpoint is reported **twice — on the full suite and on the
probe-filtered subset**, with the full-suite figure given first. No task is
ever dropped for failing to separate the arms.

## 6. Integrity gates

Run by CI before any trial, and published with the results.

1. **Oracle gate.** Every task ships `oracle/` — a known-good solution
   driving the real tool. It must pass the checker in a fresh environment.
   This catches defective tasks and discharges "the task is achievable".
   An oracle that writes the expected end state directly instead of
   performing the procedure fails review.
2. **Trivial-agent gate.** A null agent (no material, no actions) is run
   against every checker and **must score 0**. Published suites have been
   found to pass empty responses and to score trivial agents at 38%; this
   gate is three lines of harness and refutes that class of error directly.
3. **Leakage gate.** §3.2.
4. **Modality gate.** On `gui_only` tasks, a trial whose actions were
   entirely shell is flagged: roughly 15% of tasks in published GUI suites
   are silently solvable from a terminal, so a skill saying "just run this
   command" can manufacture a large, meaningless lift.
5. **Determinism gate.** Hermetic videos render byte-identically; the task
   suite and harness are content-hashed.

## 7. Materials

### 7.1 Videos: two tiers, with the trade-off stated

**Hermetic** (`videos/recipes/*.json`) — rendered deterministically at run
time by `v2sbench render`: a real terminal session, dialog sequence or
walkthrough, drawn frame by frame and narrated by TTS. Nothing binary is
committed. *Strength:* perfect reproducibility, no licensing question, no
contamination. *Weakness:* flat colours, clean fonts, no compression
artifacts, no cursor jitter, synthetic narration — so extraction quality on
this tier **overstates** real-world performance. It bounds the mechanism,
not the product.

**Real** (`videos/manifest.yaml`) — Creative-Commons or public-domain
screencasts, referenced by URL + `sha256` + licence, never redistributed.
*Strength:* external validity. *Weakness:* reproducible only while the URL
resolves; a hash mismatch invalidates the row and the harness refuses to
score it.

Every reported number states its video tier. **They are never averaged.**

### 7.2 Narration must be vague where the pixels are exact

In every hermetic recipe the narration is written the way a narrator really
speaks — "now we claim the slot and pass the token along" — while the exact
syntax appears only on screen. This asymmetry is what makes A2 and A6 fair
controls rather than strawmen. If narration alone suffices, A2/A6 match A1
and the benchmark's most important comparison collapses to zero by
construction.

### 7.3 Licensing

No video bytes, no game ROMs, no copyrighted assets in this repository.
Real videos are CC-BY/CC0/public-domain only, licence recorded per entry
and attributed on the results site. Games are free/open-source only, assets
fetched upstream at build time; commercial ROMs are categorically excluded.
Adapted third-party task definitions carry their upstream licence.

## 8. Measurement and analysis

### 8.1 Primary endpoint and sample size

**Task success**, binary, decided by execution-based assertions. A trial
succeeds only if every required assertion passes.

Variance decomposes as `Var(μ̂) = σ_b²/N + σ_w²/(N·K)` for N tasks and K
trials each. Fixing the budget `B = N·K` makes the second term `σ_w²/B`,
invariant to how the budget is split: **only N reduces the first term**. At
B = 300 runs per arm, N=300/K=1 gives a 9.4pp MDE against N=30/K=10 at
17.7pp. We nonetheless run **K=3**, because K=1 cannot detect a flaky task,
cannot estimate our own intra-task correlation, and cannot report pass^k.

Minimum detectable effect (α=.05, 80% power, .35→.50, ICC=.5, ρ=.6), in
percentage points:

| N tasks | K=1 | K=3 | K=5 | K=10 |
|---|---|---|---|---|
| 50 | 22.9 | 16.6 | 15.0 | 13.7 |
| 75 | 18.7 | 13.5 | 12.3 | 11.2 |
| **100** | 16.2 | **11.7** | 10.6 | 9.7 |
| 150 | 13.2 | 9.6 | 8.7 | 7.9 |

**Target: N=100, K=3 (MDE ≈ 11.7pp). Minimum defensible: N=60, K=3.**

These numbers are conditional on assumed ICC and ρ, so the **first run is a
pilot**: 15 tasks × 10 trials × {A0, A1} = 300 runs, to measure our actual
ICC and inter-arm correlation, after which this table is regenerated by
`v2sbench power` and the amendment recorded in §13.

The honest difficulty: the comparison we will be judged on (A1 vs A6) has a
published effect of single-digit points, below the MDE of any grid we can
afford. Two mitigations, both declared now: contrasts between two *skill*
arms are better powered than A1-vs-A0 because ρ between similar arms is
higher; and where power is insufficient we report an **estimate with its
interval and say so**, rather than a verdict.

### 8.2 Tests and intervals

- **Unit of generalisation: the task, not the trial.** Treating correlated
  trials as independent is the most common way agent benchmarks report
  intervals that are far too narrow (clustered standard errors run up to
  ~3× the naive ones).
- **Primary test:** paired **sign-flip permutation** on per-task mean
  differences — exact for N ≤ 20, sampled above. Simulated power at
  N=100/K=5/15pp is 0.97 against 0.79 for McNemar on task majorities;
  binarising discards the within-task gradient.
- **Robustness check:** exact McNemar on paired trial outcomes. Reported
  alongside, never instead.
- **Interval:** difference in success rate by **cluster bootstrap over
  tasks** (10 000 resamples, BCa). Declared limitation: at N≈100 both CLT
  and plain bootstrap intervals **under-cover** in this clustered setting,
  so the published interval is optimistic; this is stated next to it rather
  than discovered later.
- **Per-task and per-cell rates:** Wilson score intervals, matching
  `vts-extract eval`.
- **Also reported:** normalised gain `(p_treat − p_ref)/(1 − p_ref)`, which
  keeps a ceiling effect from reading as a weak result; estimated ICC; and
  effective samples per trial `1/(1 + (K−1)·ICC)`, because K=5 at ICC 0.83
  is worth about 1.15 independent trials and that should be visible.
- **Multiplicity:** Holm–Bonferroni across the ablation family (H3, H4, H5,
  H6, H8); H1 is not in the family. Uncorrected values also shown.
- **Difference-of-differences** between ablations carries a √2 penalty and
  is reported as an estimate only — no p-value, no verdict.
- **Stopping rule:** N fixed in advance. No peeking-and-extending; any
  extension is an amendment with its own analysis.
- **Exclusions:** §5.3 only, plus trials whose harness or container errored
  (recorded `error`, reported as a rate per arm, never silently dropped and
  never counted as failures).

### 8.3 Maximising the paired correlation

ρ is the highest-leverage knob in the power model, so: identical container
digests across arms, byte-identical task prompts, arms **time-interleaved**
within each (task, seed) rather than run as separate campaigns, and the
same adapter version throughout. A run that cannot hold these constant is
reported as unpaired.

### 8.4 Secondary endpoints

`turns`, `wall_secs`, `tokens_in`, `tokens_out`, `usd_cost` (tokens **and**
dated dollars, because prices move), `first_try`, `key_action_hit`
(procedure fidelity, independent of success), `hallucinated_cmd_rate`,
`policy_violations` (H7), `skill_cited`, `action_modality`, `skill_steps`
and material length (compactness moderators), plus `score`,
`milestones_reached` and `depth` on T4.

Cost is a first-class result, not an appendix: a published null measured
+8.9pp at **61× token overhead**, and a long demonstration can cost
hundreds of thousands of tokens against ~2k for a skill. A skill that helps
slightly and costs 60× is a different product decision from one that helps
slightly and is free.

### 8.5 On LLM judges

The primary endpoint never uses a judge. Judges are confined to two
labelled secondary quantities (`key_action_hit` on GUI traces where regex
is inadequate, and a failure-mode taxonomy). Any judge in use must publish
≥100 human-labelled comparisons with raw agreement, Cohen's κ, a
prevalence-adjusted coefficient, the confusion matrix and the human–human
ceiling, plus position-swapped and cross-family replication — because
temperature-0 judges still flip ~5% of items and ~40% of close calls.

## 9. Recording and reproducibility

### 9.1 Per-trial records

One immutable JSON record per trial (`schemas/run-record.json`): task id +
suite version, arm, seed, model id and sampling params, harness version and
digest, container image digest, video tier + `sha256`, material kind +
`sha256` + leakage verdict, the executed-command log, per-assertion
results, every metric above, and timestamps. Append-only.
`v2sbench verify-results` recomputes every published figure from the raw
records, so a reader can audit the arithmetic.

### 9.2 Pinning

Model ids (never aliases — an alias resolves to a different model over time
and makes an old figure uninterpretable), sampling parameters, agent
adapter version captured *inside* the container, harness content digest,
task-suite digest, container digests, and the frozen pre-registration
commit. A figure without this block is not published. Re-running a frozen
suite on a new model is a new row, never an edit to an old one.

## 10. Mock data can never become a published figure

The harness ships a stub agent that calls no model, so the whole pipeline
can be tested for free. Every record it writes is stamped `is_mock`, the
report builder **refuses** to aggregate mock and real records, and the
results site renders mock bundles with a non-removable watermark and no
figures. The failure mode this guards against — fabricated numbers quietly
becoming the headline — is the worst thing this project could do.

## 11. Threats to validity

| Threat | Mitigation | Residual risk |
|---|---|---|
| **We built the tool we are grading** | Pre-registration; grader-blind materials; real-pipeline placebo; ablations designed so a null is visible; raw records released; suite runnable by third parties | Non-zero. Independent replication invited |
| **Task selection bias** | Admission criteria fixed in advance; probes published; discoverability and feedback-bandwidth splits reported; T1 tools specified before their videos existed; **no outcome-based exclusion** (§5.3) | Choice of domains remains ours |
| **Contamination** | T1 tools novel by construction; probe scores published | T2–T4 contaminated to an unknown degree; probes bound it |
| **Weak-baseline inflation** | §3.1, enforced in code: identical tools, budgets, retries | Prompt quality is a judgement call; the prompt is committed |
| **Context-length / priming confound** | Real-pipeline placebo matched on length, register and provenance formatting (§3.3) | Cannot match topical relevance; A5 is a bound, not an exact control |
| **Grader leakage** | Structural blinding + literal-mining gate | A sufficiently indirect leak could pass a string check |
| **Defective tasks and verifiers** | Oracle gate, trivial-agent gate, modality gate (§6) | Our own checkers are as fallible as anyone's |
| **Synthetic-video optimism** | Two tiers, never averaged; weakness stated (§7.1) | Hermetic numbers will look better than reality |
| **Under-covering intervals** | Stated explicitly next to the interval (§8.2) | The published CI is optimistic at N≈100 |
| **Single-config over-reading** | Model id on every figure; multi-model replication in scope | v0.1 may ship one model's results; published per-config spread is +4.1 to +25.7pp |
| **Harness dominance** | Harness content-hashed and pinned | A harness improvement mid-series would invalidate comparability |
| **LLM nondeterminism** | K≥3 seeds; sampling params pinned and recorded; variance reported | Cannot be eliminated at temperature 0 |
| **Flaky environments** | `error` status, never failure; per-arm error rate published | T3/T4 will be flakier than T1/T2 |
| **Cost pressure → small N** | N and MDE declared up front; pilot first; tiered run plan | Wide intervals on T3/T4 |
| **GUI bypass** | Modality recorded and gated on `gui_only` tasks | Classification by command inspection is imperfect |

## 12. What would make us say v2s is not useful

Committed in advance.

- If H1's interval includes 0 **on T1** — the uncontaminated tier where
  information flow is easiest to demonstrate — the central claim has
  failed, and the results page says so in its first paragraph.
- If H1 holds but **H5 fails** (the placebo recovers most of the gain), we
  report that the measured benefit is not attributable to what the video
  taught.
- If H1 holds but **H3 and H6 both fail**, we report that v2s's advantage
  is not distinguishable from transcript summarisation, and that this
  benchmark does not support the pixel-reading claim in the project README.
- If **H4 fails**, we report that execution verification, as implemented,
  does not measurably change downstream task success — even though it is
  one of the project's headline features.
- If the effect appears only where `probe_known` is low and
  `feedback_bandwidth` is low, we report that as the finding: v2s pays off
  for unfamiliar, silently-failing tools and not in general. A published
  null with that framing already exists; the most believable version of
  this study may look like it.
- If **H2b beats H2a**, we report that the project's stated rationale for
  reading commands off pixels is not the reason its skills help — the value
  is in handing the agent a stable procedure, which a transcript-derived
  skill might supply just as well. That would be a negative result about
  the project's central design argument, and it is the outcome the adjacent
  literature currently predicts.

## 13. Amendments

Dated, appended, never rewritten. Each records what changed, why, and
whether any data had been seen at the time.

_(none yet)_
