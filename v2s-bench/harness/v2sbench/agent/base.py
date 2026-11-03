"""The adapter contract."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Protocol, runtime_checkable


class AgentError(RuntimeError):
    """The agent could not be run at all.

    Like ``SandboxError``, this produces ``status="error"`` and is excluded
    from the endpoint rather than counted as a task failure.
    """


@dataclass
class AgentRun:
    """What one trial of the agent produced."""

    commands: list[str] = field(default_factory=list)
    """Executed-command log, in order. The basis for procedure-fidelity,
    hallucinated-command and policy-violation metrics, and the only part of
    the trace the analysis reads."""

    turns: int | None = None
    tokens_in: int | None = None
    tokens_out: int | None = None
    usd_cost: float | None = None
    wall_secs: float | None = None
    timed_out: bool = False
    exit_code: int | None = None
    material_read: bool | None = None
    """Whether the trace shows the supplied material was actually read. An
    arm that wins without this is evidence of a harness leak, not utility."""

    action_modality: str = "unknown"
    stdout_tail: str = ""
    stderr_tail: str = ""
    transcript_path: str | None = None
    raw: dict = field(default_factory=dict)

    def budget_exceeded(self) -> bool:
        return self.timed_out


@runtime_checkable
class AgentAdapter(Protocol):
    """Runs one trial.

    Implementations must treat every arm identically apart from the
    material they are handed: same model, same tools, same budget, same
    prompt. Anything else makes the comparison void.
    """

    name: str

    def version(self) -> str | None: ...

    def run(
        self,
        *,
        prompt: str,
        sandbox: object,
        workdir: str,
        material_dir: str | None,
        material_text: str | None,
        forced_invocation: str | None,
        max_turns: int,
        wall_secs: int,
        seed: int,
        env: dict[str, str] | None = None,
    ) -> AgentRun: ...
