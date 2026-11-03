"""Cold knowledge probes: what does the model already know?

Each task declares the one fact or ordering its solution requires. The
probe asks for that fact directly -- no tools, no skills, no filesystem --
and records how often the answer is right. That number, ``probe_known``, is
the task's measured headroom.

It does two jobs.

It makes the **exclusion rule honest**. Dropping tasks because the arms did
not separate is selection on the outcome and mechanically inflates the
reported lift; dropping them because the model demonstrably already knew
the answer is a judgement made before any arm runs. Only the probe can
support the second kind.

And it measures the **knowing-doing gap** directly. A task with a high
probe score and a low baseline success rate is one the model can state and
cannot do -- the gap documented in the game-agent literature, and the gap a
procedural skill is supposed to close. Reporting both numbers side by side
is more informative than either alone.
"""

from __future__ import annotations

import json
import re
import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

#: Flags that make a probe a probe. `--tools ""` disables every built-in
#: tool, and slash commands are disabled so no installed skill can answer
#: on the model's behalf -- which would measure the skill, not the model.
PROBE_FLAGS = ("--tools", "", "--disable-slash-commands")

PROBE_PREAMBLE = (
    "Answer the following question directly and concisely, from your own "
    "knowledge. You have no tools and no documentation. If you do not know, "
    "say so plainly rather than guessing.\n\n"
)


@dataclass
class ProbeResult:
    task_id: str
    repeats: int
    correct: int
    answers: list[str] = field(default_factory=list)
    errors: int = 0

    @property
    def known(self) -> float:
        scored = self.repeats - self.errors
        return self.correct / scored if scored else 0.0

    def band(self) -> str:
        if self.known >= 0.9:
            return "low"
        if self.known >= 0.4:
            return "medium"
        return "high"


def matches(answer: str, pattern: str) -> bool:
    """Whether an answer counts as correct.

    Deliberately permissive about prose and strict about the fact: the
    pattern targets the specific token the task hinges on, so a correct
    answer buried in a paragraph still counts, and a confident wrong answer
    does not.
    """
    try:
        return bool(re.search(pattern, answer, re.IGNORECASE | re.DOTALL))
    except re.error as exc:
        raise ValueError(f"bad probe_answer_pattern {pattern!r}: {exc}") from exc


def ask(
    question: str,
    *,
    model: str,
    binary: str = "claude",
    timeout: int = 120,
) -> str:
    """Put one question to the model with no tools and no skills."""
    argv = [
        binary, "-p", PROBE_PREAMBLE + question,
        "--model", model,
        *PROBE_FLAGS,
        "--output-format", "json",
        "--permission-mode", "manual",
        "--permission-prompts", "none",
    ]
    proc = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
    if proc.returncode != 0:
        raise RuntimeError(f"probe failed: {(proc.stderr or proc.stdout)[-400:]}")
    return _extract_text(proc.stdout)


def _extract_text(stdout: str) -> str:
    try:
        payload = json.loads(stdout)
    except json.JSONDecodeError:
        return stdout.strip()
    if isinstance(payload, dict):
        for key in ("result", "text", "content"):
            value = payload.get(key)
            if isinstance(value, str):
                return value.strip()
    return stdout.strip()


def probe_task(
    task: Any, *, model: str, repeats: int = 10, binary: str = "claude", pause: float = 0.0
) -> ProbeResult:
    result = ProbeResult(task_id=task.id, repeats=repeats, correct=0)
    pattern = task.knowledge_gap.probe_answer_pattern
    for _ in range(repeats):
        try:
            answer = ask(task.knowledge_gap.probe_question, model=model, binary=binary)
        except (RuntimeError, subprocess.TimeoutExpired) as exc:
            result.errors += 1
            result.answers.append(f"<error: {exc}>")
            continue
        result.answers.append(answer[:600])
        if matches(answer, pattern):
            result.correct += 1
        if pause:
            time.sleep(pause)
    return result


def write_probes(root: Path, results: list[ProbeResult], *, model: str) -> Path:
    """Persist probe scores, with the answers kept for inspection.

    The raw answers are part of the audit trail: a reader who disagrees
    with a ``probe_answer_pattern`` can see what the model actually said
    and re-score it.
    """
    path = root / "results" / "probes.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "schema_version": 1,
        "model": model,
        "probe_known": {r.task_id: round(r.known, 4) for r in results},
        "bands": {r.task_id: r.band() for r in results},
        "detail": {
            r.task_id: {
                "repeats": r.repeats,
                "correct": r.correct,
                "errors": r.errors,
                "answers": r.answers,
            }
            for r in results
        },
    }
    path.write_text(json.dumps(payload, indent=2) + "\n")
    return path


def saturated(probe: ProbeResult, task: Any) -> bool:
    """The only pre-declared exclusion: known cold AND self-correcting.

    Both conditions are required. A task the model can recite but whose
    environment gives no corrective feedback is exactly the knowing-doing
    case the benchmark is most interested in, so a high probe score alone
    must not remove it.
    """
    return probe.known >= 0.9 and task.feedback_bandwidth == "high"
