"""Secondary metrics derived from a trial's executed-command log.

These are the measurements that survive a task the agent failed. Task
success is binary and coarse: an agent can follow the taught procedure
exactly and still fail on an unrelated detail, or stumble into success
without ever using what it was given. Procedure fidelity and the
skill-read flag separate those cases, and both are needed to tell a real
effect from a harness leak.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field


@dataclass
class TraceMetrics:
    key_action_hit: bool | None = None
    key_actions_matched: list[str] = field(default_factory=list)
    policy_violations: list[str] = field(default_factory=list)
    hallucinated_cmds: list[str] = field(default_factory=list)
    action_modality: str = "unknown"


def _compile(patterns: tuple[str, ...]) -> list[tuple[str, re.Pattern[str]]]:
    out: list[tuple[str, re.Pattern[str]]] = []
    for raw in patterns:
        try:
            out.append((raw, re.compile(raw)))
        except re.error as exc:
            raise ValueError(f"bad regex in task definition: {raw!r}: {exc}") from exc
    return out


def evaluate(
    commands: list[str],
    *,
    key_actions: tuple[str, ...] = (),
    forbidden_actions: tuple[str, ...] = (),
    known_commands: tuple[str, ...] = (),
) -> TraceMetrics:
    """Score a command log against a task's declared patterns.

    ``key_action_hit`` requires EVERY declared key action to appear, because
    a partial match of a multi-step procedure is not the procedure. It is
    ``None`` when a task declares none, so "no key actions declared" is
    never silently reported as a miss.
    """
    joined = "\n".join(commands)
    metrics = TraceMetrics()

    keys = _compile(key_actions)
    if keys:
        matched = [raw for raw, pattern in keys if pattern.search(joined)]
        metrics.key_actions_matched = matched
        metrics.key_action_hit = len(matched) == len(keys)

    for raw, pattern in _compile(forbidden_actions):
        if pattern.search(joined):
            metrics.policy_violations.append(raw)

    if known_commands:
        allowed = set(known_commands)
        for command in commands:
            head = _head(command)
            if head and head not in allowed:
                metrics.hallucinated_cmds.append(head)

    metrics.action_modality = _modality(commands)
    return metrics


_GUI_TOOLS = ("xdotool", "wmctrl", "scrot", "import ", "ydotool", "playwright")


def _modality(commands: list[str]) -> str:
    """Classify how the agent acted.

    Published GUI suites have a substantial fraction of tasks that are
    silently solvable from a terminal, so a skill that says "just run this
    command" can manufacture a large and meaningless lift on a task that was
    supposed to exercise the GUI. Recording modality makes that visible
    instead of letting it inflate the headline.
    """
    if not commands:
        return "unknown"
    joined = "\n".join(commands)
    gui = any(tool in joined for tool in _GUI_TOOLS)
    shell = any(_head(c) not in {None, ""} for c in commands)
    if gui and shell:
        return "mixed"
    if gui:
        return "gui"
    return "shell"


def _head(command: str) -> str | None:
    """The invoked program name, ignoring env assignments and sudo."""
    for token in command.strip().split():
        if "=" in token and not token.startswith("-"):
            continue
        if token in {"sudo", "env", "nohup", "time"}:
            continue
        return token.split("/")[-1]
    return None


def first_try(commands: list[str], *, key_actions: tuple[str, ...] = ()) -> bool | None:
    """Whether the first declared key action appeared in the first command.

    A weak proxy, and labelled as one: it answers "did it go straight
    there?" rather than "was it efficient".
    """
    if not key_actions or not commands:
        return None
    try:
        pattern = re.compile(key_actions[0])
    except re.error:
        return None
    return bool(pattern.search(commands[0]))
