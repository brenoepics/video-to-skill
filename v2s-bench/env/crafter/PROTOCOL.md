# `v2s-game crafter` — frozen agent↔game protocol, version 1

**Frozen.** `v2s_game_crafter.py`, this file, `requirements.txt` and
`PACKAGE.sha256.json` are hashed in `env/freeze.json`. A change to any of
them is a new protocol version and a new row in every results table.

**This rung is a harness diagnostic, not a v2s result.** See
`docs/preregistration.md` §4.3 and `tasks/t4-game/README.md`.

## Why Crafter is here

Feeding a model the *paper* describing Crafter has been reported to beat
every reinforcement-learning baseline with no interaction at all — 27.3
against DreamerV3's 14.5 — and Crafter's leaderboard keeps a separate
"External Knowledge" category for exactly this. The documentation effect
here is independently known to be large. So if our harness cannot recover
an effect on this rung, our null results elsewhere are not evidence of
anything. That is the whole job of this task.

## Licensing

Crafter is MIT and ships its own assets, which is why it is usable at all:
nothing is vendored into this repository and nothing is fetched from a
rights-holder at run time. The package is installed at image-build time
from `requirements.txt`, which pins every distribution by version **and**
sha256 (`pip install --require-hashes`). Building the sdist pulls
`setuptools` through pip's build isolation, unpinned; that is the one
unpinned input and it affects packaging, not game rules, whose files are
hashed separately in `PACKAGE.sha256.json`.

## Verbs — the complete action vocabulary

```
v2s-game help
v2s-game state
v2s-game do ACTION [ACTION ...]
```

`ACTION` is one of Crafter's own 17 actions, by name, in its own order:

```
noop
move_left move_right move_up move_down
do
sleep
place_stone place_table place_furnace place_plant
make_wood_pickaxe make_stone_pickaxe make_iron_pickaxe
make_wood_sword  make_stone_sword  make_iron_sword
```

`v2s-game do do` is not a typo: the verb is `do` and one of the actions is
also `do`, the action that interacts with the tile in front of you.

A fourth verb, `init`, provisions the episode. It is not part of play: it
refuses without `V2S_GAME_PROVISION=1` and refuses once an episode exists,
so an episode cannot be restarted and the seed cannot be shopped for.

## Observation — exactly these bytes

```
V2S-GAME crafter protocol=1
step=32/1000  status=playing  daylight=0.98
health=9 food=8 drink=8 energy=8  sleeping=no
facing=left  the tile in front of you is stone
inventory: wood=1 stone=1 wood_pickaxe=1 wood_sword=1
achievements 5/22: collect_stone collect_wood make_wood_pickaxe
... make_wood_sword place_table
view 9x7, you are at the centre:
  g g g s s s g g g
  ...
legend: g grass  T tree  s stone  p path  n sand  w water
        L lava  c coal  i iron  d diamond  = table  f furnace
        @ you  C cow  Z zombie  K skeleton  a arrow  P plant
        . off the map
```

- `status` ∈ {`playing`, `dead`, `over`}.
- The achievement list is the one line that can grow without bound; it
  wraps onto lines beginning `... `, which is distinct from the view's
  two-space indent so a parser can rejoin it unambiguously.
- The view is **9 wide by 7 high** — exactly the window Crafter renders,
  which is its 9×9 view minus the two rows its image uses for the
  inventory. The player is always at the centre.
- Nothing outside that window is disclosed. The text interface is a
  transcription of the pixel observation, not a cheat sheet for it: an
  agent here sees what an agent looking at the image sees, in characters.
- ASCII only, ≤ 66 columns, so the same bytes render in a 1280×720
  hermetic screencast whose bitmap font covers ASCII 32–126 at 71
  columns.

Each action also prints one line: `step 7 do: wood+1 UNLOCKED collect_wood`,
or `step 8 place_table: nothing changed`. It wraps onto `... ` continuations
too, so no line of this protocol exceeds 66 columns.

## What the environment does not tell you

An action whose requirements are unmet is **accepted and does nothing**.
The step is spent, the observation comes back unchanged, and no message
says why. That is Crafter's own behaviour, not a choice of ours, and it is
why this task declares `feedback_bandwidth: silent`: trial and error
cannot substitute for knowing that a table costs two wood, that crafting
only works while standing next to one, and that stone cannot be mined
before a wood pickaxe exists.

## Determinism, and two traps

The episode is a pure function of (seed, action list). Verified by
`env/crafter/calibrate.py determinism`, which plays the same list twice in
two processes and compares journals byte for byte.

Two traps this interface handles, and any reimplementation must:

1. **`Env.reset` seeds from `hash((seed, episode))` and increments
   `episode`.** A second `reset` in the same process is therefore a
   *different world*. Every invocation builds a fresh `Env` and resets
   exactly once. This is the same class of footgun as the NetHack Learning
   Environment, where Gymnasium-style `reset(seed=...)` does not seed the
   game at all.
2. **Crafter consumes its world RNG while rendering night-time noise.** An
   interface that skipped the render to save time would consume a
   different number of draws and the replay would silently diverge. Every
   step goes through the public `env.step`; the returned image is thrown
   away but never not produced.

There is no wall-clock input anywhere in Crafter's step function — no
moon-phase problem as in NetHack. Daylight is a function of the step
counter.

## The journal is the state, and the proof

Crafter's world cannot be serialised, so this interface is **stateless**:
each invocation replays `.v2s-game/journal.jsonl` from the seed and then
applies the new actions. A reset costs about 1.3 s and a step about
0.3 ms, so even a 1000-action replay is under two seconds.

That makes verification exact and cheap: the checker replays the same
journal through Crafter and scores *the replay*, so an edited journal
cannot manufacture an achievement. And because the replay uses the real
library rather than a reimplementation, the checker first verifies the
sha256 of Crafter's rule-bearing files against `PACKAGE.sha256.json` — a
replay is only evidence about the agent if the rules are the pinned ones.

## Known bypass, stated rather than hidden

As on the 2048 rung, the agent runs as the same user as the game, so it
can read the journal and, in principle, import Crafter itself and search
offline against the seeded world. Unlike 2048 this buys much less — the
world is 64×64 with moving creatures and a 22-step tech tree, so a search
is not a shortcut to the answer — but it is a real alternative path, it is
recorded from the command log as a non-required assertion, and no part of
this design depends on secrecy.
