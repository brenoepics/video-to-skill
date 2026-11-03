#!/usr/bin/env python3
"""Checker for lath-stage-pair.

`no-value-shaped-keys` is the assertion that earns its keep: staging the
pair the conventional way round (key first) exits 0 and writes a layer
holding two keys called 8080 and eu-west. A checker that only asked "does
the layer have two entries?" would score that as a pass.
`layer-written-by-lath` compares lath's own per-layer digest with the
file, so writing the .kv file by hand fails as well.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

LAYER = "base"
EXPECTED = {"port": "8080", "region": "eu-west"}


def digest(tag: str, payload: str, width: int) -> str:
    return hashlib.sha256((tag + "\n" + payload).encode("utf-8")).hexdigest()[:width]


def read_layer(path: Path) -> dict[str, str] | None:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return None
    pairs: dict[str, str] = {}
    for line in text.splitlines():
        if not line.strip():
            continue
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

    layer_path = work / ".lath" / "layers" / (LAYER + ".kv")
    pairs = read_layer(layer_path)
    add("layer-exists", pairs is not None, "layer file present" if pairs is not None else "no layer " + LAYER)
    pairs = pairs or {}

    add("keys-are-port-and-region", pairs == EXPECTED, "layer holds %r" % (pairs,))

    strays = sorted(k for k in pairs if k in set(EXPECTED.values()))
    add(
        "no-value-shaped-keys",
        not strays,
        "keys named after values: %r" % (strays,) if strays else "no key is one of the values",
    )

    stamp_ok = False
    detail = "no lath index"
    try:
        index = json.loads((work / ".lath" / "index.json").read_text(encoding="utf-8"))
        text = layer_path.read_text(encoding="utf-8")
        want = digest("lath-layer-v1", LAYER + "\n" + text, 16)
        stamp_ok = index.get("layers", {}).get(LAYER) == want
        detail = "lath's own digest for the layer matches the file: %s" % stamp_ok
    except (OSError, ValueError):
        pass
    add("layer-written-by-lath", stamp_ok, detail)

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
