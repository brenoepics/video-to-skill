#!/usr/bin/env python3
"""Checker for lath-prod-wins.

`prod-wins-the-conflict` is the discriminating assertion: stacking the
layers the other way round produces a well-formed four-key merge with
base's timeout, which no console output flags as wrong.
`flatten-record-matches-file` ties the file to lath's own record of the
merge and recomputes the fingerprint, so editing the JSON by hand after a
reversed merge fails too.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

OUT = "build/app.json"
EXPECTED = {"port": "8080", "region": "eu-west", "timeout": "60", "trace": "on"}


def canon(obj: object) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"))


def digest(tag: str, payload: str, width: int) -> str:
    return hashlib.sha256((tag + "\n" + payload).encode("utf-8")).hexdigest()[:width]


def load(path: Path) -> object | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def main() -> int:
    work = Path(sys.argv[1])
    results: list[dict] = []

    def add(ident: str, passed: bool, detail: str, required: bool = True) -> None:
        results.append(
            {"id": ident, "passed": bool(passed), "required": required, "detail": detail}
        )

    merged = load(work / OUT)
    add(
        "merged-file-exists",
        isinstance(merged, dict),
        "%s is a JSON object" % OUT if isinstance(merged, dict) else "no readable JSON object at " + OUT,
    )
    merged = merged if isinstance(merged, dict) else {}

    add("merged-content-exact", merged == EXPECTED, "merged=%r" % (merged,))
    add(
        "prod-wins-the-conflict",
        merged.get("timeout") == "60",
        "timeout=%r (base says 30, prod says 60)" % (merged.get("timeout"),),
    )

    record = load(work / ".lath" / "last-flatten.json")
    ok = False
    detail = "no readable merge record"
    if isinstance(record, dict):
        fp_ok = record.get("fingerprint") == digest("lath-fp-v1", canon(record.get("keys")), 16)
        stamp_ok = record.get("stamp") == digest(
            "lath-flatten-v1", canon({k: v for k, v in record.items() if k != "stamp"}), 16
        )
        ok = (
            record.get("out") == OUT
            and record.get("keys") == merged
            and fp_ok
            and stamp_ok
        )
        detail = "out=%r keys match file=%s fingerprint ok=%s stamp ok=%s" % (
            record.get("out"),
            record.get("keys") == merged,
            fp_ok,
            stamp_ok,
        )
    add("flatten-record-matches-file", ok, detail)

    order = []
    try:
        order = [l.strip() for l in (work / ".lath" / "order").read_text(encoding="utf-8").split() if l.strip()]
    except OSError:
        pass
    add(
        "stack-order-puts-prod-first",
        order[:2] == ["prod", "base"],
        "precedence order on disk: %r" % (order,),
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
