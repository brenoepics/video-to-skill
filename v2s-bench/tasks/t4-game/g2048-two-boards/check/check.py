#!/usr/bin/env python3
"""Checker for g2048-two-boards.

**This checker does not trust the game.** It never reads the score the
engine computed. It reads the engine's journal as a *claim* -- a seed and
an ordered list of moves -- and replays that claim under its own
independent implementation of the 2048 rules, written from the frozen
protocol rather than imported from the engine. Everything scored is scored
from the replay.

Two defects that cost published suites their meaning are ruled out by that
design, not by a string check:

- An agent that edits the journal, or patches the engine, gains nothing:
  the only journal that passes is one whose move list really does reach
  the target under canonical rules from the provisioned seed, which is
  exactly what playing the game means.
- A checker that merely asked "does a result file say 256?" would pass the
  wrong procedure. ``journal-is-consistent`` recomputes every board, every
  spawn and every score and compares them to what the engine recorded, so
  the engine and the canonical rules must agree move for move.

The deliberate duplication of the rules is checked in the other direction
too: ``env/2048/calibrate.py crosscheck`` plays random games through both
implementations and asserts identical trajectories.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

# -- what the environment provisioned (tasks/t4-game/g2048-two-boards) ----
EXPECTED_SEEDS = (20481, 20482)
EXPECTED_BUDGET = 500
EXPECTED_GOAL = 1024
TARGET = 256           # the binary milestone threshold, on EVERY board
PROTOCOL = 1
SIZE = 4

MASK64 = (1 << 64) - 1
GOLDEN = 0x9E3779B97F4A7C15


# -- canonical rules (independent of the engine) ---------------------------


class Rng:
    """SplitMix64, as pinned in env/2048/PROTOCOL.md."""

    def __init__(self, seed: int) -> None:
        self.state = seed & MASK64

    def draw(self) -> int:
        self.state = (self.state + GOLDEN) & MASK64
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
        return (z ^ (z >> 31)) & MASK64


def as_rows(flat: list[int]) -> list[list[int]]:
    return [list(flat[r * SIZE : (r + 1) * SIZE]) for r in range(SIZE)]


def as_flat(rows: list[list[int]]) -> list[int]:
    return [v for row in rows for v in row]


def transpose(rows: list[list[int]]) -> list[list[int]]:
    return [list(col) for col in zip(*rows)]


def squash(row: list[int]) -> tuple[list[int], int]:
    """Slide a row left and merge each pair once. Returns (row, gained)."""
    tiles = [v for v in row if v]
    out: list[int] = []
    gained = 0
    while tiles:
        head = tiles.pop(0)
        if tiles and tiles[0] == head:
            tiles.pop(0)
            head *= 2
            gained += head
        out.append(head)
    return out + [0] * (SIZE - len(out)), gained


def apply_move(flat: list[int], direction: str) -> tuple[list[int], int, bool]:
    """One move, written row-first rather than index-first on purpose."""
    rows = as_rows(flat)
    if direction in ("up", "down"):
        rows = transpose(rows)
    if direction in ("right", "down"):
        rows = [row[::-1] for row in rows]
    gained = 0
    moved = []
    for row in rows:
        squashed, got = squash(row)
        gained += got
        moved.append(squashed)
    if direction in ("right", "down"):
        moved = [row[::-1] for row in moved]
    if direction in ("up", "down"):
        moved = transpose(moved)
    out = as_flat(moved)
    return out, gained, out != list(flat)


def spawn(flat: list[int], rng: Rng) -> tuple[list[int], int | None, int]:
    empties = [i for i, v in enumerate(flat) if not v]
    if not empties:
        return list(flat), None, 0
    cell = empties[rng.draw() % len(empties)]
    value = 4 if rng.draw() % 10 == 0 else 2
    out = list(flat)
    out[cell] = value
    return out, cell, value


def opening(seed: int) -> tuple[list[int], Rng]:
    rng = Rng(seed)
    flat = [0] * (SIZE * SIZE)
    for _ in range(2):
        flat, _, _ = spawn(flat, rng)
    return flat, rng


def stuck(flat: list[int]) -> bool:
    return not any(apply_move(flat, d)[2] for d in ("up", "down", "left", "right"))


def replay(seed: int, moves: list[dict], budget: int, goal: int) -> dict:
    """Replay a board's recorded moves. Returns the canonical outcome."""
    flat, rng = opening(seed)
    score = 0
    played = 0
    faults: list[str] = []
    mismatches: list[str] = []
    for entry in moves:
        direction = entry.get("dir")
        if direction not in ("up", "down", "left", "right"):
            faults.append("move %d: %r is not a direction" % (played + 1, direction))
            break
        if played >= budget:
            faults.append("move %d exceeds the budget of %d" % (played + 1, budget))
            break
        if max(flat) >= goal or stuck(flat):
            faults.append("move %d was played after the board had ended" % (played + 1,))
            break
        nxt, gained, changed = apply_move(flat, direction)
        if not changed:
            faults.append("move %d (%s) does not change the board" % (played + 1, direction))
            break
        flat, cell, value = spawn(nxt, rng)
        score += gained
        played += 1
        if entry.get("board") is not None and list(entry["board"]) != flat:
            mismatches.append("move %d: board differs from the canonical replay" % played)
        if entry.get("score") is not None and int(entry["score"]) != score:
            mismatches.append("move %d: score %r, canonical %d" % (played, entry["score"], score))
        if entry.get("spawn") is not None and list(entry["spawn"]) != [cell, value]:
            mismatches.append("move %d: spawn %r, canonical %r" % (played, entry["spawn"], [cell, value]))
    return {
        "seed": seed, "moves": played, "score": score, "board": flat,
        "max_tile": max(flat), "faults": faults, "mismatches": mismatches,
    }


