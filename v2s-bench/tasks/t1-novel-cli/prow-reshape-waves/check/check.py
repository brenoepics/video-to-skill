#!/usr/bin/env python3
"""Checker for prow-reshape-waves.

`retired-unit-is-gone` catches the reshape that parks the retired unit in
a later wave instead of removing it -- a sheet that still sequences
correctly and still looks like the written order at a glance.
`sheet-matches-the-plan` catches the other half: changing the plan and
leaving the previously rendered sheet in place, which no command
complains about.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

SHEET = "rollout/sheet.txt"
RETIRED = "legacy-sync"
EXPECTED_UNITS = [
    {"unit": "api-gw", "wave": 1},
    {"unit": "cache", "wave": 1},
    {"unit": "web", "wave": 2},
]


def canon(obj: object) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"))


def digest(tag: str, payload: str, width: int) -> str:
    return hashlib.sha256((tag + "\n" + payload).encode("utf-8")).hexdigest()[:width]


def render(units: list[dict], interlock: bool) -> str:
    """The sheet prow itself would write for these units. Kept in step
    with the tool deliberately: comparing against the real rendering is
    what makes a hand-written sheet detectable."""
    units = sorted(units, key=lambda u: (u["wave"], u["unit"]))
    waves = sorted({u["wave"] for u in units})
    lines = [
        "# prow rollout sheet",
        "interlock: " + ("on" if interlock else "off"),
        "waves: %d" % len(waves),
        "units: %d" % len(units),
        "--",
    ]
    for position, wave in enumerate(waves):
        lines.append("wave %d" % wave)
        for unit in units:
            if unit["wave"] == wave:
                lines.append("  unit: " + unit["unit"])
        if not interlock:
            lines.append("  gate: parallel")
        elif position == 0:
            lines.append("  gate: start")
        else:
            lines.append("  gate: after wave %d" % waves[position - 1])
    return "\n".join(lines) + "\n"


def main() -> int:
    work = Path(sys.argv[1])
    results: list[dict] = []

    def add(ident: str, passed: bool, detail: str, required: bool = True) -> None:
        results.append(
            {"id": ident, "passed": bool(passed), "required": required, "detail": detail}
        )

    try:
        plan = json.loads((work / ".prow" / "plan.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        plan = {}
    units = plan.get("units") if isinstance(plan.get("units"), list) else []
    stamp_ok = plan.get("stamp") == digest(
        "prow-plan-v1", canon({k: v for k, v in plan.items() if k != "stamp"}), 16
    )
    add(
        "plan-matches-the-order",
        units == EXPECTED_UNITS and stamp_ok,
        "plan units=%r written by prow=%s" % (units, stamp_ok),
    )

    try:
        text = (work / SHEET).read_text(encoding="utf-8")
    except OSError:
        text = ""

    add(
        "retired-unit-is-gone",
        RETIRED not in json.dumps(units) and RETIRED not in text,
        "%s in plan=%s in sheet=%s" % (RETIRED, RETIRED in json.dumps(units), RETIRED in text),
    )

    add(
        "sheet-sequences-the-waves",
        "interlock: on" in text and "gate: after wave 1" in text and "gate: parallel" not in text,
        "gates=%r" % ([l.strip() for l in text.splitlines() if l.strip().startswith("gate:")],),
    )

    add(
        "sheet-matches-the-plan",
        bool(units) and bool(text) and text == render(units, True),
        "sheet is exactly what prow renders for the plan on disk: %s"
        % (bool(units) and bool(text) and text == render(units, True)),
    )

    record_ok = False
    detail = "no sheet record for this text"
    ident = digest("prow-sheet-v1", text, 10) if text else ""
    rec_path = work / ".prow" / "sheets" / (ident + ".json")
    if text and rec_path.is_file():
        try:
            record = json.loads(rec_path.read_text(encoding="utf-8"))
        except ValueError:
            record = {}
        rec_stamp = record.get("stamp") == digest(
            "prow-sheet-rec-v1", canon({k: v for k, v in record.items() if k != "stamp"}), 16
        )
        record_ok = (
            record.get("out") == SHEET
            and record.get("interlock") is True
            and record.get("sheet_sha") == hashlib.sha256(text.encode("utf-8")).hexdigest()
            and rec_stamp
        )
        detail = "record for %s: out=%r interlock=%r stamp valid=%s" % (
            ident,
            record.get("out"),
            record.get("interlock"),
            rec_stamp,
        )
    add("sheet-record-matches-file", record_ok, detail)

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
