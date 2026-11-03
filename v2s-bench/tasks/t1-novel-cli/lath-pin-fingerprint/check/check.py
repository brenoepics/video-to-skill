#!/usr/bin/env python3
"""Checker for lath-pin-fingerprint.

`pin-is-the-earlier-fingerprint` is the assertion that separates this
from a smoke test. The task's shape invites three superficially similar
wrong answers -- pinning the fingerprint of the *second* merge, pinning
some other digest of the file, or writing dist/app.json by hand -- and all
three produce a five-key JSON file that looks right.
EXPECTED_PIN is the fingerprint of the base+prod merge, fixed by the
task's deterministic setup; the checker also recomputes it from the merge
record so a changed setup fails loudly instead of silently.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

BUILD = "build/app.json"
DIST = "dist/app.json"
BASE_MERGE = {"port": "8080", "region": "eu-west", "timeout": "60", "trace": "on"}
EXPECTED_PIN = "6c7fe426b465514c"


def canon(obj: object) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"))


def digest(tag: str, payload: str, width: int) -> str:
    return hashlib.sha256((tag + "\n" + payload).encode("utf-8")).hexdigest()[:width]


def load(path: Path) -> object | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def read_layer(path: Path) -> dict[str, str] | None:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return None
    pairs: dict[str, str] = {}
    for line in text.splitlines():
        if line.strip():
            key, _, value = line.partition("\t")
            pairs[key] = value
    return pairs


def main() -> int:
    work = Path(sys.argv[1])
    results: list[dict] = []

    def add(ident: str, passed: bool, detail: str, required: bool = True) -> None:
        results.append(
            {"id": ident, "passed": bool(passed), "required": required, "detail": detail}
        )

    want_pin = digest("lath-fp-v1", canon(BASE_MERGE), 16)
    if want_pin != EXPECTED_PIN:  # the setup drifted; fail loudly rather than quietly
        add("checker-constants-agree", False, "expected pin %s but recomputed %s" % (EXPECTED_PIN, want_pin))

    build = load(work / BUILD)
    build = build if isinstance(build, dict) else {}
    add("build-file-exact", build == BASE_MERGE, "%s = %r" % (BUILD, build))

    release = read_layer(work / ".lath" / "layers" / "release.kv")
    stamp_ok = False
    try:
        index = json.loads((work / ".lath" / "index.json").read_text(encoding="utf-8"))
        text = (work / ".lath" / "layers" / "release.kv").read_text(encoding="utf-8")
        stamp_ok = index.get("layers", {}).get("release") == digest(
            "lath-layer-v1", "release\n" + text, 16
        )
    except (OSError, ValueError):
        stamp_ok = False
    add(
        "release-layer-holds-the-fingerprint",
        release == {"pinned_from": want_pin} and stamp_ok,
        "release layer = %r (written by lath: %s)" % (release, stamp_ok),
    )

    expected_dist = dict(BASE_MERGE, pinned_from=want_pin)
    dist = load(work / DIST)
    dist = dist if isinstance(dist, dict) else {}
    add("dist-file-exact", dist == expected_dist, "%s = %r" % (DIST, dist))

    record = load(work / ".lath" / "last-flatten.json")
    ok = False
    detail = "no readable merge record"
    if isinstance(record, dict):
        fp_ok = record.get("fingerprint") == digest("lath-fp-v1", canon(record.get("keys")), 16)
        stamp = record.get("stamp") == digest(
            "lath-flatten-v1", canon({k: v for k, v in record.items() if k != "stamp"}), 16
        )
        ok = record.get("out") == DIST and record.get("keys") == dist and fp_ok and stamp
        detail = "out=%r keys match file=%s fingerprint ok=%s stamp ok=%s" % (
            record.get("out"),
            record.get("keys") == dist,
            fp_ok,
            stamp,
        )
    add("dist-written-by-lath", ok, detail)

    final_fp = digest("lath-fp-v1", canon(expected_dist), 16)
    pinned = dist.get("pinned_from")
    add(
        "pin-is-the-earlier-fingerprint",
        pinned == want_pin and pinned != final_fp,
        "pinned_from=%r (merge that wrote %s = %s, this merge = %s)"
        % (pinned, BUILD, want_pin, final_fp),
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
