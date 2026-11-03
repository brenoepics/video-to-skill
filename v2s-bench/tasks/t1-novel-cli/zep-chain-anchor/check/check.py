#!/usr/bin/env python3
"""Checker for zep-chain-anchor.

The discriminating assertion is `parent-is-the-base-anchor`: a drop
opened without declaring its parent, staged and sealed correctly, is a
sealed anchored drop holding the right fact -- it simply is not chained,
and nothing in zep's console output on the way makes that obvious. Both
records are also recomputed from their entries, so neither the child's
anchor nor the parent link can be written in by hand.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

CHILD = "patch-20"
PARENT = "base-19"
CHILD_FACT = {"note": "rollback-safe"}
PARENT_FACTS = {"lane": "amber", "release": "19"}


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


def genuine(name: str, record: dict) -> tuple[bool, str]:
    entries = record.get("entries")
    if not isinstance(entries, list) or not entries:
        return False, "no entries in " + name
    body = "".join("%s=%s\n" % (e.get("key"), e.get("value")) for e in entries)
    want_anchor = digest(
        "zep-anchor-v1", name + "\n" + body + (record.get("parent_anchor") or "-"), 16
    )
    want_stamp = digest(
        "zep-stamp-v1", canon({k: v for k, v in record.items() if k != "stamp"}), 16
    )
    anchor_ok = record.get("anchor") == want_anchor
    stamp_ok = record.get("stamp") == want_stamp
    return anchor_ok and stamp_ok, "anchor recomputes=%s stamp valid=%s" % (anchor_ok, stamp_ok)


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

    base = load(work / ".zep" / "drops" / PARENT / "sealed.json") or {}
    base_ok, base_detail = genuine(PARENT, base)
    add(
        "parent-record-untouched",
        base.get("state") == "sealed"
        and pairs_of(base.get("entries")) == PARENT_FACTS
        and base.get("parent_anchor") is None
        and base_ok,
        "state=%r entries=%r %s" % (base.get("state"), pairs_of(base.get("entries")), base_detail),
    )
    base_anchor = base.get("anchor")

    child = load(work / ".zep" / "drops" / CHILD / "sealed.json")
    add(
        "record-readable",
        child is not None,
        "closing record present" if child else "no readable closing record for " + CHILD,
    )
    child = child or {}

    anchor = child.get("anchor")
    add(
        "state-sealed-not-provisional",
        child.get("state") == "sealed"
        and isinstance(anchor, str)
        and re.fullmatch(r"[0-9a-f]{16}", anchor) is not None,
        "state=%r anchor=%r" % (child.get("state"), anchor),
    )

    entries = child.get("entries")
    add(
        "entry-is-the-patch-fact",
        pairs_of(entries) == CHILD_FACT and isinstance(entries, list) and len(entries) == 1,
        "entries=%r" % (pairs_of(entries),),
    )

    child_ok, child_detail = genuine(CHILD, child)
    add("anchor-is-genuine", child_ok, child_detail)

    add(
        "parent-is-the-base-anchor",
        isinstance(base_anchor, str) and child.get("parent_anchor") == base_anchor,
        "child parent=%r base anchor=%r" % (child.get("parent_anchor"), base_anchor),
    )

    ledger = load(work / ".zep" / "ledger.json") or {}
    drops = ledger.get("drops") or {}
    add(
        "ledger-agrees",
        (drops.get(CHILD) or {}).get("state") == "sealed"
        and (drops.get(CHILD) or {}).get("anchor") == anchor
        and (drops.get(PARENT) or {}).get("state") == "sealed"
        and anchor is not None,
        "rows=%r" % ({k: drops.get(k) for k in (PARENT, CHILD)},),
    )

    add(
        "parent-declared-at-creation",
        re.search(r"(?mi)^\s*zep\s+open\s+" + CHILD + r"\b[^\n]*--after\b", log_text) is not None,
        "the command log shows the child opened with its parent declared",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
