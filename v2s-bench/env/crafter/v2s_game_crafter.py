#!/usr/bin/env python3
"""v2s-game crafter -- the FROZEN agent<->game interface for the Crafter rung.

Crafter (MIT, assets included) is a 2D open-world survival game with a
22-achievement tech tree. It is here for one reason, stated in
``docs/preregistration.md`` 4.3: feeding a model the *paper* describing
this environment has been reported to beat every reinforcement-learning
baseline with no interaction at all (27.3 against DreamerV3's 14.5), and
Crafter's own leaderboard keeps a separate "External Knowledge"
category. So a documentation effect here is known to be large, and this
rung is how we find out whether our harness can detect a large effect at
all. **It is a harness diagnostic, never a v2s result.**

Design, and why:

1. **The journal is the state.** Crafter's world cannot be serialised, so
   each invocation rebuilds the episode by replaying the journal's action
   list from the seed and then applies the new actions. Resetting the
   world costs about 1.3s and a step about 0.3ms, so a 1000-action replay
   is under two seconds -- cheap enough to be stateless, and it makes the
   checker's verification literally the same replay.
2. **Always through ``env.step``.** Crafter consumes its world RNG while
   *rendering* the night-time noise, so an interface that skipped the
   render to save time would produce a different tile stream and the
   replay would not reproduce. The image is thrown away; the render is
   not skipped.
3. **One reset per process, with an explicit seed.** ``Env.reset`` seeds
   the world from ``hash((seed, episode))`` and increments ``episode``,
   so a second reset in the same process is a *different world*. Every
   invocation therefore constructs a fresh ``Env`` and resets exactly
   once. This is the same class of footgun as the NetHack Learning
   Environment's ``reset(seed=...)``, which does not seed it at all.
4. **ASCII only, <= 66 columns.** The same rendering has to appear in a
   1280x720 hermetic screencast whose bitmap font covers ASCII 32..126 at
   71 columns, so the bytes the agent reads are the bytes the video
   shows.

The observation is a text transcription of exactly the window Crafter
draws (9 wide by 7 high, player at the centre) plus the inventory and
vitals that its image carries in the bottom two rows. Nothing outside the
rendered view is disclosed -- the text interface is not a cheat sheet for
the pixel one.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

PROTOCOL = 1
GAME = "crafter"
STATE_DIR = ".v2s-game"
VIEW = (9, 7)            # Crafter's local view minus its two inventory rows
CENTRE = (VIEW[0] // 2, VIEW[1] // 2)

#: One glyph per material or object. Frozen: a renaming is a new protocol.
GLYPHS = {
    "water": "w", "grass": "g", "stone": "s", "path": "p", "sand": "n",
    "tree": "T", "lava": "L", "coal": "c", "iron": "i", "diamond": "d",
    "table": "=", "furnace": "f",
    "Player": "@", "Cow": "C", "Zombie": "Z", "Skeleton": "K",
    "Arrow": "a", "Plant": "P",
}
OFFMAP = "."

LEGEND = (
    "legend: g grass  T tree  s stone  p path  n sand  w water\n"
    "        L lava  c coal  i iron  d diamond  = table  f furnace\n"
    "        @ you  C cow  Z zombie  K skeleton  a arrow  P plant\n"
    "        . off the map"
)

FACINGS = {(-1, 0): "left", (1, 0): "right", (0, -1): "up", (0, 1): "down"}

HELP = """\
v2s-game {game} -- protocol {protocol}

  state                print the observation
  do ACTION [ACTION ...]   take one or more actions, in order
  help                 this text

The 17 actions, and nothing else:
  noop
  move_left move_right move_up move_down
  do
  sleep
  place_stone place_table place_furnace place_plant
  make_wood_pickaxe make_stone_pickaxe make_iron_pickaxe
  make_wood_sword make_stone_sword make_iron_sword

`do` the ACTION and `do` the action named "do" are different things:
`v2s-game do do` interacts with the tile you are facing -- chopping a
tree, mining stone, drinking water, hitting a cow.

What the game does not tell you, and you have to work out or be told:
which actions have requirements, what those requirements are, and what
has to be standing next to you for a `make_` action to work. An action
whose requirements are not met is accepted and simply does nothing: the
step is spent, the observation comes back unchanged, and nothing says
why.

Vitals (health, food, drink, energy) fall over time. They are restored
by eating, drinking and sleeping. At health 0 the episode ends.

Observation format (exactly this, every time):
  line 1  V2S-GAME {game} protocol={protocol}
  line 2  step=<k>/<budget>  status=<st>  daylight=<0.00-1.00>
  line 3  health=<n> food=<n> drink=<n> energy=<n>  sleeping=<yes|no>
  line 4  facing=<dir>  the tile in front of you is <material>
  line 5  inventory: <item>=<n> ...   (only non-zero items)
  line 6  achievements <n>/22: <names>   (wrapped, blank when none)
  then    the {w}x{h} view, one glyph per tile, you at the centre
  then    the legend
