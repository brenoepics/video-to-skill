#!/usr/bin/env python3
"""Checker for rsync-mirror-protect-excluded.

`server-logs-survived` is the assertion that carries the task: the obvious
mirror command, and the option that sounds like a stricter version of it,
both delete the receiver-only directory and report nothing at all. The other
three discriminating assertions cover the remaining silent wrong results --
the scratch directory published, the retired page left behind, and the whole
source nested one level down because of a missing trailing slash.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

SITE = {
    "index.html": b"<h1>Harbour</h1><p>the current copy</p>\n",
    "about.html": b"<h1>About</h1>\n",
    "assets/logo.txt": b"logo bytes\n",
    "tmp/scratch.bin": b"scratch build artefact\n",
}
LOGS = {
    "logs/access.log": b"10.0.0.1 - GET /\n10.0.0.2 - GET /about.html\n",
    "logs/error.log": b"no errors yet\n",
}
PUBLISHABLE = {k: v for k, v in SITE.items() if not k.startswith("tmp/")}


def tree(root: Path) -> dict[str, bytes]:
    out: dict[str, bytes] = {}
    if not root.is_dir():
        return out
    for path in sorted(root.rglob("*")):
        if path.is_file() and not path.is_symlink():
            out[path.relative_to(root).as_posix()] = path.read_bytes()
    return out


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

    public = tree(work / "public")
    site = tree(work / "site")

    pages_ok = all(public.get(name) == body for name, body in PUBLISHABLE.items())
    add(
        "pages-brought-up-to-date",
        pages_ok,
        "every publishable file in public/ matches site/"
        if pages_ok
        else "mismatched: %r"
        % ([n for n, b in PUBLISHABLE.items() if public.get(n) != b],),
    )

    add(
        "retired-page-removed",
        "removed.html" not in public,
        "removed.html is gone from public/"
        if "removed.html" not in public
        else "removed.html is still published",
    )

    scratch = [name for name in public if name.startswith("tmp/") or name.endswith("scratch.bin")]
    add(
        "scratch-not-published",
        not scratch,
        "no build scratch in public/" if not scratch else "published: %r" % (scratch,),
    )

    logs_ok = all(public.get(name) == body for name, body in LOGS.items())
    add(
        "server-logs-survived",
        logs_ok,
        "public/logs/ came through byte for byte"
        if logs_ok
        else "server-side logs lost or altered: %r"
        % ({n: public.get(n) for n in LOGS},),
    )

    nested = [name for name in public if name.startswith("site/")]
    add(
        "no-nested-copy-of-the-source",
        not nested,
        "public/ has no directory named after the source"
        if not nested
        else "nested copy present: %r" % (nested[:4],),
    )

    extras = sorted(set(public) - set(PUBLISHABLE) - set(LOGS))
    add(
        "source-untouched",
        site == SITE,
        "site/ is unchanged" if site == SITE else "site/ now holds %r" % (sorted(site),),
    )
    add(
        "nothing-else-in-public",
        not extras,
        "public/ holds only the published files and the server's own logs"
        if not extras
        else "unexpected files in public/: %r" % (extras,),
        required=False,
    )

    add(
        "mirrored-with-a-deletion-pass",
        re.search(r"(?mi)^\s*rsync\b[^\n]*--delete", log_text) is not None,
        "a mirroring rsync with a deletion pass appears in the command log",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
