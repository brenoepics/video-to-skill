#!/usr/bin/env python3
"""Checker for sqlite-attach-merge-ledgers.

Decided on the rows actually in the databases, read through a read-only
connection so the checker cannot itself alter them.

`existing-rows-survived` is the assertion that carries the task: the
conflict resolution that first comes to mind replaces the row already
present instead of keeping it, and SQLite reports nothing when it does, so
a checker that only counted rows would pass the wrong procedure with a
ledger whose own figures had been overwritten by the incoming restatement.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

LEDGER_BEFORE = {
    1: ("2026-03-01", 1500, "opening float"),
    2: ("2026-03-02", -240, "courier"),
    5: ("2026-03-05", 7731, "grant drawdown"),
}
INCOMING_BEFORE = {
    2: ("2026-03-02", -999, "courier RESTATED"),
    3: ("2026-03-03", 410, "parts"),
    4: ("2026-03-04", -60, "postage"),
    5: ("2026-03-05", 1, "bogus restatement"),
}


def query(path: Path, sql: str) -> list[list] | None:
    """Run one query read-only. Falls back to the sqlite3 tool."""
    try:
        import sqlite3
    except ImportError:
        result = subprocess.run(
            ["sqlite3", "-json", "-readonly", str(path), sql],
            capture_output=True, text=True, timeout=120,
        )
        if result.returncode != 0:
            return None
        text = result.stdout.strip()
        if not text:
            return []
        try:
            return [list(row.values()) for row in json.loads(text)]
        except ValueError:
            return None
    try:
        conn = sqlite3.connect("file:%s?mode=ro" % path, uri=True, timeout=10)
    except sqlite3.Error:
        return None
    try:
        return [list(row) for row in conn.execute(sql)]
    except sqlite3.Error:
        return None
    finally:
        conn.close()


def rows(path: Path) -> dict[int, tuple] | None:
    got = query(path, "SELECT id, ts, amount, note FROM entries ORDER BY id")
    if got is None:
        return None
    return {int(r[0]): (r[1], r[2], r[3]) for r in got}


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
        results.append({"id": ident, "passed": bool(passed), "required": required, "detail": detail})

    ledger = rows(work / "ledger.db")
    add(
        "ledger-readable",
        ledger is not None,
        "ledger.db opens and its entries table reads"
        if ledger is not None
        else "ledger.db is missing, unreadable, or has no entries table",
    )
    ledger = ledger or {}

    expected = dict(INCOMING_BEFORE)
    expected.update(LEDGER_BEFORE)  # the row already present wins

    missing = sorted(set(expected) - set(ledger))
    extra = sorted(set(ledger) - set(expected))
    add(
        "every-id-present",
        not missing and not extra,
        "ids are exactly %s" % sorted(expected)
        if not missing and not extra
        else "missing ids %r, unexpected ids %r" % (missing, extra),
    )

    clobbered = {
        key: {"now": ledger.get(key), "was": value}
        for key, value in LEDGER_BEFORE.items()
        if ledger.get(key) != value
    }
    add(
        "existing-rows-survived",
        not clobbered,
        "every row that was already in ledger.db is unchanged in every column"
        if not clobbered
        else "overwritten by the incoming version: %r" % (clobbered,),
    )

    only_incoming = {k: v for k, v in INCOMING_BEFORE.items() if k not in LEDGER_BEFORE}
    wrong = {k: ledger.get(k) for k, v in only_incoming.items() if ledger.get(k) != v}
    add(
        "new-rows-copied-faithfully",
        not wrong,
        "the ids only incoming.db had arrived with every column intact"
        if not wrong
        else "copied wrongly: %r" % (wrong,),
    )

    integrity = query(work / "ledger.db", "PRAGMA integrity_check")
    schema = query(
        work / "ledger.db",
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='entries'",
    )
    schema_text = (schema[0][0] if schema else "") or ""
    integrity_ok = (
        integrity is not None
        and integrity
        and str(integrity[0][0]).lower() == "ok"
        and "INTEGER PRIMARY KEY" in schema_text.upper()
    )
    add(
        "ledger-integrity-ok",
        integrity_ok,
        "integrity_check=%r and the entries table still declares its primary key"
        % (integrity[0][0] if integrity else None,),
    )

    incoming = rows(work / "incoming.db")
    add(
        "incoming-untouched",
        incoming == INCOMING_BEFORE,
        "incoming.db still holds its original %d rows" % len(INCOMING_BEFORE)
        if incoming == INCOMING_BEFORE
        else "incoming.db changed: %r" % (incoming,),
    )

    add(
        "merged-across-an-attached-database",
        re.search(r"(?mi)attach\s+(database\s+)?.?incoming\.db", log_text) is not None,
        "an attach of the second database appears in the command log",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
