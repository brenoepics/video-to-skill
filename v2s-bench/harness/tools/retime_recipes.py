#!/usr/bin/env python3
"""Re-space a recipe's narration cues so no cue speaks over the next one.

Why this matters more than it sounds like it does.

Overlapping cues mix into one audio track, and the result is garbled
speech. Garbled speech transcribes badly. The transcript is the *only*
input the transcript-only arm (A2) and the raw-transcript arm (A6) get.
So sloppy cue timing silently handicaps precisely the arms v2s is being
compared against — it would make the pipeline look better by making its
control worse, which is the one direction of error this benchmark cannot
afford.

A truncated final cue has the same character: the closing summary, which
often states the point of the procedure, is cut off in the audio but
remains fully legible on screen.

Both are fixed here rather than by hand, because the fix depends on how
long a TTS backend actually takes to say each line, and that is measured,
not guessed.

The algorithm is a forward pass: each cue starts no earlier than intended
and no earlier than the previous cue's end plus a gap. If the schedule
runs past the end of the video, the video is extended rather than the
speech being clipped — a longer tail just holds the final frame, whereas
clipped narration loses content.

A cue must stay near the thing it describes, so drift is reported per cue
and a cue that would move more than ``--max-drift`` is flagged instead of
being quietly relocated away from its own screen.

Usage:
    python3 tools/retime_recipes.py ../videos/recipes            # report only
    python3 tools/retime_recipes.py ../videos/recipes --write
"""

from __future__ import annotations

import argparse
import json
import math
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from v2sbench.render import load_recipe  # noqa: E402
from v2sbench.render.narration import detect_backend, synthesise  # noqa: E402

#: Silence between one cue ending and the next beginning. Enough that a
#: listener, and an ASR model, hear two utterances rather than one.
DEFAULT_GAP = 0.35

#: How far a cue may be moved before it risks describing the wrong frame.
DEFAULT_MAX_DRIFT = 2.5

#: Slack for --check. Cue starts are written to two decimals and durations
#: to one, so a re-run always finds a residual of a few hundredths. Real
#: overlaps are whole seconds, so this separates rounding from a problem.
CHECK_TOLERANCE = 0.1


def cue_durations(recipe, backend: str, work: Path) -> list[float]:
    """Measure each cue on its own, so one cue's length is not another's."""
    out: list[float] = []
    for index, cue in enumerate(recipe.narration):
        single = (cue,)
        with tempfile.TemporaryDirectory(dir=work) as tmp:
            track = synthesise(single, 10_000.0, Path(tmp), backend=backend)
        out.append(track.spoken_secs)
        if track.spoken_secs <= 0:
            print(f"    cue {index} produced no audio: {cue.text[:60]!r}", file=sys.stderr)
    return out


def retime(
    starts: list[float], durations: list[float], total: float, gap: float
) -> tuple[list[float], float]:
    """Forward pass. Returns the new starts and the duration needed."""
    new: list[float] = []
    cursor = 0.0
    for start, length in zip(starts, durations, strict=True):
        begin = max(start, cursor)
        new.append(round(begin, 2))
        cursor = begin + length + gap
    # `cursor` sits one gap past the last cue's end, so the speech really
    # ends at `cursor - gap`. Extend only when that overruns the video, and
    # extend to exactly what is required: adding slack on every pass would
    # make the tool never converge, since each run would find the previous
    # run's slack insufficient and add more.
    speech_ends = cursor - gap
    needed = total if speech_ends <= total else _ceil_tenth(speech_ends)
    return new, needed


def _ceil_tenth(value: float) -> float:
    """Round up to a tenth of a second, avoiding float residue.

    `round()` would sometimes land a hundredth below the true end and leave
    the final cue clipped by an inaudible but test-visible margin.
    """
    return math.ceil(value * 10.0 - 1e-9) / 10.0


def process(
    path: Path, backend: str, *, gap: float, max_drift: float, write: bool, check: bool = False
) -> bool:
    recipe = load_recipe(path)
    if not recipe.narration:
        print(f"{path.stem}: no narration")
        return True

    with tempfile.TemporaryDirectory() as work:
        durations = cue_durations(recipe, backend, Path(work))

    starts = [cue.t for cue in recipe.narration]
    new_starts, needed = retime(starts, durations, recipe.duration_secs, gap)
    drifts = [round(n - o, 2) for n, o in zip(new_starts, starts, strict=True)]
    worst = max(drifts) if drifts else 0.0
    extended = needed > recipe.duration_secs

    if check:
        # In check mode the question is not "can this be fixed" but "is it
        # already right": any required move at all means a cue currently
        # speaks over another, which garbles the transcript the control
        # arms depend on.
        needs_change = (
            worst > CHECK_TOLERANCE
            or round(needed - recipe.duration_secs, 3) > CHECK_TOLERANCE
        )
        status = "NEEDS RETIMING" if needs_change else "ok"
    else:
        status = "ok" if worst <= max_drift else "DRIFT"
    print(
        f"{path.stem:<24} worst drift {worst:+.2f}s"
        f"{f' · duration {recipe.duration_secs} -> {needed}' if extended else ''}"
        f"  [{status}]"
    )
    for index, (drift, length) in enumerate(zip(drifts, durations, strict=True)):
        if abs(drift) > max_drift:
            text = recipe.narration[index].text
            print(f"    cue {index} moves {drift:+.2f}s (speaks {length:.1f}s): {text[:70]!r}")

    if check:
        return not needs_change

    if write:
        payload = json.loads(path.read_text())
        for cue, start in zip(payload.get("narration", []), new_starts, strict=True):
            cue["t"] = start
        if extended:
            payload["duration_secs"] = needed
        path.write_text(json.dumps(payload, indent=2) + "\n")

    return worst <= max_drift


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("directory", type=Path)
    parser.add_argument("--write", action="store_true", help="rewrite the recipes in place")
    parser.add_argument(
        "--check", action="store_true",
        help="fail if any recipe's narration needs retiming (for CI)",
    )
    parser.add_argument("--gap", type=float, default=DEFAULT_GAP)
    parser.add_argument("--max-drift", type=float, default=DEFAULT_MAX_DRIFT)
    parser.add_argument("--only", action="append")
    args = parser.parse_args(argv[1:])

    backend = detect_backend()
    if backend is None:
        print(
            "no TTS backend: install espeak-ng (Linux) or run on macOS. Cue lengths "
            "cannot be measured without one, and guessing them is what produced the "
            "overlaps in the first place.",
            file=sys.stderr,
        )
        return 2
    print(f"backend: {backend}\n")

    paths = sorted(args.directory.glob("*.json"))
    if args.only:
        paths = [p for p in paths if p.stem in set(args.only)]
    if not paths:
        print(f"no recipes in {args.directory}", file=sys.stderr)
        return 1

    # A list, not a generator: `all` short-circuits, and stopping at the
    # first drifting recipe would hide every later one.
    results = [
        process(
            p, backend, gap=args.gap, max_drift=args.max_drift,
            write=args.write, check=args.check,
        )
        for p in paths
    ]
    clean = all(results)
    if args.check:
        print(
            "\n"
            + ("every recipe's narration fits its slots" if clean
               else "some recipes have cues that speak over each other; run with --write")
        )
    else:
        print("\n" + ("rewritten" if args.write else "report only; pass --write to apply"))
    return 0 if clean else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
