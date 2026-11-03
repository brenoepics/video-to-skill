#!/usr/bin/env python3
"""Regenerate env/crafter/PACKAGE.sha256.json from the installed Crafter.

Run inside the rung's image (or any environment installed from
``env/crafter/requirements.txt``) after changing the pin:

    python3 env/crafter/freeze_package.py

The checker verifies these hashes before it replays a journal. A replay is
only evidence about the agent if the rules it replays under are the pinned
ones -- an agent that edited ``objects.py`` could otherwise make every
achievement unlock on the first step and the replay would agree with it.
"""

from __future__ import annotations

import hashlib
import json
import pathlib

FILES = ("env.py", "objects.py", "engine.py", "constants.py", "worldgen.py", "data.yaml")
OUT = pathlib.Path(__file__).resolve().parent / "PACKAGE.sha256.json"
NOTE = (
    "sha256 of Crafter's rule-bearing files, as installed from "
    "env/crafter/requirements.txt. The checker verifies these before it "
    "replays a journal: the replay is only evidence if the rules it replays "
    "under are the pinned ones. Regenerate with "
    "env/crafter/freeze_package.py when the pin changes."
)

if __name__ == "__main__":
    import crafter

    package = pathlib.Path(crafter.__file__).parent
    payload = {
        "schema_version": 1,
        "note": NOTE,
        "package": "crafter==1.8.3",
        "files": {
            name: hashlib.sha256((package / name).read_bytes()).hexdigest()
            for name in FILES
        },
    }
    OUT.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print("wrote %s" % OUT)
