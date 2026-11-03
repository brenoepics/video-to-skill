#!/usr/bin/env python3
"""Determinism, resume and reference-band evidence for the Crafter rung.

    python3 env/crafter/calibrate.py determinism
    python3 env/crafter/calibrate.py random [--episodes 50]
    python3 env/crafter/calibrate.py oracle [--seeds 1-12]

**determinism** plays the same action list twice, in two directories, in
two processes, and compares the journals byte for byte. **resume**, run as
part of it, plays the same list split across several invocations and
checks the final observation is identical -- that is the load-bearing one,
because this interface is stateless and rebuilds the episode by replay,
and because Crafter consumes its world RNG while rendering night-time
noise, so a replay that skipped the render would silently diverge.

**random** is the floor: what a policy that presses buttons unlocks.
**oracle** is the ceiling the scripted policy reaches, across seeds rather
than on the one the task pins, so "the oracle passes" is not a statement
about one lucky world.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
INTERFACE = HERE / "v2s_game_crafter.py"
ORACLE = ROOT / "tasks" / "t4-game" / "crafter-first-pickaxe" / "oracle" / "play.py"
PINNED_SEED = 7
STEPS = 1000
CHAIN = ("collect_wood", "place_table", "make_wood_pickaxe", "collect_stone")


def _env(binary_dir: Path, provision: bool = False):
    env = dict(os.environ)
    env["PATH"] = "%s:%s" % (binary_dir, env.get("PATH", ""))
    if provision:
        env["V2S_GAME_PROVISION"] = "1"
    return env


def _bin() -> Path:
    """A directory holding the interface under the name agents call it by."""
    path = Path(tempfile.mkdtemp(prefix="v2s-crafter-bin-"))
    shutil.copy(INTERFACE, path / "v2s-game")
    (path / "v2s-game").chmod(0o755)
    return path


def _play(binary_dir, seed, batches):
    root = Path(tempfile.mkdtemp(prefix="v2s-crafter-"))
    subprocess.run(["v2s-game", "init", "--seed", str(seed), "--steps", str(STEPS)],
                   cwd=root, env=_env(binary_dir, True), capture_output=True, check=True)
    for batch in batches:
        subprocess.run(["v2s-game", "do", *batch], cwd=root,
                       env=_env(binary_dir), capture_output=True, check=True)
    state = subprocess.run(["v2s-game", "state"], cwd=root, env=_env(binary_dir),
                           capture_output=True, text=True, check=True).stdout
    journal = (root / ".v2s-game" / "journal.jsonl").read_bytes()
    shutil.rmtree(root, ignore_errors=True)
    return state, journal


def _script(n=120, seed=5):
    """An arbitrary but fixed action list, long enough to involve objects."""
    import random

    rng = random.Random(seed)
    verbs = ["move_left", "move_right", "move_up", "move_down", "do", "do",
             "place_table", "make_wood_pickaxe", "noop", "place_plant"]
    return [rng.choice(verbs) for _ in range(n)]


def cmd_determinism(_argv):
    binary = _bin()
    try:
        script = _script()
        one = _play(binary, PINNED_SEED, [script])
        two = _play(binary, PINNED_SEED, [script])
        split = [script[i : i + 20] for i in range(0, len(script), 20)]
        three = _play(binary, PINNED_SEED, split)
        import hashlib

        print("crafter determinism: %d actions, seed %d" % (len(script), PINNED_SEED))
        print("  run 1 vs run 2, journal bytes identical : %s (%d bytes, sha256 %s)"
              % (one[1] == two[1], len(one[1]),
                 hashlib.sha256(one[1]).hexdigest()[:16]))
        print("  run 1 vs run 2, final observation equal : %s" % (one[0] == two[0]))
        print("  one batch vs %d batches, observation equal: %s   <- resume by replay"
              % (len(split), one[0] == three[0]))
        print()
        print(one[0].rstrip())
        return 0 if (one[1] == two[1] and one[0] == two[0] and one[0] == three[0]) else 1
    finally:
        shutil.rmtree(binary, ignore_errors=True)


def cmd_random(argv):
    episodes = int(argv[argv.index("--episodes") + 1]) if "--episodes" in argv else 50
    try:
        import crafter
        import numpy as np
    except ImportError:
        print("crafter is not importable here; run this inside the rung's image")
        return 2
    from crafter import constants

    tally = Counter()
    counts = []
    chains = 0
    for episode in range(episodes):
        env = crafter.Env(seed=1000 + episode, length=STEPS, reward=False)
        env.reset()
        rng = np.random.RandomState(episode)
        info = {}
        for _ in range(STEPS):
            _, _, done, info = env.step(rng.randint(0, len(constants.actions)))
            if done:
                break
        unlocked = {k for k, v in info["achievements"].items() if v > 0}
        counts.append(len(unlocked))
        chains += all(name in unlocked for name in CHAIN)
        tally.update(unlocked)
    print("random policy: %d episodes of up to %d steps, seeds 1000-%d"
          % (episodes, STEPS, 999 + episodes))
    print("  mean achievements unlocked: %.2f" % (sum(counts) / len(counts)))
    print("  p(the task's four-step chain): %.2f" % (chains / episodes))
    for name, hits in tally.most_common():
        mark = "  <- in the required chain" if name in CHAIN else ""
        print("    %-20s %4.2f%s" % (name, hits / episodes, mark))
    missing = [name for name in CHAIN if not tally[name]]
    print("  never unlocked by chance: %s" % (", ".join(missing) or "(none)"))
    return 0


def cmd_oracle(argv):
    span = argv[argv.index("--seeds") + 1] if "--seeds" in argv else "1-12"
    low, high = (int(part) for part in span.split("-"))
    binary = _bin()
    try:
        print("scripted oracle across seeds %d-%d (the task pins %d)"
              % (low, high, PINNED_SEED))
        wins = 0
        for seed in range(low, high + 1):
            root = Path(tempfile.mkdtemp(prefix="v2s-crafter-or-"))
            subprocess.run(["v2s-game", "init", "--seed", str(seed), "--steps", str(STEPS)],
                           cwd=root, env=_env(binary, True), capture_output=True, check=True)
            done = subprocess.run([sys.executable, str(ORACLE)], cwd=root,
                                  env=_env(binary), capture_output=True, text=True)
            rows = [json.loads(line) for line in
                    (root / ".v2s-game" / "journal.jsonl").read_text().splitlines()]
            steps = [r for r in rows if r.get("kind") == "step"]
            unlocked = set(steps[-1]["achievements"]) if steps else set()
            chain = all(name in unlocked for name in CHAIN)
            wins += chain
            print("  seed %-4d steps %-4d achievements %-2d chain %-3s %s"
                  % (seed, len(steps), len(unlocked), "yes" if chain else "NO",
                     done.stdout.strip().splitlines()[-1][:40] if done.stdout else ""))
            shutil.rmtree(root, ignore_errors=True)
        print("  chain reached on %d/%d seeds" % (wins, high - low + 1))
        return 0
    finally:
        shutil.rmtree(binary, ignore_errors=True)


COMMANDS = {"determinism": cmd_determinism, "random": cmd_random, "oracle": cmd_oracle}

if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        print(__doc__)
        raise SystemExit(2)
    raise SystemExit(COMMANDS[sys.argv[1]](sys.argv[2:]))
