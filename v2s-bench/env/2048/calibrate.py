#!/usr/bin/env python3
"""Calibration, determinism and cross-check evidence for the 2048 rung.

    python3 env/2048/calibrate.py bands [--trials 40]
    python3 env/2048/calibrate.py pinned
    python3 env/2048/calibrate.py determinism
    python3 env/2048/calibrate.py crosscheck [--games 20]

Why this is committed rather than a note in a README:

**bands** is how the task's milestone threshold was chosen. A game task
can be made to show almost any effect by moving its threshold: set it too
high and every arm floors, too low and every arm ceilings, and in both
cases the comparison between arms measures nothing. So the threshold is
set against measured reference policies, before any model is run, and the
table is reproducible by a reader who doubts it.

**pinned** prints what each reference policy does on the two boards the
task actually provisions, because a population rate is not a statement
about a specific board.

**determinism** drives the real interface twice, in two directories, with
the same seeds and the same moves, and compares the journals byte for
byte. A seeded game that is not actually seeded is a known footgun
(Gymnasium-style `reset(seed=...)` does not seed the NetHack Learning
Environment), so the claim is tested rather than asserted.

**crosscheck** plays random games through the engine's rules and through
the checker's independent reimplementation of them, asserting identical
trajectories. The duplication is deliberate -- the checker must not
import code the agent could edit -- and this is what keeps the two copies
honest.
"""

from __future__ import annotations

import importlib.util
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
TASK = ROOT / "tasks" / "t4-game" / "g2048-two-boards"
PINNED_SEEDS = (20481, 20482)
BUDGET = 500
GOAL = 1024


def _load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


game = _load("v2s_game_2048", HERE / "v2s_game_2048.py")


# -- reference policies ----------------------------------------------------
#
# Deliberately a ladder, not a single opponent: "what does a no-skill arm
# look like" is a guess, so the band is bracketed from a direction-cycling
# policy up to a two-ply search, and the threshold is chosen where the
# interesting policies separate.

SNAKE_PATH = [12, 13, 14, 15, 11, 10, 9, 8, 4, 5, 6, 7, 3, 2, 1, 0]
WEIGHT = [4.0 ** rank for rank in [0, 1, 2, 3, 7, 6, 5, 4, 8, 9, 10, 11, 15, 14, 13, 12]]


def empties(board):
    return sum(1 for v in board if not v)


def snake_order(board):
    values = [board[i] for i in SNAKE_PATH]
    score = sum(1 for a, b in zip(values, values[1:]) if a >= b)
    return score + (4 if board[12] == max(board) and max(board) else 0)


def p_random(board, options, rng, history):
    return options[rng.next() % len(options)]


CYCLE = ("left", "up", "right", "down")


def p_cycle(board, options, rng, history):
    start = len(history) % 4
    for step in range(4):
        direction = CYCLE[(start + step) % 4]
        if direction in options:
            return direction
    return options[0]


def p_greedy(board, options, rng, history):
    best, chosen = -1, options[0]
    for direction in options:
        after, gained, _ = game.slide(board, direction)
        key = gained * 100 + empties(after)
        if key > best:
            best, chosen = key, direction
    return chosen


def p_priority(board, options, rng, history):
    for direction in ("down", "left", "right", "up"):
        if direction in options:
            return direction
    return options[0]


def p_anchor(board, options, rng, history):
    """Corner awareness only: keep the bottom row monotone, avoid `up`."""
    allowed = [d for d in ("down", "left", "right") if d in options] or list(options)
    best, chosen = None, allowed[0]
    for direction in allowed:
        after, gained, _ = game.slide(board, direction)
        row = after[12:16]
        monotone = sum(1 for a, b in zip(row, row[1:]) if a >= b)
        key = (monotone + (3 if row[0] == max(after) else 0), empties(after), gained)
        if best is None or key > best:
            best, chosen = key, direction
    return chosen


