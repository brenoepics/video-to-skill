#!/usr/bin/env python3
"""Checker for git-bisect-first-bad.

The answer is recomputed here from scratch rather than hard-coded: every
commit on main is extracted into a scratch directory and judged by the
repository's own verdict script, so the expected commit follows from the
fixture and from the script's three-way contract, not from a constant.
That is also what makes `answer-is-the-first-bad-commit` catch the
plausible wrong procedure -- a hand-driven search that reads the
cannot-judge verdict as a failure converges on the unjudgeable commit,
which looks entirely reasonable and is the wrong commit.

`no-operation-in-progress` catches the other common wrong end state: an
automated search that found the right commit and was never told to let go,
leaving the repository parked on a detached revision.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

SUBJECTS = [
    "add the ratio helper",
    "note the algorithm",
    "describe the output format",
    "document the argument types",
    "start on the input guard",
    "drop the half-written input guard",
    "simplify the common-factor step",
    "mention the usage line",
    "point at the background note",
]
SKIP = 125


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
        return done.returncode, done.stdout if done.returncode == 0 else done.stderr

    code, listing = git("rev-list", "--reverse", "main")
    commits = listing.split() if code == 0 else []

    # Judge every revision the way the repository's own script does, without
    # checking anything out.
    scratch = Path(tempfile.mkdtemp(prefix="verdict-"))
    verdicts: list[tuple[str, int]] = []
    try:
        for sha in commits:
            ok = True
            for name in ("ratio.sh", "check-ratio.sh"):
                code_show, blob = git("show", "%s:%s" % (sha, name))
                if code_show != 0:
                    ok = False
                    break
                (scratch / name).write_text(blob, encoding="utf-8")
            if not ok:
                verdicts.append((sha, SKIP))
                continue
            done = subprocess.run(
                ["sh", "check-ratio.sh"], cwd=scratch, capture_output=True, timeout=120
            )
            verdicts.append((sha, done.returncode))
    finally:
        shutil.rmtree(scratch, ignore_errors=True)

    expected = next((sha for sha, verdict in verdicts if verdict not in (0, SKIP)), None)

    try:
        answer_raw = (work / "first-bad.txt").read_text(encoding="utf-8")
    except OSError:
        answer_raw = ""
    answer_lines = [line for line in answer_raw.splitlines() if line.strip()]
    answer = answer_lines[0].strip() if len(answer_lines) == 1 else ""
    well_formed = bool(re.fullmatch(r"[0-9a-f]{40}", answer))
    resolves = False
    if well_formed:
        code_cat, kind = git("cat-file", "-t", answer)
        resolves = code_cat == 0 and kind.strip() == "commit"
    add(
        "answer-is-a-commit",
        well_formed and resolves,
        "first-bad.txt holds one full commit hash that resolves in this repository"
        if well_formed and resolves
        else "first-bad.txt holds %r" % (answer_raw[:120],),
    )

    add(
        "answer-is-the-first-bad-commit",
        expected is not None and answer == expected,
        "names the first commit the verdict script actually rejects"
        if expected is not None and answer == expected
        else "verdicts were %r; expected %s, got %s"
        % (
            [(s[:7], v) for s, v in verdicts],
            None if expected is None else expected[:7],
            answer[:7] or None,
        ),
    )

    gitdir = work / ".git"
    in_progress = [
        name
        for name in ("BISECT_START", "BISECT_LOG", "BISECT_EXPECTED_REV",
                     "rebase-merge", "rebase-apply", "CHERRY_PICK_HEAD", "MERGE_HEAD")
        if (gitdir / name).exists()
    ]
    if (gitdir / "refs" / "bisect").is_dir() and any((gitdir / "refs" / "bisect").iterdir()):
        in_progress.append("refs/bisect")
    code_head, head = git("symbolic-ref", "--short", "HEAD")
    on_main = code_head == 0 and head.strip() == "main"
    add(
        "no-operation-in-progress",
        not in_progress and on_main,
        "HEAD is on main and nothing is in progress"
        if not in_progress and on_main
        else "HEAD=%r, left behind: %r" % (head.strip(), in_progress),
    )

    code_sub, subjects = git("log", "--format=%s", "--reverse", "main")
    seen = [line for line in subjects.splitlines() if line.strip()] if code_sub == 0 else []
    code_status, status = git("status", "--porcelain")
    tip_broken = verdicts and verdicts[-1][1] not in (0, SKIP)
    untouched = (
        seen == SUBJECTS
        and code_status == 0
        and not [line for line in status.splitlines() if not line.startswith("??")]
        and bool(tip_broken)
    )
    add(
        "history-and-tree-untouched",
        untouched,
        "the nine commits are unchanged, no tracked file was modified, and the tip is still broken"
        if untouched
        else "subjects=%r status=%r tip-still-broken=%s" % (seen, status.strip()[:120], bool(tip_broken)),
    )

    add(
        "search-was-driven-by-git",
        re.search(r"(?mi)^\s*git\s+bisect\s+run\b", log_text) is not None,
        "an automated bisect appears in the command log",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
