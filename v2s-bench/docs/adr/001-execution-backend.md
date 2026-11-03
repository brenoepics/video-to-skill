# ADR 001 — Own the experiment, borrow the execution

**Status:** accepted for v0.1 · **Date:** 2026-11-03

## Context

v2s-bench needs to run an agent against containerised tasks, many times,
across arms. Several existing harnesses already do that, and rebuilding
sandbox orchestration badly is a real risk. A literature and tooling review
on 2026-11-03 surveyed the options.

Three candidates that appear in older write-ups are **no longer viable**:
METR's `vivaria` is archived, the Holistic Agent Leaderboard harness is
archived and closed to submissions, and Terminal-Bench's own `tb` runner is
superseded. Planning around any of them would be planning around
abandonware.

Of the live options:

| Option | What it gives | Concern |
|---|---|---|
| **Harbor** (Apache-2.0, successor to Terminal-Bench's runner) | Agent × model × task × sandbox matrix, attempts (`-k`), concurrency (`-n`), retries (`-r`), sandbox selection (`-e`), trajectory output, job locks | Whether it models *skills* as a per-agent field, and records their digest, is **unverified**: the landing and run-a-job pages do not show it. Adopting it for that reason would be adopting it on an unchecked claim. |
| **`inspect_ai`** (MIT) + `inspect_swe` | Mature scoring API, authoritative token accounting, model-agnostic, runs the real Claude Code CLI in-sandbox with version pinning | Its bridge withholds some tools and drops generation parameters by default, which reduces external validity for exactly the agent we are testing |
| **SkillsBench** (Apache-2.0) | Closest prior art: paired with/without-skill runs, per-task Docker, deterministic verifiers, and a runner that already ships `--skill-mode no-skill\|with-skill\|self-gen` and `--skills-dir` | Its task format and verifier conventions would become ours; and we need arms it does not have |
| **Build own** | Exactly our arms, gates and records | Re-implements sandboxing, retries and concurrency that others have debugged |
| `lm-evaluation-harness`, OpenAI `evals`/`simple-evals`, promptfoo | — | Wrong shape: no agent loop, no sandbox, or prompt-oriented |
| LangSmith, Braintrust, Weave | Hosted tracing and eval UX | Account-gated. An open benchmark a third party must sign up to re-run is not reproducible in the sense we need |

## Decision

**Keep the experiment layer; treat execution as replaceable.**

The harness in `harness/v2sbench/` owns what none of the candidates
provide and what this benchmark is actually about:

- the arm definitions and the pre-registered comparisons (`arms.py`)
- the integrity gates — grader-blind materials, the leakage gate, the
  oracle gate, the trivial-agent gate, checker-after-agent isolation
- the statistical analysis plan, standard library only, so published
  figures can be re-derived without a scientific stack
- the immutable run record and `verify-results`
- the refusal to aggregate mock with measured data

Execution — provisioning a container, running the agent, collecting a
trajectory — sits behind two narrow interfaces, `sandbox.Sandbox` and
`agent.AgentAdapter`, each about seventy lines. A Harbor, Inspect or
SkillsBench backend is an adapter against those, not a rewrite.

For v0.1 the shipped backends are `docker` (the only one valid for a
published figure) and `local` (development only, opt-in, recorded on every
trial).

## Why not adopt Harbor now

It is the strongest candidate and may well be the right answer. But the
specific property that would justify restructuring around it — skills as a
first-class per-agent field with digests recorded in a job lock — is
**unverified**. The honest move is to record that as the open question it
is, keep the interfaces narrow enough that adopting it stays cheap, and
check before committing.

**To settle it**, read Harbor's `claude-code` integration source and
confirm: (a) whether skills are installed into a discoverable location or
passed via `--add-dir`, and (b) whether it passes `--bare`. If it is
`--add-dir` plus `--bare`, every ambient-discovery arm run through it is
silently broken — see below — and the integration would have to be patched
before use, not merely configured.

## Two execution details that are not negotiable

Both were verified against the installed CLI (2.1.317) rather than taken
from documentation, because getting either wrong produces a confident wrong
number rather than an error.

**Never pass `--bare`.** It is the obvious choice for a hermetic run — it
skips hooks, plugin sync and CLAUDE.md discovery — but its own help text
says *"Skills still resolve via /skill-name."* Under `--bare` a skill is
present but undiscoverable from its description, so arm A1 collapses to the
baseline and the benchmark reports a null caused entirely by a harness
flag. Hermeticity comes from the container instead. The adapter refuses the
flag.

**Always set `--permission-mode` explicitly.** A `-p` run that does not set
one starts in `manual`, where anything needing approval is denied. Leave it
unset and the measurement is of the permission system.

A third, for cost: read token usage from `modelUsage`, never `usage`, which
omits subagent tokens and would therefore undercount precisely the arms
whose skills lead the agent to delegate — biasing the cost comparison
toward the treatment.

## Consequences

- Two backends to maintain, both thin.
- A third-party replication can swap in its own execution layer and still
  compute identical figures from the same records.
- If Harbor's skill handling checks out, adopting it is an adapter and a
  provenance field, not a migration.
- We carry the risk of our own sandbox bugs. The integration suite exercises
  the full path on every pull request to bound that.
