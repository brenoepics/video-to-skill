# Prompt for a local Claude session

Copy everything in the block below into a fresh Claude Code session on a
machine with Docker. It is written to stand alone — that session has none
of the context this one had.

---

```text
I want you to execute one pilot run of v2s-bench, a benchmark that measures
whether a skill distilled from a tutorial video makes an AI agent better at
real tasks. I get effectively one attempt at the paid parts, so being
careful matters more than being fast.

REPO (private): https://github.com/brenoepics/v2s-bench
Clone it, then read these three files before doing anything else:
  - docs/running-a-pilot.md   <- the runbook; it is authoritative, follow it
  - docs/preregistration.md   <- the frozen design; do not change it
  - docs/cost.md              <- what this should cost

THE SHAPE OF THE RUN
  - Agent under test: GPT-6.1 via the Codex CLI (`codex exec`).
  - Materials (the skills themselves) are built with Claude, not GPT. Those
    are two different roles on purpose: "v2s makes a good skill" and "the
    skill helps this agent" are separate claims, and cross-model transfer
    is the stronger result.
  - Target: tier 1 only (12 tasks, purpose-built CLIs). It is the
    uncontaminated tier, so a null result there would actually mean
    something.

WHAT I HAVE
  - Docker running
  - `codex` CLI installed, authenticated for GPT-6.1
  - `claude` CLI installed (for building materials)
  - API keys available as env vars
  - cargo, and the video-to-skill source for building vts-extract

THE STAGES. Each is durable — a failure never costs the earlier ones. Do
them in order and stop at the first thing that looks wrong.

  1. Build vts-extract FROM SOURCE, not from a release:
       cd /path/to/video-to-skill && cargo build --release
     The extractor is the thing being graded, so a tagged release would
     measure code that isn't the code in the tree. v2sbench prefers
     target/release/vts-extract automatically and records its version, git
     commit and dirty state on every trial.

  2. Build the two images (from the v2s-bench root):
       docker build -t v2sbench/t1-novel-cli:0.1 -f env/t1-novel-cli/Dockerfile .
       docker build -t v2sbench/t2-real-cli:0.1  -f env/t2-real-cli/Dockerfile .

  3. Preflight. It is free and checks everything:
       cd harness
       python3 -m v2sbench.cli preflight --agent codex --model <GPT-6.1-MODEL-ID> \
         --tier t1-novel-cli --arms core --seeds 10
     Fix everything it reports before spending anything.

  4. Build materials (costs ~$15-35, resumable, durable):
       python3 -m v2sbench.cli build-materials --model <CLAUDE-MODEL-ID> --tier t1-novel-cli
     If it dies partway, just re-run it — tasks already built are skipped.
     If `vts-extract verify` fails for want of a sandbox, install bubblewrap
     rather than using --skip-verify: without the badge, arms A1 and A3
     differ only in badge text and hypothesis H4 cannot be tested.

  5. Preflight again — it should now be clean.

  6. SMOKE TEST FIRST, before the real run. This is important (see below):
       python3 -m v2sbench.cli run --agent codex --model <GPT-6.1-MODEL-ID> \
         --task zep-seal-with-grant --arms A0 --seeds 1 --max-usd 2
     Then show me the resulting record from results/runs/. I want to see
     sane values for `turns`, `tokens_in` and the command log before we
     commit to the full run. If tokens_in is implausibly small, or the
     command log is empty, STOP and tell me — the adapter needs fixing.

  7. Probes, then the pilot:
       python3 -m v2sbench.cli probe --agent codex --model <GPT-6.1-MODEL-ID> \
         --repeats 10 --tier t1-novel-cli
       python3 -m v2sbench.cli run --agent codex --model <GPT-6.1-MODEL-ID> \
         --tier t1-novel-cli --arms core --seeds 10 --max-usd 150

  8. Report and audit:
       python3 -m v2sbench.cli report
       python3 -m v2sbench.cli verify-results
     verify-results recomputes every published figure from the raw records.
     Silence means the arithmetic checks out.

THINGS THAT ARE NOT VERIFIED — be suspicious here
The harness was built in a container with no Docker daemon and no built
extractor, so these have never actually run:
  - the two Dockerfiles have never been built
  - vts-extract extract/compile/verify have never been invoked by this
    harness; the arguments come from the docs, not a successful call
  - the agent analysis stage has never produced a real procedure.json
  - no trial has ever run under the Docker backend
  - THE CODEX ADAPTER HAS NEVER BEEN RUN AGAINST THE REAL BINARY. It was
    written from OpenAI's published interface. That is why step 6 exists.
Stages 2, 4 and 6 are where a first run is most likely to break. All are
cheap to retry and none are destructive.

TRAPS THAT HAVE ALREADY BITTEN — do not reintroduce these
  - Never pass `--bare` to the claude CLI. Its own help says skills then
    resolve only via /name, so a skill becomes present-but-undiscoverable
    and the treatment arm silently collapses to the baseline.
  - Cached tokens are still input tokens. Both adapters sum the cache
    fields; if you touch token accounting, keep it that way, or longer-
    prompt arms will look artificially cheap.
  - Under Codex, AGENTS.md is always concatenated into the prompt, so
    inlining a skill there is the FORCED arm (A7), not the ambient one
    (A1). The adapter already handles this. Don't "simplify" it.
  - Codex defaults to a read-only sandbox; trials must use
    workspace-write, which the adapter sets.
  - `leakage_forbidden` entries must name the GRADER's vocabulary only.
    An entry naming part of the procedure refuses every honest skill and
    silently deletes treatment cells. `validate --blind` enforces this.

HOW I WANT YOU TO WORK
  - Do not fabricate or estimate any result. If something fails, say so
    with the actual output.
  - Do not edit docs/preregistration.md to match what happened. If reality
    diverges from the design, append a dated amendment in section 13
    saying what changed and whether data had been seen.
  - Do not relabel a task's strata to make a table look balanced.
  - Nothing in this run produces a verdict. The suite is 26 tasks against
    the ~100 the sample-size analysis asks for, and the comparison this
    work will be judged on has a published effect in the single digits.
    The pilot exists to measure our actual intra-task and inter-arm
    correlation. When it finishes, run:
      python3 -m v2sbench.cli power --tasks 50 100 150 --seeds 1 3 5 10
    and record the measured values as an amendment.
  - Report cost as you go, and stop if it exceeds what --max-usd allows.

Start by cloning the repo and reading docs/running-a-pilot.md, then tell me
your plan and what you need from me before you spend anything.
```
