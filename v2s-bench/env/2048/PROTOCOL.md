# `v2s-game 2048` — frozen agent↔game protocol, version 1

**Frozen.** The bytes of `v2s_game_2048.py` and of this file are hashed in
`env/freeze.json`. Any change to either is a new protocol version and a new
row in every results table — never an edit to an old one.

## Why this is frozen and hashed

ARC-AGI-3 moved one model from 62.7% to 99.9% on the same tasks by changing
only the harness. In a game tier the harness *is* the measurement: how the
board is rendered, whether a no-op move costs a turn, whether moves can be
batched, and how many moves the budget allows all move measured performance
further than the choice of model does. A benchmark whose interface drifts
between runs cannot support a comparison between arms, so this interface is
pinned exactly like the model id and the container digest
(`docs/preregistration.md` §9.2).

## Verbs — the complete action vocabulary

```
v2s-game help                         the text below, in short form
v2s-game state                        print the current observation
v2s-game move DIR [DIR ...]           play moves in order, left to right
v2s-game new                          advance to the next board
```

`DIR` ∈ {`up`, `down`, `left`, `right`}. Nothing else is accepted: no
abbreviations, no synonyms, no numeric codes, no `quit`, no `undo`.

A fifth verb, `init`, provisions a run. It is **not part of play**: it
refuses unless `V2S_GAME_PROVISION=1` is set in the environment, and it
refuses outright once a run exists, so a run cannot be restarted and seeds
cannot be shopped for. The task's `environment.setup` is the only caller.

Exit status: `0` accepted, `2` usage error, `3` refused by the rules.
Everything the agent needs is on stdout; refusals are on stderr.

## Observation — exactly these bytes

```
V2S-GAME 2048 protocol=1
board=1 of 2  seed=20481  move=7/500  status=playing
score=132  max_tile=32  goal=1024
+-----+-----+-----+-----+
|    2|    4|     |     |
+-----+-----+-----+-----+
|   16|   32|    2|     |
+-----+-----+-----+-----+
|     |     |     |     |
+-----+-----+-----+-----+
|     |     |     |     |
+-----+-----+-----+-----+
row-major: 2,4,.,.,16,32,2,.,.,.,.,.,.,.,.,.
```

- Line 1 identifies the game and the protocol version.
- Line 2: 1-based board index, that board's seed, moves used against the
  budget, and `status` ∈ {`playing`, `won`, `lost`, `exhausted`}.
- Line 3: cumulative score on this board, its largest tile, the goal tile.
- Then the grid: cells right-aligned in width 5, empty cells blank.
- Last line repeats the grid row-major with `.` for empty, so a parser
  written by the agent does not have to read the box drawing.

ASCII only, ≤ 66 columns, 10 lines. Two reasons, both deliberate: the same
rendering has to appear in a 1280×720 hermetic screencast whose bitmap font
covers ASCII 32–126 at 71 columns (`harness/v2sbench/render/screens.py`),
so the pixels the extractor reads and the bytes the agent reads are the
same; and a fixed-width grid cannot be misread as prose.

`move` prints one line per move before the observation:

```
move 7 left: ok  gained=8  spawn=r2c3=2
move left: rejected (board unchanged; not counted)
board 1 ended: lost
```

## Rules

1. 4×4 grid. A move slides every tile as far as it can go in `DIR`.
2. Two equal tiles that collide merge into their sum, once per move,
   resolved from the far edge inwards. The score rises by the sum.
3. **A move that would leave the board unchanged is rejected**: it does not
   count against the budget and spawns nothing. Probing a direction is
   therefore free.
4. After every accepted move, one tile appears in a uniformly chosen empty
   cell: a 2 with probability 0.9, a 4 with probability 0.1.
5. `status=won` as soon as a tile reaches `goal`; the board ends.
   `status=lost` when no direction changes the board. `status=exhausted`
   when the move budget runs out.
6. A run is a fixed sequence of boards with per-board seeds, provisioned
   before the agent starts. Boards are played in order, `new` is refused
   while a board is still playing, and there is no retry.

## Determinism

Tile spawns come from SplitMix64 seeded with the board's seed — not from
Python's `random`, so the stream is a contract any reimplementation can
reproduce: `state += 0x9E3779B97F4A7C15`, then the two standard mixes, then
`z ^ (z >> 31)`. Each spawn draws twice: `draw1 % len(empty_cells)` picks
the cell, `draw2 % 10 == 0` makes it a 4. The opening position is two
spawns on an empty board.

Consequence: board *i* is a pure function of its seed, and the whole
trajectory is a pure function of (seed, move sequence). Replaying a journal
reproduces every board, score and tile exactly.

## The journal is a proof, not a report

Every accepted move is appended to `.v2s-game/journal.jsonl`, under a
per-board header carrying the seed.

The checker does **not** read the score the engine computed. It replays the
recorded move list under its own independent copy of these rules
(`check/check.py`, deliberately a second implementation) and scores the
replay. So:

- a hand-written journal passes only if its move list really does reach the
  target under canonical rules — which is solving the task;
- a patched engine cannot inflate a score, because its arithmetic is
  discarded;
- any disagreement between the engine and the canonical replay fails the
  `journal-is-consistent` assertion and the trial fails.

## Known bypass, stated rather than hidden

The engine is readable by the agent, and `.v2s-game/session.json` holds the
live PRNG state. In a single-uid container there is no way to hide them: the
process that must know the next tile runs as the same user as the agent, and
`--cap-drop ALL --security-opt no-new-privileges` rules out a setuid helper.
An agent that reads the source and mirrors SplitMix64 can therefore predict
every future spawn and search instead of playing.

That is a real alternative solution path. It is not cheating — it still has
to produce a legal winning move sequence — but it is not the strategy the
video teaches, so it is declared as this rung's chief threat to validity
(`tasks/t4-game/README.md`), the command log is checked for it as a
non-required assertion, and the move budget and turn budget are set so that
a deep search is expensive. The flagship rung of this tier must not depend
on secrecy either.