def p_snakeorder(board, options, rng, history):
    """The discipline the task's video teaches, as a one-ply rule."""
    allowed = [d for d in ("down", "left", "right") if d in options] or list(options)
    best, chosen = None, allowed[0]
    for direction in allowed:
        after, gained, _ = game.slide(board, direction)
        key = (snake_order(after), empties(after), gained)
        if best is None or key > best:
            best, chosen = key, direction
    return chosen


def _value(board):
    if not game.legal(board):
        return -1e18
    return sum(w * v for w, v in zip(WEIGHT, board)) + 3e5 * empties(board)


def _chance(board, depth):
    cells = [i for i, v in enumerate(board) if not v]
    if not cells:
        return _value(board)
    sample = cells if len(cells) <= 4 else cells[:: max(1, len(cells) // 4)][:4]
    total = 0.0
    for cell in sample:
        after = list(board)
        after[cell] = 2
        total += _best(after, depth)
    return total / len(sample)


def _best(board, depth):
    if depth == 0:
        return _value(board)
    best = -1e18
    for direction in game.DIRS:
        after, _, changed = game.slide(board, direction)
        if changed:
            best = max(best, _chance(after, depth - 1))
    return best if best > -1e17 else _value(board)


def p_expecti2(board, options, rng, history):
    """The oracle's policy: the same discipline plus a two-ply lookahead."""
    best, chosen = None, options[0]
    for direction in options:
        after, _, _ = game.slide(board, direction)
        value = _chance(after, 1)
        if best is None or value > best:
            best, chosen = value, direction
    return chosen


POLICIES = [
    ("random", p_random), ("cycle", p_cycle), ("greedy", p_greedy),
    ("priority", p_priority), ("anchor", p_anchor),
    ("snakeorder", p_snakeorder), ("expecti2", p_expecti2),
]


def play(seed, policy, budget=BUDGET, goal=GOAL):
    board, rng = game.start(seed)
    policy_rng = game.Rng(seed ^ 0xA5A5A5)
    score = moves = 0
    history = []
    while moves < budget:
        options = game.legal(board)
        if not options:
            break
        direction = policy(board, options, policy_rng, history)
        if direction not in options:
            direction = options[0]
        board, gained, _ = game.slide(board, direction)
        board, _, _ = game.spawn(board, rng)
        score += gained
        moves += 1
        history.append(direction)
        if max(board) >= goal:
            break
    return max(board), score, moves


# -- commands --------------------------------------------------------------


def cmd_bands(argv):
    trials = int(argv[argv.index("--trials") + 1]) if "--trials" in argv else 40
    boards = len(PINNED_SEEDS)
    base = 4000
    runs = [tuple(base + boards * k + i for i in range(boards)) for k in range(trials)]
    print("2048 reference-policy bands")
    print("  boards per trial %d, budget %d moves, %d trials, seeds %d..%d"
          % (boards, BUDGET, trials, base, base + boards * trials - 1))
    print()
    print("  policy      p(every>=256)  p(every>=512)  median total score")
    for name, policy in POLICIES:
        worst, totals = [], []
        for seeds in runs:
            outcomes = [play(seed, policy) for seed in seeds]
            worst.append(min(o[0] for o in outcomes))
            totals.append(sum(o[1] for o in outcomes))
        print("  %-10s %13.2f %14.2f %19d"
              % (name, sum(1 for t in worst if t >= 256) / len(worst),
                 sum(1 for t in worst if t >= 512) / len(worst),
                 statistics.median(totals)))
    print()
    print("  The task's binary threshold is 256 on EVERY board: the only rung")
    print("  of the ladder where the naive policies neither floor nor ceiling.")
    return 0


def cmd_pinned(_argv):
    print("2048 reference policies on the provisioned boards %r (budget %d)"
          % (list(PINNED_SEEDS), BUDGET))
    print()
    print("  policy     " + "".join("  board %d (tile/score/moves)" % i
                                    for i in range(1, len(PINNED_SEEDS) + 1))
          + "   every>=256")
    for name, policy in POLICIES:
        outcomes = [play(seed, policy) for seed in PINNED_SEEDS]
        cells = "".join("  %6d %6d %5d" % o for o in outcomes)
        print("  %-10s%s   %s"
              % (name, cells, "yes" if min(o[0] for o in outcomes) >= 256 else "no"))
    return 0


def _provision(root: Path, binary: Path):
    env = dict(os.environ)
    env["V2S_GAME_PROVISION"] = "1"
    env["PATH"] = "%s:%s" % (binary.parent, env.get("PATH", ""))
    subprocess.run([str(binary), "init", "--seeds", ",".join(str(s) for s in PINNED_SEEDS),
                    "--goal", str(GOAL), "--budget", str(BUDGET)],
                   cwd=str(root), env=env, capture_output=True, text=True, check=True)
    return env


def cmd_determinism(_argv):
    """Two runs, same seeds, same moves, through the real interface."""
    binary = HERE / "v2s_game_2048.py"
    script = []
    rng = game.Rng(99)
    for _ in range(240):
        script.append(game.DIRS[rng.next() % 4])
    journals = []
    states = []
    for run in (1, 2):
        root = Path(tempfile.mkdtemp(prefix="v2s-2048-det-"))
        try:
            env = _provision(root, binary)
            for chunk in range(0, len(script), 20):
                subprocess.run([str(binary), "move", *script[chunk : chunk + 20]],
                               cwd=str(root), env=env, capture_output=True, text=True)
            states.append(subprocess.run([str(binary), "state"], cwd=str(root), env=env,
                                         capture_output=True, text=True).stdout)
            journals.append((root / ".v2s-game" / "journal.jsonl").read_bytes())
        finally:
            shutil.rmtree(root, ignore_errors=True)
    same_journal = journals[0] == journals[1]
    same_state = states[0] == states[1]
    print("determinism: %d scripted moves played twice in two directories" % len(script))
    print("  journal bytes identical : %s (%d bytes, sha256 %s)"
          % (same_journal, len(journals[0]), _sha(journals[0])[:16]))
    print("  final observation equal : %s" % same_state)
    print()
    print(states[0].rstrip())
    return 0 if (same_journal and same_state) else 1


def _sha(blob: bytes) -> str:
    import hashlib

    return hashlib.sha256(blob).hexdigest()


def cmd_crosscheck(argv):
    """Engine rules vs the checker's independent copy of the rules."""
    games = int(argv[argv.index("--games") + 1]) if "--games" in argv else 20
    checker = _load("check2048", TASK / "check" / "check.py")
    bad = []
    for seed in range(7000, 7000 + games):
        board, rng = game.start(seed)
        c_board, c_rng = checker.opening(seed)
        if board != c_board:
            bad.append("seed %d: opening differs" % seed)
            continue
        rng_policy = game.Rng(seed)
        for move in range(200):
            options = game.legal(board)
            if not options:
                break
            direction = options[rng_policy.next() % len(options)]
            board, gained, _ = game.slide(board, direction)
            board, cell, value = game.spawn(board, rng)
            c_next, c_gained, c_changed = checker.apply_move(c_board, direction)
            if not c_changed or c_gained != gained:
                bad.append("seed %d move %d: gain %d vs %d" % (seed, move, gained, c_gained))
                break
            c_board, c_cell, c_value = checker.spawn(c_next, c_rng)
            if (c_board, c_cell, c_value) != (board, cell, value):
                bad.append("seed %d move %d: state diverged" % (seed, move))
                break
    print("crosscheck: %d games, engine rules vs checker rules" % games)
    for problem in bad[:5]:
        print("  FAIL %s" % problem)
    print("  %s" % ("identical trajectories in every game" if not bad
                    else "%d divergence(s)" % len(bad)))
    return 1 if bad else 0


COMMANDS = {"bands": cmd_bands, "pinned": cmd_pinned,
            "determinism": cmd_determinism, "crosscheck": cmd_crosscheck}

if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        print(__doc__)
        raise SystemExit(2)
    raise SystemExit(COMMANDS[sys.argv[1]](sys.argv[2:]))
