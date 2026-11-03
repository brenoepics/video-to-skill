# Tier 4 — game environments

> **Grader-visible documentation.** This file states the exact strategy
> each task's video teaches, the assertion ids its checker decides on, and
> the reference-policy bands the thresholds were set against. Anyone
> authoring arm material (A1–A5, including the hand-written A4 skill) must
> not read it — the same structural blinding `docs/preregistration.md`
> §3.2 applies to `check/` and `oracle/`. A procedure shaped by this file
> is an answer key, and the string-level leakage gate cannot detect prose.
> It is here for reviewers and maintainers. The same applies to
> `docs/game-tier.md` and to the `env/<game>/PROTOCOL.md` files, which name
> the knowledge gap in the course of explaining why the interface is shaped
> as it is.

Two tasks, built and gated. One documented stub. Nothing here is a
placeholder pretending to be a measurement.

| task | game | licence | binary endpoint | discoverability | status |
|---|---|---|---|---|---|
| `g2048-two-boards` | 2048, our own implementation | MIT (ours) | a 256 tile on **both** provisioned boards | poor | built, oracle + trivial gates pass |
| `crafter-first-pickaxe` | Crafter 1.8.3 | MIT, assets included | `collect_wood` + `place_table` + `make_wood_pickaxe` + `collect_stone` | **good** | built, oracle + trivial gates pass. **HARNESS DIAGNOSTIC** (§4.3) |
| NetHack | NLE 1.3.0 / NetHack 3.6 | NGPL | proposed: alive at dlvl 4, xp 3 | — | **stub** — `env/nethack/README.md` |

## Why games, and why this tier is the one most easily faked

Games are knowledge-dense, tutorial video about them is abundant, and the
outcome is a number rather than a judgement. The adjacent literature is
unusually clear about both the opportunity and the trap:

- **The opportunity.** Feeding a model the *paper* describing Crafter beat
  every RL baseline with zero interaction — 27.3 against DreamerV3's 14.5
  — and Crafter's leaderboard keeps a separate "External Knowledge"
  category. Read-and-Reward did the same with Atari manuals. So
  documentation → performance in games is a known, large effect.
- **The gap worth measuring.** BALROG (arXiv:2411.13543, App. F.5)
  documented a **knowing–doing gap**: models answer NetHack quiz questions
  correctly and then eat rotten food in play. Closing that gap is exactly
  what a *procedural* skill is for, which is this tier's motivation and the
  reason `probe_known` is measured separately from the outcome (§5.1).
- **The trap.** ARC-AGI-3 moved the same model from 62.7% to 99.9% on the
  same tasks by changing only the harness. In a game tier the harness *is*
  the measurement.

What is new here is not that documentation helps in games. It is running it
as a controlled ablation with a length-and-pipeline-matched placebo (§3.3),
which nobody has done.

## The interface is frozen and content-hashed

Each game's agent-facing interface is one stdlib-only file plus one
document that pins the exact observation bytes and the exact action
vocabulary:

```
env/2048/v2s_game_2048.py      env/2048/PROTOCOL.md
env/crafter/v2s_game_crafter.py  env/crafter/PROTOCOL.md
```

`env/freeze.json` pins the sha256 of each, and `python3 env/freeze.py
--check` fails on any drift. A change to an interface is a new protocol
version and a new row in every results table, never an edit to an old one —
the same treatment the model id and the container digest get (§9.2).

The freeze file also pins **mirrors**. A task delivers its interface
through `files/`, so the repository holds two copies of each interface on
purpose: one canonical under `env/<game>/` and one in the task. `--check`
compares them byte for byte, so the duplication cannot drift into two
different games, and `--update` re-mirrors them.

Both protocols are ASCII-only and ≤ 66 columns because the same rendering
has to appear in a 1280×720 hermetic screencast whose bitmap font covers
ASCII 32–126 at 71 columns. The bytes the agent reads are the bytes the
video shows.

## The journal is a proof, not a report

Neither checker trusts what the game reported.

Each interface appends every accepted action to `.v2s-game/journal.jsonl`
under a header carrying the provisioned seed. The checker reads that as a
*claim* and replays it:

- **2048** replays under a second, independent implementation of the rules
  written from the protocol document (`check/check.py`), because the agent
  can edit the engine that sits in its own container.
  `env/2048/calibrate.py crosscheck` plays random games through both
  implementations and asserts identical trajectories — 12/12 identical.
