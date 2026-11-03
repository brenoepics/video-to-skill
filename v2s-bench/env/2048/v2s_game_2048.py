#!/usr/bin/env python3
"""v2s-game 2048 -- the FROZEN agent<->game interface for tier t4-game.

This file is the whole environment: rules, text protocol and journal. It
is content-hashed (``env/freeze.json``) and must not be edited without
bumping ``PROTOCOL`` and re-freezing, because a harness change moves
measured agent performance by more than a model change does (ARC-AGI-3
swung one model from 62.7% to 99.9% by changing only the harness). An
unpinned interface makes every number in this tier uninterpretable.

Three properties are deliberate and load-bearing:

1. **Stdlib only, ASCII only, <= 66 columns.** The same rendering has to
   appear in a hermetic 1280x720 screencast whose bitmap font covers
   ASCII 32..126 at 71 columns, so the observation the agent reads and
   the observation the video shows are the same bytes.
2. **The journal is a proof, not a report.** Every accepted move is
   appended to ``.v2s-game/journal.jsonl`` with the seed in the session
   header. The checker replays that move list under its own independent
   copy of these rules and scores *the replay*, never this file's
   arithmetic. Editing the journal therefore cannot manufacture a win:
   the only thing that passes is a move sequence that really does reach
   the target under canonical rules.
3. **The seed is provisioned, not chosen.** ``init`` writes it once;
   the agent-facing verbs cannot set it. Seed-shopping -- restarting
   until an easy board appears -- would otherwise be the dominant
   strategy and would measure luck.

Verbs (the frozen action vocabulary):

    v2s-game help
    v2s-game state
    v2s-game move <up|down|left|right> [<dir> ...]
    v2s-game new
    v2s-game init --seed N --goal T --budget B --sessions S   (provisioning)

Exit status: 0 on an accepted verb, 2 on a usage error, 3 when a verb is
refused by the rules (``new`` while a game is still playing, ``move``
after the game ended).
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

PROTOCOL = 1
GAME = "2048"
STATE_DIR = ".v2s-game"
SIZE = 4
DIRS = ("up", "down", "left", "right")

MASK64 = (1 << 64) - 1
GOLDEN = 0x9E3779B97F4A7C15

HELP = """\
v2s-game {game} -- protocol {protocol}

  state                 print the board and status
  move DIR [DIR ...]    play one or more moves, in order
  new                   start the next attempt (refused while playing)
  help                  this text

DIR is one of: up down left right.  Nothing else is accepted.

Rules, in full:
  - 4x4 grid. Each move slides every tile as far as it can go in DIR.
  - Two equal tiles that collide merge into their sum, once per move,
    resolved from the far edge inwards. Score increases by the sum.
  - A move that would leave the board unchanged is REJECTED: it is not
    counted against the move budget and spawns nothing. Probing a
    direction therefore costs nothing but a line of output.
  - After every accepted move one tile appears in a uniformly chosen
    empty cell: a 2 with probability 0.9, a 4 with probability 0.1.
  - status=won as soon as a tile reaches the goal; the attempt then
    ends. status=lost when no direction changes the board.
    status=exhausted when the move budget runs out.
  - The seed is fixed by the environment and is the same for every
    attempt. You cannot change it, and `new` is refused while the
    current attempt is still playing.

Observation format (exactly this, every time):
  line 1  V2S-GAME {game} protocol={protocol}
  line 2  session=<i> of <n>  seed=<s>  move=<k>/<budget>  status=<st>
  line 3  score=<n>  max_tile=<n>  goal=<n>
  then    a {size}x{size} grid, cells right-aligned in width 5, empty cells blank
  last    row-major: comma-separated cells, '.' for empty
