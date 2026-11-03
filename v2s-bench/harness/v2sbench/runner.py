"""The run loop.

Two scheduling decisions matter for the statistics, and both are made here
rather than left to chance.

**Arms are interleaved within a (task, seed).** The inter-arm correlation
is the highest-leverage term in the power model -- at 100 tasks the
detectable effect moves by nearly a factor of two across plausible values
of it -- and running arms as separate campaigns hours apart invites drift
in model serving, container state and machine load that decorrelates them.
Adjacent-in-time arms keep the pairing tight.

**Arm order rotates with the seed.** If A0 always ran first it would
systematically inherit a cold cache and A1 a warm one, which would look
exactly like a treatment effect.
"""

from __future__ import annotations

import time
import uuid
from collections.abc import Iterator, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from . import arms as arms_mod
from . import record
from .trial import TrialSpec, run_trial


@dataclass
class RunConfig:
    root: Path
    model: str
    armset: tuple[str, ...] = arms_mod.ARMSET_CORE
    seeds: tuple[int, ...] = (0, 1, 2)
    backend: str = "docker"
    allow_unsandboxed: bool = False
    max_usd: float | None = None
    stop_on_error: bool = False
    run_id: str = field(default_factory=lambda: f"run_{uuid.uuid4().hex[:10]}")
    prereg_commit: str | None = None
    suite_commit: str | None = None
    model_params: dict[str, Any] = field(default_factory=dict)
    strict_materials: bool = True
    probe_scores: dict[str, float] = field(default_factory=dict)


@dataclass
class RunSummary:
    run_id: str
    written: int = 0
    ok: int = 0
    errors: int = 0
    refused: int = 0
    usd: float = 0.0
    stopped_early: str | None = None
    path: Path | None = None

    def line(self) -> str:
        bits = [
            f"run {self.run_id}",
            f"{self.written} trials",
            f"{self.ok} ok",
            f"{self.errors} error",
        ]
        if self.refused:
            bits.append(f"{self.refused} refused")
        if self.usd:
            bits.append(f"${self.usd:.2f}")
        if self.stopped_early:
            bits.append(f"STOPPED: {self.stopped_early}")
        return " · ".join(bits)


def schedule(
    tasks: Sequence[Any], armset: tuple[str, ...], seeds: tuple[int, ...]
) -> Iterator[tuple[Any, arms_mod.Arm, int]]:
    """Yield trials task-major, then seed, then arm -- arms adjacent in time.

    The arm order rotates by seed so no arm is permanently first.
    """
    resolved = [arms_mod.resolve(a) for a in armset]
    for task in tasks:
        for seed in seeds:
            offset = seed % len(resolved)
            rotated = resolved[offset:] + resolved[:offset]
            for arm in rotated:
                yield task, arm, seed


def run(
    config: RunConfig,
    tasks: Sequence[Any],
    adapter,
    *,
    suite_version: str,
    on_trial=None,
) -> RunSummary:
    """Execute every (task, arm, seed) and append each record as it lands.

    Records are written incrementally: a run that dies halfway has still
    produced usable data, and nothing is held only in memory.
    """
    from .agent.factory import is_mock

    summary = RunSummary(run_id=config.run_id)
    out_path = config.root / "results" / "runs" / f"{config.run_id}.jsonl"
    summary.path = out_path
    mock = is_mock(adapter)

    for task, arm, seed in schedule(tasks, config.armset, config.seeds):
        if config.max_usd is not None and summary.usd >= config.max_usd:
            summary.stopped_early = f"cost guard reached (${summary.usd:.2f})"
            break

        provenance = record.Provenance(
            model=config.model,
            started_at=record.utcnow(),
            suite_version=suite_version,
            suite_commit=config.suite_commit,
            prereg_commit=config.prereg_commit,
            model_params=dict(config.model_params),
            agent_adapter=adapter.name,
            sandbox_backend=config.backend,
            is_mock=mock,
        )
        outcome = run_trial(
            TrialSpec(task=task, arm=arm, seed=seed, run_id=config.run_id),
            root=config.root,
            adapter=adapter,
            provenance=provenance,
            backend=config.backend,
            allow_unsandboxed=config.allow_unsandboxed,
            probe_known=config.probe_scores.get(task.id),
            strict_materials=config.strict_materials,
        )

        record.write(outcome.record, out_path, root=config.root)
        summary.written += 1
        if outcome.status == "ok":
            summary.ok += 1
        elif outcome.status == "refused":
            summary.refused += 1
        else:
            summary.errors += 1
        summary.usd += float(outcome.record["metrics"].get("usd_cost") or 0.0)

        if on_trial:
            on_trial(outcome)
        if config.stop_on_error and outcome.status == "error":
            summary.stopped_early = (
                f"{task.id}/{arm.id}: {outcome.record['outcome']['error_detail']}"
            )
            break

    return summary


def trivial_agent_gate(
    config: RunConfig, tasks: Sequence[Any], *, suite_version: str
) -> list[str]:
    """Run a do-nothing agent against every checker; it must score zero.

    Published suites have been found to pass empty responses and to score
    trivial agents at a third of tasks. This gate costs three lines and
    refutes that class of defect directly. It returns the ids of any task
    a null agent passed -- a non-empty list means the suite is broken.
    """
    from .agent.stub import StubAgent

    null = StubAgent(execute=False)
    null.name = "trivial"  # type: ignore[attr-defined]
    passed: list[str] = []
    for task in tasks:
        provenance = record.Provenance(
            model="none", started_at=record.utcnow(),
            suite_version=suite_version, agent_adapter="trivial", is_mock=True,
        )
        outcome = run_trial(
            TrialSpec(task=task, arm=arms_mod.A0, seed=0, run_id="gate_trivial"),
            root=config.root, adapter=null, provenance=provenance,
            backend=config.backend, allow_unsandboxed=config.allow_unsandboxed,
            strict_materials=False,
        )
        if outcome.record["outcome"].get("success"):
            passed.append(task.id)
    return passed


def oracle_gate(
    config: RunConfig, tasks: Sequence[Any], *, suite_version: str
) -> tuple[list[str], list[tuple[str, str]]]:
    """Run each task's oracle; it must pass.

    Returns ``(failed, errored)``. The split matters: a *failed* oracle
    means the task or its checker is broken, while an *errored* one means
    the environment did not come up -- a missing image, a setup command
    that assumed a path. Reporting the second as the first sends a reviewer
    hunting for a logic bug in a task that is fine.

    Independent measurements put the defect rate in borrowed agent
    benchmarks at 10-15%, so this gate pays for itself; but only if its
    output distinguishes a broken task from a broken machine.
    """
    from .agent.oracle import OracleAgent

    failed: list[str] = []
    errored: list[tuple[str, str]] = []
    for task in tasks:
        provenance = record.Provenance(
            model="none", started_at=record.utcnow(),
            suite_version=suite_version, agent_adapter="oracle", is_mock=True,
        )
        outcome = run_trial(
            TrialSpec(task=task, arm=arms_mod.A0, seed=0, run_id="gate_oracle"),
            root=config.root, adapter=OracleAgent(task), provenance=provenance,
            backend=config.backend, allow_unsandboxed=config.allow_unsandboxed,
            strict_materials=False,
        )
        result = outcome.record["outcome"]
        if result.get("status") != "ok":
            errored.append((task.id, (result.get("error_detail") or "")[:200]))
        elif not result.get("success"):
            failed.append(task.id)
    return failed, errored


def elapsed(started: float) -> str:
    secs = time.monotonic() - started
    return f"{secs/60:.1f}m" if secs >= 120 else f"{secs:.0f}s"
