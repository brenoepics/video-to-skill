# What a run costs

Grounded on one real measurement plus trial counts from the harness. Every
number below is re-derivable: the counts come from
`v2sbench run --dry-run`, and the per-trial figure is extrapolated from a
recorded `claude -p` invocation, not from a price list.

**Confidence: moderate on the counts, low on the per-trial cost.** The
honest way to tighten it is two real trials (§5).

## 1. The one measurement we have

A real `claude -p --output-format stream-json` run on `claude-sonnet-5-5`,
2 turns, one trivial Bash command:

| | |
|---|---|
| reported cost | **$0.0463** |
| fresh input tokens | 4 |
| cache **read** input tokens | 62,096 |
| cache **creation** input tokens | 9,759 |
| output tokens | 107 |
| `costBasis` | `list` (a client-side estimate, not a billed figure) |

The shape matters more than the total. Almost the entire input is Claude
Code's own system prompt and tool definitions, cached and **re-read every
turn**. So a trial has a large per-turn floor that is independent of the
task, and output tokens are nearly free by comparison.

Per-turn marginal cost at minimal context: **≈ $0.023**.

## 2. Per-trial estimate

Context grows as tool output accumulates, so later turns read more than
early ones. Taking 1.5–2.5× growth over a trial:

| Turns used | What that looks like | Estimated cost |
|---|---|---|
| ~5 | finds the procedure quickly, or the skill told it | $0.12 – $0.18 |
| ~12 | explores, reads `--help`, verifies | $0.30 – $0.50 |
| ~20 (cap) | flails and never converges | $0.50 – $0.90 |

**Working figure: $0.25 – $0.75 per trial, midpoint ≈ $0.45** on Sonnet.

Two consequences worth noticing:

- **Failure costs more than success.** A trial that hits the turn cap is
  the expensive one, so a weak baseline arm is not only bad science, it is
  also the largest line item.
- **The skill arm may be cheaper per trial**, because a procedure that
  works shortens the trajectory. Cost per arm is a reported endpoint partly
  for this reason.

Opus is materially more expensive per token than Sonnet. Rather than quote
a multiple we have not measured, treat the Sonnet figures as the grounded
ones and re-measure before committing to a larger model.

## 3. Trial counts (from `v2sbench run --dry-run`)

At the suite as it stands (26 tasks):

| Run | Shape | Trials |
|---|---|---|
| Pilot | 12 tasks × 2 arms × 10 seeds | 240 |
| Core (A0 vs A1) | 26 × 2 × 3 | 156 |
| Packaging only (A1 vs A6) | 26 × 2 × 3 | 156 |
| Full ablation | 26 × 8 × 3 | 624 |
| Pre-registered target | 100 × 8 × 3 | 2,400 |

## 4. Totals

Trials are not the only cost. Materials are built once per task and reused
across every seed and arm, but the analysis stage is itself an agent run
that inspects **keyframes**, and images are expensive — roughly 1.1k tokens
per 720p frame, times however many the agent chooses to read, times the
full and transcript-only passes.

| Line item | Basis | Estimate |
|---|---|---|
| Materials, per task | ~3 agent runs, frame-heavy | $1 – $3 |
| Probes | 10 single-turn, no-tool calls per task | $0.05 – $0.15 per task |
| Trials | §2 × §3 | $0.25 – $0.75 each |

| Scenario | Total |
|---|---|
| **Pilot only** (12 T1 tasks: materials + probes + 240 trials) | **$75 – $220** |
| Core at 26 tasks (materials for 26 + 156 trials) | **$70 – $200** |
| Full ablation at 26 tasks | **$190 – $550** |
| Pre-registered target, 100 tasks × 8 arms × 3 seeds | **$700 – $2,100** |
| …replicated on a second model | roughly double |

A second model is not optional for a strong claim: published
per-configuration gains for this kind of intervention span +4.1 to +25.7pp,
so a single-configuration result does not generalise.

### Wall clock

A trial is capped at 600s and typically runs 2–6 minutes. 624 trials at 4
minutes is ~42 hours serial, or ~5 hours at 8-way parallelism. It needs a
machine with a Docker daemon; GitHub's standard runners are free for public
repositories and adequate for the CLI tiers (4 vCPU, ~14GB disk), but not
for a hypervisor-backed desktop tier, which is out of scope.

## 5. How to replace these estimates with measurements

```bash
cd v2s-bench/harness
```

```bash
python3 -m v2sbench.cli run --agent claude-code --model <full-model-id> --arms A0 --seeds 1 --task prow-interlock-sheet --max-usd 2
```

```bash
python3 -m v2sbench.cli run --agent claude-code --model <full-model-id> --arms A0 --seeds 1 --task zep-seal-with-grant --max-usd 2
```

Those two bracket the range deliberately: the first task's tool documents
its own trap in `--help`, so a competent baseline should finish in a few
turns; the second's does not, so the baseline should flail toward the cap.
Read `metrics.usd_cost` and `metrics.turns` out of the resulting records and
rescale §2.

## 6. Cost controls in the harness

- `--max-usd` is a hard cap checked between trials; the run stops and the
  records already written stay valid.
- `--dry-run` prints the schedule and spends nothing.
- `--arms packaging` runs only the comparison that decides the headline
  question, at a quarter of the full ablation's cost.
- The `stub` agent exercises the entire pipeline — sandbox, checker,
  records, statistics, site — for **$0**, which is how CI tests it.
- Paid runs are behind a manual dispatch and a protected environment in
  `.github/workflows/bench-run.yml`, so a branch push cannot start one.

## 7. The thing to spend on first

Not the full ablation. The comparison this work will be judged on (a
compiled skill versus the same transcript pasted in) has a published effect
in the single digits, which is below the minimum detectable effect of any
grid affordable at 26 tasks. Buying a 624-trial ablation now risks
purchasing a confidently underpowered answer.

The sequence that does not waste money is: **pilot → measure our actual
intra-task correlation and inter-arm correlation → regenerate the
sample-size table with `v2sbench power` → then decide what the real run
costs.** That is why the pilot is specified first, and it is the $75–220
line, not the $700–2,100 one.