"""


# -- deterministic randomness ---------------------------------------------
#
# SplitMix64 rather than `random`: the checker, and any reimplementation
# of these rules in another language, must produce the identical tile
# stream from the identical seed. A language's own PRNG is not a contract.


def _mix(state):
    state = (state + GOLDEN) & MASK64
    z = state
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
    return (z ^ (z >> 31)) & MASK64, state


class Rng:
    def __init__(self, state):
        self.state = state & MASK64

    def next(self):
        value, self.state = _mix(self.state)
        return value


# -- rules -----------------------------------------------------------------


def empty_board():
    return [0] * (SIZE * SIZE)


def _line(board, index, direction):
    """The four cells a move pushes toward the far edge, nearest edge first."""
    if direction == "left":
        return [index * SIZE + c for c in range(SIZE)]
    if direction == "right":
        return [index * SIZE + c for c in range(SIZE - 1, -1, -1)]
    if direction == "up":
        return [r * SIZE + index for r in range(SIZE)]
    return [r * SIZE + index for r in range(SIZE - 1, -1, -1)]


def _collapse(values):
    """Slide and merge one line toward its head. Returns (line, gained)."""
    tiles = [v for v in values if v]
    out = []
    gained = 0
    i = 0
    while i < len(tiles):
        if i + 1 < len(tiles) and tiles[i] == tiles[i + 1]:
            merged = tiles[i] * 2
            out.append(merged)
            gained += merged
            i += 2
        else:
            out.append(tiles[i])
            i += 1
    return out + [0] * (SIZE - len(out)), gained


def slide(board, direction):
    """Apply one move. Returns (board, gained, changed) and never mutates."""
    if direction not in DIRS:
        raise ValueError("unknown direction %r" % (direction,))
    out = list(board)
    gained = 0
    for index in range(SIZE):
        cells = _line(board, index, direction)
        collapsed, got = _collapse([board[c] for c in cells])
        gained += got
        for cell, value in zip(cells, collapsed):
            out[cell] = value
    return out, gained, out != list(board)


def spawn(board, rng):
    """Place one tile. Returns (board, cell, value) or (board, None, 0)."""
    empties = [i for i, v in enumerate(board) if not v]
    if not empties:
        return list(board), None, 0
    cell = empties[rng.next() % len(empties)]
    value = 4 if rng.next() % 10 == 0 else 2
    out = list(board)
    out[cell] = value
    return out, cell, value


def legal(board):
    return [d for d in DIRS if slide(board, d)[2]]


def start(seed):
    """The provisioned opening position: two tiles, from the seed alone."""
    rng = Rng(seed)
    board = empty_board()
    for _ in range(2):
        board, _, _ = spawn(board, rng)
    return board, rng


# -- rendering (frozen: these exact bytes) ---------------------------------


def render(session):
    board = session["board"]
    head = "V2S-GAME %s protocol=%d" % (GAME, PROTOCOL)
    meta = "session=%d of %d  seed=%d  move=%d/%d  status=%s" % (
        session["index"], session["sessions"], session["seed"],
        session["moves"], session["budget"], session["status"],
    )
    stats = "score=%d  max_tile=%d  goal=%d" % (
        session["score"], max(board), session["goal"]
    )
    rule = "+" + "+".join(["-" * 5] * SIZE) + "+"
    rows = [rule]
    for r in range(SIZE):
        cells = board[r * SIZE : (r + 1) * SIZE]
        rows.append("|" + "|".join(("%5d" % v) if v else " " * 5 for v in cells) + "|")
        rows.append(rule)
    flat = ",".join(str(v) if v else "." for v in board)
    return "\n".join([head, meta, stats] + rows + ["row-major: " + flat])


# -- persistence -----------------------------------------------------------


class Store:
    def __init__(self, root):
        self.dir = Path(root) / STATE_DIR
        self.config = self.dir / "config.json"
        self.session = self.dir / "session.json"
        self.journal = self.dir / "journal.jsonl"

    def read_config(self):
        return json.loads(self.config.read_text(encoding="utf-8"))

    def write_session(self, session):
        self.dir.mkdir(parents=True, exist_ok=True)
        tmp = self.session.with_suffix(".tmp")
        tmp.write_text(json.dumps(session, sort_keys=True), encoding="utf-8")
        os.replace(str(tmp), str(self.session))

    def read_session(self):
        if not self.session.exists():
            return None
        return json.loads(self.session.read_text(encoding="utf-8"))

    def append(self, row):
        self.dir.mkdir(parents=True, exist_ok=True)
        with self.journal.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(row, sort_keys=True) + "\n")

    def sessions_opened(self):
        if not self.journal.exists():
            return 0
        count = 0
        for line in self.journal.read_text(encoding="utf-8").splitlines():
            if line.strip().startswith("{") and '"kind": "session"' in line:
                count += 1
        return count


def new_session(store, config, index):
    board, rng = start(config["seed"])
    session = {
        "index": index,
        "sessions": config["sessions"],
        "seed": config["seed"],
        "goal": config["goal"],
        "budget": config["budget"],
        "board": board,
        "score": 0,
        "moves": 0,
        "status": "playing",
        "rng": rng.state,
    }
    store.append({
        "kind": "session", "protocol": PROTOCOL, "game": GAME,
        "index": index, "seed": config["seed"], "goal": config["goal"],
        "budget": config["budget"], "sessions": config["sessions"],
        "board": board,
    })
    store.write_session(session)
    return session


def apply_move(store, session, direction):
    """One move. Returns the line printed for it."""
    board, gained, changed = slide(session["board"], direction)
    if not changed:
        store.append({
            "kind": "reject", "index": session["index"],
            "move": session["moves"], "dir": direction,
        })
        return "move %s: rejected (board unchanged; not counted)" % (direction,)

    rng = Rng(session["rng"])
    board, cell, value = spawn(board, rng)
    session["board"] = board
    session["rng"] = rng.state
    session["score"] += gained
    session["moves"] += 1
    reached = max(board) >= session["goal"]
    if reached:
        session["status"] = "won"
    elif session["moves"] >= session["budget"]:
        session["status"] = "exhausted"
    elif not legal(board):
        session["status"] = "lost"

    store.append({
        "kind": "move", "index": session["index"], "move": session["moves"],
        "dir": direction, "gained": gained, "score": session["score"],
        "spawn": [cell, value], "board": board, "status": session["status"],
    })
    where = "none" if cell is None else "r%dc%d=%d" % (cell // SIZE, cell % SIZE, value)
    return "move %d %s: ok  gained=%d  spawn=%s" % (
        session["moves"], direction, gained, where
    )


# -- verbs -----------------------------------------------------------------


def cmd_init(store, argv):
    opts = {"seed": None, "goal": 512, "budget": 400, "sessions": 3}
    i = 0
    while i < len(argv):
        key = argv[i].lstrip("-")
        if key not in opts or i + 1 >= len(argv):
            return usage("init takes --seed N [--goal T] [--budget B] [--sessions S]")
        opts[key] = int(argv[i + 1])
        i += 2
    if opts["seed"] is None:
        return usage("init requires --seed")
    store.dir.mkdir(parents=True, exist_ok=True)
    store.config.write_text(json.dumps(opts, sort_keys=True), encoding="utf-8")
    session = new_session(store, opts, 1)
    print("initialised %s: seed=%d goal=%d budget=%d sessions=%d"
          % (GAME, opts["seed"], opts["goal"], opts["budget"], opts["sessions"]))
    print(render(session))
    return 0


def cmd_state(store, _argv):
    session = store.read_session()
    if session is None:
        return refuse("no game in progress; the environment did not provision one")
    print(render(session))
    return 0


def cmd_new(store, _argv):
    session = store.read_session()
    config = store.read_config()
    if session is not None and session["status"] == "playing":
        return refuse(
            "attempt %d is still playing (move=%d/%d); `new` is refused until it "
            "ends" % (session["index"], session["moves"], session["budget"])
        )
    index = (session["index"] + 1) if session else 1
    if index > config["sessions"]:
        return refuse("no attempts left: %d of %d used" % (config["sessions"], config["sessions"]))
    session = new_session(store, config, index)
    print(render(session))
    return 0


def cmd_move(store, argv):
    if not argv:
        return usage("move needs at least one direction")
    bad = [a for a in argv if a not in DIRS]
    if bad:
        return usage("not a direction: %s (use up down left right)" % " ".join(bad))
    session = store.read_session()
    if session is None:
        return refuse("no game in progress")
    if session["status"] != "playing":
        return refuse(
            "attempt %d is %s; start the next one with `new`"
            % (session["index"], session["status"])
        )
    for direction in argv:
        print(apply_move(store, session, direction))
        if session["status"] != "playing":
            print("attempt %d ended: %s" % (session["index"], session["status"]))
            break
    store.write_session(session)
    print(render(session))
    return 0


def cmd_help(_store, _argv):
    print(HELP.format(game=GAME, protocol=PROTOCOL, size=SIZE), end="")
    return 0


VERBS = {
    "init": cmd_init, "state": cmd_state, "move": cmd_move,
    "new": cmd_new, "help": cmd_help,
}


def usage(message):
    sys.stderr.write("usage error: %s\n" % message)
    return 2


def refuse(message):
    sys.stderr.write("refused: %s\n" % message)
    return 3


def main(argv):
    root = os.environ.get("V2S_GAME_ROOT") or os.getcwd()
    store = Store(root)
    if not argv or argv[0] in ("-h", "--help"):
        return cmd_help(store, [])
    verb = VERBS.get(argv[0])
    if verb is None:
        return usage("unknown verb %r; try `help`" % (argv[0],))
    if verb is not cmd_init and not store.config.exists():
        return refuse("no game provisioned in %s" % (store.dir,))
    try:
        return verb(store, argv[1:])
    except (OSError, ValueError) as exc:
        sys.stderr.write("error: %s\n" % exc)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