"""


# -- crafter ---------------------------------------------------------------


def load_crafter():
    try:
        import crafter  # noqa: F401
    except ImportError as exc:  # pragma: no cover - environment problem
        sys.stderr.write(
            "error: the crafter package is not importable (%s). This rung "
            "needs the image built from env/crafter/Dockerfile.\n" % exc
        )
        raise SystemExit(1)
    return crafter


def actions():
    from crafter import constants

    return list(constants.actions)


def achievements():
    from crafter import constants

    return list(constants.achievements)


def build(seed: int, budget: int, action_names: list[str]):
    """A fresh episode, replayed up to ``action_names``. One reset, ever."""
    crafter = load_crafter()
    env = crafter.Env(area=(64, 64), view=(9, 9), size=(64, 64),
                      reward=False, length=budget, seed=seed)
    env.reset()
    index = {name: i for i, name in enumerate(actions())}
    info = snapshot(env, None)
    done = False
    for name in action_names:
        if done:
            break
        _, _, done, raw = env.step(index[name])
        info = snapshot(env, raw)
    return env, info, done


def snapshot(env, raw):
    player = env._player
    return {
        "step": env._step,
        "pos": [int(v) for v in player.pos],
        "inventory": dict(player.inventory),
        "achievements": sorted(
            name for name, count in player.achievements.items() if count > 0
        ),
        "health": int(player.inventory["health"]),
        "dead": player.inventory["health"] <= 0,
        "sleeping": bool(player.sleeping),
        "facing": FACINGS.get(tuple(int(v) for v in player.facing), "?"),
        "daylight": round(float(env._world.daylight), 2),
    }


def ids_to_glyphs(env):
    """Map the semantic view's integer ids to this protocol's glyphs."""
    view = env._sem_view
    glyphs = {0: OFFMAP}
    for name, ident in view._mat_ids.items():
        glyphs[ident] = GLYPHS.get(name, "?") if name else OFFMAP
    for kind, ident in view._obj_ids.items():
        glyphs[ident] = GLYPHS.get(kind.__name__, "?")
    return glyphs


def render(env, budget: int, status: str) -> str:
    """The frozen observation. A transcription of Crafter's own window."""
    info = snapshot(env, None)
    semantic = env._sem_view()
    glyphs = ids_to_glyphs(env)
    area = env._world.area
    px, py = info["pos"]

    rows = []
    for y in range(VIEW[1]):
        cells = []
        for x in range(VIEW[0]):
            wx, wy = px + x - CENTRE[0], py + y - CENTRE[1]
            if 0 <= wx < area[0] and 0 <= wy < area[1]:
                cells.append(glyphs.get(int(semantic[wx, wy]), "?"))
            else:
                cells.append(OFFMAP)
        rows.append("  " + " ".join(cells))

    front = env._world[env._player.pos + env._player.facing][0] or "the edge of the map"
    items = " ".join("%s=%d" % (k, v) for k, v in info["inventory"].items()
                     if v and k not in ("health", "food", "drink", "energy"))
    unlocked = info["achievements"]
    head = [
        "V2S-GAME %s protocol=%d" % (GAME, PROTOCOL),
        "step=%d/%d  status=%s  daylight=%.2f"
        % (info["step"], budget, status, info["daylight"]),
        "health=%d food=%d drink=%d energy=%d  sleeping=%s"
        % (info["inventory"]["health"], info["inventory"]["food"],
           info["inventory"]["drink"], info["inventory"]["energy"],
           "yes" if info["sleeping"] else "no"),
        "facing=%s  the tile in front of you is %s" % (info["facing"], front),
        "inventory: " + (items or "(empty)"),
    ]
    head += wrap("achievements %d/%d: " % (len(unlocked), len(achievements())),
                 " ".join(unlocked) or "(none yet)")
    head.append("view %dx%d, you are at the centre:" % VIEW)
    return "\n".join(head + rows + [LEGEND])


def wrap(prefix: str, body: str, width: int = 66) -> list[str]:
    out = []
    line = prefix
    pad = " " * len(prefix.rstrip().rstrip(":")) if False else "  "
    for word in body.split():
        if len(line) + len(word) + 1 > width and line.strip() != prefix.strip():
            out.append(line.rstrip())
            line = pad + word + " "
        else:
            line += word + " "
    out.append(line.rstrip())
    return out


# -- journal ---------------------------------------------------------------


