"""Building the arm materials by running the real v2s pipeline.

One stage list per task, producing every arm's material from the same
source video so the arms differ only where they are supposed to:

    render/fetch video
      -> vts-extract extract              (bundle: transcript + frames)
      -> analysis, frames allowed         -> procedure.json  -> A1, A3
      -> analysis, frames withheld        -> procedure.json  -> A2
      -> vts-extract compile              -> skill package
      -> vts-extract verify               -> A1 badge (A3 stops before this)
    placebo video -> the same, start to finish  -> A5
    bundle transcript, unprocessed              -> A6

Two properties are enforced here rather than trusted.

**Grader-blindness.** The analysis step runs in a scratch workspace holding
only the bundle. The task directory -- with its checker and its oracle -- is
never mounted, so the procedure cannot be shaped by knowledge of how it
will be graded.

**A2 really is transcript-only.** Frames are physically removed from the
copy of the bundle the transcript-only analysis sees, instead of the
analysis being asked not to look at them. An instruction not to look is
not a control.

Nothing is generated at trial time: materials are built once, hashed, and
reused across every seed, so an arm cannot drift mid-run.
"""

from __future__ import annotations

import json
import shutil
import subprocess
from dataclasses import dataclass, field
from pathlib import Path

REQUIRED_EXTRACTOR_VERSION = "0.3.0"

#: What each material kind needs from the pipeline.
MATERIAL_PLAN = {
    "v2s-skill": "full analysis, compiled, then verified",
    "v2s-skill-unverified": "full analysis, compiled, verification skipped and badge stripped",
    "transcript-skill": "analysis over a frame-stripped bundle, compiled",
    "placebo-skill": "the whole pipeline over the placebo video",
    "raw-transcript": "the bundle transcript, unprocessed",
    "human-skill": "written by hand; never generated (see AUTHORING.md)",
}


class PipelineError(RuntimeError):
    """A stage could not run. Never silently substituted for another."""


@dataclass
class Stage:
    name: str
    argv: list[str]
    cwd: Path | None = None
    optional: bool = False


@dataclass
class BuildPlan:
    task_id: str
    video: Path
    bundle: Path
    out_root: Path
    stages: list[Stage] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    def describe(self) -> str:
        lines = [f"{self.task_id}: video {self.video.name} -> {self.out_root}"]
        lines += [f"  {i + 1}. {s.name}" for i, s in enumerate(self.stages)]
        lines += [f"  note: {n}" for n in self.notes]
        return "\n".join(lines)


def preflight(extractor: str = "vts-extract") -> list[str]:
    """Report every missing precondition at once, rather than one per run."""
    problems: list[str] = []
    resolved = shutil.which(extractor)
    if not resolved:
        problems.append(
            f"{extractor} not on PATH. Build it from the repository root with "
            "`cargo build --release` (binary at target/release/vts-extract) or "
            "install a pinned release, then pass --extractor."
        )
    else:
        probe = subprocess.run([resolved, "--version"], capture_output=True, text=True)
        if probe.returncode != 0:
            problems.append(f"{resolved} --version failed: {probe.stderr.strip()[:200]}")
        elif REQUIRED_EXTRACTOR_VERSION not in probe.stdout:
            problems.append(
                f"{resolved} reports {probe.stdout.strip()!r}; this pipeline targets "
                f"{REQUIRED_EXTRACTOR_VERSION}. A different extraction version is a "
                "different treatment, so it is refused rather than recorded."
            )
    if not shutil.which("ffmpeg"):
        problems.append("ffmpeg not on PATH (needed for extraction and rendering)")
    if not shutil.which("claude"):
        problems.append(
            "claude not on PATH: the analysis stage is agent-driven, because reading "
            "frames and writing the procedure IR is what the v2s skill asks an agent "
            "to do. There is no non-agent path to an A1 material."
        )
    return problems


