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
            _provision(box, task, root, seed=spec.seed)
            material_dir = _inject(box, task, material, adapter, spec.arm)
            run = _run_agent(adapter, spec, box, task, material, material_dir)
            assertions, graded = _check(box, task, root, run.commands)
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
            score=graded.get("score"),
            milestones=graded.get("milestones") or [],
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


def _provision(box, task, root: Path, *, seed: int = 0) -> None:
    """Install the task's files and run its setup, before any arm material."""
    files = task.directory / "files"
    if files.is_dir():
        for entry in sorted(files.iterdir()):
            box.put(entry, f"{box.workdir}/{entry.name}")
    tools = root / "tools"
    if tools.is_dir():
        box.exec(["mkdir", "-p", box.bin_dir])
        for tool_dir in sorted(tools.iterdir()):
            # Anything under tools/ with the executable bit becomes a command
            # in the task container, so a helper script dropped here would
            # silently become part of the environment under test. Names
            # starting with _ or . are not tools.
            if not tool_dir.is_dir() or tool_dir.name.startswith((".", "_")):
                continue
            for script in sorted(tool_dir.glob("*")):
                if script.name.startswith((".", "_")) or script.name.startswith("test_"):
                    continue
                if script.is_file() and script.stat().st_mode & 0o111:
                    target = f"{box.bin_dir}/{script.stem}"
                    box.put(script, target)
                    box.exec(["chmod", "+x", target])
    # Setup commands must not hardcode a backend's directory layout: the
    # docker backend installs tools into /usr/local/bin, the local one into
    # a per-sandbox bin so the host is never modified. Tasks address them
    # through these variables instead.
    setup_env = {
        "V2SBENCH_BIN": box.bin_dir,
        "V2SBENCH_WORKDIR": box.workdir,
        "PATH_PREPEND": box.bin_dir,
        # Without this a seedable environment is pinned per task, so every
        # trial of a task shares one world: fine for pairing, but it means
        # repeated trials re-measure the same instance rather than sampling
        # the environment, which is not what `environment.seedable` implies.
        "V2SBENCH_SEED": str(seed),
    }
    for command in task.environment.setup:
        result = box.exec(["sh", "-lc", command], timeout=300, env=setup_env)
        if not result.ok:
            raise sandbox_mod.SandboxError(
                f"setup failed for {task.id}: {command!r}: {result.stderr.strip()[:300]}"
            )


def _inject(box, task, material, adapter, arm) -> str | None:
    """Place the arm's material. The only intended difference between arms.

    How a skill is *offered* differs by agent, and getting that wrong
    silently changes which arm you are running. Claude Code discovers a
    skill from its description in `.claude/skills/`; Codex concatenates
    every `AGENTS.md` into the prompt, so inlining there produces the
    forced condition, not the ambient one. An adapter that knows the
    difference supplies its own injector.
    """
    if not material.skill_dir:
        return None
    source = Path(material.skill_dir)

    injector = getattr(adapter, "inject_skill", None)
    if callable(injector):
        return injector(box, box.workdir, source, forced=(arm.id == "A7"))

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


def _check(
    box, task, root: Path, commands: list[str]
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
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
    payload = _parse_checker_output(result.stdout, task.id, result.stderr)
    graded = {k: payload[k] for k in ("score", "milestones") if k in payload}
    return payload["assertions"], graded


def _write_log(root: Path, task_id: str, commands: list[str]) -> Path:
    path = root / ".cache" / f"commands-{task_id}.log"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(commands) + ("\n" if commands else ""))
    return path


def _parse_checker_output(stdout: str, task_id: str, stderr: str) -> dict[str, Any]:
    """Read the checker's JSON.

    Returns the whole payload, not just the assertions. An earlier version
    returned the assertion list alone, which quietly discarded `score` and
    `milestones` -- the graded endpoint the game tier exists to produce.
    Both checkers were emitting them correctly and nothing was reading
    them, so the loss was invisible from either end.

    A checker that printed nothing usable is an environment error, not a
    task failure.
    """
    for line in reversed(stdout.strip().splitlines()):
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            payload = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(payload.get("assertions"), list):
            return payload
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
