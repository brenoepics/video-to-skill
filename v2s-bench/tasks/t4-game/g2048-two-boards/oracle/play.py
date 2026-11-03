#!/usr/bin/env python3
"""The oracle policy for g2048-two-boards: corner discipline with a
two-ply lookahead, driven entirely through `v2s-game`.

It plays the strategy the task's video teaches -- anchor the largest tile
in the bottom-left corner, keep the board in snake order, never lift tiles
off the anchor row unless nothing else is legal -- and adds a shallow
lookahead so the gate is deterministic rather than merely likely. The
population rates measured by `env/2048/calibrate.py bands` are 0.75 for
the taught discipline alone and 0.95 for this policy; on the two
provisioned boards this policy reaches 1024 and 512.

It reads only what an agent reads: the observation printed by `state` and
by `move`. It does not read the engine, the config or the PRNG state.
"""

from __future__ import annotations

import re
import subprocess
import sys

GAME = "v2s-game"
SIZE = 4
DIRS = ("up", "down", "left", "right")

# Snake weights anchored at the bottom-left corner (cell 12). This is the
# numeric form of the discipline: tiles should descend along the snake
# path, and the corner should hold the largest tile.
SNAKE = [0, 1, 2, 3, 7, 6, 5, 4, 8, 9, 10, 11, 15, 14, 13, 12]
WEIGHT = [4.0 ** rank for rank in SNAKE]


def run(*args: str) -> str:
    done = subprocess.run([GAME, *args], capture_output=True, text=True)
    if done.returncode not in (0, 3):
        sys.stderr.write(done.stderr)
        raise SystemExit("`%s %s` failed: %s" % (GAME, " ".join(args), done.stderr.strip()))
    return done.stdout


def parse(text: str) -> dict | None:
    """Pull the last observation out of whatever the interface printed."""
    flat = re.findall(r"^row-major: (.+)$", text, re.M)
    meta = re.findall(r"^board=(\d+) of (\d+)\s+seed=\d+\s+move=(\d+)/(\d+)\s+status=(\w+)",
                      text, re.M)
    if not flat or not meta:
        return None
    cells = [0 if cell == "." else int(cell) for cell in flat[-1].split(",")]
    index, boards, moves, budget, status = meta[-1]
    return {"board": cells, "index": int(index), "boards": int(boards),
            "moves": int(moves), "budget": int(budget), "status": status}


# -- rules, as published by `v2s-game help` --------------------------------


def slide(flat, direction):
    out = list(flat)
    gained = 0
    for index in range(SIZE):
        if direction == "left":
            cells = [index * SIZE + c for c in range(SIZE)]
        elif direction == "right":
            cells = [index * SIZE + c for c in range(SIZE - 1, -1, -1)]
        elif direction == "up":
            cells = [r * SIZE + index for r in range(SIZE)]
        else:
            cells = [r * SIZE + index for r in range(SIZE - 1, -1, -1)]
        tiles = [flat[c] for c in cells if flat[c]]
        merged = []
        while tiles:
            head = tiles.pop(0)
            if tiles and tiles[0] == head:
                tiles.pop(0)
                head *= 2
                gained += head
            merged.append(head)
        merged += [0] * (SIZE - len(merged))
        for cell, value in zip(cells, merged):
            out[cell] = value
    return out, gained, out != list(flat)


def legal(flat):
    return [d for d in DIRS if slide(flat, d)[2]]


def heuristic(flat):
    if not legal(flat):
        return -1e18
    empties = sum(1 for v in flat if not v)
    return sum(w * v for w, v in zip(WEIGHT, flat)) + 3e5 * empties


def chance(flat, depth):
    """Average over a sample of spawn cells; the value is a 2, the common case."""
    empties = [i for i, v in enumerate(flat) if not v]
    if not empties:
        return heuristic(flat)
    sample = empties if len(empties) <= 4 else empties[:: max(1, len(empties) // 4)][:4]
    total = 0.0
    for cell in sample:
        after = list(flat)
        after[cell] = 2
        total += best_value(after, depth)
    return total / len(sample)


def best_value(flat, depth):
    if depth == 0:
        return heuristic(flat)
    best = -1e18
    for direction in DIRS:
        after, _, changed = slide(flat, direction)
        if changed:
            best = max(best, chance(after, depth - 1))
    return best if best > -1e17 else heuristic(flat)


def choose(flat):
    """The move the discipline prefers, among those that are legal."""
    options = legal(flat)
    if not options:
        return None
    scored = []
    for direction in options:
        after, _, _ = slide(flat, direction)
        value = chance(after, 1)
        # Ties go to the move that keeps weight on the anchor row.
        scored.append((value, direction != "up", direction))
    scored.sort(reverse=True)
    return scored[0][2]


def main() -> int:
    state = parse(run("state"))
    if state is None:
        sys.stderr.write("no observation from `v2s-game state`\n")
        return 1
    while True:
        if state["status"] != "playing":
            if state["index"] >= state["boards"]:
                break
            nxt = parse(run("new"))
            if nxt is None:
                break
            state = nxt
            continue
        direction = choose(state["board"])
        if direction is None:
            state = parse(run("state")) or state
            continue
        out = run("move", direction)
        nxt = parse(out)
        if nxt is None:
            sys.stderr.write("unparseable observation after move %s:\n%s\n" % (direction, out))
            return 1
        state = nxt
    final = parse(run("state"))
    print("oracle finished: board %d of %d, status %s"
          % (final["index"], final["boards"], final["status"]) if final else "oracle finished")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
