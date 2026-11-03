"""The Codex adapter's contract.

Written against the documented `codex exec` interface. The binary was not
available where this was written, so these tests pin the two decisions
that would silently corrupt a cross-model comparison rather than claiming
the adapter has been exercised end to end.
"""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

import pytest

from v2sbench.agent.base import AgentError
from v2sbench.agent.codex import CodexAgent, parse_jsonl
from v2sbench.agent.factory import create
from v2sbench.sandbox import create as make_sandbox

#: Shape taken from the documented `turn.completed` event.
_TURN_COMPLETED = {
    "type": "turn.completed",
    "usage": {
        "input_tokens": 120,
        "cached_input_tokens": 48_000,
        "output_tokens": 300,
        "reasoning_output_tokens": 900,
    },
}


def _stream(*events: dict) -> str:
    return "\n".join(json.dumps(e) for e in events) + "\n"


def test_cached_input_counts_as_input() -> None:
    """The same trap as the Claude adapter, in a different field name.

    Omitting `cached_input_tokens` understates input, and understates it
    more for arms carrying a longer prompt — which is exactly the
    comparison being made.
    """
    run = parse_jsonl(_stream({"type": "turn.started"}, _TURN_COMPLETED))
    assert run.tokens_in == 120 + 48_000
    assert run.tokens_out == 300 + 900, "reasoning tokens are billed output"


def test_commands_are_extracted_in_order() -> None:
    events = [
        {"type": "turn.started"},
        {"type": "item.completed", "item": {"item_type": "command_execution",
                                            "command": ["sh", "-c", "widget open"]}},
        {"type": "item.completed", "item": {"item_type": "command_execution",
                                            "command": "widget seal --grant g-7731"}},
        _TURN_COMPLETED,
    ]
    run = parse_jsonl(_stream(*events))
    assert run.commands == ["sh -c widget open", "widget seal --grant g-7731"]
    assert run.action_modality == "shell"


def test_a_malformed_line_loses_the_line_not_the_trial() -> None:
    raw = "not json\n" + json.dumps(_TURN_COMPLETED) + "\n{broken\n"
    run = parse_jsonl(raw)
    assert run.tokens_in > 0


def test_invocation_sets_a_writable_sandbox_and_json() -> None:
    """Codex defaults to a read-only sandbox, so a trial that does not set
    this cannot act at all and fails for a reason unrelated to the skill."""
    argv = CodexAgent(model="gpt-6.1").argv("do it")
    assert argv[:2] == ["codex", "exec"]
    assert argv[argv.index("--sandbox") + 1] == "workspace-write"
    assert "--json" in argv and "--ignore-user-config" in argv
    assert argv[argv.index("--model") + 1] == "gpt-6.1"


def test_full_access_is_refused() -> None:
    with pytest.raises(AgentError, match="danger-full-access"):
        CodexAgent(model="gpt-6.1", sandbox="danger-full-access")


def test_a_model_is_required() -> None:
    with pytest.raises(AgentError, match="model"):
        create("codex")


def _skill(tmp: Path) -> Path:
    skill = tmp / "widget-sealing"
    skill.mkdir()
    (skill / "SKILL.md").write_text(
        "---\nname: widget-sealing\n"
        "description: Use when sealing a widget drop.\n---\n\n"
        "# Seal\n\nRun `widget seal --grant g-7731`.\n"
    )
    return skill


def test_ambient_advertises_but_does_not_inline() -> None:
    """A1 must stay ambient under Codex.

    `AGENTS.md` is concatenated into the prompt on every run, so inlining
    a skill there is the *forced* arm. If A1 inlined, a cross-model
    comparison would be ambient-Claude against forced-Codex and the gap
    would read as a model effect.
    """
    agent = CodexAgent(model="gpt-6.1")
    with tempfile.TemporaryDirectory() as tmp:
        skill = _skill(Path(tmp))
        box = make_sandbox("local", allow_unsandboxed=True)
        with box:
            agent.inject_skill(box, box.workdir, skill, forced=False)
            agents = (Path(box.workdir) / "AGENTS.md").read_text()
            assert "Use when sealing a widget drop." in agents
            assert "--grant g-7731" not in agents, "the procedure must not be inlined"
            assert (Path(box.workdir) / "skills" / "widget-sealing" / "SKILL.md").exists()


def test_forced_inlines_the_whole_procedure() -> None:
    agent = CodexAgent(model="gpt-6.1")
    with tempfile.TemporaryDirectory() as tmp:
        skill = _skill(Path(tmp))
        box = make_sandbox("local", allow_unsandboxed=True)
        with box:
            agent.inject_skill(box, box.workdir, skill, forced=True)
            agents = (Path(box.workdir) / "AGENTS.md").read_text()
            assert "--grant g-7731" in agents
