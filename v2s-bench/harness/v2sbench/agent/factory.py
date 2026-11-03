"""Adapter selection.

Like the sandbox, the adapter is a property of the run and is recorded on
every trial: two arms run by different adapters are not comparable.
"""

from __future__ import annotations

from .base import AgentError

ADAPTERS = ("stub", "claude-code")


def create(name: str, **kwargs):
    if name == "stub":
        return _stub(**kwargs)
    if name in {"claude-code", "claude"}:
        from .claude_code import ClaudeCodeAgent

        if not kwargs.get("model"):
            raise AgentError(
                "the claude-code adapter needs an explicit --model with a full model id. "
                "Aliases are refused because they resolve to different models over time, "
                "which would make the recorded figure uninterpretable later."
            )
        return ClaudeCodeAgent(**kwargs)
    raise AgentError(f"unknown adapter {name!r}; known: {ADAPTERS}")


def _stub(**kwargs):
    from .stub import StubAgent

    kwargs.pop("model", None)
    allowed = {"execute", "max_commands"}
    return StubAgent(**{k: v for k, v in kwargs.items() if k in allowed})


def is_mock(adapter: object) -> bool:
    """Whether this adapter's output may never be published as a measurement."""
    return bool(getattr(adapter, "is_mock", False))
