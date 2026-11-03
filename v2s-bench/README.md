<div align="center">

# v2s-bench

_Does a skill distilled from a tutorial video actually make an agent better at anything?_

**[Protocol](PAPER.md)** · [Pre-registration](docs/preregistration.md) · [Run it](#run-it) · [Design](#why-it-is-built-this-way) · [Status](#status)

</div>

The [video-to-skill](https://github.com/brenoepics/video-to-skill) project
already measures whether its pipeline reads a video *correctly*:
`vts-extract eval` scores boundary and step F1 and command recall against a
synthetic gold set. That is an **intrinsic** measure.

It has never measured whether the resulting skill makes an agent *better at
anything*, which is the claim the README makes. v2s-bench is that
**extrinsic** measurement: a pre-registered, paired experiment running an
agent over real tasks with and without a v2s-generated skill — plus the
ablations that separate "a skill helps" from "**this pipeline** helps".

## Status

> [!IMPORTANT]
> **No measurements exist yet.** The suite, harness, statistics and site are
> built and tested; no trials have been run. `v2sbench report` emits a
> bundle whose status is `no-results`, and the site renders an empty state
> rather than a chart. Any number you see in this repository today came from
> the stub agent, is stamped `is_mock`, and the report builder **refuses**
> to aggregate it with real data.

## Why it is built this way

Four design decisions do most of the work. Each exists because the obvious
alternative produces a confident, wrong answer.

**The headline comparison is not the interesting one.** "Agent with skill
beats agent without skill" is nearly tautological, and published work puts
it around +17pp. The comparison that decides whether *this pipeline* earns
its keep is skill versus **the same information presented rawer** — a
pasted transcript — published at single-digit points. The arm set and the
power analysis are built around the smaller number.

**The placebo is a real skill about the wrong thing.** Filler text is the
obvious length-matched control and the wrong one: topically-adjacent
irrelevant documents have been measured to *improve* task performance while
random text helps least. So arm A5 runs the real v2s pipeline on a
*different* video from the same tool family — genuine extraction, genuine
citations, genuine badge, wrong procedure.

**Tier 1 invents its own tools.** Benchmarking this on `git init` measures
saturation, not utility: every current model knows it cold, so the baseline
scores ~100% and nothing can move it. Tier 1 ships small, real, working
CLIs that did not exist before this benchmark, so the contamination
question has an actual answer.

**Exclusions are declared on a cold knowledge probe, never on observed
separation between arms.** Dropping tasks because the arms did not separate
is selection on the outcome and mechanically inflates the lift. Every task
is probed first — the key fact asked of the model with no tools — and the
primary endpoint is reported on the full suite *and* the probe-filtered
subset, full suite first.

## The arms

Every arm gets the same model, tools, sandbox, budget, retries and prompt.
**Only the context material differs.**

| | Arm | Material | Isolates |
|---|---|---|---|
| `A0` | baseline | nothing | reference |
| `A1` | v2s-full | the compiled, verified skill, discovered by description | the product |
| `A2` | transcript-only | a skill built from a **frame-stripped** bundle | pixel grounding |
| `A3` | v2s-unverified | same extraction, `verify` not run, badge stripped | execution verification |
| `A4` | human-expert | hand-written, grader-blind | the ceiling |
| `A5` | placebo | the real pipeline over the **wrong video** | context length + priming |
| `A6` | raw-transcript | the unprocessed transcript, pasted | "why not just paste it?" |
| `A7` | v2s-forced | A1's skill, invoked explicitly | skill discovery |

A2 is a real control because the frames are *physically removed* from the
bundle it is built from, not because the analysis was asked not to look.

## Run it

```bash
cd v2s-bench/harness && uv sync
```

```bash
python3 -m v2sbench.cli doctor
```

**If you are about to do a real run, read
[docs/running-a-pilot.md](docs/running-a-pilot.md) first** — it is written
for one attempt, lists what has actually been verified against what your
run will be the first test of, and the stages are checkpointed so a failure
never costs you the earlier ones.

Start with the free readiness check, which spends nothing:

```bash
python3 -m v2sbench.cli preflight --arms core --seeds 3 --model <full-model-id>
```

The pipeline, in order. Each step refuses to proceed on a missing
precondition rather than substituting something else:

```bash
python3 -m v2sbench.cli validate --all          # oracle, trivial-agent and leakage gates
```

```bash
python3 -m v2sbench.cli render                  # hermetic videos, from recipes
```

```bash
python3 -m v2sbench.cli build-materials --model <full-model-id>
```

```bash
python3 -m v2sbench.cli probe --model <full-model-id> --repeats 10
```

```bash
python3 -m v2sbench.cli run --agent claude-code --model <full-model-id> --arms core --seeds 3
```

```bash
python3 -m v2sbench.cli report && python3 -m v2sbench.cli verify-results
```

To exercise the whole pipeline for free, with no model calls, swap in the
stub agent — its records are stamped mock and can never become a figure:

```bash
python3 -m v2sbench.cli run --agent stub --sandbox local --allow-unsandboxed
```

### What it costs

Roughly **$75–220** for the pilot and **$700–2,100** for the
pre-registered target of 100 tasks × 8 arms × 3 seeds, on Sonnet-class
pricing — extrapolated from one recorded trial, with the derivation and
the two commands that would replace the estimate with a measurement in
[docs/cost.md](docs/cost.md). `--max-usd` caps any run, `--dry-run` spends
nothing, and the stub agent exercises the whole pipeline for free.

### The first real run should be a pilot

Sample size depends on the intra-task correlation and the correlation
between arms, and we do not yet know ours. So the first run is 15 tasks ×
10 trials × {A0, A1} ≈ 300 trials, after which `v2sbench power` regenerates
the sample-size table from the measured values and the result is recorded
as an amendment to the pre-registration.

```bash
python3 -m v2sbench.cli power --tasks 50 100 150 --seeds 1 3 5 10
```

## Integrity gates

Each is enforced in code and pinned by a test in
`harness/tests/test_pipeline_integration.py`.

| Gate | What it prevents |
|---|---|
| Checker copied in **after** the agent stops, outside the workspace | the agent reading the assertions and satisfying them directly |
| Every task ships an **oracle** that must pass | shipping a task nobody can pass, or a broken checker |
| A **null agent** must score zero on every checker | checkers satisfiable without doing the task |
| Materials gated against the **grader's own vocabulary** | a skill becoming a hidden answer key |
| Stub records stamped `is_mock`; mixing refused | fabricated numbers quietly becoming the headline |
| `verify-results` recomputes every published figure | numbers taken on trust |
| Harness, suite, model, image and material all content-hashed | a figure that cannot be attributed to what produced it |

## Layout

```
v2s-bench/
  PAPER.md            the protocol: question, design, analysis plan, threats
  docs/
    preregistration.md  frozen before any trial; amendments appended, never rewritten
  harness/            the runner, statistics and report builder (stdlib only)
  tasks/<tier>/<id>/  task.yaml + check/ + oracle/ (+ files/)
  tools/              the invented CLIs of tier 1
  videos/recipes/     tutorial videos as data; rendered, never committed
  env/                container definitions for the GUI and game tiers
  schemas/            task, run-record, recipe and report contracts
  results/            committed run records; the site reads results/report.json
  site/               the results website
```

## Honesty notes

- **Hermetic videos flatter the pipeline.** A rendered screencast has flat
  colours, clean glyphs, no compression artifacts and synthetic narration,
  so extraction quality on it *overstates* real screencasts. Hermetic and
  real video tiers are reported separately and **never averaged**.
- **Our intervals are optimistic.** At ~100 tasks, cluster bootstrap
  intervals under-cover in this design. It is stated next to the interval
  rather than discovered later.
- **We built the tool we are grading.** The answer to that is pre-registration,
  grader-blind materials, a real placebo, ablations designed so a null is
  visible, released raw records, and a suite a third party can run. It is a
  mitigation, not a refutation — independent replication is invited.

[MIT](../LICENSE) · part of [video-to-skill](https://github.com/brenoepics/video-to-skill) · **[v2s.dev](https://v2s.dev)**
