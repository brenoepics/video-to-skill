# Running the pilot

Written for the case where you get one attempt.

The path is staged, and **every stage is durable**: a failure never costs
you the stages before it. Build the images once, build the materials once,
and if the trial run then dies halfway you re-run only the trials — the
materials are on disk and `build-materials` skips what exists.

Read §6 before you start. It lists what has actually been verified and what
your run will be the first real test of.

## 0. What you need

| | |
|---|---|
| Docker daemon | required; there is no path to a valid figure without it |
| `claude` CLI | the agent under test |
| `ANTHROPIC_API_KEY` | for both the analysis stage and the trials |
| `cargo` | to build `vts-extract` from source |
| `bubblewrap` (Linux) | for `vts-extract verify`; without it A1 gets no badge |
| `espeak-ng` (Linux) | to narrate the hermetic videos |
| ~20GB disk | images and extraction bundles |

```bash
export ANTHROPIC_API_KEY=...
```

## 1. Build the extractor from source, not from a release

The extractor is **the system under test**. A tagged release grades code
that is not the code in your tree, which is almost never what you want:

```bash
cd /path/to/video-to-skill && cargo build --release
```

`v2sbench` prefers `target/release/vts-extract` over anything on `PATH`,
and records its version, git commit and whether the tree was dirty on
every trial. A dirty tree still runs; it is flagged, because a dirty build
cannot be attributed to any commit.

## 2. Build the images

```bash
cd v2s-bench
```

```bash
docker build -t v2sbench/t1-novel-cli:0.1 -f env/t1-novel-cli/Dockerfile .
```

```bash
docker build -t v2sbench/t2-real-cli:0.1 -f env/t2-real-cli/Dockerfile .
```

```bash
docker pull python:3.13-slim
```

The tier-4 Crafter image is only needed if you run that tier:
`docker build -t v2sbench/t4-game-crafter:0.1 -f env/crafter/Dockerfile .`

Both tier images run the agent as an **unprivileged user**, and that is not
hardening — six tier-1 tasks hide their tool's source with `chmod 0111`,
which does nothing against root. An image running the agent as root
silently turns every `discoverability: poor` task into a `good` one.
Preflight checks this.

## 3. Preflight — free, and the whole point

```bash
cd harness && python3 -m v2sbench.cli preflight --arms core --seeds 3 --model <full-model-id>
```

It spends nothing and checks the daemon, every declared image, the non-root
guarantee, the agent and model id, the extractor build, every arm material,
the leakage rules, disk, and the estimated cost. Fix everything it reports
before going further. A model **alias** (`sonnet`, `opus`) is refused: an
alias resolves to a different model over time, which makes the recorded
figure uninterpretable later.

## 4. Build the materials — costs money, and is resumable

```bash
python3 -m v2sbench.cli build-materials --model <full-model-id>
```

Roughly **$1–3 per task**, so $15–35 for tier 1 alone. This is the step
that runs v2s for real: render → extract → agent analysis over the
keyframes → compile → verify. It produces A1, A2 (from a bundle with the
frames physically deleted), A3 (badge stripped) and A6 per task, then
copies each task's declared sibling skill in as A5.

It is **resumable and isolated**: a task whose materials exist is skipped,
and one task failing does not stop the rest. If it dies at task 9 of 12,
re-run the same command — the eight already paid for are not paid for
again.

If `vts-extract verify` fails for want of a sandbox, install bubblewrap
rather than reaching for `--skip-verify`: without the badge, A1 and A3
differ only in badge text and H4 cannot be tested at all.

Then preflight again. It should now be clean:

```bash
python3 -m v2sbench.cli preflight --arms core --seeds 3 --model <full-model-id>
```

## 5. Probe, then run

The probes measure what the model already knows, and the exclusion rule
depends on them, so run them before the trials:

```bash
python3 -m v2sbench.cli probe --model <full-model-id> --repeats 10 --tier t1-novel-cli
```

Then the pilot. Tier 1 is the right first target: it is the uncontaminated
tier, so it is where a null result would actually mean something.

```bash
python3 -m v2sbench.cli run --agent claude-code --model <full-model-id> --tier t1-novel-cli --arms core --seeds 10 --max-usd 150
```

`--max-usd` is a hard cap checked between trials; the run stops there and
every record already written stays valid. Records stream to
`results/runs/<run_id>.jsonl` as they complete, so an interrupted run keeps
everything it finished.

```bash
python3 -m v2sbench.cli report && python3 -m v2sbench.cli verify-results
```

`verify-results` recomputes every published figure from the raw records and
prints any disagreement. Silence means the arithmetic checks out.

## 6. What is verified, and what your run will be the first test of

Stated plainly, because "it should work" is not the same claim.

**Verified in a real container:** the harness end to end against the real
26-task suite with the stub agent (sandbox → tools → material injection →
agent → checker → records → report → `verify-results`); all 26 oracles
passing and no checker satisfiable by a null agent; the hermetic renderer
producing byte-identical output with narration that never overlaps; the
`claude -p` adapter parsed against genuine CLI output including cache-token
accounting; the report builder's refusal to mix mock with measured data;
330 tests and a clean lint.

**Not verified, because this container has no Docker daemon and no built
extractor:**

- the two Dockerfiles have never been built
- `vts-extract extract | compile | verify` have never been run by the
  harness — the arguments are taken from `references/ir-format.md` and the
  project README, not from a successful invocation
- the agent-driven analysis stage has never produced a real
  `procedure.json`
- no trial has ever run under the Docker backend
- tier 2's pinned apt versions have never been resolved by a real build

So §2 and §4 are where a first run is most likely to break. Both are
cheap to retry and neither is destructive: the images rebuild, and
`build-materials` resumes.

**One task will fail for a known reason:** `crafter-first-pickaxe` needs
the Crafter image, and the two tier-2 tasks `ffmpeg-stack-dual-camera` and
`sqlite-attach-merge-ledgers` have no `placebo_source`, so A5 cannot be
built for them and H5 will report `insufficient-data` on those rows.

## 7. What the pilot is for

Not a verdict. The suite has 26 tasks against the ~100 the sample-size
analysis asks for, and the comparison this work will be judged on — a
compiled skill versus the same transcript pasted in — has a published
effect in the single digits, below what 26 tasks can resolve.

The pilot measures **our actual intra-task correlation and inter-arm
correlation**, which every sample-size figure currently depends on as an
assumption. Feed them back in:

```bash
python3 -m v2sbench.cli power --tasks 50 100 150 --seeds 1 3 5 10
```

and record the result as a dated amendment in
`docs/preregistration.md` §13. That is what turns the cost estimate in
`docs/cost.md` from an extrapolation into a plan.
