#!/usr/bin/env python3
"""Checker for zep-seal-with-grant.

Decided entirely on zep's own state on disk. The assertion that carries
the task is `state-sealed-not-provisional`: closing the drop without its
grant nonce exits 0 and prints a plausible confirmation, so a checker
that only asked "does a closing record exist?" would pass the wrong
procedure. `anchor-is-genuine` recomputes the anchor and the record
stamp, so a hand-written record does not pass either.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

EXPECTED = {"port": "8080", "region": "eu-west", "build": "441"}
DROP = "rc-7"


def digest(tag: str, payload: str, width: int) -> str:
    return hashlib.sha256((tag + "\n" + payload).encode("utf-8")).hexdigest()[:width]


def canon(obj: object) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"))


def load(path: Path) -> dict | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


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

    record = load(work / ".zep" / "drops" / DROP / "sealed.json")
    add(
        "record-readable",
        record is not None,
        "closing record present and parseable"
        if record
        else "no readable .zep closing record for " + DROP,
    )
    record = record or {}

    state = record.get("state")
    anchor = record.get("anchor")
    add(
        "state-sealed-not-provisional",
        state == "sealed" and isinstance(anchor, str) and re.fullmatch(r"[0-9a-f]{16}", anchor or "") is not None,
        "state=%r anchor=%r" % (state, anchor),
    )

    entries = record.get("entries")
    pairs = (
        {e.get("key"): e.get("value") for e in entries}
        if isinstance(entries, list) and all(isinstance(e, dict) for e in entries)
        else None
    )
    add(
        "entries-intact",
        pairs == EXPECTED and isinstance(entries, list) and len(entries) == 3,
        "entries=%r" % (pairs,),
    )

    genuine = False
    detail = "no record to verify"
    if entries and isinstance(entries, list):
        body = "".join("%s=%s\n" % (e.get("key"), e.get("value")) for e in entries)
        parent = record.get("parent_anchor")
        want_anchor = digest("zep-anchor-v1", DROP + "\n" + body + (parent or "-"), 16)
        bare = {k: v for k, v in record.items() if k != "stamp"}
        want_stamp = digest("zep-stamp-v1", canon(bare), 16)
        anchor_ok = anchor == want_anchor
        stamp_ok = record.get("stamp") == want_stamp
        genuine = anchor_ok and stamp_ok
        detail = "anchor recomputes=%s record stamp valid=%s" % (anchor_ok, stamp_ok)
    add("anchor-is-genuine", genuine, detail)

    ledger = load(work / ".zep" / "ledger.json") or {}
    row = (ledger.get("drops") or {}).get(DROP) or {}
    add(
        "ledger-agrees",
        row.get("state") == "sealed" and row.get("anchor") == anchor and anchor is not None,
        "ledger row=%r" % (row,),
    )

    add(
        "sealed-through-the-tool",
        re.search(r"(?mi)^\s*zep\s+seal\s+" + DROP + r"\b[^\n]*--grant\b", log_text) is not None,
        "a grant-carrying seal appears in the command log",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
