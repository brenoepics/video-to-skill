#!/usr/bin/env python3
"""Generate the tier-1 screencast recipes from real tool runs.

Runs each demo session with the REAL tools in a temp directory, captures
the real output, and writes ``videos/recipes/<id>.json``. Nothing in a
recipe is invented output: if a tool's behaviour changes, re-run this and
the recipes change with it. Hand-editing a recipe is how a video comes to
teach something the tool does not do.

It also owns the narration budget. Cue text is written to a per-slot time
budget derived from still spacing, because two failure modes sit on either
side of it and both bias the benchmark the same way:

* Cues too long overlap, the mixed audio garbles, and the ASR transcript
  degrades -- which handicaps the transcript-only (A2) and raw-transcript
  (A6) arms, the very arms v2s is measured against.
* Cues trimmed to fit the clock lose *content*, which handicaps those same
  arms by making the narration carry less.

So trimming for time must remove verbosity, never a teaching point. After
any rewrite, diff the cues and check each change for whether it dropped a
word or a fact.

Verify with::

    cd ../../harness && python3 tools/retime_recipes.py ../videos/recipes --check

which must report +0.00s drift and no duration extension for every recipe.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

BENCH = Path("/home/user/video-to-skill/v2s-bench")
SCRATCH = Path(__file__).resolve().parent
ENV = dict(os.environ, PATH=str(SCRATCH / "bin") + ":" + os.environ["PATH"])
MAX_TYPE, MAX_OUT = 48, 50

RECIPES: list[dict] = []


def recipe(**kw):
    RECIPES.append(kw)


def run(work: Path, cmd: str) -> list[str]:
    res = subprocess.run(cmd, shell=True, cwd=work, env=ENV, capture_output=True, text=True)
    lines = res.stdout.splitlines() + res.stderr.splitlines()
    return lines


def build(spec: dict) -> dict:
    work = Path(tempfile.mkdtemp(prefix="recipe-" + spec["id"] + "-"))
    for cmd in spec.get("setup", []):
        res = subprocess.run(cmd, shell=True, cwd=work, env=ENV, capture_output=True, text=True)
        if res.returncode != 0:
            raise SystemExit("setup failed in %s: %s\n%s" % (spec["id"], cmd, res.stderr))
    caps: dict[str, str] = {}
    gap = spec.get("gap", 6.0)
    stills = [{"t": 0.0, "terminal": {"title": spec.get("title_bar", "work - bash")}}]
    narration = [{"t": 0.5, "text": spec["intro"]}]
    timings = []
    warnings = []
    for index, step in enumerate(spec["steps"]):
        cmd = step["cmd"].format(**caps)
        label = "%02d-%s" % (index + 1, step["label"])
        typed_t = round(4.0 + gap * index, 1)
        out_t = round(typed_t + 3.0, 1)
        lines = run(work, cmd)
        for name, pattern in step.get("capture", {}).items():
            found = None
            for line in lines:
                match = re.search(pattern, line)
                if match:
                    found = match.group(1)
                    break
            if found is None:
                raise SystemExit("capture %s failed in %s step %s\n%s" % (name, spec["id"], label, lines))
            caps[name] = found
        if len(cmd) > MAX_TYPE:
            warnings.append("%s %s: typed %d chars: %s" % (spec["id"], label, len(cmd), cmd))
        for line in lines:
            if len(line) > MAX_OUT:
                warnings.append("%s %s: output %d chars: %s" % (spec["id"], label, len(line), line))
        stills.append({"t": typed_t, "step": label, "terminal": {"type": cmd}})
        stills.append({"t": out_t, "step": label, "terminal": {"output": lines}})
        narration.append({"t": round(typed_t + 0.8, 1), "text": step["say"]})
        timings.append((label, typed_t, out_t, cmd, lines))
    end = stills[-1]["t"] + 7.0
    narration.append({"t": round(stills[-1]["t"] + 1.8, 1), "text": spec["outro"]})
    doc = {
        "schema_version": 1,
        "id": spec["id"],
        "title": spec["title"],
        "width": 1280,
        "height": 720,
        "fps": 10,
        "duration_secs": round(end, 1),
        "stills": stills,
        "narration": narration,
        "notes": spec["notes"],
    }
    return {"doc": doc, "timings": timings, "warnings": warnings, "work": work, "caps": caps}


# ----------------------------------------------------------------- zep
recipe(
    id="zep-seal-with-grant",
    title="Sealing a zep drop properly",
    intro="A closed drop is not always a finished drop.",
    outro="That is the difference, and it is easy to miss.",
    notes="Teaches that closing a drop without its nonce leaves a provisional record, and where the nonce can be read back from.",
    steps=[
        dict(label="open-a-drop", cmd="zep open ship-42",
             say="We open a drop, and zep hands us a nonce. It matters at the end."),
        dict(label="stage-a-fact", cmd="zep put ship-42 port 8080",
             say="Stage a fact into it."),
        dict(label="close-it-plainly", cmd="zep seal ship-42",
             say="Now watch what happens if we just close it."),
        dict(label="look-at-the-state", cmd="zep show ship-42",
             say="It closed, but look at the state line, and at the anchor. Not finished."),
        dict(label="read-the-nonce-back", cmd="zep grant ship-42",
             capture={"nonce": r"^grant: (\S+)"},
             say="The nonce is still on disk, and zep will print it again."),
        dict(label="close-with-the-nonce", cmd="zep seal ship-42 --grant {nonce}",
             say="So we close it again, passing that nonce along."),
        dict(label="confirm-it-is-sealed", cmd="zep show ship-42",
             say="Sealed this time, with an anchor."),
    ],
)

recipe(
    id="zep-stage-three-facts",
    title="Building a zep drop from a handover file",
    intro="The whole life of a drop. The order matters.",
    outro="Everything has to go in before you close it.",
    notes="Teaches that staging requires an existing drop, that the close needs the nonce, and that staging is refused after sealing.",
    setup=["printf 'tier gold\\nowner search\\n' > notes.txt"],
    gap=5.5,
    steps=[
        dict(label="read-the-handover", cmd="cat notes.txt",
             say="The facts are in a file, a name and a value each."),
        dict(label="stage-too-early", cmd="zep put ship-43 tier gold",
             say="Go straight at it and zep stops us. Nothing to stage into yet."),
        dict(label="create-the-drop", cmd="zep open ship-43",
             capture={"nonce": r"^grant: (\S+)"},
             say="So we create the drop first. Again, a nonce."),
        dict(label="stage-the-first-fact", cmd="zep put ship-43 tier gold",
             say="Now the facts go in, one at a time."),
        dict(label="stage-the-second-fact", cmd="zep put ship-43 owner search",
             say="Same again for the second."),
        dict(label="close-with-the-nonce", cmd="zep seal ship-43 --grant {nonce}",
             say="Then we close it, passing the nonce along."),
        dict(label="too-late-to-stage", cmd="zep put ship-43 extra x",
             say="Add anything now and it refuses."),
        dict(label="confirm-the-drop", cmd="zep show ship-43",
             say="Sealed, anchored, both facts in."),
    ],
)

recipe(
    id="zep-chain-anchor",
    title="Chaining one zep drop onto another",
    intro="Drops can be chained. The link is set up early.",
    outro="Declared when the drop is created, and only a real one will do.",
    notes="Teaches that the parent is declared at creation from the parent's anchor, and that zep validates the anchor.",
    setup=[
        "zep open ship-42", "zep put ship-42 port 8080",
        "zep seal ship-42 --grant 661471fe6aa6",
    ],
    steps=[
        dict(label="look-at-the-ledger", cmd="zep ledger",
             say="One sealed drop here, and what it ended up with."),
        dict(label="read-the-parent", cmd="zep show ship-42",
             capture={"anchor": r"^anchor: (\S+)"},
             say="This is the value the next drop has to point at."),
        dict(label="create-the-child", cmd="zep open ship-44 --after {anchor}",
             capture={"nonce": r"^grant: (\S+)"},
             say="So as we create it, we say what it follows, carrying that value over."),
        dict(label="stage-a-fact", cmd="zep put ship-44 note hotfix",
             say="Then it is an ordinary drop."),
        dict(label="close-with-the-nonce", cmd="zep seal ship-44 --grant {nonce}",
             say="Close it with its own nonce, not the parent's."),
        dict(label="confirm-the-parent", cmd="zep show ship-44",
             say="And there is the parent line, pointing back where we came from."),
        dict(label="made-up-parent", cmd="zep open ship-45 --after deadbeefdeadbeef",
             say="And you cannot invent the value."),
    ],
)

# ---------------------------------------------------------------- lath
recipe(
    id="lath-stage-pair",
    title="Staging values into a lath layer",
    intro="lath keeps settings in layers. Staging one is odd.",
    outro="Watch the order, and read the layer back after.",
    notes="Teaches lath's value-before-key argument order and shows the reversed order silently staging a key named after the value.",
    gap=5.5,
    steps=[
        dict(label="make-a-tree", cmd="lath init",
             say="Start a tree here."),
        dict(label="stage-a-value", cmd="lath set edge gateway-1 host",
             say="Now stage a setting. Watch the order, it is not what you expect."),
        dict(label="look-at-the-layer", cmd="lath show edge",
             say="lath reads it back the way it stored it. Worth checking."),
        dict(label="the-other-way-round", cmd="lath set edge host gateway-2",
             say="Here is the trap. The other way round, no complaint."),
        dict(label="see-the-damage", cmd="lath show edge",
             say="Look at the layer. An entry nobody asked for, and no error."),
        dict(label="drop-the-mistake", cmd="lath unset edge gateway-2",
             say="So we take that out again."),
        dict(label="stage-the-second-value", cmd="lath set edge standard tier",
             say="And stage the second the same way."),
        dict(label="confirm-the-layer", cmd="lath show edge",
             say="Two settings, the names we wanted."),
    ],
)

recipe(
    id="lath-prod-wins",
    title="Which lath layer wins a conflict",
    intro="Two layers, one setting. Which one wins?",
    outro="Change the order, not the layers.",
    notes="Teaches first-stacked-wins precedence by merging the same two layers in both orders and showing both results.",
    setup=["lath init"],
    gap=5.0,
    steps=[
        dict(label="stage-the-edge-value", cmd="lath set edge 3 retries",
             say="One layer says three."),
        dict(label="stage-the-stage-value", cmd="lath set stage 9 retries",
             say="The other says nine, under the same name."),
        dict(label="stack-edge-first", cmd="lath stack edge",
             say="This one goes into the order first. Note the position."),
        dict(label="stack-stage-second", cmd="lath stack stage",
             say="Then the other."),
        dict(label="merge-them", cmd="lath flatten --out tmp/a.json",
             say="Now merge the whole thing into a file."),
        dict(label="see-who-won", cmd="cat tmp/a.json",
             say="Look at the value that survived, then back at the order we built."),
        dict(label="take-it-out", cmd="lath unstack edge",
             say="To get the other value, we take this one out."),
        dict(label="put-it-back-last", cmd="lath stack edge",
             say="And add it again. Watch the position now."),
        dict(label="check-the-order", cmd="lath layers",
             say="Here is the order, heading included. Read it carefully."),
        dict(label="merge-again", cmd="lath flatten --out tmp/b.json",
             say="Merge again, into a second file."),
        dict(label="see-the-difference", cmd="cat tmp/b.json",
             say="A different answer, same two layers."),
    ],
)

recipe(
    id="lath-pin-fingerprint",
    title="Pinning a lath merge into the next one",
    intro="A merge prints a fingerprint. We carry one forward.",
    outro="Carry what the first merge printed, add it, and merge again.",
    notes="Teaches that the fingerprint is only available from the merge output, that it is carried as a value, and that the second merge has a different fingerprint.",
    setup=[
        "lath init", "lath set edge gateway-1 host", "lath set stage 9 retries",
        "lath stack stage", "lath stack edge",
    ],
    steps=[
        dict(label="check-the-order", cmd="lath layers",
             say="Two layers, already in the order we want."),
        dict(label="merge-once", cmd="lath flatten --out tmp/one.json",
             capture={"mark": r"^fingerprint: (\S+)"},
             say="Merge them. Look at the middle line, that is what we keep."),
        dict(label="pin-the-fingerprint", cmd="lath set tagged {mark} built_from",
             say="We stage it into a layer of its own. Watch the order."),
        dict(label="look-at-the-pin", cmd="lath show tagged",
             say="There it is, under a name of our choosing."),
        dict(label="stack-the-pin", cmd="lath stack tagged",
             say="A layer outside the order contributes nothing, so we add it."),
        dict(label="merge-again", cmd="lath flatten --out tmp/two.json",
             say="Merge again. Notice this one reports a different fingerprint."),
        dict(label="see-the-result", cmd="cat tmp/two.json",
             say="The file carries the earlier one, not this one's."),
    ],
)


# ---------------------------------------------------------------- kiln
KILN_PIECES = [
    "mkdir -p pieces",
    "printf 'one ok\\n' > pieces/01-one.txt",
    "printf 'two ok\\n' > pieces/02-two.txt",
    "printf 'three ok\\n' > pieces/03-three.txt",
    "printf 'four ok\\n' > pieces/04-four.txt",
]
COBALT = "printf 'fire-lane: cobalt\\nshard-glob: pieces/*.txt\\nhold-count: %d\\n' > kiln.profile"

recipe(
    id="kiln-author-profile",
    title="Describing a kiln lane and preparing it",
    intro="kiln does nothing until the lane is described, in a file.",
    outro="Three settings, then prepare it, and kiln knows its lane.",
    notes="Teaches the profile's filename and its three field names, which kiln's own help never mentions, and the name of the preparation step.",
    setup=KILN_PIECES,
    steps=[
        dict(label="nothing-to-work-from", cmd="kiln prime",
             say="Ask it with nothing set up and it names the file it wants."),
        dict(label="name-the-lane", cmd="echo 'fire-lane: cobalt' >> kiln.profile",
             say="So we write that file. First the lane's name."),
        dict(label="say-which-pieces", cmd="echo 'shard-glob: pieces/*.txt' >> kiln.profile",
             say="Then which files make up a batch."),
        dict(label="say-how-many", cmd="echo 'hold-count: 2' >> kiln.profile",
             say="And how many one batch holds."),
        dict(label="read-it-back", cmd="cat kiln.profile",
             say="Read it back. Those three field names are the whole thing, nothing else."),
        dict(label="prepare-the-lane", cmd="kiln prime",
             say="Now prepare the lane. It says what it found and how many matched."),
        dict(label="check-the-state", cmd="kiln peek",
             say="What kiln holds. Prepared, not fired."),
    ],
)

recipe(
    id="kiln-fire-newest-three",
    title="Changing a kiln lane's batch size",
    intro="Too many pieces per batch. Fixing that is two steps.",
    outro="Edit, prepare again, then fire. In that order.",
    notes="Teaches the hold-count field, that a changed description leaves the lane stale until it is prepared again, and that a batch keeps the last pieces in name order.",
    setup=KILN_PIECES + [COBALT % 4, "kiln prime"],
    steps=[
        dict(label="look-at-the-lane", cmd="kiln peek",
             say="Prepared, and set to take four at a time. We want two."),
        dict(label="change-the-count", cmd="sed -i 's/count: 4/count: 2/' kiln.profile",
             say="So we edit that one field."),
        dict(label="read-it-back", cmd="cat kiln.profile",
             say="Two it is."),
        dict(label="notice-it-is-stale", cmd="kiln peek",
             say="But look at the last line. It still holds what it read before."),
        dict(label="firing-is-refused", cmd="kiln fire",
             say="And it will not fire like that. One line, no advice."),
        dict(label="prepare-it-again", cmd="kiln prime",
             say="So prepare it again, which picks the edit up."),
        dict(label="fire-the-batch", cmd="kiln fire",
             capture={"batch": r"^batch: (\S+)"},
             say="Now it fires, names the batch, and says where it went."),
        dict(label="see-which-pieces", cmd="grep '^-- ' out/cobalt-{batch}.rollup",
             say="And here is what it took, which may surprise you."),
    ],
)

recipe(
    id="kiln-chain-batches",
    title="Chaining two kiln firings together",
    intro="Two firings, the second following the first.",
    outro="The batch you fired goes in the description. Prepare again, then fire.",
    notes="Teaches that descent is declared in the profile with prior-batch from the earlier firing's batch, and that the lane must be prepared again afterwards.",
    setup=KILN_PIECES + [COBALT % 2],
    steps=[
        dict(label="prepare-the-lane", cmd="kiln prime",
             say="Prepare the lane as usual."),
        dict(label="fire-the-first-batch", cmd="kiln fire",
             capture={"first": r"^batch: (\S+)"},
             say="Fire it once. Note the batch, we will reuse it."),
        dict(label="record-the-prior", cmd="echo 'prior-batch: {first}' >> kiln.profile",
             say="It goes into the lane description, as one more field, carrying that batch over."),
        dict(label="firing-is-refused", cmd="kiln fire",
             say="Firing now gets us nowhere, because the description changed."),
        dict(label="prepare-it-again", cmd="kiln prime",
             say="Prepare it again, and it reports what it follows."),
        dict(label="fire-the-second-batch", cmd="kiln fire",
             capture={"second": r"^batch: (\S+)"},
             say="Fire again. New batch, naming the one before."),
        dict(label="read-the-header", cmd="head -4 out/cobalt-{second}.rollup",
             say="The artifact says so too, right at the top."),
        dict(label="check-the-state", cmd="kiln peek",
             say="And kiln is holding the chain, both ends of it."),
    ],
)

# ---------------------------------------------------------------- prow
PROW_PLAN = [
    "prow plan --wave 1 --unit queue",
    "prow plan --wave 1 --unit mailer",
    "prow plan --wave 2 --unit reports",
]

recipe(
    id="prow-interlock-sheet",
    title="Making a prow sheet sequence its waves",
    intro="One sheet can promise two very different things.",
    outro="Without that switch the order is for show. With it, each wave waits.",
    notes="Teaches that rendering only sequences the waves when the interlock switch is passed, and shows the parallel gates it writes otherwise.",
    setup=PROW_PLAN,
    steps=[
        dict(label="look-at-the-plan", cmd="prow status",
             say="Three units here, over two waves."),
        dict(label="render-it-plainly", cmd="prow sheet --out plan.txt",
             say="Render it the obvious way. It is happy, and gives us an id."),
        dict(label="read-the-gates", cmd="cat plan.txt",
             say="But read the gate lines. Nothing waits for anything, nothing is sequenced."),
        dict(label="render-it-locked", cmd="prow sheet --out plan.txt --interlock",
             say="Render again, asking for the waves to be locked together. Different id already."),
        dict(label="read-the-gates-again", cmd="cat plan.txt",
             say="Now the first wave starts and the second waits for it."),
        dict(label="check-the-status", cmd="prow status",
             say="Two renderings, nothing released."),
    ],
)

recipe(
    id="prow-reshape-waves",
    title="Reshaping a prow plan and re-rendering",
    intro="Plans change. The sheet on disk does not notice.",
    outro="Then render again. The sheet is a snapshot, not a view.",
    notes="Teaches that re-planning a unit moves it, that removal is its own step, and that the rendered sheet does not follow later plan changes.",
    setup=PROW_PLAN,
    gap=5.5,
    steps=[
        dict(label="render-the-old-shape", cmd="prow sheet --out plan.txt --interlock",
             say="Here is the sheet as the plan stands today."),
        dict(label="move-a-unit", cmd="prow plan --wave 2 --unit mailer",
             say="This one moves to the later wave, not added twice."),
        dict(label="retire-a-unit", cmd="prow drop --unit reports",
             say="And this one is retired, which is its own step, not a far-off wave."),
        dict(label="the-sheet-is-stale", cmd="cat plan.txt",
             say="But the sheet still describes the old shape. No warning."),
        dict(label="check-the-plan", cmd="prow status",
             say="The plan itself is right: two units, two waves."),
        dict(label="render-the-new-shape", cmd="prow sheet --out plan.txt --interlock",
             say="So we render it again, locked together as before."),
        dict(label="read-the-new-sheet", cmd="cat plan.txt",
             say="Now it matches the plan, retired unit gone."),
    ],
)

recipe(
    id="prow-latch-sheet",
    title="Releasing a rendered prow sheet",
    intro="Rendering a sheet is not releasing it.",
    outro="If the file is touched, render it again before releasing.",
    notes="Teaches that the release step is latch rather than release, that it takes the id the render printed, and that it refuses once the file has changed.",
    setup=["prow plan --wave 1 --unit queue", "prow plan --wave 2 --unit mailer"],
    steps=[
        dict(label="render-the-sheet", cmd="prow sheet --out plan.txt --interlock",
             capture={"ident": r"^sheet: (\S+)"},
             say="Render it locked together, and keep the id it prints."),
        dict(label="the-obvious-name", cmd="prow release {ident}",
             say="The obvious word is not the word. No such command."),
        dict(label="release-it", cmd="prow latch {ident}",
             say="This is the one, and it takes that id."),
        dict(label="check-the-status", cmd="prow status",
             say="And the status names what has been released."),
        dict(label="touch-the-file", cmd="echo '# note' >> plan.txt",
             say="Now suppose somebody tidies the file afterwards."),
        dict(label="release-is-refused", cmd="prow latch {ident}",
             say="Releasing it again is refused. What was released no longer matches."),
        dict(label="render-it-again", cmd="prow sheet --out plan.txt --interlock",
             capture={"again": r"^sheet: (\S+)"},
             say="So we render it again, and it tells us the id."),
        dict(label="release-the-new-one", cmd="prow latch {again}",
             say="Release that, and it goes through."),
    ],
)


def main() -> int:
    out_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else BENCH / "videos" / "recipes"
    out_dir.mkdir(parents=True, exist_ok=True)
    all_warnings = []
    for spec in RECIPES:
        built = build(spec)
        (out_dir / (spec["id"] + ".json")).write_text(
            json.dumps(built["doc"], indent=2) + "\n", encoding="utf-8"
        )
        all_warnings += built["warnings"]
        print("== %s  %.1fs  %d stills" % (spec["id"], built["doc"]["duration_secs"], len(built["doc"]["stills"])))
        for label, typed_t, out_t, cmd, lines in built["timings"]:
            print("   t=%5.1f/%5.1f  %-26s %s" % (typed_t, out_t, label, cmd))
            for line in lines:
                print("                 | %s" % line)
    if all_warnings:
        print("\n!! LINE WIDTH WARNINGS")
        for w in all_warnings:
            print("  " + w)
    return 0


if __name__ == "__main__":
    sys.exit(main())
