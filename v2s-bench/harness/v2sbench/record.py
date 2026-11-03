"""Building and validating immutable trial records.

A record is the only durable output of a trial. Everything published is
recomputed from a set of them, so the shape is validated on write: a
malformed record discovered at analysis time is a trial that has to be
re-run and paid for twice.
"""

from __future__ import annotations

import hashlib
import json
import platform
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from . import SCHEMA_VERSION, __version__


def utcnow() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def trial_id(run_id: str, task: str, arm: str, seed: int) -> str:
    digest = hashlib.sha256(f"{run_id}|{task}|{arm}|{seed}".encode()).hexdigest()
    return f"t_{digest[:16]}"


def harness_digest() -> str:
    """Content hash of the harness source.

    Harness changes have been shown to move the same model's score on the
    same tasks by tens of points, so the harness is pinned exactly like the
    model is.
    """
    here = Path(__file__).parent
    digest = hashlib.sha256()
    for path in sorted(here.rglob("*.py")):
        digest.update(path.relative_to(here).as_posix().encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()[:16]


def host_info() -> dict[str, Any]:
    return {
        "platform": platform.platform(),
        "python": platform.python_version(),
        "machine": platform.machine(),
    }


@dataclass
class Provenance:
    model: str
    started_at: str
    harness_version: str = __version__
    harness_digest: str = field(default_factory=harness_digest)
    suite_version: str | None = None
    suite_commit: str | None = None
    prereg_commit: str | None = None
    model_params: dict[str, Any] = field(default_factory=dict)
    agent_adapter: str = "stub"
    agent_version: str | None = None
    image_digest: str | None = None
    sandbox_backend: str = "docker"
    network_policy: str = "none"
    host: dict[str, Any] = field(default_factory=host_info)
    finished_at: str | None = None
    is_mock: bool = False


def build(
    *,
    run_id: str,
    task,
    arm,
    seed: int,
    status: str,
    success: bool | None,
    assertions: list[dict[str, Any]],
    metrics: dict[str, Any],
    material,
    provenance: Provenance,
    commands: list[str],
    probe_known: float | None = None,
    score: float | None = None,
    milestones: list[str] | None = None,
    error_detail: str | None = None,
    transcript_path: str | None = None,
    extra: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Assemble one record. Pure: no I/O, so it is trivially testable."""
    return {
        "schema_version": SCHEMA_VERSION,
        "trial_id": trial_id(run_id, task.id, arm.id, seed),
        "run_id": run_id,
        "task": {
            "id": task.id,
            "tier": task.tier,
            "suite_version": provenance.suite_version or "unknown",
            "discoverability": task.discoverability,
            "headroom_band": task.headroom_band(probe_known),
            "saturated": task.saturated,
        },
        "arm": arm.id,
        "seed": seed,
        "outcome": {
            "status": status,
            "success": success,
            "assertions": assertions,
            "score": score,
            "milestones": milestones or [],
            "error_detail": error_detail,
        },
        "metrics": metrics,
        "material": {
            "kind": material.kind,
            "sha256": material.sha256,
            "chars": material.chars,
            "verified_steps": material.verified_steps,
            "blind_check": material.blind_check,
        },
        "video": {
            "tier": task.video.tier,
            "ref": task.video.ref,
            "sha256": task.video.sha256,
            "license": task.license,
        },
        "provenance": asdict(provenance),
        "trace": {
            "commands": commands,
            "transcript_path": transcript_path,
            "transcript_sha256": None,
        },
        "extra": extra or {},
    }


def decide_success(
    assertions: list[dict[str, Any]], required: tuple[str, ...]
) -> bool:
    """Success is conjunctive over the required assertions.

    When a task declares none, every assertion marked ``required`` by the
    checker must pass. An empty assertion list is a failure, not a pass: a
    checker that produced nothing has told us nothing, and defaulting that
    to success is how suites end up passing empty responses.
    """
    if not assertions:
        return False
    if required:
        by_id = {a["id"]: a for a in assertions}
        missing = [r for r in required if r not in by_id]
        if missing:
            return False
        return all(bool(by_id[r]["passed"]) for r in required)
    flagged = [a for a in assertions if a.get("required", True)]
    if not flagged:
        return False
    return all(bool(a["passed"]) for a in flagged)


def validator(root: Path):
    import jsonschema

    schema = json.loads((root / "schemas" / "run-record.schema.json").read_text())
    return jsonschema.Draft202012Validator(schema)


def write(record: dict[str, Any], path: Path, *, root: Path | None = None) -> Path:
    """Append a validated record to a JSONL shard."""
    if root is not None:
        errors = sorted(validator(root).iter_errors(record), key=str)
        if errors:
            detail = "; ".join(f"{list(e.absolute_path)}: {e.message}" for e in errors[:4])
            raise ValueError(f"refusing to write an invalid record: {detail}")
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, sort_keys=True) + "\n")
    return path


def read_all(results_dir: Path) -> list[dict[str, Any]]:
    """Load every record under a results directory, newest shard last."""
    records: list[dict[str, Any]] = []
    for path in sorted(results_dir.rglob("*.jsonl")):
        for line in path.read_text().splitlines():
            line = line.strip()
            if line:
                records.append(json.loads(line))
    return records
