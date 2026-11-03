#!/usr/bin/env python3
"""Oracle policy for crafter-first-pickaxe.

A scripted competent policy, not a replay: it reads each observation,
finds its target in the 9x7 view by breadth-first search over walkable
tiles, walks there, turns to face it and acts. Given a different seed it
plays a different route, which is the point of an oracle -- it discharges
"this task is achievable" rather than "this recording exists".

It knows exactly what the task's video teaches and nothing more:

  * wood comes from facing a tree and using `do`, one tree at a time;
  * a table costs two wood and must be PLACED before anything can be
    crafted, and crafting only works while standing next to it;
  * stone cannot be mined at all until a wood pickaxe exists.

Every action goes through the frozen `v2s-game` interface. The oracle
never imports crafter, never reads the journal and never looks outside
the 9x7 window the interface prints.
"""

from __future__ import annotations

import re
import subprocess
import sys
from collections import deque

GAME = "v2s-game"
W, H = 9, 7
CENTRE = (W // 2, H // 2)
WALKABLE = set("gpn")
DIRS = {"move_left": (-1, 0), "move_right": (1, 0),
        "move_up": (0, -1), "move_down": (0, 1)}
STEP_LIMIT = 900


def call(*args):
    done = subprocess.run([GAME, *args], capture_output=True, text=True)
    if done.returncode not in (0, 3):
        sys.stderr.write(done.stderr)
        raise SystemExit("`%s %s` failed" % (GAME, " ".join(args)))
    return done.stdout


def parse(text):
    """Read the frozen observation. Returns None when there is none."""
    rows = text.splitlines()
    try:
        start = next(i for i, line in enumerate(rows)
                     if line.startswith("view %dx%d" % (W, H)))
    except StopIteration:
        return None
    grid = [row.split() for row in rows[start + 1 : start + 1 + H]]
    if any(len(row) != W for row in grid):
        return None
    meta = {}
    for line in rows:
        found = re.match(r"step=(\d+)/(\d+)\s+status=(\w+)", line)
        if found:
            meta["step"], meta["budget"], meta["status"] = (
                int(found.group(1)), int(found.group(2)), found.group(3))
        found = re.match(r"health=(\d+) food=(\d+) drink=(\d+) energy=(\d+)", line)
        if found:
            meta.update(zip(("health", "food", "drink", "energy"),
                            (int(g) for g in found.groups())))
        found = re.match(r"facing=(\w+)\s+the tile in front of you is (.+)$", line)
        if found:
            meta["facing"], meta["front"] = found.group(1), found.group(2).strip()
        if line.startswith("inventory: "):
            meta["inventory"] = dict(
                (k, int(v)) for k, v in re.findall(r"(\w+)=(\d+)", line))
        if line.startswith("achievements "):
            meta["unlocked"] = set(re.findall(r"[a-z]+_[a-z_]+", line.split(":", 1)[1]))
            collecting = True
            continue
        if line.startswith("... ") and meta.get("unlocked") is not None:
            # the achievement list is the one line that can wrap
            meta["unlocked"] |= set(re.findall(r"[a-z]+_[a-z_]+", line[4:]))
    meta["grid"] = grid
    meta.setdefault("unlocked", set())
    meta.setdefault("inventory", {})
    return meta


def at(grid, cell):
    x, y = cell
    return grid[y][x] if 0 <= x < W and 0 <= y < H else "."


def route_to(grid, glyphs):
    """Shortest walk to a tile adjacent to any of ``glyphs``.

    Returns (moves, facing_move) or None. ``moves`` walks there, then
    ``facing_move`` turns to face the target: in Crafter a move into a
    tile you cannot enter only changes your facing, which is what makes
    "walk up to it and then act" a two-part manoeuvre at all.
    """
    start = CENTRE
    seen = {start: []}
    queue = deque([start])
    while queue:
        cell = queue.popleft()
        path = seen[cell]
        for action, (dx, dy) in DIRS.items():
            target = (cell[0] + dx, cell[1] + dy)
            if target in seen:
                continue
            glyph = at(grid, target)
            if glyph in glyphs:
                return path, action
            if glyph in WALKABLE and len(path) < 24:
                seen[target] = path + [action]
                queue.append(target)
    return None


def most_open(grid):
    """A direction with room to walk, for when the target is out of view."""
    counts = []
    for action, (dx, dy) in DIRS.items():
        tally = 0
        for step in range(1, 4):
            cell = (CENTRE[0] + dx * step, CENTRE[1] + dy * step)
            if at(grid, cell) in WALKABLE:
                tally += 1
            else:
                break
        counts.append((tally, action))
    counts.sort(reverse=True)
    return counts[0][1] if counts[0][0] else None


class Oracle:
    def __init__(self):
        self.state = parse(call("state"))
        if self.state is None:
            raise SystemExit("no observation from `v2s-game state`")
        self.explored = 0

    def act(self, *actions):
        actions = [a for a in actions if a]
        if not actions:
            return False
        out = call("do", *actions)
        nxt = parse(out)
        if nxt is None:
            sys.stderr.write("unparseable observation:\n%s\n" % out)
            raise SystemExit(1)
        self.state = nxt
        return True

    def wood(self):
        return self.state["inventory"].get("wood", 0)

    def playing(self):
        return self.state["status"] == "playing" and self.state["step"] < STEP_LIMIT

    def go_and_do(self, glyphs, times=1):
        """Walk to something and use it. True when the trip was made."""
        found = route_to(self.state["grid"], glyphs)
        if found is None:
            return self.wander()
        moves, facing = found
        return self.act(*moves, facing, *(["do"] * times))

    def wander(self):
        direction = most_open(self.state["grid"])
        if direction is None:
            return self.act("move_up", "move_left")
        self.explored += 1
        return self.act(*([direction] * 4))

    def face_walkable(self):
        """Turn to face a tile a table can be placed on."""
        for action, (dx, dy) in DIRS.items():
            if at(self.state["grid"], (CENTRE[0] + dx, CENTRE[1] + dy)) in WALKABLE:
                return action
        return None

    def run(self):
        # 1. Five wood: two for the table, one for the pickaxe, spare for a
        #    sword and a second table if the first one is left behind.
        while self.playing() and self.wood() < 5:
            self.go_and_do("T")
            self.top_up()
        # 2. The table has to exist before anything can be made.
        if self.playing() and "place_table" not in self.state["unlocked"]:
            self.act(self.face_walkable(), "place_table")
        # 3. Craft while standing next to it.
        if self.playing():
            self.act("make_wood_pickaxe", "make_wood_sword")
        # 4. Only now is stone mineable.
        tries = 0
        while self.playing() and "collect_stone" not in self.state["unlocked"] and tries < 60:
            self.go_and_do("s")
            self.top_up()
            tries += 1
        # 5. Cheap extras, once the chain is done.
        if self.playing() and self.state["inventory"].get("stone", 0) >= 1:
            self.act(self.face_walkable(), "place_stone")
        return self.state

    def top_up(self):
        """Drink or eat when it is nearly free to do so."""
        if not self.playing():
            return
        if self.state.get("drink", 9) <= 4 and self.go_and_do("w", times=5):
            return
        if self.state.get("food", 9) <= 4:
            self.go_and_do("C", times=3)


def main():
    oracle = Oracle()
    final = oracle.run()
    print("oracle finished at step %d, status %s, %d achievement(s): %s"
          % (final["step"], final["status"], len(final["unlocked"]),
             " ".join(sorted(final["unlocked"]))))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
