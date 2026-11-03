"""Running one trial: one (task, arm, seed).

The ordering here carries most of the benchmark's integrity:

1. The environment is provisioned and the task's own files are installed.
2. The arm's material is injected -- and nothing else differs between arms.
3. The agent runs.
4. **Only then** is the checker copied in, outside the workdir.

Step 4 is not a tidiness preference. If the checker is present while the
agent works, the agent can read the assertions and satisfy them directly,
and the benchmark measures its own grader. The checker is copied to a
directory the agent never saw, after it has stopped.
"""

from __future__ import annotations

import json
import shlex
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from . import arms as arms_mod
from . import materials, metrics, record
from . import sandbox as sandbox_mod
from .agent.base import AgentError, AgentRun

#: Where a skill package is installed so it is discoverable by description.
SKILL_PARENT = ".claude/skills"


@dataclass
class TrialSpec:
    task: Any
    arm: arms_mod.Arm
    seed: int
    run_id: str


@dataclass
class TrialOutcome:
    record: dict[str, Any]
    status: str


def run_trial(
    spec: TrialSpec,
    *,
    root: Path,
    adapter,
    provenance: record.Provenance,
    backend: str = "docker",
    allow_unsandboxed: bool = False,
    probe_known: float | None = None,
    strict_materials: bool = True,
) -> TrialOutcome:
    task, arm = spec.task, spec.arm
    provenance.started_at = record.utcnow()
    provenance.network_policy = task.environment.network

    try:
        material = materials.resolve(root, task, arm, strict=strict_materials)
    except materials.MaterialError as exc:
        return _error(spec, provenance, material=None, detail=str(exc))

    if material.blind_check == "fail":
        # A material that contains the checker's literals is an answer key.
        # Refusing is the only safe outcome: scoring it would publish a
        # number produced by leakage.
        return _error(
            spec, provenance, material=material,
            detail="material failed the leakage gate; refusing to score this trial",
            status="refused",
        )

    box = sandbox_mod.create(
        backend,
        image=task.environment.image,
        workdir=task.environment.workdir,
        network=task.environment.network,
        display=task.environment.display,
        allow_unsandboxed=allow_unsandboxed,
    )

    try:
        with box:
            provenance.sandbox_backend = box.backend
            provenance.image_digest = box.image_digest()
            _provision(box, task, root)
            material_dir = _inject(box, task, material)
            run = _run_agent(adapter, spec, box, task, material, material_dir)
            assertions = _check(box, task, root, run.commands)
    except (sandbox_mod.SandboxError, AgentError) as exc:
        return _error(spec, provenance, material=material, detail=str(exc))

    provenance.finished_at = record.utcnow()
    provenance.agent_version = adapter.version()
    trace = metrics.evaluate(
        run.commands,
        key_actions=task.check.key_actions,
        forbidden_actions=task.check.forbidden_actions,
    )
    success = record.decide_success(assertions, task.check.required_assertions)
    status = "budget_exceeded" if run.timed_out else "ok"

    return TrialOutcome(
        status=status,
        record=record.build(
            run_id=spec.run_id,
            task=task,
            arm=arm,
            seed=spec.seed,
            status=status,
            success=success if status == "ok" else None,
            assertions=assertions,
            metrics={
                "turns": run.turns,
                "wall_secs": run.wall_secs,
                "tokens_in": run.tokens_in,
                "tokens_out": run.tokens_out,
                "usd_cost": run.usd_cost,
                "first_try": metrics.first_try(run.commands, key_actions=task.check.key_actions),
                "key_action_hit": trace.key_action_hit,
                "key_actions_matched": trace.key_actions_matched,
                "hallucinated_cmds": trace.hallucinated_cmds,
                "policy_violations": trace.policy_violations,
                "skill_cited": run.material_read,
                "action_modality": trace.action_modality,
                "skill_steps": material.extra.get("steps"),
            },
            material=material,
            provenance=provenance,
            commands=run.commands,
            probe_known=probe_known,
        ),
    )


# -- phases ----------------------------------------------------------------


def _provision(box, task, root: Path) -> None:
    """Install the task's files and run its setup, before any arm material."""
    files = task.directory / "files"
    if files.is_dir():
        for entry in sorted(files.iterdir()):
            box.put(entry, f"{box.workdir}/{entry.name}")
    tools = root / "tools"
    if tools.is_dir():
        box.exec(["mkdir", "-p", box.bin_dir])
        for tool_dir in sorted(tools.iterdir()):
            for script in sorted(tool_dir.glob("*")):
                if script.is_file() and script.stat().st_mode & 0o111:
                    target = f"{box.bin_dir}/{script.stem}"
                    box.put(script, target)
                    box.exec(["chmod", "+x", target])
    for command in task.environment.setup:
        result = box.exec(["sh", "-lc", command], timeout=300)
        if not result.ok:
            raise sandbox_mod.SandboxError(
                f"setup failed for {task.id}: {command!r}: {result.stderr.strip()[:300]}"
            )


