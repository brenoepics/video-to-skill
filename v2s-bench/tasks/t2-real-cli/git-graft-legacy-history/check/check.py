#!/usr/bin/env python3
"""Checker for git-graft-legacy-history.

The task's whole point is the difference between what git's walk *shows*
and what the commit objects *record*, so the checker reads both: once
normally, and once with object substitution switched off.

`no-commit-was-rewritten` is the assertion that carries the task. A rebase
or a history filter produces a log identical to the one asked for, and the
only trace is that the recorded parents have changed -- which is exactly
what is read here, with no commit hash hard-coded: the vendor import must
still *record* no parent at all, and each branch must still have the same
number of real commits behind it.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

MAIN_SUBJECTS = ["fix the retry ceiling", "wire up the loader", "import the vendor drop"]
LEGACY_SUBJECTS = ["first round of fixes", "initial import"]


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

    def git(*args: str, raw: bool = False) -> tuple[int, str]:
        argv = ["git", "-c", "safe.directory=*", "-C", str(work)]
        if raw:
            argv.append("--no-replace-objects")
        argv.extend(args)
        try:
            done = subprocess.run(argv, capture_output=True, text=True, timeout=120)
        except (OSError, subprocess.TimeoutExpired) as exc:
            return 128, str(exc)
        return done.returncode, (done.stdout or "") + ("" if done.returncode == 0 else done.stderr)

    code, subjects = git("log", "--format=%s", "main")
    shown = [line for line in subjects.splitlines() if line.strip()] if code == 0 else []
    want = MAIN_SUBJECTS + LEGACY_SUBJECTS
    add(
        "apparent-history-is-one-line",
        shown == want,
        "main's log walks %r" % (shown,) if shown != want else "main's log walks all five commits, oldest last",
    )

    code, _ = git("merge-base", "--is-ancestor", "legacy", "main")
    add(
        "legacy-reachable-from-main",
        code == 0,
        "legacy is an ancestor of main" if code == 0 else "legacy is not reachable from main",
    )

    # What the objects actually record, with substitution switched off.
    code_main, raw_main = git("log", "--format=%s", "main", raw=True)
    code_leg, raw_leg = git("log", "--format=%s", "legacy", raw=True)
    real_main = [line for line in raw_main.splitlines() if line.strip()] if code_main == 0 else []
    real_leg = [line for line in raw_leg.splitlines() if line.strip()] if code_leg == 0 else []
    code_roots, roots = git("rev-list", "--max-parents=0", "main", raw=True)
    root_count = len([line for line in roots.split() if line]) if code_roots == 0 else 0
    unrewritten = real_main == MAIN_SUBJECTS and real_leg == LEGACY_SUBJECTS and root_count == 1
    add(
        "no-commit-was-rewritten",
        unrewritten,
        "the objects still record three parentless-rooted commits on main and two on legacy"
        if unrewritten
        else "recorded history changed: main=%r legacy=%r parentless roots on main=%d"
        % (real_main, real_leg, root_count),
    )

    code_m, main_sha = git("rev-parse", "main")
    code_l, legacy_sha = git("rev-parse", "legacy")
    code_t, tip_subject = git("log", "-1", "--format=%s", "main", raw=True)
    code_ls, legacy_subject = git("log", "-1", "--format=%s", "legacy", raw=True)
    refs_ok = (
        code_m == 0 and code_l == 0
        and main_sha.strip() != legacy_sha.strip()
        and tip_subject.strip() == MAIN_SUBJECTS[0]
        and legacy_subject.strip() == LEGACY_SUBJECTS[0]
    )
    add(
        "refs-still-name-the-same-commits",
        refs_ok,
        "main and legacy still name their original tips"
        if refs_ok
        else "main tip=%r legacy tip=%r" % (tip_subject.strip(), legacy_subject.strip()),
    )

    code_status, status = git("status", "--porcelain")
    code_head, head = git("symbolic-ref", "--short", "HEAD")
    files_ok = (work / "README.md").is_file() and (work / "config.ini").is_file()
    config_text = (work / "config.ini").read_text(encoding="utf-8") if files_ok else ""
    clean_ok = (
        code_status == 0 and not status.strip()
        and code_head == 0 and head.strip() == "main"
        and files_ok and "retries = 5" in config_text
    )
    add(
        "worktree-clean-on-main",
        clean_ok,
        "clean working tree, HEAD on main, files intact"
        if clean_ok
        else "HEAD=%r status=%r" % (head.strip(), status.strip()[:120]),
    )

    add(
        "joined-by-object-substitution",
        bool(git("for-each-ref", "refs/replace")[1].strip()),
        "a replacement ref is present",
        required=False,
    )

    add(
        "no-rewriting-command-in-the-log",
        re.search(r"(?mi)^\s*git\s+(rebase|filter-branch)\b", log_text) is None,
        "no history-rewriting command appears in the command log",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