- **Crafter** replays through Crafter itself, and therefore first verifies
  the sha256 of Crafter's rule-bearing files against
  `env/crafter/PACKAGE.sha256.json`. A replay is only evidence about the
  agent if the rules are the pinned ones.

So an edited journal buys nothing: the only action list that passes is one
that really does reach the target from the provisioned seed, which is what
playing the game means. A fabricated journal is rejected by name:

```
journal-is-consistent  FAIL  move 1: board differs from the canonical replay;
                             move 1: score 512, canonical 4
every-board-reaches-target FAIL max tile per board: [4, 4] (target 256 ...)
```

This also makes the trivial-agent gate (§6.2) pass trivially: a do-nothing
agent leaves one board header and no moves, so `boards-as-provisioned` and
the milestone both fail. Verified, not assumed.

## Floor and ceiling, measured before any model was run

A game task can be made to show almost any effect by moving its threshold.
Both thresholds here were set against reference policies measured in
advance, and the scripts are committed so a sceptic can re-run them.

### 2048 — `env/2048/calibrate.py bands` (40 trials × 2 boards, 500 moves)

| policy | p(256 on every board) | p(512 on every board) | median total score |
|---|---|---|---|
| random | 0.00 | 0.00 | 2 066 |
| cycle the four directions | 0.23 | 0.00 | 3 960 |
| greedy merge | 0.28 | 0.03 | 5 630 |
| fixed `down>left>right>up` | 0.30 | 0.00 | 4 756 |
| corner awareness only | 0.50 | 0.03 | 7 290 |
| **the taught discipline** | **0.75** | 0.35 | 10 548 |
| the oracle's two-ply version | 0.95 | 0.88 | 14 798 |

256-on-every-board is the only rung of that ladder that neither floors nor
ceilings: at 512 every naive policy is 0–3%, and at 128 everything passes.
**Expected no-skill band: 25–55%.** On the two *pinned* boards
(`calibrate.py pinned`) the corner-only and fixed-priority policies do
clear 256 while random, cycling and greedy do not, so this pair sits on the
easy side of the distribution and A0 should be read nearer the top of that
range. The seeds were fixed before the sweep and were not re-chosen on the
basis of it.

### Crafter — `env/crafter/calibrate.py random` (40 episodes × 1000 steps)

Mean achievements unlocked: **2.45**. Rate for the four-step chain:
**0.00**. Per-achievement: `wake_up` 0.95, `collect_sapling` 0.57,
`place_plant` 0.55, `collect_wood` 0.28, `place_table` 0.05,
`collect_drink` 0.03, `eat_cow` 0.03, `make_wood_pickaxe` **0.00**,
`collect_stone` **0.00**. The floor is clean: the chain is not reachable by
chance. The expected no-skill band (20–50%) is **borrowed** from published
LLM-agent results on Crafter, not measured here, and is labelled as such in
the task.

## Seeding, verified by replay

Every rung pins its seeds in `environment.setup`, and `init` refuses to run
twice and refuses to run at all without `V2S_GAME_PROVISION=1`.
Seed-shopping — restarting until an easy board appears — would otherwise be
the dominant strategy and would measure luck.

- **2048** draws tiles from SplitMix64 rather than a language's own PRNG, so
  the stream is a contract any reimplementation can reproduce. 240 scripted
  moves played twice in two directories: **byte-identical journals**
  (39 854 bytes, sha256 `8c3a21866afb3480…`) and identical final
  observation.
- **Crafter** is a pure function of (seed, action list). 120 actions played
  twice in two processes: **byte-identical journals** (45 919 bytes, sha256
  `863d0cd415e26bbb…`). The same 120 actions split across six invocations
  reproduce the final observation exactly, which is the load-bearing check,
  because the interface is stateless and rebuilds the episode by replay.
  Two traps are handled in code and documented: `Env.reset` seeds from
  `hash((seed, episode))` and increments `episode`, so a second reset in one
  process is a different world; and Crafter consumes its world RNG while
  rendering night-time noise, so an interface that skipped the render to
  save time would silently diverge.
- **NetHack** seeds correctly too, which is the one thing about that rung
  that is settled: two seeded runs produce identical trajectory hashes,
  an unseeded run does not (`env/nethack/README.md`). Gymnasium-style
  `reset(seed=…)` does **not** seed it; `NetHackChallenge-v0` refuses
  seeding outright; and `fix_moon_phase=True` removes the wall-clock
  dependency.

## Crafter's gap is discoverable on the box, and is declared so

