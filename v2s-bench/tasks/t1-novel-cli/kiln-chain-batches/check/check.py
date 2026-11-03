#!/usr/bin/env python3
"""Checker for kiln-chain-batches.

`second-rollup-chains-the-first` is the discriminating assertion. Two
firings with something changed in between -- an extra shard, a different
hold count -- leave two well-formed rollups for the lane, two batch
records, and fresh state, and look exactly like a chain from the outside;
what they do not have is the older batch named as the newer one's prior.
Both batch ids are recomputed from the artifacts' own contents, so neither
rollup can be edited into shape afterwards.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

LANE = "amber"


def canon(obj: object) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"))


def digest(tag: str, payload: str, width: int) -> str:
    return hashlib.sha256((tag + "\n" + payload).encode("utf-8")).hexdigest()[:width]


def parse_rollup(path: Path) -> tuple[dict[str, str], str]:
    text = path.read_text(encoding="utf-8", errors="replace")
    head, sep, body = text.partition("--\n")
    fields: dict[str, str] = {}
    for line in head.splitlines():
        if ":" in line:
            key, _, value = line.partition(":")
            fields[key.strip()] = value.strip()
    return fields, (body if sep else "")


def main() -> int:
    work = Path(sys.argv[1])
    results: list[dict] = []

    def add(ident: str, passed: bool, detail: str, required: bool = True) -> None:
        results.append(
            {"id": ident, "passed": bool(passed), "required": required, "detail": detail}
        )

    out_dir = work / "out"
    rollups = sorted(out_dir.glob("*.rollup")) if out_dir.is_dir() else []
    add("two-rollups", len(rollups) == 2, "rollups in out/: %r" % ([p.name for p in rollups],))

    parsed = [(p, *parse_rollup(p)) for p in rollups]
    roots = [item for item in parsed if item[1].get("prior") == "-"]
    children = [item for item in parsed if item[1].get("prior", "-") != "-"]

    add(
        "first-rollup-unchained",
        len(roots) == 1 and roots[0][1].get("lane") == LANE,
        "rollups with no prior: %r" % ([item[0].name for item in roots],),
    )

    chained = False
    detail = "no chained rollup found"
    root_batch = roots[0][1].get("batch") if len(roots) == 1 else None
    child = children[0] if len(children) == 1 else None
    if child is not None:
        chained = child[1].get("prior") == root_batch and child[1].get("lane") == LANE
        detail = "newer rollup %s names prior=%r, older batch is %r" % (
            child[0].name,
            child[1].get("prior"),
            root_batch,
        )
    add("second-rollup-chains-the-first", bool(chained), detail)

    ids_ok = len(parsed) == 2
    notes = []
    for path, head, body in parsed:
        want = digest(
            "kiln-batch-v1", LANE + "\n" + head.get("prior", "-") + "\n" + body, 8
        )
        this_ok = head.get("batch") == want and path.name == "%s-%s.rollup" % (LANE, want)
        notes.append("%s: recorded=%r recomputed=%r" % (path.name, head.get("batch"), want))
        ids_ok = ids_ok and this_ok
    add("both-batch-ids-recompute", ids_ok, "; ".join(notes) or "no rollups")

    records_ok = False
    detail = "batch records missing"
    batch_dir = work / ".kiln" / "batches"
    if batch_dir.is_dir() and root_batch and child is not None:
        child_batch = child[1].get("batch")
        loaded = {}
        for name in (root_batch, child_batch):
            try:
                loaded[name] = json.loads((batch_dir / (str(name) + ".json")).read_text(encoding="utf-8"))
            except (OSError, ValueError):
                loaded[name] = {}
        stamps = all(
            rec.get("stamp")
            == digest("kiln-batch-rec-v1", canon({k: v for k, v in rec.items() if k != "stamp"}), 16)
            for rec in loaded.values()
        )
        records_ok = (
            stamps
            and loaded.get(root_batch, {}).get("prior") in (None, "")
            and loaded.get(child_batch, {}).get("prior") == root_batch
            and len(sorted(batch_dir.glob("*.json"))) == 2
        )
        detail = "records=%r stamps valid=%s" % (
            {k: v.get("prior") for k, v in loaded.items()},
            stamps,
        )
    add("batch-records-agree", records_ok, detail)

    try:
        profile_text = (work / "kiln.profile").read_text(encoding="utf-8")
    except OSError:
        profile_text = ""
    try:
        state = json.loads((work / ".kiln" / "state.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        state = {}
    fresh = bool(profile_text) and state.get("profile_stamp") == digest(
        "kiln-profile-v1", profile_text, 16
    )
    add(
        "state-fresh-and-chained",
        fresh
        and state.get("prior") == root_batch
        and child is not None
        and state.get("last_batch") == child[1].get("batch")
        and state.get("stamp")
        == digest("kiln-state-v1", canon({k: v for k, v in state.items() if k != "stamp"}), 16),
        "state prior=%r last_batch=%r built from the profile on disk=%s"
        % (state.get("prior"), state.get("last_batch"), fresh),
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
