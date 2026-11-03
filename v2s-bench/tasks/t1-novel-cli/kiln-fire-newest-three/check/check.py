#!/usr/bin/env python3
"""Checker for kiln-fire-newest-three.

`rollup-holds-the-newest-three` and `batch-id-recomputes` are the two
assertions that separate this from a smoke test. Firing the lane as it
arrives produces one well-formed rollup for lane amber with kiln's own
batch record beside it -- it simply holds five shards instead of three,
and kiln says nothing about it. Trimming that artifact by hand afterwards
leaves a file whose recorded batch no longer digests to its contents.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

LANE = "amber"
HOLD = 3


def canon(obj: object) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"))


def digest(tag: str, payload: str, width: int) -> str:
    return hashlib.sha256((tag + "\n" + payload).encode("utf-8")).hexdigest()[:width]


def parse_rollup(text: str) -> tuple[dict[str, str], str]:
    head, sep, body = text.partition("--\n")
    fields: dict[str, str] = {}
    for line in head.splitlines():
        if ":" in line:
            key, _, value = line.partition(":")
            fields[key.strip()] = value.strip()
    return fields, (body if sep else "")


def expected_body(work: Path, glob: str, hold: int) -> tuple[str, list[str]]:
    held = sorted(p for p in work.glob(glob) if p.is_file())[-hold:]
    body = "".join(
        "-- %s\n%s" % (p.name, p.read_text(encoding="utf-8", errors="replace")) for p in held
    )
    return body, [p.name for p in held]


def main() -> int:
    work = Path(sys.argv[1])
    results: list[dict] = []

    def add(ident: str, passed: bool, detail: str, required: bool = True) -> None:
        results.append(
            {"id": ident, "passed": bool(passed), "required": required, "detail": detail}
        )

    try:
        profile_text = (work / "kiln.profile").read_text(encoding="utf-8")
    except OSError:
        profile_text = ""
    fields: dict[str, str] = {}
    for line in profile_text.splitlines():
        if ":" in line and not line.strip().startswith("#"):
            key, _, value = line.partition(":")
            fields[key.strip()] = value.strip()
    add(
        "profile-holds-three",
        fields.get("hold-count") == str(HOLD) and fields.get("fire-lane") == LANE,
        "profile fields=%r" % (fields,),
    )

    out_dir = work / "out"
    rollups = sorted(out_dir.glob("*.rollup")) if out_dir.is_dir() else []
    add(
        "exactly-one-rollup",
        len(rollups) == 1,
        "rollups in out/: %r" % ([p.name for p in rollups],),
    )

    head: dict[str, str] = {}
    body = ""
    if len(rollups) == 1:
        head, body = parse_rollup(rollups[0].read_text(encoding="utf-8", errors="replace"))

    want_body, want_names = expected_body(work, fields.get("shard-glob", "shards/*.txt"), HOLD)
    add(
        "rollup-holds-the-newest-three",
        head.get("lane") == LANE
        and head.get("shards") == str(HOLD)
        and body == want_body
        and want_names == ["03-merge.txt", "04-audit.txt", "05-pack.txt"],
        "header=%r shards wanted=%r body matches=%s" % (head, want_names, body == want_body),
    )

    batch = head.get("batch", "")
    prior = head.get("prior", "-")
    want_batch = digest("kiln-batch-v1", LANE + "\n" + prior + "\n" + body, 8)
    add(
        "batch-id-recomputes",
        bool(batch) and batch == want_batch and rollups and rollups[0].name == "%s-%s.rollup" % (LANE, batch),
        "batch=%r recomputed=%r filename=%r"
        % (batch, want_batch, rollups[0].name if rollups else None),
    )

    record_ok = False
    detail = "no batch record"
    rec_path = work / ".kiln" / "batches" / (batch + ".json")
    if batch and rec_path.is_file():
        try:
            record = json.loads(rec_path.read_text(encoding="utf-8"))
        except ValueError:
            record = {}
        stamp_ok = record.get("stamp") == digest(
            "kiln-batch-rec-v1", canon({k: v for k, v in record.items() if k != "stamp"}), 16
        )
        record_ok = (
            record.get("lane") == LANE
            and record.get("shards") == want_names
            and record.get("prior") in (None, "")
            and stamp_ok
        )
        detail = "record=%r stamp valid=%s" % (record, stamp_ok)
    add("batch-record-agrees", record_ok, detail)

    try:
        state = json.loads((work / ".kiln" / "state.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        state = {}
    fresh = bool(profile_text) and state.get("profile_stamp") == digest(
        "kiln-profile-v1", profile_text, 16
    )
    add(
        "state-fresh-and-current",
        fresh
        and state.get("hold") == HOLD
        and state.get("last_batch") == batch
        and state.get("stamp")
        == digest("kiln-state-v1", canon({k: v for k, v in state.items() if k != "stamp"}), 16),
        "state hold=%r last_batch=%r built from the profile on disk=%s"
        % (state.get("hold"), state.get("last_batch"), fresh),
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
