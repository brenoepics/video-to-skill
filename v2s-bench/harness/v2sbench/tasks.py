"""Task loading and validation.

A task is a directory: ``task.yaml`` beside a ``check/`` directory. The
separation is load-bearing -- ``check/`` is deliberately withheld from the
process that writes the skills, so a procedure cannot be shaped by
knowledge of how it will be graded (see ``docs/preregistration.md`` 3.2).
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import yaml

TIERS = ("t1-novel-cli", "t2-real-cli", "t3-gui", "t4-game")


def _schema_dir(root: Path) -> Path:
    return root / "schemas"


@dataclass(frozen=True)
class KnowledgeGap:
    """What the video teaches that the task requires.

    Without a declared gap a task cannot show an effect: the agent either
    already knew the procedure or could not learn it from the video. The
    probe fields turn that into a measurement rather than an assumption.
    """

    summary: str
    probe_question: str
    probe_answer_pattern: str
    taught_at: tuple[float, ...]


@dataclass(frozen=True)
class CheckSpec:
    entrypoint: str
    required_assertions: tuple[str, ...] = ()
    key_actions: tuple[str, ...] = ()
    forbidden_actions: tuple[str, ...] = ()
    leakage_forbidden: tuple[str, ...] = ()
    score: dict[str, Any] | None = None


@dataclass(frozen=True)
class OracleSpec:
    """A known-good solution that must pass before any agent trial runs."""

    entrypoint: str
    modality: str = "shell"


@dataclass(frozen=True)
class VideoRef:
    tier: str
    ref: str
    sha256: str | None = None


@dataclass(frozen=True)
class Budget:
    max_turns: int = 40
    wall_secs: int = 600
    max_usd: float = 2.0


@dataclass(frozen=True)
class Environment:
    image: str
    workdir: str
    setup: tuple[str, ...] = ()
    network: str = "none"
    display: bool = False
    seedable: bool = False
    gui_only: bool = False


@dataclass(frozen=True)
class Task:
    id: str
    tier: str
    title: str
    goal: str
    knowledge_gap: KnowledgeGap
    environment: Environment
    check: CheckSpec
    video: VideoRef
    oracle: OracleSpec
    directory: Path
    discoverability: str = "good"
    feedback_bandwidth: str = "high"
    placebo_source: str | None = None
    budget: Budget = field(default_factory=Budget)
    saturated: bool = False
    license: str | None = None
    attribution: str | None = None
    notes: str | None = None

    @property
    def check_dir(self) -> Path:
        return self.directory / "check"

    @property
    def oracle_entrypoint(self) -> str:
        return self.oracle.entrypoint

    @property
    def oracle_modality(self) -> str:
        return self.oracle.modality

    def headroom_band(self, probe_known: float | None) -> str:
        """Bucket a task by what the model already knows.

        Hypothesis H2 is tested across these bands. The thresholds are
        declared here, once, rather than chosen after seeing the data.
        """
        if self.saturated:
            return "saturated"
        if probe_known is None:
            return "unknown"
        if probe_known >= 0.9:
            return "low"
        if probe_known >= 0.4:
            return "medium"
        return "high"


def _validator(root: Path, name: str):
    import jsonschema

    schema = json.loads((_schema_dir(root) / name).read_text())
    return jsonschema.Draft202012Validator(schema)


def load_task(path: Path, root: Path, *, validate: bool = True) -> Task:
    """Load one ``task.yaml``.

    ``path`` may be the directory or the file. Validation is on by default:
    a malformed task is a benchmark-integrity problem, not a convenience
    problem, so the default is strict.
    """
    directory = path if path.is_dir() else path.parent
    data = yaml.safe_load((directory / "task.yaml").read_text())
    if validate:
        errors = sorted(_validator(root, "task.schema.json").iter_errors(data), key=str)
        if errors:
            joined = "; ".join(f"{list(e.absolute_path)}: {e.message}" for e in errors[:5])
            raise ValueError(f"{directory}/task.yaml does not validate: {joined}")

    gap = data["knowledge_gap"]
    env = data["environment"]
    chk = data["check"]
    vid = data["video"]
    bud = data.get("budget", {})

    task = Task(
        id=data["id"],
        tier=data["tier"],
        title=data["title"],
        goal=data["goal"],
        knowledge_gap=KnowledgeGap(
            summary=gap["summary"],
            probe_question=gap["probe_question"],
            probe_answer_pattern=gap["probe_answer_pattern"],
            taught_at=tuple(float(t) for t in gap["taught_at"]),
        ),
        environment=Environment(
            image=env["image"],
            workdir=env["workdir"],
            setup=tuple(env.get("setup", ())),
            network=env.get("network", "none"),
            display=bool(env.get("display", False)),
            seedable=bool(env.get("seedable", False)),
            gui_only=bool(env.get("gui_only", False)),
        ),
        check=CheckSpec(
            entrypoint=chk["entrypoint"],
            required_assertions=tuple(chk.get("required_assertions", ())),
            key_actions=tuple(chk.get("key_actions", ())),
            forbidden_actions=tuple(chk.get("forbidden_actions", ())),
            leakage_forbidden=tuple(chk.get("leakage_forbidden", ())),
            score=chk.get("score"),
        ),
        video=VideoRef(tier=vid["tier"], ref=vid["ref"], sha256=vid.get("sha256")),
        oracle=OracleSpec(
            entrypoint=data["oracle"]["entrypoint"],
            modality=data["oracle"].get("modality", "shell"),
        ),
        directory=directory,
        discoverability=data.get("discoverability", "good"),
        feedback_bandwidth=data.get("feedback_bandwidth", "high"),
        placebo_source=data.get("placebo_source"),
        budget=Budget(
            max_turns=int(bud.get("max_turns", 40)),
            wall_secs=int(bud.get("wall_secs", 600)),
            max_usd=float(bud.get("max_usd", 2.0)),
        ),
        saturated=bool(data.get("saturated", False)),
        license=data.get("license"),
        attribution=data.get("attribution"),
        notes=data.get("notes"),
    )
    _check_consistency(task, directory)
    return task


def _check_consistency(task: Task, directory: Path) -> None:
    """Catch the integrity problems a JSON Schema cannot express."""
    if directory.name != task.id:
        raise ValueError(f"task id {task.id!r} must equal its directory name {directory.name!r}")
    if task.tier not in TIERS:
        raise ValueError(f"unknown tier {task.tier!r}")
    if directory.parent.name != task.tier:
        raise ValueError(
            f"task {task.id} declares tier {task.tier} "
            f"but sits in {directory.parent.name}"
        )
    entry = directory / "check" / task.check.entrypoint
    if not entry.exists():
        raise ValueError(f"task {task.id}: missing checker {entry}")
    oracle = directory / "oracle" / task.oracle.entrypoint
    if not oracle.exists():
        raise ValueError(
            f"task {task.id}: missing oracle {oracle}. Every task needs a known-good "
            "solution that passes the checker: it proves the task is achievable and "
            "catches a defective checker before any trials are paid for."
        )
    if task.video.tier == "real" and not task.video.sha256:
        raise ValueError(
            f"task {task.id}: a real video needs a sha256, otherwise the row is not reproducible"
        )


def load_suite(root: Path, *, tiers: tuple[str, ...] = TIERS, validate: bool = True) -> list[Task]:
    """Load every task under ``root/tasks``, sorted by tier then id."""
    tasks: list[Task] = []
    for tier in tiers:
        tier_dir = root / "tasks" / tier
        if not tier_dir.is_dir():
            continue
        for entry in sorted(tier_dir.iterdir()):
            if entry.is_dir() and (entry / "task.yaml").exists():
                tasks.append(load_task(entry, root, validate=validate))
    seen: dict[str, Path] = {}
    for t in tasks:
        if t.id in seen:
            raise ValueError(f"duplicate task id {t.id!r}: {seen[t.id]} and {t.directory}")
        seen[t.id] = t.directory
    return tasks


def suite_version(root: Path) -> str:
    """A content hash of the task suite.

    Recorded on every trial so a figure can never be silently attributed
    to a suite that has since changed.
    """
    import hashlib

    h = hashlib.sha256()
    for path in sorted((root / "tasks").rglob("*")):
        if path.is_file() and not path.name.startswith("."):
            h.update(path.relative_to(root).as_posix().encode())
            h.update(path.read_bytes())
    return h.hexdigest()[:16]