# -- journal ---------------------------------------------------------------


def read_journal(path: Path) -> tuple[list[dict], str | None]:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        return [], "cannot read %s: %s" % (path, exc)
    rows: list[dict] = []
    for number, line in enumerate(text.splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        try:
            row = json.loads(line)
        except ValueError:
            return rows, "line %d of the journal is not JSON" % number
        if isinstance(row, dict):
            rows.append(row)
    return rows, None


def main() -> int:
    work = Path(sys.argv[1])
    log = ""
    if len(sys.argv) > 2:
        try:
            log = Path(sys.argv[2]).read_text(encoding="utf-8", errors="replace")
        except OSError:
            log = ""

    results: list[dict] = []

    def add(ident: str, passed: bool, detail: str, required: bool = True) -> None:
        results.append({"id": ident, "passed": bool(passed), "required": required,
                        "detail": detail})

    rows, broken = read_journal(work / ".v2s-game" / "journal.jsonl")
    headers = [r for r in rows if r.get("kind") == "board"]
    add("journal-present", bool(headers) and not broken,
        broken or ("%d board header(s), %d journal row(s)" % (len(headers), len(rows))))

    seeds = [h.get("seed") for h in headers]
    indices = [h.get("index") for h in headers]
    as_provisioned = (
        tuple(seeds) == EXPECTED_SEEDS
        and indices == list(range(1, len(EXPECTED_SEEDS) + 1))
        and all(h.get("budget") == EXPECTED_BUDGET for h in headers)
        and all(h.get("goal") == EXPECTED_GOAL for h in headers)
        and all(h.get("protocol") == PROTOCOL for h in headers)
    )
    add("boards-as-provisioned", as_provisioned,
        "boards=%r seeds=%r (expected %r)" % (indices, seeds, list(EXPECTED_SEEDS)))

    outcomes = []
    for header in headers:
        moves = [r for r in rows
                 if r.get("kind") == "move" and r.get("index") == header.get("index")]
        moves.sort(key=lambda r: r.get("move") or 0)
        outcomes.append(replay(int(header.get("seed") or 0), moves,
                               int(header.get("budget") or EXPECTED_BUDGET),
                               int(header.get("goal") or EXPECTED_GOAL)))

    faults = [f for o in outcomes for f in o["faults"]]
    add("moves-are-legal", bool(outcomes) and not faults,
        "; ".join(faults[:3]) if faults else
        "%d move(s) replayed, all legal" % sum(o["moves"] for o in outcomes))

    mismatches = [m for o in outcomes for m in o["mismatches"]]
    add("journal-is-consistent", bool(outcomes) and not mismatches,
        "; ".join(mismatches[:3]) if mismatches else
        "engine and canonical rules agree on every move")

    restart = re.search(r"(?mi)(v2s-game\s+init\b|rm\b[^\n]*\.v2s-game\b)", log)
    add("run-not-restarted", restart is None,
        "command log shows %r" % (restart.group(0),) if restart else
        "no re-provisioning in the command log")

    tiles = [o["max_tile"] for o in outcomes]
    reached = bool(outcomes) and len(outcomes) == len(EXPECTED_SEEDS) and min(tiles) >= TARGET
    add("every-board-reaches-target", reached,
        "max tile per board: %r (target %d on every board)" % (tiles, TARGET))

    total = sum(o["score"] for o in outcomes)
    add("graded-score", True,
        "score=%d max_tile=%r moves=%r" % (total, tiles, [o["moves"] for o in outcomes]),
        required=False)

    add("every-board-reaches-512", bool(outcomes) and min(tiles or [0]) >= 512,
        "max tile per board: %r" % (tiles,), required=False)
    add("any-board-reaches-1024", bool(tiles) and max(tiles) >= 1024,
        "best board: %r" % (max(tiles) if tiles else 0,), required=False)

    played = re.search(r"(?mi)v2s-game\s+move\b", log)
    add("played-through-the-interface", played is not None,
        "the command log %s a `v2s-game move`" % ("shows" if played else "does not show"),
        required=False)
    # The known bypass (mirroring the tile PRNG instead of playing a
    # strategy) is recorded, not punished: see env/2048/PROTOCOL.md.
    peeked = re.search(r"(?mi)(splitmix|0x9e3779b9|(cat|less|head|grep|python3?)\b[^\n]*v2s-game\b)", log)
    add("engine-source-not-inspected", peeked is None,
        "command log shows %r" % (peeked.group(0)[:60],) if peeked else "no source inspection seen",
        required=False)

    milestones = []
    for index, tile in enumerate(tiles, start=1):
        for step in (256, 512, 1024, 2048):
            if tile >= step:
                milestones.append("board-%d-tile-%d" % (index, step))
    if reached:
        milestones.append("every-board-tile-256")
    if tiles and min(tiles) >= 512:
        milestones.append("every-board-tile-512")

    print(json.dumps({"assertions": results, "score": total, "milestones": milestones}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
