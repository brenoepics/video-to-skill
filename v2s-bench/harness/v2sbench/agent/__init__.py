"""Adapters for the agent under test.

The adapter's only job is to run one trial and report what happened, in
enough detail that the trial can be audited later: the commands actually
executed, the turn count, the token usage, and whether the supplied
material was ever read.

Two adapters ship:

``stub``
    Deterministic, offline, free. It does not call a model. Everything it
    produces is marked ``is_mock`` and can never be aggregated into a
    published figure. It exists so the whole pipeline -- sandbox, checker,
    records, statistics, site -- can be exercised and tested without
    spending money, which is also how the harness is tested in CI.
``claude-code``
    The real thing: the Claude Code CLI in print mode.
"""

from __future__ import annotations

from .base import AgentAdapter, AgentError, AgentRun
from .factory import create

__all__ = ["AgentAdapter", "AgentError", "AgentRun", "create"]