class Store:
    def __init__(self, root):
        self.dir = Path(root) / STATE_DIR
        self.config = self.dir / "config.json"
        self.journal = self.dir / "journal.jsonl"

    def read_config(self):
        return json.loads(self.config.read_text(encoding="utf-8"))

    def rows(self):
        if not self.journal.exists():
            return []
        out = []
        for line in self.journal.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if line.startswith("{"):
                out.append(json.loads(line))
        return out

    def append(self, row):
        self.dir.mkdir(parents=True, exist_ok=True)
        with self.journal.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(row, sort_keys=True) + "\n")

    def action_names(self):
        return [r["action"] for r in self.rows() if r.get("kind") == "step"]

    def finished(self):
        steps = [r for r in self.rows() if r.get("kind") == "step"]
        return bool(steps) and bool(steps[-1].get("done"))


def status_of(done: bool, info: dict, budget: int) -> str:
    if not done:
        return "playing"
    return "dead" if info.get("dead") else "over"


# -- verbs -----------------------------------------------------------------


def cmd_init(store, argv):
    if os.environ.get("V2S_GAME_PROVISION") != "1":
        return refuse("init is a provisioning verb; it is not part of play")
    if store.config.exists() or store.journal.exists():
        return refuse("this episode is already provisioned; it cannot be restarted")
    opts = {"seed": None, "steps": 1000}
    i = 0
    while i < len(argv):
        key = argv[i].lstrip("-")
        if key not in opts or i + 1 >= len(argv):
            return usage("init takes --seed N [--steps M]")
        opts[key] = int(argv[i + 1])
        i += 2
    if opts["seed"] is None:
        return usage("init requires --seed")
    store.dir.mkdir(parents=True, exist_ok=True)
    crafter = load_crafter()
    version = getattr(crafter, "__version__", "unknown")
    store.config.write_text(json.dumps(opts, sort_keys=True), encoding="utf-8")
    store.append({"kind": "episode", "protocol": PROTOCOL, "game": GAME,
                  "seed": opts["seed"], "steps": opts["steps"],
                  "crafter": str(version), "actions": actions()})
    env, info, done = build(opts["seed"], opts["steps"], [])
    print("provisioned %s: seed=%d steps=%d" % (GAME, opts["seed"], opts["steps"]))
    print(render(env, opts["steps"], "playing"))
    return 0


def cmd_state(store, _argv):
    config = store.read_config()
    env, info, done = build(config["seed"], config["steps"], store.action_names())
    print(render(env, config["steps"], status_of(done, info, config["steps"])))
    return 0


def cmd_do(store, argv):
    if not argv:
        return usage("do needs at least one action")
    names = set(actions())
    bad = [a for a in argv if a not in names]
    if bad:
        return usage("not an action: %s (see `help`)" % " ".join(bad))
    config = store.read_config()
    if store.finished():
        return refuse("the episode is over; there is nothing left to do")

    env, info, done = build(config["seed"], config["steps"], store.action_names())
    index = {name: i for i, name in enumerate(actions())}
    for name in argv:
        if done:
            break
        before = snapshot(env, None)
        _, _, done, _ = env.step(index[name])
        after = snapshot(env, None)
        gained = [a for a in after["achievements"] if a not in before["achievements"]]
        changed = [
            "%s%+d" % (k, v - before["inventory"][k])
            for k, v in after["inventory"].items()
            if v != before["inventory"][k]
        ]
        store.append({
            "kind": "step", "step": after["step"], "action": name,
            "done": bool(done), "pos": after["pos"],
            "achievements": after["achievements"],
            "inventory": after["inventory"],
        })
        moved = []
        if after["pos"] != before["pos"]:
            moved.append("pos=(%d,%d)" % tuple(after["pos"]))
        if after["facing"] != before["facing"]:
            moved.append("facing=" + after["facing"])
        note = " ".join(moved + changed)
        print("step %d %s: %s%s" % (
            after["step"], name, note or "nothing changed",
            (" UNLOCKED " + " ".join(gained)) if gained else "",
        ))
        if done:
            print("the episode is over: %s" % status_of(True, after, config["steps"]))
    print(render(env, config["steps"], status_of(done, snapshot(env, None), config["steps"])))
    return 0


def cmd_help(_store, _argv):
    print(HELP.format(game=GAME, protocol=PROTOCOL, w=VIEW[0], h=VIEW[1]), end="")
    return 0


VERBS = {"init": cmd_init, "state": cmd_state, "do": cmd_do, "help": cmd_help}


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
    if verb not in (cmd_init, cmd_help) and not store.config.exists():
        return refuse("no episode provisioned in %s" % (store.dir,))
    try:
        return verb(store, argv[1:])
    except (OSError, ValueError, KeyError) as exc:
        sys.stderr.write("error: %s\n" % exc)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