`crafter-first-pickaxe` declares `discoverability: good`, which looks wrong
for a task whose whole point is a hidden requirement, and is not.

Crafter is installed as a Python library, and its own `data.yaml` encodes
every recipe: an agent that thinks to look in `site-packages` can read "a
table costs two wood, a wood pickaxe needs a table nearby, stone needs the
pickaxe" straight off the box. In a single-uid container that cannot be
prevented, and stripping the file would both break the pinned hash manifest
and diverge from the published Crafter setting that this rung is calibrating
against. So it is declared rather than hidden, and the tier's
`discoverability` stratum (§4.1) carries it.

Two consequences, both deliberate:

- The checker records `rules-not-read-from-the-library` (not required) from
  the command log. Reading the library is legitimate discovery under §3.1 —
  the baseline is allowed to find the procedure — so it is measured, never
  punished. How often an arm goes looking bounds how much of this rung's
  effect was knowledge the environment was carrying all along.
- Nothing that states the gap is installed in the container by us.
  `env/crafter/PROTOCOL.md` is **not** copied into the image for exactly
  that reason: it explains the interface by naming the gap, which inside
  the container would be an answer key handed to every arm. The agent gets
  `v2s-game help`, which says requirements exist and refuses to say what
  they are.

The 2048 rung is `poor` by contrast: the engine in the container documents
the *rules* (and `help` prints them), and nothing on the box states the
*strategy*.

## Known bypasses, stated rather than hidden

In a single-uid container the process that must know the next tile runs as
the same user as the agent, and `--cap-drop ALL --security-opt
no-new-privileges` rules out a setuid helper. So hidden state is not
achievable here, and no part of this design depends on secrecy.

1. **2048 PRNG mirroring.** An agent that reads the engine and mirrors
   SplitMix64 can predict every spawn and search instead of playing a
   strategy. It still has to produce a legal winning move sequence, so it
   is not cheating, but it is not the strategy the video teaches. This is
   the 2048 rung's chief threat to validity. It is recorded from the
   command log as a non-required assertion
   (`engine-source-not-inspected`), and the move and turn budgets make a
   deep search expensive.
2. **Crafter bytecode.** A `.pyc` whose header matches its source would be
   loaded in preference to the source and would sidestep the checker's hash
   check on Crafter's rules. The image sets `PYTHONDONTWRITEBYTECODE` and
   clears the caches, which raises the cost without eliminating it.
3. **Restarting a run.** `init` refuses, but an agent that deletes
   `.v2s-game` entirely could re-provision. The command log is the
   harness's, not the agent's, so `run-not-restarted` is a *required*
   assertion computed from it, and the same patterns are declared in
   `forbidden_actions`.
4. **Mode 0111 does not help.** Making the interpreted interface
   execute-only would stop an agent reading it, except that the interpreter
   itself must read the file, and the image's user is root anyway. It is
   not used here.

## The Crafter rung is a diagnostic, and is labelled everywhere

`crafter-first-pickaxe` exists to answer one question: **can this harness
detect a known-large documentation effect at all?** If it cannot, our null
results elsewhere are not evidence of anything (§4.3).

It must never be reported as a v2s result. The warning is in the task's
`title` (so it appears in every table that prints titles), in its `notes`,
in `env/crafter/PROTOCOL.md` and in `docs/game-tier.md`. The effect it
measures is one that SPRING already demonstrated; it says nothing about our
pipeline.

## Harness gaps this tier found

Three, all outside this tier's files and none worked around silently —
`docs/game-tier.md` has the detail and the one-line fixes.

1. **`score` and `milestones` are dropped.** Both checkers print them, and
   `record.build()` accepts them, but `trial.py` never passes them, so the
   graded secondary endpoint §8.4 requires is lost. Both checkers therefore
   also put the score into an informational assertion's `detail`, so the
   number is recoverable from existing records — but that is a workaround,
   not a fix.
2. **The trial seed does not reach `setup`.** Setup commands are static
   strings, so the game seed is pinned per task rather than per trial, and
   K=3 trials share one world. That is correct for pairing and weak for
   generalisation: a per-trial world needs the harness to interpolate the
   seed.
3. **The local backend writes to the host.** `setup` installs the
   interface with `install -m 0755 v2s-game /usr/local/bin/`, which on the
   `local` backend is the host's `/usr/local/bin`. Fine for the Docker
   backend, which is the only one valid for a published figure, and noted
   because a local-backend gate run leaves that file behind.
