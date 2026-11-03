#!/usr/bin/env python3
"""Content-hash gate for the tier-4 agent<->game interfaces.

    python3 env/freeze.py --check     # CI: fail on any drift
    python3 env/freeze.py --update    # deliberate re-freeze, bump the version

Two things are enforced, and both exist because this tier's numbers are
otherwise uninterpretable (``docs/preregistration.md`` 9.2):

1. **The interface has not changed.** Each frozen file's sha256 is pinned
   in ``env/freeze.json``. A harness change has been measured to swing one
   model from 62.7% to 99.9% on the same tasks, so an unpinned interface
   confounds every arm comparison with an undocumented environment change.

2. **The copy the task ships is byte-identical to the canonical one.** A
   task delivers its interface through ``files/``, so the repository holds
   two copies on purpose: one canonical under ``env/<game>/`` and one in
   the task. ``mirrors`` pins them together, so the duplication cannot
   silently drift into two different games.

A deliberate change is: edit the interface, bump ``PROTOCOL`` in it and the
version in its ``PROTOCOL.md``, re-run ``--update``, and treat results from
before and after as different environments.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "env" / "freeze.json"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load() -> dict:
    if not MANIFEST.exists():
        return {"schema_version": 1, "frozen": {}, "mirrors": {}}
    return json.loads(MANIFEST.read_text(encoding="utf-8"))


def check() -> int:
    manifest = load()
    problems: list[str] = []
    for rel, want in sorted(manifest.get("frozen", {}).items()):
        path = ROOT / rel
        if not path.exists():
            problems.append(f"missing: {rel}")
            continue
        got = sha256(path)
        state = "ok  " if got == want else "DRIFT"
        print(f"  {state} {rel}  {got[:16]}")
        if got != want:
            problems.append(f"drift: {rel}\n        frozen {want}\n        found  {got}")
    for mirror, canonical in sorted(manifest.get("mirrors", {}).items()):
        a, b = ROOT / mirror, ROOT / canonical
        if not (a.exists() and b.exists()):
            problems.append(f"mirror missing: {mirror} <- {canonical}")
            continue
        same = a.read_bytes() == b.read_bytes()
        print(f"  {'ok  ' if same else 'DRIFT'} {mirror} mirrors {canonical}")
        if not same:
            problems.append(f"mirror diverged: {mirror} != {canonical}")
    print()
    if problems:
        for problem in problems:
            print(f"FAIL {problem}")
        return 1
    print(f"{len(manifest.get('frozen', {}))} frozen file(s), "
          f"{len(manifest.get('mirrors', {}))} mirror(s): all pinned")
    return 0


def update() -> int:
    manifest = load()
    for mirror, canonical in sorted(manifest.get("mirrors", {}).items()):
        source, target = ROOT / canonical, ROOT / mirror
        if source.exists():
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(source.read_bytes())
            target.chmod(source.stat().st_mode)
            print(f"  mirrored {canonical} -> {mirror}")
    for rel in sorted(manifest.get("frozen", {})):
        path = ROOT / rel
        if path.exists():
            manifest["frozen"][rel] = sha256(path)
            print(f"  froze {rel}  {manifest['frozen'][rel][:16]}")
    MANIFEST.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "--check"
    if mode == "--update":
        raise SystemExit(update())
    if mode != "--check":
        print(__doc__)
        raise SystemExit(2)
    raise SystemExit(check())
