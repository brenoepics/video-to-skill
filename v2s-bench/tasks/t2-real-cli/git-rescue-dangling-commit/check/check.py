#!/usr/bin/env python3
"""Checker for git-rescue-dangling-commit.

`rescue-is-the-original-commit` is the assertion that carries the task. The
plausible wrong procedure is to read the lost work and commit it again: the
log then shows the same subject over the same parent with the same file, and
the only difference is the commit's own identity and its committer record.
So the recorded committer instant is checked against the one the fixture
wrote, which a commit made during the trial cannot have, and the commit is
required to be a parent of nothing new and to sit directly on main's tip.

No commit hash is hard-coded: main's tip is read from the repository and the
rescued commit is required to point at it.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

SUBJECT = "add the retry ceiling"
COMMITTED_AT = "2026-06-04T00:00:00+00:00"
CONTENT = "retries stop at five, then the caller sees the error\n"
MAIN_SUBJECTS = ["add the tokeniser", "scaffold the parser"]


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

    def git(*args: str) -> tuple[int, str]:
        argv = ["git", "-c", "safe.directory=*", "-C", str(work), *args]
        try:
            done = subprocess.run(argv, capture_output=True, text=True, timeout=120)
        except (OSError, subprocess.TimeoutExpired) as exc:
            return 128, str(exc)
        return done.returncode, (done.stdout if done.returncode == 0 else done.stderr)

    code, rescue_sha = git("rev-parse", "--verify", "refs/heads/rescue")
    exists = code == 0 and re.fullmatch(r"[0-9a-f]{40}", rescue_sha.strip() or "")
    add(
        "rescue-branch-exists",
        bool(exists),
        "refs/heads/rescue resolves to %s" % rescue_sha.strip()[:12]
        if exists
        else "there is no branch called rescue",
    )
    rescue = rescue_sha.strip() if exists else ""

    code_main, main_sha = git("rev-parse", "--verify", "main")
    main_tip = main_sha.strip() if code_main == 0 else ""

    fields = ("%s", "%cI", "%aI", "%P", "%an", "%cn")
    if rescue:
        code_f, blob = git("log", "-1", "--format=" + "%x00".join(fields), rescue)
        parts = blob.strip("\n").split("\x00") if code_f == 0 else []
    else:
        parts = []
    subject, committed, authored, parents, author, committer = (parts + [""] * 6)[:6]
    original = (
        subject == SUBJECT
        and committed == COMMITTED_AT
        and authored == COMMITTED_AT
        and parents.strip() == main_tip
        and bool(main_tip)
    )
    add(
        "rescue-is-the-original-commit",
        original,
        "rescue names the commit that was made: %s, committed %s, on main's tip"
        % (subject, committed)
        if original
        else "subject=%r committer-instant=%r author-instant=%r parent=%r (main is %r)"
        % (subject, committed, authored, parents.strip()[:12], main_tip[:12]),
    )

    code_show, content = git("show", "%s:retry.md" % rescue) if rescue else (1, "")
    add(
        "rescued-content-is-intact",
        code_show == 0 and content == CONTENT,
        "retry.md in the rescued commit is byte-identical to what was committed"
        if code_show == 0 and content == CONTENT
        else "retry.md in the rescued commit reads %r" % (content[:80],),
    )

    code_sub, subjects = git("log", "--format=%s", "main")
    seen = [line for line in subjects.splitlines() if line.strip()] if code_sub == 0 else []
    code_status, status = git("status", "--porcelain")
    code_head, head = git("symbolic-ref", "--short", "HEAD")
    worktree_ok = (
        seen == MAIN_SUBJECTS
        and code_status == 0
        and not [line for line in status.splitlines() if not line.startswith("??")]
        and code_head == 0 and head.strip() == "main"
        and not (work / "retry.md").exists()
    )
    add(
        "main-unmoved-and-clean",
        worktree_ok,
        "main still holds its two commits and the working tree is clean on main"
        if worktree_ok
        else "main=%r HEAD=%r status=%r retry.md checked out=%s"
        % (seen, head.strip(), status.strip()[:100], (work / "retry.md").exists()),
    )

    add(
        "found-by-auditing-the-object-store",
        re.search(r"(?mi)git\s+fsck|--lost-found|--unreachable|--dangling", log_text) is not None,
        "an object-store audit appears in the command log",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
