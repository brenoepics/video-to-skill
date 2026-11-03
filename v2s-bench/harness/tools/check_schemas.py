#!/usr/bin/env python3
"""Check that every file in schemas/ is itself a valid JSON Schema.

A malformed schema does not fail loudly -- `jsonschema` will happily accept
a schema whose keywords are misspelled and then validate nothing, so a
typo in a `required` list silently disables the check it was meant to
enforce. `check_schema` is what catches that.

Run from the v2s-bench root, or pass the schemas directory.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import jsonschema


def main(argv: list[str]) -> int:
    directory = Path(argv[1]) if len(argv) > 1 else Path("schemas")
    if not directory.is_dir():
        print(f"no such directory: {directory}", file=sys.stderr)
        return 1

    paths = sorted(directory.glob("*.json"))
    if not paths:
        print(f"no schemas found in {directory}", file=sys.stderr)
        return 1

    failures = 0
    for path in paths:
        try:
            schema = json.loads(path.read_text())
            jsonschema.Draft202012Validator.check_schema(schema)
        except (json.JSONDecodeError, jsonschema.SchemaError) as exc:
            print(f"FAIL {path.name}: {exc}", file=sys.stderr)
            failures += 1
        else:
            print(f"ok   {path.name}")

    if failures:
        print(f"\n{failures} invalid schema(s)", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
