# NetHack — status: **STUB, NOT IN THE SUITE**

There is no `tasks/t4-game/nethack-*/` directory and no task in the suite,
deliberately. `v2sbench tasks --tier t4-game` lists two tasks, and neither
is this one. Nothing here is reported, scored or counted.

This directory holds what has actually been verified, a draft protocol, a
dependency pin and an unbuilt Dockerfile, so that whoever finishes the rung
starts from evidence rather than from a blank page.

## What is verified, with the command that shows it

Run on CPython 3.13, linux/amd64, with no `DISPLAY` and no GPU, from
`pip install nle==1.3.0` (a manylinux wheel exists for cp313, so nothing is
compiled from source).

```
$ env -u DISPLAY python3 env/nethack/probe_determinism.py
seeded   task=NetHackScore-v0 steps=120 (episode ended at 45)
  trajectory sha256 : 2b994ad7ff6f866d72de0cb0e2be0674ca23b1e9f9c3ef07520575b754165dfd
  blstats           : [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
  seeds             : (20481, 20482, False, 20483)

$ env -u DISPLAY python3 env/nethack/probe_determinism.py      # again
seeded   task=NetHackScore-v0 steps=120 (episode ended at 45)
  trajectory sha256 : 2b994ad7ff6f866d72de0cb0e2be0674ca23b1e9f9c3ef07520575b754165dfd
  ...

$ env -u DISPLAY python3 env/nethack/probe_determinism.py --unseeded
unseeded task=NetHackScore-v0 steps=120
  trajectory sha256 : f00e66187ec69511aedb1459f3dc4f23d4340d2816f84974013a4689e6e81b55
```

So, established:

1. **It runs headless.** No X11, no display, no GPU. The observation is
   NetHack's own 24×80 terminal screen (`tty_chars`), which is also why a
   text protocol here needs almost no invention.
2. **It seeds, and the seeding is reproducible across processes** —
   byte-identical trajectory hashes from two separate runs.
3. **The three seeding traps are real and are handled**, which is the whole
   point of the committed probe:
   - Gymnasium-style `reset(seed=...)` does **not** seed the game. Only
     `env.unwrapped.seed(core=..., disp=..., reseed=False, lgen=...)`
     before `reset()` does, and `reseed=False` is load-bearing: NetHack 3.6
     reseeds itself from true randomness as an anti-TAS measure.
   - **`NetHackChallenge-v0` cannot be seeded at all.** It raises
     `RuntimeError("NetHackChallenge doesn't allow seed changes")`, and
     `get_seeds()` raises as well. A seeded rung must use a seedable task
     such as `NetHackScore-v0`. This matters when borrowing numbers:
     BALROG runs the Challenge task, i.e. unseeded.
   - **Wall-clock effects are real.** NetHack reads the real date for the
     moon phase and Friday the 13th, which changes monster generation and
     luck. `fix_moon_phase=True` derives them from the seed instead; NLE's
     own docstring says so, and the option is accepted here.
4. **The dependency set is pinned by version and sha256** in
   `requirements.txt`, verified by `pip install --require-hashes` into a
   clean environment.

## What blocks the task, concretely

**The oracle.** Every task in this suite ships a known-good solution that
must pass its checker in a fresh environment (`docs/preregistration.md`
§6.1), and for a game that means a scripted policy competent enough to
reach the milestone. For NetHack that is a dungeon-crawling bot: parse the
map out of the tty screen, path through corridors and doors, fight by
walking into things, eat before fainting, answer `--More--` and `y/n`
prompts, and find the down staircase repeatedly. Projects exist that do
this (BotHack, AutoAscend) and they are not small. Writing a bad one is
worse than not writing one: an oracle that fails its own checker on a fresh
seed would mark the task broken, and an oracle that only replays a recorded
win would pass review by mistake.

Nothing else is blocking. The runtime, the seeding, the headless operation
and the licence position are all settled above.

## The other reason to be careful here

Full-game progress for LLM agents on NetHack sits at roughly 1.5–13%, which
is a floor: on an unbounded task every arm scores near zero and the
comparison between arms measures nothing. So the task **must** be bounded,
and the bound has to be chosen against a measured reference policy, exactly
as `env/2048/calibrate.py bands` and `env/crafter/calibrate.py random` do
for the two rungs that are in the suite. `task.yaml.draft` proposes a
bound; it is a proposal, not a measurement.

BALROG's own progress metric is the right graded score to reuse rather than
reinvent: it maps dungeon level and experience level onto an empirical human
win probability from a human-gameplay dataset, and BALROG is MIT.

**It is not on PyPI.** `pip install balrog` installs an unrelated
access-control library by Paylogic. The metric has to be vendored from
`github.com/balrog-ai/BALROG` at a pinned commit, with attribution.

## Licence position

NLE ships a compiled NetHack 3.6 under the **NetHack General Public
Licence**. That is usable here — no purchase, no ROM, nothing excluded by
`docs/preregistration.md` §7.3 — and nothing is committed to this
repository: the wheel is installed at image-build time from a pinned,
hashed `requirements.txt`.

One obligation to carry forward: the NGPL requires that corresponding
source be available. An image built from this directory and then
*published* must point at `github.com/facebookresearch/nle` and NetHack
upstream. Building it locally for a run creates no new obligation.

## Files here

| file | status |
|---|---|
| `probe_determinism.py` | **verified**, output pasted above |
| `requirements.txt` | **verified** by `pip install --require-hashes` into a clean venv |
| `PROTOCOL.draft.md` | draft, **unfrozen**, not hashed in `env/freeze.json` |
| `Dockerfile` | **never built** — this container has the docker CLI and no daemon |
| `task.yaml.draft` | draft. Deliberately not named `task.yaml`, so the suite loader ignores it |
