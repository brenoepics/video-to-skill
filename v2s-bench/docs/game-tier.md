# Tier 4 — how to run it, and what it does and does not measure

Design notes, licensing, the harness-freeze argument and the measured
bands live in `tasks/t4-game/README.md`. This page is the operational one:
what to type, what was verified when it was built, and what is still
unverified and why.

## What is in the tier

```
$ cd v2s-bench/harness && python3 -m v2sbench.cli tasks --tier t4-game
id                     tier         disc  fb    video     title
crafter-first-pickaxe  t4-game      poor  silent hermetic  HARNESS DIAGNOSTIC (calibration rung): Crafter, first pickaxe and first stone
g2048-two-boards       t4-game      poor  silent hermetic  Clear two provisioned 2048 boards, each to a 256 tile

2 tasks · suite version 43e6f22fc4eb599e
```

NetHack is a **documented stub** under `env/nethack/`, with no `task.yaml`,
so the suite loader ignores it. Its runtime, seeding and headless operation
are verified; its oracle does not exist, which is why it is not a task.

## Running it

Build the images (one of them is only needed for Crafter):

```sh
docker build -t v2sbench/t4-game-crafter:0.1 v2s-bench/env/crafter
# g2048-two-boards needs no custom image: it declares python:3.13-slim
# and carries its own stdlib-only interface in files/.
```

Integrity gates (§6), which must pass before any trial is paid for:

```sh
cd v2s-bench/harness
python3 -m v2sbench.cli validate --tier t4-game --oracle --trivial
```

Trials, as for any other tier:

```sh
python3 -m v2sbench.cli run --tier t4-game --agent claude-code \
    --model <exact model id> --arms core --seeds 3
```

The interface freeze gate, which CI should run alongside the others:

```sh
python3 v2s-bench/env/freeze.py --check
```

The calibration evidence, all re-runnable:

```sh
python3 v2s-bench/env/2048/calibrate.py bands          # the policy ladder
python3 v2s-bench/env/2048/calibrate.py pinned         # on the two pinned boards
python3 v2s-bench/env/2048/calibrate.py determinism    # same seed twice
python3 v2s-bench/env/2048/calibrate.py crosscheck     # engine vs checker rules
python3 v2s-bench/env/crafter/calibrate.py determinism # same seed twice, and resume
python3 v2s-bench/env/crafter/calibrate.py random      # the floor
python3 v2s-bench/env/crafter/calibrate.py oracle      # the ceiling, across seeds
env -u DISPLAY python3 v2s-bench/env/nethack/probe_determinism.py
```

The Crafter scripts need Crafter importable, so run them inside that
rung's image or in an environment installed from
`env/crafter/requirements.txt`.

## What was verified when this tier was built

All of the following was run, and the output is quoted in
`tasks/t4-game/README.md` and `env/nethack/README.md`:

| claim | how |
|---|---|
| both tasks load and validate | `v2sbench tasks --tier t4-game`, no errors |
| each oracle passes its checker | `validate --oracle` — 2/2, via the real harness (`trial.py`, real sandbox, real checker) |
| a do-nothing agent scores zero | `validate --trivial` — 0 tasks passed |
| a fabricated journal is rejected | hand-written journal claiming a 512 tile: `journal-is-consistent` and the milestone both fail |
| 2048 is deterministic | 240 scripted moves twice in two directories → byte-identical journals, sha256 `8c3a21866afb3480…` |
| the 2048 checker's rules match the engine's | `crosscheck` — 12/12 games with identical trajectories |
| Crafter is deterministic | 120 actions twice in two processes → byte-identical journals, sha256 `863d0cd415e26bbb…` |
| Crafter's resume-by-replay is exact | the same 120 actions in 1 batch vs 6 → identical observation |
| the thresholds avoid floor and ceiling | the reference-policy tables in the tier README |
| NetHack seeds reproducibly, headless | two seeded runs → sha256 `2b994ad7ff6f866d…`; unseeded → `f00e66187ec69511…` |
| the videos render headlessly and reproducibly | `render --only …` with `DISPLAY` unset: 2048 96.0 s sha256 `d0cbfb3dfee6fffb…` (byte-identical on a second render), Crafter 105.7 s sha256 `12d421c40b26dd3d…` |
| the Crafter dependency pin installs | `pip install --require-hashes -r env/crafter/requirements.txt` into a clean CPython 3.13 venv, then the interface, oracle and checker all run against it |

### Headless proof

