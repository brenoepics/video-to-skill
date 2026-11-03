#!/usr/bin/env python3
"""Does the NetHack Learning Environment actually seed, headless, here?

    python3 env/nethack/probe_determinism.py            # one trajectory hash
    python3 env/nethack/probe_determinism.py --unseeded  # the control

Run it twice and compare. Two seeded runs must print the same hash; an
unseeded run must not. This exists as a committed script rather than a
claim in a README because the seeding of this environment is a known
footgun with three separate traps:

1. **Gymnasium-style `reset(seed=...)` does not seed NetHack.** It seeds
   the wrapper's own RNG. The game's two RNGs are set by
   `env.unwrapped.seed(core=..., disp=..., reseed=False)`, *before*
   `reset()`, and `reseed=False` is the part that matters: NetHack 3.6
   reseeds itself from true randomness periodically as an anti-TAS
   measure, and with `reseed=True` no trajectory reproduces.
2. **`NetHackChallenge-v0` refuses to be seeded at all** -- it raises
   `RuntimeError("NetHackChallenge doesn't allow seed changes")`, and
   `get_seeds()` raises too. The competition task is deliberately
   unseedable, so a seeded rung has to use a seedable task such as
   `NetHackScore-v0`.
3. **Wall-clock effects.** NetHack reads the real date for the moon phase
   and Friday the 13th, which changes monster generation and luck. NLE's
   `fix_moon_phase=True` derives those from the seed instead. Without it
   a "reproducible" seed reproduces differently on different days.

`lgen` additionally fixes the level generator, so the dungeon layout is
independent of in-game choices.
"""

from __future__ import annotations

import hashlib
import sys

CORE, DISP, LGEN = 20481, 20482, 20483
STEPS = 120
TASK = "NetHackScore-v0"


def main(argv) -> int:
    import gymnasium as gym
    import numpy as np
    import nle  # noqa: F401  (registers the environments)

    seeded = "--unseeded" not in argv
    env = gym.make(TASK, fix_moon_phase=True)
    if seeded:
        env.unwrapped.seed(core=CORE, disp=DISP, reseed=False, lgen=LGEN)
    obs, _ = env.reset()

    digest = hashlib.sha256()
    digest.update(obs["tty_chars"].tobytes())
    rng = np.random.RandomState(0)      # the action stream, not the game's RNG
    ended = None
    for step in range(STEPS):
        obs, _, terminated, truncated, _ = env.step(rng.randint(0, env.action_space.n))
        digest.update(obs["tty_chars"].tobytes())
        digest.update(obs["blstats"].tobytes())
        if terminated or truncated:
            ended = step
            break

    print("%-8s task=%s steps=%d%s" % (
        "seeded" if seeded else "unseeded", TASK, STEPS,
        "" if ended is None else " (episode ended at %d)" % ended))
    print("  trajectory sha256 : %s" % digest.hexdigest())
    print("  blstats           : %s" % obs["blstats"][:14].tolist())
    if seeded:
        print("  seeds             : %r" % (env.unwrapped.get_seeds(),))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
