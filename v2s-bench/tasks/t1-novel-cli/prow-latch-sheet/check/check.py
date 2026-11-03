#!/usr/bin/env python3
"""Checker for prow-latch-sheet.

`released-sheet-is-the-one-on-disk` is the assertion that earns its keep.
Releasing a sheet and then re-rendering it -- the obvious order if you
release as soon as something is rendered -- leaves a released id, a
sequenced sheet on disk and a clean status line, with nothing anywhere
saying that the released id belongs to text that no longer exists. The id
is recomputed from the file's own bytes, so only the released rendering
passes.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

SHEET = "rollout/sheet.txt"


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
        text = (work / SHEET).read_text(encoding="utf-8")
    except OSError:
        text = ""

    add(
        "sheet-sequences-the-waves",
        bool(text)
        and "interlock: on" in text
        and "gate: after wave 1" in text
        and "gate: after wave 2" in text
        and "gate: parallel" not in text,
        "gates=%r" % ([l.strip() for l in text.splitlines() if l.strip().startswith("gate:")],),
    )

    try:
        plan = json.loads((work / ".prow" / "plan.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        plan = {}
    units = plan.get("units") if isinstance(plan.get("units"), list) else []
    add(
        "sheet-matches-the-plan",
        bool(units) and bool(text) and text == render(units, True),
        "sheet is exactly what prow renders for the plan on disk: %s"
        % (bool(units) and bool(text) and text == render(units, True)),
    )

    try:
        latched = json.loads((work / ".prow" / "latched.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        latched = {}
    latch_stamp = latched.get("stamp") == digest(
        "prow-latch-v1", canon({k: v for k, v in latched.items() if k != "stamp"}), 16
    )
    add(
        "release-is-recorded",
        bool(latched) and latch_stamp and latched.get("interlock") is True,
        "release record=%r stamp valid=%s" % (latched, latch_stamp),
    )

    want_id = digest("prow-sheet-v1", text, 10) if text else ""
    add(
        "released-sheet-is-the-one-on-disk",
        bool(want_id) and latched.get("id") == want_id and latched.get("out") == SHEET,
        "released id=%r, id of the sheet on disk=%r" % (latched.get("id"), want_id),
    )

    record_ok = False
    detail = "no sheet record for this text"
    rec_path = work / ".prow" / "sheets" / (want_id + ".json")
    if want_id and rec_path.is_file():
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
        detail = "record for %s: interlock=%r sha matches file=%s stamp valid=%s" % (
            want_id,
            record.get("interlock"),
            record.get("sheet_sha") == hashlib.sha256(text.encode("utf-8")).hexdigest(),
            rec_stamp,
        )
    add("sheet-record-matches-file", record_ok, detail)

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
