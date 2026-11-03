# `v2s-game nethack` — protocol DRAFT, **unfrozen**

Not hashed in `env/freeze.json`, not used by any task, not reported. It is
written down now so that the rung's interface is argued about before it is
built, since the harness dominates the measurement in this literature.

## Shape

NetHack is already a text game, so the honest interface is its own screen.

```
v2s-game help
v2s-game state
v2s-game keys "<keystrokes>"      e.g.  "hhhj"  ">"  "Eelbereth\r"
v2s-game init --seed N --turns M  (provisioning only)
```

Observation: NetHack's 24×80 `tty_chars`, verbatim, plus one status block
this protocol adds:

```
V2S-GAME nethack protocol=1
turn=412/5000  status=playing  dlvl=3  xplevel=4  hp=21/27  ac=6
<24 rows of the game's own screen, exactly as NetHack drew them>
```

Three consequences of using the real screen, all deliberate:

- **80 columns.** The hermetic renderer gives 71 columns at 1280×720 and
  exactly 80 at 1920×1080, so a NetHack recipe must be `1920x1080`. The
  schema already permits it.
- **Menus and prompts are part of the game**, not something to paper over.
  `--More--`, `y/n`, inventory letters and the `#` extended commands must
  all be reachable by `keys`, or the agent is playing a different game than
  the one every published number describes.
- **Nothing is summarised for the agent.** No "there is a door to the
  north" layer. A convenience layer is a harness change, and harness
  changes have swung the same model from 62.7% to 99.9%.

## Seeding (settled; see README)

```python
env = gym.make("NetHackScore-v0", fix_moon_phase=True)
env.unwrapped.seed(core=C, disp=D, reseed=False, lgen=L)   # BEFORE reset
env.reset()
```

`NetHackChallenge-v0` must not be used: it refuses to be seeded.

## Journal and verification

Same discipline as the two rungs in the suite: the journal records the seed
and every keystroke, and the checker **replays** it rather than trusting a
reported outcome. NetHack is deterministic given (core, disp, lgen,
`reseed=False`, `fix_moon_phase=True`) and a keystroke stream, which is
what `probe_determinism.py` demonstrates, so the replay is exact.

Unlike Crafter, resume-by-replay may be too slow to be stateless at 5000
turns; a long-running process with the journal written alongside is the
likely shape, and that trade-off should be measured before it is chosen.

## Milestone, and why it must be bounded

Full-game progress for LLM agents is 1.5–13%. An unbounded task floors
every arm. `task.yaml.draft` proposes "alive at dungeon level 4 with
experience level 3 or more, within 5000 turns", with BALROG's
dungeon-level × experience-level progress metric as the graded score. That
is a proposal: the threshold must be set against a measured reference
policy before any arm is run, the way the other two rungs were.

## The knowledge gap this rung is for

The BALROG authors documented a **knowing–doing gap** on NetHack: models
answer quiz questions about the game correctly and then eat rotten food
in-game (arXiv:2411.13543, App. F.5). That gap is the reason this tier
exists — a procedure that closes it is exactly what a skill is supposed to
be — and it is why the probe (`docs/preregistration.md` §5.1) is measured
separately from the outcome rather than assumed to predict it.
