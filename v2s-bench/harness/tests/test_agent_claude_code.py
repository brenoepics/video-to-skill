"""The Claude Code adapter's contract, pinned against real CLI output.

The fixture in ``_REAL_RESULT_EVENT`` is a trimmed copy of a genuine
``result`` event captured from ``claude -p ... --output-format stream-json``
(version 2.1.317). It is here because the two bugs these tests guard
against are invisible in synthetic fixtures: both only show up when the
real field names and real magnitudes are present.
"""

from __future__ import annotations

import json

import pytest

from v2sbench.agent.base import AgentError
from v2sbench.agent.claude_code import (
    FORBIDDEN_FLAGS,
    ClaudeCodeAgent,
    parse_stream_json,
)

#: Captured from a real run. inputTokens is genuinely 4 while the cache
#: fields are five figures -- which is the whole point of the test below.
_REAL_RESULT_EVENT = {
    "type": "result",
    "subtype": "success",
    "is_error": False,
    "duration_ms": 9598,
    "num_turns": 2,
    "session_id": "4fb39b88-1bdb-410f-9aa1-8f84cce360a4",
    "total_cost_usd": 0.0463236,
    "modelUsage": {
        "claude-sonnet-5-5": {
            "inputTokens": 4,
            "outputTokens": 107,
            "cacheReadInputTokens": 62096,
            "cacheCreationInputTokens": 9759,
            "costUSD": 0.0463236,
            "costBasis": "list",
        }
    },
    # Present in real output and deliberately ignored: it omits subagent
    # tokens, so reading it would undercount arms whose skill delegates.
    "usage": {"input_tokens": 4, "output_tokens": 107},
}

_ASSISTANT_EVENT = {
    "type": "assistant",
    "message": {
        "content": [
            {"type": "text", "text": "I'll run that."},
            {
                "type": "tool_use",
                "name": "Bash",
                "input": {"command": "echo v2sbench-adapter-probe"},
            },
        ]
    },
}


def _stream(*events: dict) -> str:
    return "\n".join(json.dumps(e) for e in events) + "\n"


def test_input_tokens_include_the_cache_fields() -> None:
    """Cache tokens are input tokens.

    Summing only ``inputTokens`` reported 4 against an actual 71 859 on the
    captured run -- and it would understate longer-prompt arms more than
    short ones, biasing the cost comparison toward the treatment. Cost is a
    reported endpoint, so this is a correctness issue, not bookkeeping.
    """
    run = parse_stream_json(_stream(_ASSISTANT_EVENT, _REAL_RESULT_EVENT))

    assert run.tokens_in == 4 + 62096 + 9759
    assert run.tokens_out == 107
    breakdown = run.raw["token_breakdown"]
    assert breakdown["cacheReadInputTokens"] == 62096
    assert breakdown["cacheCreationInputTokens"] == 9759
    assert breakdown["costBasis"] == "list"


def test_cost_and_turns_come_from_the_result_event() -> None:
    run = parse_stream_json(_stream(_ASSISTANT_EVENT, _REAL_RESULT_EVENT))
    assert run.usd_cost == pytest.approx(0.0463236)
    assert run.turns == 2
    assert run.raw["result"]["subtype"] == "success"


def test_bash_commands_are_logged_in_order() -> None:
    second = {
        "type": "assistant",
        "message": {
            "content": [
                {"type": "tool_use", "name": "Bash", "input": {"command": "ls -la"}}
            ]
        },
    }
    run = parse_stream_json(_stream(_ASSISTANT_EVENT, second, _REAL_RESULT_EVENT))
    assert run.commands == ["echo v2sbench-adapter-probe", "ls -la"]
    assert run.action_modality == "shell"


def test_a_malformed_line_loses_the_line_not_the_trial() -> None:
    """A trial discarded for a parse error is a trial silently removed from
    one arm, which is worse than a slightly incomplete trace."""
    raw = "not json\n" + json.dumps(_ASSISTANT_EVENT) + "\n{broken\n" + json.dumps(
        _REAL_RESULT_EVENT
    ) + "\n"
    run = parse_stream_json(raw)
    assert run.commands == ["echo v2sbench-adapter-probe"]
    assert run.tokens_in > 0


def test_bare_is_refused() -> None:
    """--bare leaves a skill present but undiscoverable by description, so
    the ambient arm would collapse to the baseline and the benchmark would
    report a null caused entirely by a harness flag."""
    with pytest.raises(AgentError, match="ambient skill discovery"):
        ClaudeCodeAgent(model="claude-sonnet-5-5", extra_args=("--bare",))
    assert "--bare" in FORBIDDEN_FLAGS


@pytest.mark.parametrize("alias", ["opus", "sonnet", "haiku", "fable"])
def test_model_aliases_are_refused(alias: str) -> None:
    """An alias resolves to a different model over time, which makes a
    figure recorded against it uninterpretable later."""
    with pytest.raises(AgentError, match="alias"):
        ClaudeCodeAgent(model=alias)


def test_invocation_sets_permission_mode_explicitly() -> None:
    """A -p run with no --permission-mode starts in `manual` and denies
    anything needing approval, which would measure the permission system."""
    argv = ClaudeCodeAgent(model="claude-sonnet-5-5").argv("do it", max_turns=7)

    assert "--permission-mode" in argv
    assert argv[argv.index("--permission-mode") + 1] == "acceptEdits"
    assert argv[argv.index("--permission-prompts") + 1] == "none"
    assert argv[argv.index("--max-turns") + 1] == "7"
    assert argv[argv.index("--output-format") + 1] == "stream-json"
    assert "--bare" not in argv


def test_raw_transcript_arm_enters_through_the_prompt() -> None:
    from v2sbench.agent.claude_code import _compose_prompt

    composed = _compose_prompt("Do the task.", "spoken words here", None)
    assert "Do the task." in composed
    assert "spoken words here" in composed
    assert "transcript of a tutorial video" in composed


def test_forced_arm_prefixes_an_explicit_invocation() -> None:
    from v2sbench.agent.claude_code import _compose_prompt

    composed = _compose_prompt("Do the task.", None, "/widget-sealing")
    assert composed.startswith("/widget-sealing")