Everything above ran with **no `DISPLAY` and no GPU**. Both renders were
invoked as `env -u DISPLAY python3 -m v2sbench.cli render …`. Crafter draws
with numpy and Pillow — no OpenGL, no X11 — and its image is produced and
thrown away (the render must not be skipped: it consumes world RNG).
NetHack's observation is its own 24×80 terminal screen.

## What is NOT verified, and why

- **No Docker image was built.** The container this tier was written in has
  the docker CLI and no daemon. So `env/crafter/Dockerfile` and
  `env/nethack/Dockerfile` are **unbuilt**. What is verified is the step
  that can actually fail — the hash-pinned `pip install` — run into a clean
  CPython 3.13 environment on linux/amd64, the same interpreter version and
  platform as the base image. Treat "docker build works" as an open item,
  not a result.
- **Every gate above ran on the `local` backend** (`--sandbox local
  --allow-unsandboxed`), because that is what a daemonless machine has.
  `sandbox/local.py` says plainly that a figure built from local trials is
  not a reproducible figure; these runs are integrity gates, not figures.
  They must be re-run on the Docker backend in CI.
- **No model has been run against either task.** Every band in the tier
  README is a *reference-policy* measurement, not an arm.
- **NetHack has no oracle**, and therefore no task. See
  `env/nethack/README.md` for exactly what remains.

## What this tier measures

- Whether an agent **enacts** a procedure over a long horizon, not whether
  it can state one. Both tasks are `feedback_bandwidth: silent`: a wrong
  policy returns a plausible observation and the loss arrives hundreds of
  steps later, so trial and error cannot substitute for instruction. This
  is the regime where a skill should matter most (§5.2), and the regime
  where the published null attributes its own result to the opposite
  condition.
- A **graded** outcome alongside the binary one: 2048's score and best
  tile, Crafter's achievement count, with declared milestone ladders, so a
  result is not reduced to one bit (§8.4).
- The **knowing–doing gap** directly. The 2048 probe asks for a fact most
  models can state ("keep the largest tile in a corner"). If `probe_known`
  is high and A0 is low, the gap is measured rather than assumed. Note the
  pre-declared exclusion rule (§5.3) excludes only tasks with
  `probe_known ≥ 0.9` **and** informative feedback; both tasks here are
  `silent`, so neither can be excluded on that rule.

## What this tier does not measure

- **It is not about game skill.** The threshold is set where a stated
  procedure separates from plausible-but-undisciplined play, not where good
  play separates from great play. A 2048 agent that reaches 2048 and one
  that reaches 256 on both boards score the same on the binary endpoint;
  that is what the graded score is for.
- **`crafter-first-pickaxe` is not a v2s result.** It is the calibration
  rung of §4.3: a documentation effect here is independently known to be
  large (SPRING: 27.3 vs DreamerV3's 14.5, with Crafter's leaderboard
  keeping a separate "External Knowledge" category), so it answers "can
  this harness detect a large effect at all" and nothing about our
  pipeline. The warning is in the task title, so it travels into any table
  that prints titles.
- **It does not generalise across worlds.** The game seed is pinned per
  task, so all trials of a task share one world (see below). Variance
  across worlds is not estimated by K=3.
- **Two alternative solution paths exist and are not blocked**: mirroring
  2048's tile PRNG, and searching rather than following a strategy. Both
  are recorded; see the tier README's bypass list.

## Three harness changes this tier needs

Outside tier-4 files, so not made here. Each is small and each is a real
loss until it is made.

1. **Pass the checker's `score` and `milestones` through to the record.**
   `harness/v2sbench/trial.py` parses only `assertions` from the checker's
   JSON, and `record.build()` already accepts `score=` and `milestones=`.
   One line in `_check()` to return the whole payload and two in the
   `record.build(...)` call. Until then the §8.4 graded endpoint for T4 is
   dropped on the floor; both checkers also write the score into an
   informational assertion's `detail` so it is recoverable from records
   written in the meantime.
2. **Interpolate the trial seed into `environment.setup`.** Setup commands
   are static strings today, so a game seed cannot vary with the trial
   seed. Substituting `{seed}` (or exporting `V2S_TRIAL_SEED` into the
   setup environment) would let a task provision a different world per
   trial, which is what `environment.seedable` implies and what K=3 ought
   to be sampling.
3. **Give `setup` a writable, PATH-visible bin directory in every
   backend.** The Docker backend's `bin_dir` is `/usr/local/bin`; the local
   backend's is a temp directory that setup cannot name. Exporting
   `V2S_BIN_DIR` into the setup environment would let a task install its
   interface without writing to the host's `/usr/local/bin` on the local
   backend.
