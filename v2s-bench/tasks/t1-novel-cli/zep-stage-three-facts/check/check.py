#!/usr/bin/env python3
"""Checker for zep-stage-three-facts.

Reads only zep's state on disk. `entries-are-the-three-facts` catches the
agent that stages the file's lines verbatim as one entry, or that swaps a
key for its value; `state-sealed-not-provisional` catches the close that
was made without the grant nonce (exit 0, plausible output, no anchor);
`anchor-is-genuine` recomputes the anchor and the record stamp, so a
hand-written record fails too.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

DROP = "hotfix-3"
EXPECTED = {"lane": "amber", "window": "30m", "owner": "payments"}


def digest(tag: str, payload: str, width: int) -> str:
    return hashlib.sha256((tag + "\n" + payload).encode("utf-8")).hexdigest()[:width]


def canon(obj: object) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"))


def load(path: Path) -> dict | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def pairs_of(entries: object) -> dict | None:
    if not isinstance(entries, list) or not all(isinstance(e, dict) for e in entries):
        return None
    return {e.get("key"): e.get("value") for e in entries}


def main() -> int:
    work = Path(sys.argv[1])
    log_text = ""
    if len(sys.argv) > 2:
        try:
            log_text = Path(sys.argv[2]).read_text(encoding="utf-8", errors="replace")
        except OSError:
            log_text = ""

    results: list[dict] = []

    def add(ident: str, passed: bool, detail: str, required: bool = True) -> None:
        results.append(
            {"id": ident, "passed": bool(passed), "required": required, "detail": detail}
        )

    drop_dir = work / ".zep" / "drops" / DROP
    record = load(drop_dir / "sealed.json")
    add(
        "record-readable",
        record is not None,
        "closing record present" if record else "no readable closing record for " + DROP,
    )
    record = record or {}

    anchor = record.get("anchor")
    add(
        "state-sealed-not-provisional",
        record.get("state") == "sealed"
        and isinstance(anchor, str)
        and re.fullmatch(r"[0-9a-f]{16}", anchor) is not None,
        "state=%r anchor=%r" % (record.get("state"), anchor),
    )

    entries = record.get("entries")
    pairs = pairs_of(entries)
    add(
        "entries-are-the-three-facts",
        pairs == EXPECTED and isinstance(entries, list) and len(entries) == 3,
        "entries=%r" % (pairs,),
    )

    genuine = False
    detail = "no entries to verify"
    if isinstance(entries, list) and entries:
        body = "".join("%s=%s\n" % (e.get("key"), e.get("value")) for e in entries)
        want_anchor = digest(
            "zep-anchor-v1", DROP + "\n" + body + (record.get("parent_anchor") or "-"), 16
        )
        want_stamp = digest(
            "zep-stamp-v1", canon({k: v for k, v in record.items() if k != "stamp"}), 16
        )
        genuine = anchor == want_anchor and record.get("stamp") == want_stamp
        detail = "anchor recomputes=%s record stamp valid=%s" % (
            anchor == want_anchor,
            record.get("stamp") == want_stamp,
        )
    add("anchor-is-genuine", genuine, detail)

    stage = load(drop_dir / "stage.json") or {}
    staged = pairs_of(stage.get("entries"))
    add(
        "stage-matches-record",
        staged is not None and staged == pairs,
        "staged=%r" % (staged,),
    )

    ledger = load(work / ".zep" / "ledger.json") or {}
    row = (ledger.get("drops") or {}).get(DROP) or {}
    add(
        "ledger-agrees",
        row.get("state") == "sealed" and anchor is not None and row.get("anchor") == anchor,
        "ledger row=%r" % (row,),
    )

    add(
        "opened-before-staging",
        re.search(
            r"(?mi)^\s*zep\s+open\s+" + DROP + r"\b[\s\S]*^\s*zep\s+put\s+" + DROP + r"\b",
            log_text,
        )
        is not None,
        "the command log shows the drop opened before anything was staged",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
