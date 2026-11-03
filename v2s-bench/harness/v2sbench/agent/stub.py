"""A deterministic, free, offline stand-in for the agent under test.

It calls no model. Instead it extracts the backticked and fenced commands
from whatever material its arm was given and runs them in order. With no
material it does nothing and the task fails.

That makes its results trivially predictable -- skill arms pass, the
baseline fails -- which is useless as a measurement and exactly right as a
test fixture: it drives the real sandbox, the real checker, the real record
writer and the real statistics, so the whole pipeline can be exercised in
CI for nothing.

Every record it produces is stamped ``is_mock``, and the report builder
refuses to aggregate mock and real records into one figure. The stub
cannot become a published number by accident.
"""

from __future__ import annotations

import re
import time

from .base import AgentRun

#: Commands inside single backticks, and inside ```sh fences.
_INLINE = re.compile(r"`([^`\n]{2,400})`")
_FENCE = re.compile(r"```(?:sh|bash|shell|console)?\n(.*?)```", re.DOTALL)

#: Lines that are prose or prompts rather than commands.
_SKIP = re.compile(r"^\s*(#|\$\s*$|>|//)")


class StubAgent:
    name = "stub"
    is_mock = True

    def __init__(self, *, execute: bool = True, max_commands: int = 40) -> None:
        self.execute = execute
        self.max_commands = max_commands

    def version(self) -> str | None:
        return "stub-1"

    def run(
        self,
        *,
        prompt: str,
        sandbox,
        workdir: str,
        material_dir: str | None = None,
        material_text: str | None = None,
        forced_invocation: str | None = None,
        max_turns: int = 40,
        wall_secs: int = 600,
        seed: int = 0,
        env: dict[str, str] | None = None,
    ) -> AgentRun:
        started = time.monotonic()
        run = AgentRun()
        source = _material_source(sandbox, material_dir, material_text)
        commands = extract_commands(source)[: self.max_commands]
        run.material_read = bool(source)

        for command in commands:
            run.commands.append(command)
            if not self.execute:
                continue
            result = sandbox.exec(["sh", "-lc", command], timeout=60, cwd=workdir)
            if result.timed_out:
                run.timed_out = True
                break
            if time.monotonic() - started > wall_secs:
                run.timed_out = True
                break

        run.turns = len(commands) or 1
        run.tokens_in = 0
        run.tokens_out = 0
        run.usd_cost = 0.0
        run.wall_secs = time.monotonic() - started
        run.exit_code = 0
        run.action_modality = "shell" if run.commands else "unknown"
        return run


def _material_source(sandbox, material_dir: str | None, material_text: str | None) -> str:
    if material_text:
        return material_text
    if not material_dir:
        return ""
    probe = sandbox.exec(
        ["sh", "-lc", f"cat {material_dir}/SKILL.md 2>/dev/null || true"], timeout=30
    )
    return probe.stdout or ""


def extract_commands(text: str) -> list[str]:
    """Pull runnable commands out of a skill or transcript, in document order.

    Order matters and is easy to get wrong: collecting all fenced blocks
    and then all inline spans runs a later step before an earlier one, so a
    two-phase procedure executes backwards and the fixture reports a
    failure that the material did not actually contain. Both kinds of span
    are therefore collected in one pass, sorted by position, the way
    anything reading the document top to bottom would.
    """
    spans: list[tuple[int, str]] = []

    for match in _FENCE.finditer(text):
        offset = match.start()
        for index, line in enumerate(match.group(1).splitlines()):
            line = line.strip()
            if line and not _SKIP.match(line):
                spans.append((offset + index, _strip_prompt(line)))

    fenced = [(m.start(), m.end()) for m in _FENCE.finditer(text)]
    for match in _INLINE.finditer(text):
        if any(start <= match.start() < end for start, end in fenced):
            continue  # already captured as part of a fenced block
        candidate = _strip_prompt(match.group(1).strip())
        if candidate and _looks_runnable(candidate):
            spans.append((match.start(), candidate))

    seen: set[str] = set()
    ordered: list[str] = []
    for _, command in sorted(spans, key=lambda pair: pair[0]):
        if command not in seen:
            seen.add(command)
            ordered.append(command)
    return ordered


def _strip_prompt(line: str) -> str:
    for prefix in ("$ ", "% ", "> "):
        if line.startswith(prefix):
            return line[len(prefix) :].strip()
    return line


def _looks_runnable(candidate: str) -> bool:
    """Reject inline code spans that are plainly not commands."""
    if candidate.startswith((":", "-", "{", "[")) or " " not in candidate:
        return bool(re.match(r"^[a-z][a-z0-9_.-]*$", candidate))
    return not candidate.endswith((".", "?", "!"))
