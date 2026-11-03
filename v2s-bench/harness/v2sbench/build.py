"""Executing the materials pipeline, one task at a time.

Separate from ``pipeline.py``, which describes the stages; this runs them.

Three properties matter more than speed here, because this is the step
somebody with one attempt has to get through:

**Resumable.** A task whose materials already exist is skipped unless
``--force``. So a build that dies on task 9 of 26 is re-run, not restarted,
and the eight tasks already paid for are not paid for twice.

**Isolated.** One task failing does not stop the others. The build reports
which tasks produced materials and which did not, and a partial build is a
usable state: arms are resolved per task, so a run can proceed over the
tasks that worked.

**Durable.** Materials land in ``artifacts/`` with a manifest recording the
extractor build that made them. They outlive the session, so a failed
trial run never costs the materials again.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path

from . import pipeline
from .materials import material_root


@dataclass
class TaskBuild:
    task_id: str
    produced: list[str] = field(default_factory=list)
    skipped: bool = False
    error: str | None = None
    seconds: float = 0.0

    @property
    def ok(self) -> bool:
        return self.error is None


@dataclass
class BuildSummary:
    builds: list[TaskBuild] = field(default_factory=list)
    extractor: dict[str, str] = field(default_factory=dict)

    def line(self) -> str:
        done = sum(1 for b in self.builds if b.ok and not b.skipped)
        skipped = sum(1 for b in self.builds if b.skipped)
        failed = [b for b in self.builds if not b.ok]
        bits = [f"{done} built", f"{skipped} already present", f"{len(failed)} failed"]
        return " · ".join(bits)


def _run(argv: list[str], *, cwd: Path | None = None, timeout: int = 1800) -> str:
    done = subprocess.run(
        argv, capture_output=True, text=True, cwd=cwd, timeout=timeout
    )
    if done.returncode != 0:
        raise pipeline.PipelineError(
            f"{argv[0]} failed ({done.returncode}): "
            f"{(done.stderr or done.stdout).strip()[-400:]}"
        )
    return done.stdout


def build_task(
    task,
    *,
    root: Path,
    extractor: str,
    model: str,
    force: bool = False,
    skip_verify: bool = False,
) -> TaskBuild:
    """Produce every generable material for one task."""
    result = TaskBuild(task_id=task.id)
    out_root = material_root(root, task.id)
    started = time.monotonic()

    if out_root.exists() and (out_root / "v2s-skill" / "SKILL.md").exists() and not force:
        result.skipped = True
        result.produced = [p.name for p in sorted(out_root.iterdir()) if p.is_dir()]
        return result

    work = root / ".cache" / "materials" / task.id
    work.mkdir(parents=True, exist_ok=True)
    out_root.mkdir(parents=True, exist_ok=True)

    try:
        video = _video(task, root)
        bundle = work / "bundle"
        if not bundle.exists() or force:
            _run([extractor, "extract", str(video), "--out", str(bundle)])

        # A1 / A3: analysis with frames.
        _analyse(bundle, model, frames=True)
        full = out_root / "v2s-skill"
        _compile(extractor, bundle, full, force=force)
        if not skip_verify:
            _verify(extractor, full)
        result.produced.append("v2s-skill")

        # A3: the same extraction with the badge stripped and no verify.
        unverified = out_root / "v2s-skill-unverified"
        if unverified.exists():
            shutil.rmtree(unverified)
        shutil.copytree(full, unverified)
        pipeline.strip_badge(unverified)
        result.produced.append("v2s-skill-unverified")

        # A2: frames physically removed, so it cannot look at pixels.
        stripped = pipeline.strip_frames(bundle, work / "bundle-transcript-only")
        _analyse(stripped, model, frames=False)
        transcript_skill = out_root / "transcript-skill"
        _compile(extractor, stripped, transcript_skill, force=force)
        result.produced.append("transcript-skill")

        # A6: the raw transcript, unprocessed.
        pipeline.write_raw_transcript(bundle, out_root / "raw-transcript.txt")
        result.produced.append("raw-transcript.txt")

    except (pipeline.PipelineError, subprocess.TimeoutExpired, OSError) as exc:
        result.error = str(exc)[:500]

    result.seconds = time.monotonic() - started
    return result


def link_placebos(root: Path, tasks) -> list[str]:
    """Point each task's A5 at its declared sibling's built skill.

    The placebo is the real pipeline over a different video for the same
    tool, so it is not generated again -- it *is* the sibling's material.
    Copied rather than symlinked so the hash recorded on a trial is of the
    bytes that were actually injected.
    """
    by_id = {t.id: t for t in tasks}
    linked: list[str] = []
    for task in tasks:
        source_id = task.placebo_source
        if not source_id or source_id not in by_id:
            continue
        source = material_root(root, source_id) / "v2s-skill"
        target = material_root(root, task.id) / "placebo-skill"
        if not source.exists():
            continue
        if target.exists():
            shutil.rmtree(target)
        shutil.copytree(source, target)
        linked.append(task.id)
    return linked


def build_all(
    tasks,
    *,
    root: Path,
    extractor: str,
    model: str,
    force: bool = False,
    skip_verify: bool = False,
    on_task=None,
) -> BuildSummary:
    summary = BuildSummary(extractor=pipeline.extractor_provenance(extractor))
    for task in tasks:
        build = build_task(
            task, root=root, extractor=extractor, model=model,
            force=force, skip_verify=skip_verify,
        )
        summary.builds.append(build)
        if on_task:
            on_task(build)
    linked = link_placebos(root, tasks)
    _write_manifest(root, summary, linked)
    return summary


def _video(task, root: Path) -> Path:
    """Render the hermetic video, or locate the verified real one."""
    if task.video.tier == "hermetic":
        from .render import render

        out_dir = root / ".cache" / "videos"
        out_dir.mkdir(parents=True, exist_ok=True)
        target = out_dir / f"{task.video.ref}.mp4"
        if not target.exists():
            recipe = root / "videos" / "recipes" / f"{task.video.ref}.json"
            if not recipe.exists():
                raise pipeline.PipelineError(f"no recipe at {recipe}")
            render(recipe, target, require_narration=True)
        return target
    raise pipeline.PipelineError(
        f"task {task.id} uses a real video; videos/manifest.yaml has no entries yet, "
        "and a real video must be fetched and sha256-verified before use"
    )


def _analyse(bundle: Path, model: str, *, frames: bool) -> None:
    """Run the agent-driven analysis in a workspace holding only the bundle.

    The task directory is never passed, so the checker and oracle are not
    reachable: the procedure cannot be shaped by how it will be graded.
    """
    argv = pipeline._analysis_argv(bundle, model, frames=frames)
    _run(argv, cwd=bundle, timeout=3600)
    ir = bundle / "procedure.json"
    if not ir.exists():
        raise pipeline.PipelineError(
            f"the analysis stage produced no procedure.json in {bundle}. "
            "A non-procedural video legitimately produces none, but these "
            "recipes all teach a procedure, so this is a failure."
        )


def _compile(extractor: str, bundle: Path, out: Path, *, force: bool) -> None:
    if out.exists():
        if not force:
            return
        shutil.rmtree(out)
    _run([
        extractor, "compile",
        "--bundle", str(bundle),
        "--ir", str(bundle / "procedure.json"),
        "--out", str(out),
    ])


def _verify(extractor: str, package: Path) -> None:
    """Execute the steps and write the badge.

    Verification needs an OS sandbox; where none exists the extractor
    refuses rather than running video-derived commands unconfined. That
    refusal is correct behaviour, so it is surfaced with the remedy rather
    than swallowed -- but it only costs arm A1 its badge, and A3 exists
    precisely to be the unverified comparison.
    """
    try:
        _run([extractor, "verify", "--package", str(package)])
    except pipeline.PipelineError as exc:
        raise pipeline.PipelineError(
            f"verify failed for {package.name}: {exc}. On Linux this usually "
            "means bubblewrap is missing (`apt-get install bubblewrap`). Pass "
            "--skip-verify to build A1 without a badge, but then A1 and A3 "
            "differ only in the badge text and H4 cannot be tested."
        ) from exc


def _write_manifest(root: Path, summary: BuildSummary, linked: list[str]) -> Path:
    """Record what was built and which v2s build built it."""
    path = root / "artifacts" / "manifest.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "extractor": summary.extractor,
                "placebos_linked": linked,
                "tasks": {
                    b.task_id: {
                        "produced": b.produced,
                        "skipped": b.skipped,
                        "error": b.error,
                        "seconds": round(b.seconds, 1),
                    }
                    for b in summary.builds
                },
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    return path
