#!/usr/bin/env python3
"""Checker for rsync-hardlink-snapshot.

Judged on the inode numbers and link counts actually on disk, never on
apparent size: a plain recursive copy produces a snapshot that matches the
source file for file and content for content, and the only difference is
that nothing is shared. `unchanged-files-are-one-stored-file` is therefore
the assertion that carries the task, and `dropped-file-absent` catches the
other plausible procedure -- hard-linking yesterday's tree wholesale and
copying the source over it, which leaves the retired file behind.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

OLD = "snapshots/2026-11-01"
NEW = "snapshots/2026-11-02"
UNCHANGED = ("alpha.txt", "sub/nested.txt")
CHANGED = "beta.txt"
DROPPED = "retired.txt"
OLD_CONTENTS = {
    "alpha.txt": b"alpha\n",
    "beta.txt": b"beta v1\n",
    "sub/nested.txt": b"nested\n",
    "retired.txt": b"retired\n",
}


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

    source = tree(work / "source")
    new = tree(work / NEW)
    old = tree(work / OLD)

    add(
        "new-snapshot-matches-the-source",
        bool(source) and new == source,
        "the new snapshot holds exactly the %d files in source/, byte for byte" % len(source)
        if new == source
        else "new snapshot %r against source %r"
        % (sorted(new), sorted(source)),
    )

    shared: list[str] = []
    unshared: list[str] = []
    for name in UNCHANGED:
        a, b = work / OLD / name, work / NEW / name
        if not (a.is_file() and b.is_file()):
            unshared.append("%s missing" % name)
            continue
        sa, sb = a.stat(), b.stat()
        if sa.st_ino == sb.st_ino and sa.st_dev == sb.st_dev and sa.st_nlink >= 2:
            shared.append("%s inode %d, %d links" % (name, sa.st_ino, sa.st_nlink))
        else:
            unshared.append(
                "%s: inodes %d/%d, links %d/%d" % (name, sa.st_ino, sb.st_ino, sa.st_nlink, sb.st_nlink)
            )
    add(
        "unchanged-files-are-one-stored-file",
        not unshared and len(shared) == len(UNCHANGED),
        "; ".join(shared) if not unshared else "copied instead of shared: " + "; ".join(unshared),
    )

    a, b = work / OLD / CHANGED, work / NEW / CHANGED
    changed_ok = False
    detail = "%s is missing from one of the snapshots" % CHANGED
    if a.is_file() and b.is_file():
        sa, sb = a.stat(), b.stat()
        changed_ok = (
            sa.st_ino != sb.st_ino
            and a.read_bytes() == OLD_CONTENTS[CHANGED]
            and b.read_bytes() == (work / "source" / CHANGED).read_bytes()
            and b.read_bytes() != OLD_CONTENTS[CHANGED]
        )
        detail = "old %r / new %r, inodes %d/%d" % (
            a.read_bytes()[:24], b.read_bytes()[:24], sa.st_ino, sb.st_ino,
        )
    add("changed-file-is-its-own-copy", changed_ok, detail)

    add(
        "dropped-file-absent",
        DROPPED not in new,
        "%s does not appear in the new snapshot" % DROPPED
        if DROPPED not in new
        else "%s was carried into the new snapshot" % DROPPED,
    )

    source_ok = source == {
        "alpha.txt": b"alpha\n",
        "beta.txt": b"beta v2, with the courier note appended\n",
        "gamma.txt": b"gamma\n",
        "sub/nested.txt": b"nested\n",
    }
    old_ok = old == OLD_CONTENTS
    add(
        "source-and-old-snapshot-untouched",
        source_ok and old_ok,
        "source/ and yesterday's snapshot are unchanged"
        if source_ok and old_ok
        else "source ok=%s, old snapshot ok=%s (old holds %r)" % (source_ok, old_ok, sorted(old)),
    )

    add(
        "linked-against-the-previous-snapshot",
        re.search(r"(?mi)^\s*rsync\b[^\n]*--link-dest", log_text) is not None,
        "rsync was pointed at the previous snapshot in the command log",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
