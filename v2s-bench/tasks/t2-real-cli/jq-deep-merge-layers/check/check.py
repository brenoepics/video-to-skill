#!/usr/bin/env python3
"""Checker for jq-deep-merge-layers.

The assertion that carries the task is `nested-siblings-survived`: a
one-level merge (`add`, `+`) exits 0 and prints well-formed configuration
in which `service.listen.host`, `service.tls.ciphers` and `log.sinks` have
silently vanished, so a checker that only asked "is the overridden port
right?" would pass the wrong procedure. The expected object is recomputed
here from the three layer files rather than written out, so the checker
cannot drift from the fixtures.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

LAYERS = ("base.json", "site.json", "local.json")


def deep_merge(left: object, right: object) -> object:
    """jq's `*`: objects recurse, everything else is replaced."""
    if isinstance(left, dict) and isinstance(right, dict):
        merged = dict(left)
        for key, value in right.items():
            merged[key] = deep_merge(left.get(key), value) if key in left else value
        return merged
    return right


def load(path: Path) -> object | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None


def dig(obj: object, path: str) -> object:
    cursor = obj
    for part in path.split("."):
        if not isinstance(cursor, dict) or part not in cursor:
            return KeyError
        cursor = cursor[part]
    return cursor


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

    sources = [load(work / name) for name in LAYERS]
    if any(s is None for s in sources):
        add("layers-untouched", False, "a layer file is missing or no longer parses")
        for ident in ("effective-parses", "later-layers-win", "nested-siblings-survived",
                      "whole-object-exact"):
            add(ident, False, "cannot evaluate without the three layer files")
        print(json.dumps({"assertions": results}))
        return 0

    expected = {}
    for layer in sources:
        expected = deep_merge(expected, layer)

    produced = load(work / "effective.json")
    add(
        "effective-parses",
        isinstance(produced, dict),
        "effective.json parses as a JSON object"
        if isinstance(produced, dict)
        else "effective.json is missing or is not a JSON object",
    )
    produced_obj = produced if isinstance(produced, dict) else {}

    overrides = {
        "service.listen.port": 9443,
        "service.tls.enabled": True,
        "service.tls.min_version": "1.3",
        "log.level": "warn",
        "limits.rps": 1000,
        "retries": 5,
    }
    wrong_overrides = {k: dig(produced_obj, k) for k, v in overrides.items() if dig(produced_obj, k) != v}
    add(
        "later-layers-win",
        not wrong_overrides,
        "every setting a later layer mentions took effect"
        if not wrong_overrides
        else "not overridden: %r" % (wrong_overrides,),
    )

    # The keys a one-level merge drops: each lives beside a key that a later
    # layer does overwrite, so only a recursive merge keeps them.
    survivors = {
        "service.listen.host": "0.0.0.0",
        "service.listen.backlog": 128,
        "service.tls.ciphers": ["ECDHE-RSA-AES128-GCM-SHA256"],
        "log.sinks": ["stdout"],
        "log.redact": ["authorization"],
        "limits.burst": 50,
        "service.name": "harbour-api",
    }
    lost = {k: dig(produced_obj, k) for k, v in survivors.items() if dig(produced_obj, k) != v}
    add(
        "nested-siblings-survived",
        not lost,
        "every nested sibling an override did not mention is still present"
        if not lost
        else "dropped or altered by a one-level merge: %r" % (lost,),
    )

    add(
        "whole-object-exact",
        produced_obj == expected,
        "effective.json equals the recursive merge of the three layers"
        if produced_obj == expected
        else "differs from the recursive merge; got keys %r" % (sorted(produced_obj),),
    )

    digests = {
        name: hashlib.sha256((work / name).read_bytes()).hexdigest()[:12]
        for name in LAYERS
        if (work / name).is_file()
    }
    reparsed = [load(work / name) for name in LAYERS]
    add(
        "layers-untouched",
        reparsed == sources and len(digests) == 3,
        "the three layer files are still present and parse (%r)" % (sorted(digests),),
    )

    add(
        "merged-through-jq",
        re.search(r"(?mi)^\s*jq\b[^\n]*\*", log_text) is not None,
        "a jq invocation using the recursive-merge operator appears in the command log",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