def _inject(box, task, material) -> str | None:
    """Place the arm's material. The only intended difference between arms."""
    if not material.skill_dir:
        return None
    source = Path(material.skill_dir)
    target_parent = f"{box.workdir}/{SKILL_PARENT}"
    box.exec(["mkdir", "-p", target_parent])
    target = f"{target_parent}/{source.name}"
    box.put(source, target)
    return target


def _run_agent(adapter, spec, box, task, material, material_dir) -> AgentRun:
    prompt = _prompt(task)
    forced = None
    if spec.arm.id == "A7" and material.skill_dir:
        # The forced arm names the skill explicitly instead of relying on
        # discovery from its description.
        forced = f"/{Path(material.skill_dir).name}"
    probe = getattr(adapter, "probe", None)
    if callable(probe):
        probe(box)
    return adapter.run(
        prompt=prompt,
        sandbox=box,
        workdir=box.workdir,
        material_dir=material_dir,
        material_text=material.text,
        forced_invocation=forced,
        max_turns=task.budget.max_turns,
        wall_secs=task.budget.wall_secs,
        seed=spec.seed,
    )


def _prompt(task) -> str:
    """The task prompt, identical across arms."""
    base = (task.directory.parent.parent / "_prompts" / "baseline.md")
    preamble = base.read_text().strip() if base.exists() else _DEFAULT_PREAMBLE
    return f"{preamble}\n\n## Task\n\n{task.goal}\n"


_DEFAULT_PREAMBLE = (
    "You are working in a throwaway container. Complete the task below.\n\n"
    "Before acting, inspect what is available: read any help output, manual "
    "pages, README files and configuration present on the system. Prefer "
    "verifying the effect of a command over assuming it. Work only inside the "
    "current directory."
)


def _check(box, task, root: Path, commands: list[str]) -> list[dict[str, Any]]:
    """Copy the checker in and run it, after the agent has stopped."""
    mount = f"{box.support_dir}/check"
    box.exec(["mkdir", "-p", mount])
    box.put(task.check_dir, mount)
    log = f"{box.support_dir}/commands.log"
    box.put(_write_log(root, task.id, commands), log)
    entry = f"{mount}/{task.check.entrypoint}"
    probe = box.exec(["sh", "-lc", f"test -f {shlex.quote(entry)} || echo missing"])
    if "missing" in probe.stdout:
        entry = f"{mount}/{task.check_dir.name}/{task.check.entrypoint}"
    result = box.exec(
        ["sh", "-lc", f"chmod +x {shlex.quote(entry)} 2>/dev/null; "
         f"{shlex.quote(entry)} {shlex.quote(box.workdir)} {shlex.quote(log)}"],
        timeout=300,
    )
    if result.timed_out:
        raise sandbox_mod.SandboxError(f"checker timed out for {task.id}")
    return _parse_assertions(result.stdout, task.id, result.stderr)


def _write_log(root: Path, task_id: str, commands: list[str]) -> Path:
    path = root / ".cache" / f"commands-{task_id}.log"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(commands) + ("\n" if commands else ""))
    return path


def _parse_assertions(stdout: str, task_id: str, stderr: str) -> list[dict[str, Any]]:
    """Read the checker's JSON. A checker that printed nothing usable is an
    environment error, not a task failure."""
    for line in reversed(stdout.strip().splitlines()):
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        found = payload.get("assertions")
        if isinstance(found, list):
            return found
    raise sandbox_mod.SandboxError(
        f"checker for {task_id} printed no assertion JSON "
        f"(stdout tail: {stdout.strip()[-200:]!r}, stderr tail: {stderr.strip()[-200:]!r})"
    )


def _error(spec, provenance, *, material, detail: str, status: str = "error") -> TrialOutcome:
    provenance.finished_at = record.utcnow()
    blank = material or materials.ArmMaterial(kind=spec.arm.material, blind_check="skipped")
    return TrialOutcome(
        status=status,
        record=record.build(
            run_id=spec.run_id, task=spec.task, arm=spec.arm, seed=spec.seed,
            status=status, success=None, assertions=[], metrics={},
            material=blank, provenance=provenance, commands=[], error_detail=detail,
        ),
    )