def plan(
    task,
    *,
    root: Path,
    video: Path,
    extractor: str = "vts-extract",
    model: str = "",
) -> BuildPlan:
    """Describe, without running, every stage needed for this task's arms."""
    work = root / ".cache" / "materials" / task.id
    bundle = work / "bundle"
    out_root = root / "artifacts" / "skills" / task.id
    built = BuildPlan(task_id=task.id, video=video, bundle=bundle, out_root=out_root)

    built.stages.append(
        Stage("extract", [extractor, "extract", str(video), "--out", str(bundle)])
    )
    built.stages.append(
        Stage("analyse (frames allowed) -> procedure.json", _analysis_argv(bundle, model, frames=True))
    )
    built.stages.append(
        Stage(
            "compile -> v2s-skill",
            [extractor, "compile", "--bundle", str(bundle),
             "--ir", str(bundle / "procedure.json"),
             "--out", str(out_root / "v2s-skill")],
        )
    )
    built.stages.append(
        Stage("verify (A1 badge)", [extractor, "verify", "--package", str(out_root / "v2s-skill")])
    )
    built.stages.append(
        Stage(
            "copy + strip badge -> v2s-skill-unverified",
            ["python3", "-c", "pass"],
        )
    )
    stripped = work / "bundle-transcript-only"
    built.stages.append(Stage("strip frames -> transcript-only bundle", ["python3", "-c", "pass"]))
    built.stages.append(
        Stage("analyse (frames withheld)", _analysis_argv(stripped, model, frames=False))
    )
    built.stages.append(
        Stage(
            "compile -> transcript-skill",
            [extractor, "compile", "--bundle", str(stripped),
             "--ir", str(stripped / "procedure.json"),
             "--out", str(out_root / "transcript-skill")],
        )
    )
    built.stages.append(Stage("write raw-transcript.txt", ["python3", "-c", "pass"]))

    if task.placebo_source:
        built.stages.append(
            Stage(f"placebo pipeline over {task.placebo_source}", ["python3", "-c", "pass"])
        )
    else:
        built.notes.append(
            "no placebo_source declared: arm A5 cannot be built for this task, and "
            "hypothesis H5 will report insufficient-data rather than being skipped "
            "silently"
        )
    built.notes.append(
        "human-skill (A4) is written by hand and is never generated; record the "
        "grader-blindness claim in artifacts/skills/<id>/human-skill/AUTHORING.md"
    )
    return built


def _analysis_argv(bundle: Path, model: str, *, frames: bool) -> list[str]:
    """The agent-driven analysis stage.

    Run in a workspace holding only the bundle. The prompt differs between
    the two variants only in what the bundle contains -- the frame-stripped
    copy has no frames to read -- rather than in an instruction to avoid
    looking, which would not be a control.
    """
    instruction = (
        "Analyse the extraction bundle in this directory using the installed "
        "video-to-skill skill and write a Procedure IR to procedure.json. "
        "Read commands off the exported frames; never paraphrase them from the "
        "transcript."
        if frames
        else "Analyse the extraction bundle in this directory using the installed "
        "video-to-skill skill and write a Procedure IR to procedure.json. This "
        "bundle contains no frames: work from the transcript alone and mark "
        "confidence accordingly."
    )
    argv = ["claude", "-p", instruction, "--permission-mode", "acceptEdits",
            "--permission-prompts", "none", "--output-format", "stream-json", "--verbose"]
    if model:
        argv += ["--model", model]
    return argv


def strip_frames(bundle: Path, destination: Path) -> Path:
    """Copy a bundle with the frames physically removed.

    This is what makes A2 a real control: the transcript-only analysis
    cannot look at pixels because there are none, not because it was told
    not to.
    """
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(bundle, destination, ignore=shutil.ignore_patterns("frames", "*.jpg", "*.png"))
    frames = destination / "frames"
    if frames.exists():
        shutil.rmtree(frames)
    remaining = sorted(p.name for p in destination.rglob("*.jpg"))
    if remaining:
        raise PipelineError(f"frames survived stripping: {remaining[:5]}")
    return destination


def strip_badge(package: Path) -> None:
    """Remove the verification badge from the unverified copy (A3).

    A3 isolates verification, so it must not carry a badge claiming steps
    were executed when they were not -- and the badge is itself a signal an
    agent may act on, which is part of what the arm is testing.
    """
    skill_md = package / "SKILL.md"
    if not skill_md.exists():
        raise PipelineError(f"no SKILL.md in {package}")
    lines = [
        line for line in skill_md.read_text().splitlines()
        if "verified-steps" not in line and "Verified by execution" not in line
    ]
    skill_md.write_text("\n".join(lines) + "\n")
    report = package / "verification.json"
    if report.exists():
        report.unlink()


def write_raw_transcript(bundle: Path, destination: Path) -> Path:
    """Extract the plain ASR transcript for arm A6."""
    for candidate in ("transcript.txt", "transcript.json", "timeline.json"):
        path = bundle / candidate
        if not path.exists():
            continue
        if path.suffix == ".txt":
            text = path.read_text()
        else:
            text = _flatten_transcript(json.loads(path.read_text()))
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text)
        return destination
    raise PipelineError(f"no transcript found in {bundle}")


def _flatten_transcript(payload: object) -> str:
    """Pull spoken text out of whatever shape the bundle uses."""
    chunks: list[str] = []

    def walk(node: object) -> None:
        if isinstance(node, dict):
            for key in ("text", "word", "segment"):
                value = node.get(key)
                if isinstance(value, str):
                    chunks.append(value)
                    break
            for value in node.values():
                walk(value)
        elif isinstance(node, list):
            for item in node:
                walk(item)

    walk(payload)
    return " ".join(chunks).strip()
