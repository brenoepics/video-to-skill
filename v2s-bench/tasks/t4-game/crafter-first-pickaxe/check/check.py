#!/usr/bin/env python3
"""Checker for crafter-first-pickaxe.

**This checker does not trust the game's report.** It reads the journal as
a claim -- a seed and an ordered list of actions -- and replays that claim
through Crafter itself, scoring the replay. An edited journal therefore
cannot manufacture an achievement: the only action list that passes is one
that really does unlock the chain from the provisioned seed.

Replaying through the real library means the rules are not duplicated
here, which raises a different question: what if the agent edited the
library? So the hashes of Crafter's rule-bearing files are verified first,
against ``env/crafter/PACKAGE.sha256.json``. A replay is only evidence
about the agent if the rules it replays under are the pinned ones.

Two Crafter-specific traps the replay has to respect, both documented in
env/crafter/PROTOCOL.md:

- ``Env.reset`` seeds the world from ``hash((seed, episode))`` and
  increments ``episode``, so the episode must be built with one fresh
  ``Env`` and exactly one ``reset``.
- Crafter consumes its world RNG while rendering night-time noise, so the
  replay must go through ``env.step`` and must not skip the render, or it
  diverges from the run it is checking.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import pathlib
import re
import sys

EXPECTED_SEED = 7
EXPECTED_STEPS = 1000
PROTOCOL = 1
CHAIN = ("collect_wood", "place_table", "make_wood_pickaxe", "collect_stone")
LADDER = ("collect_drink", "eat_cow", "place_stone", "collect_coal",
          "make_stone_pickaxe", "make_stone_sword", "wake_up")
#: Travels with the checker rather than living in the image: the checker
#: directory is copied in after the agent has stopped, so the manifest is
#: never readable from the workdir and needs no path inside the image.
PACKAGE_MANIFEST = pathlib.Path(__file__).resolve().parent / "PACKAGE.sha256.json"


def read_journal(path: pathlib.Path):
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        return [], "cannot read %s: %s" % (path, exc)
    rows = []
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


def package_verdict():
    """Are Crafter's rule-bearing files the pinned ones?"""
    try:
        import crafter
    except ImportError as exc:
        return False, "crafter is not importable: %s" % exc
    package = pathlib.Path(crafter.__file__).parent
    manifest_path = PACKAGE_MANIFEST
    if not manifest_path.exists():
        return False, "no pinned manifest beside the checker (%s)" % manifest_path.name
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except ValueError as exc:
        return False, "unreadable manifest: %s" % exc
    bad = []
    for name, want in sorted(manifest.get("files", {}).items()):
        target = package / name
        if not target.exists():
            bad.append("%s missing" % name)
            continue
        got = hashlib.sha256(target.read_bytes()).hexdigest()
        if got != want:
            bad.append("%s changed" % name)
    if bad:
        return False, "; ".join(bad)
    return True, "crafter %s matches the pinned manifest" % manifest.get("package", "?")


def replay(seed: int, budget: int, action_names):
    """Rebuild the episode. Returns (trace, faults)."""
    import crafter
    from crafter import constants

    vocabulary = {name: i for i, name in enumerate(constants.actions)}
    faults = []
    bad = [name for name in action_names if name not in vocabulary]
    if bad:
        return None, ["not in the action vocabulary: %s" % ", ".join(sorted(set(bad))[:4])]
    if len(action_names) > budget:
        return None, ["%d actions exceed the budget of %d" % (len(action_names), budget)]

    env = crafter.Env(area=(64, 64), view=(9, 9), size=(64, 64),
                      reward=False, length=budget, seed=seed)
    env.reset()
    steps = []
    done = False
    for position, name in enumerate(action_names, start=1):
        if done:
            faults.append("action %d (%s) was played after the episode ended" % (position, name))
            break
        _, _, done, info = env.step(vocabulary[name])
        steps.append({
            "step": position,
            "achievements": sorted(k for k, v in info["achievements"].items() if v > 0),
            "inventory": dict(info["inventory"]),
            "pos": [int(v) for v in info["player_pos"]],
        })
    player = env._player
    return {
        "steps": steps,
        "unlocked": sorted(k for k, v in player.achievements.items() if v > 0),
        "used": len(steps),
        "dead": player.inventory["health"] <= 0,
        "done": bool(done),
    }, faults


def main() -> int:
    work = pathlib.Path(sys.argv[1])
    log = ""
    if len(sys.argv) > 2:
        try:
            log = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8", errors="replace")
        except OSError:
            log = ""

    results = []

    def add(ident, passed, detail, required=True):
        results.append({"id": ident, "passed": bool(passed), "required": required,
                        "detail": detail})

    rows, broken = read_journal(work / ".v2s-game" / "journal.jsonl")
    headers = [r for r in rows if r.get("kind") == "episode"]
    actions = [r for r in rows if r.get("kind") == "step"]
    add("journal-present", len(headers) == 1 and not broken,
        broken or "%d episode header(s), %d action row(s)" % (len(headers), len(actions)))

    header = headers[0] if headers else {}
    provisioned = (
        header.get("seed") == EXPECTED_SEED
        and header.get("steps") == EXPECTED_STEPS
        and header.get("protocol") == PROTOCOL
        and header.get("game") == "crafter"
    )
    add("episode-as-provisioned", provisioned,
        "seed=%r steps=%r protocol=%r (expected %d/%d/%d)"
        % (header.get("seed"), header.get("steps"), header.get("protocol"),
           EXPECTED_SEED, EXPECTED_STEPS, PROTOCOL))

    package_ok, package_detail = package_verdict()
    add("crafter-unmodified", package_ok, package_detail)

    restart = re.search(r"(?mi)(v2s-game\s+init\b|rm\b[^\n]*\.v2s-game\b)", log)
    add("run-not-restarted", restart is None,
        "command log shows %r" % (restart.group(0),) if restart
        else "no re-provisioning in the command log")

    trace, faults = (None, ["no journal to replay"])
    if provisioned and package_ok:
        actions.sort(key=lambda r: r.get("step") or 0)
        trace, faults = replay(EXPECTED_SEED, EXPECTED_STEPS,
                               [r.get("action") for r in actions])

    add("actions-replay", trace is not None and not faults,
        "; ".join(faults[:3]) if faults else "%d action(s) replayed" % trace["used"])

    mismatches = []
    if trace:
        for recorded, replayed in zip(actions, trace["steps"]):
            if sorted(recorded.get("achievements") or []) != replayed["achievements"]:
                mismatches.append("step %d: achievements differ" % replayed["step"])
            if recorded.get("inventory") and dict(recorded["inventory"]) != replayed["inventory"]:
                mismatches.append("step %d: inventory differs" % replayed["step"])
            if len(mismatches) > 3:
                break
    add("journal-is-consistent", trace is not None and not mismatches,
        "; ".join(mismatches[:3]) if mismatches
        else "the journal agrees with the replay at every step")

    unlocked = set(trace["unlocked"]) if trace else set()
    missing = [name for name in CHAIN if name not in unlocked]
    add("tool-chain-complete", not missing and bool(trace),
        "missing: %s" % ", ".join(missing) if missing
        else "all four of %s" % ", ".join(CHAIN))

    add("graded-score", True,
        "score=%d achievements=%s steps_used=%d died=%s"
        % (len(unlocked), ",".join(sorted(unlocked)) or "(none)",
           trace["used"] if trace else 0, trace["dead"] if trace else "n/a"),
        required=False)
    add("survived-the-episode", bool(trace) and not trace["dead"],
        "died=%s" % (trace["dead"] if trace else "n/a"), required=False)
    add("played-through-the-interface",
        re.search(r"(?mi)v2s-game\s+do\b", log) is not None,
        "the command log %s a `v2s-game do`"
        % ("shows" if re.search(r"(?mi)v2s-game\s+do\b", log) else "does not show"),
        required=False)

    # Crafter's own data.yaml encodes every recipe, and in a single-uid
    # container it cannot be hidden. Reading it is legitimate discovery
    # (preregistration 3.1), so it is recorded, never punished -- but how
    # often an arm does it bounds how much of this rung's effect was
    # knowledge the environment was carrying all along.
    looked = re.search(
        r"(?mi)(site-packages/crafter|crafter/data\.yaml|from\s+crafter\s+import|"
        r"constants\.(make|place|collect)|import\s+crafter\b)", log)
    add("rules-not-read-from-the-library", looked is None,
        "command log shows %r" % (looked.group(0)[:50],) if looked
        else "the recipes were not read out of the installed library",
        required=False)

    milestones = [name for name in CHAIN + LADDER if name in unlocked]
    if not missing:
        milestones.append("tool-chain-complete")

    print(json.dumps({"assertions": results, "score": float(len(unlocked)),
                      "milestones": milestones}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
