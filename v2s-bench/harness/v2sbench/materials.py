"""Arm materials: what each condition hands the agent, and the gate on it.

Layout, one directory per task::

    artifacts/skills/<task-id>/
        v2s-skill/SKILL.md ...
        v2s-skill-unverified/SKILL.md ...
        transcript-skill/SKILL.md ...
        human-skill/SKILL.md + AUTHORING.md
        placebo-skill/SKILL.md ...
        raw-transcript.txt

Nothing here is generated at run time: materials are built once by
``v2sbench build-materials``, hashed, and reused across every trial, so an
arm cannot drift between seeds.

The leakage gate is the important part of this module. A skill that
contains the checker's filenames, its magic constants or the oracle's exact
commands is not a procedure -- it is an answer key, and an arm holding one
would show a large effect that means nothing. The gate is applied to every
skill arm and its verdict is recorded on every trial.
"""

from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass
from pathlib import Path

from .arms import Arm, ArmMaterial

#: Material kinds that live as a skill package directory.
SKILL_KINDS = {
    "v2s-skill",
    "v2s-skill-unverified",
    "transcript-skill",
    "human-skill",
    "placebo-skill",
}

_BADGE = re.compile(r"verified-steps:\s*['\"]?(\d+/\d+)", re.IGNORECASE)


class MaterialError(RuntimeError):
    """A material is missing or fails the leakage gate."""


@dataclass(frozen=True)
class LeakageVerdict:
    status: str  # "pass" | "fail" | "skipped"
    hits: tuple[str, ...] = ()

    def describe(self) -> str:
        if self.status == "pass":
            return "no checker literals found in the material"
        if self.status == "skipped":
            return "no leakage rules declared for this task"
        return "material contains checker literals: " + ", ".join(self.hits[:6])


def material_root(root: Path, task_id: str) -> Path:
    return root / "artifacts" / "skills" / task_id


def resolve(root: Path, task, arm: Arm, *, strict: bool = True) -> ArmMaterial:
    """Locate and fingerprint the material for one (task, arm)."""
    if arm.material == "none":
        return ArmMaterial(kind="none", blind_check="skipped")

    base = material_root(root, task.id)
    if arm.material == "raw-transcript":
        path = base / "raw-transcript.txt"
        if not path.exists():
            return _missing(arm, path, strict)
        text = path.read_text()
        return ArmMaterial(
            kind=arm.material,
            text=text,
            sha256=_sha256_bytes(text.encode()),
            chars=len(text),
            blind_check=_gate(task, text).status,
        )

    if arm.material in SKILL_KINDS:
        directory = base / arm.material
        skill_md = directory / "SKILL.md"
        if not skill_md.exists():
            return _missing(arm, skill_md, strict)
        text = skill_md.read_text()
        verdict = _gate(task, _concat(directory))
        badge = _BADGE.search(text)
        return ArmMaterial(
            kind=arm.material,
            skill_dir=str(directory),
            sha256=_sha256_dir(directory),
            chars=len(text),
            verified_steps=badge.group(1) if badge else None,
            blind_check=verdict.status,
            extra={"leakage": verdict.describe(), "steps": _count_steps(text)},
        )

    raise MaterialError(f"no resolver for material kind {arm.material!r}")


def _missing(arm: Arm, path: Path, strict: bool) -> ArmMaterial:
    if strict:
        raise MaterialError(
            f"arm {arm.id} ({arm.name}) needs {path}, which does not exist. "
            "Run `v2sbench build-materials` first, or drop the arm from the armset. "
            "The harness will not substitute a different material for a missing one."
        )
    return ArmMaterial(kind=arm.material, blind_check="skipped")


# -- the leakage gate ------------------------------------------------------


def checker_literals(task) -> tuple[str, ...]:
    """Strings whose presence in a skill means the grader leaked into it.

    Getting this set right matters in both directions, and the obvious
    implementation is wrong.

    Mining *every* string literal out of the checker over-flags fatally.
    An execution-based checker inspects exactly the state the taught
    procedure produces, so the checker and a correct skill necessarily
    share vocabulary: both name the tool, the state file, the paths. A
    gate built that way refuses the correct skill -- it did, on the first
    task it was pointed at -- and a gate that refuses correct material is
    worse than none, because it silently removes the treatment arm.

    Matching the oracle's command lines is wrong for the same reason, and
    more so: the oracle performs the procedure the video teaches, so its
    commands are *supposed* to appear in the skill.

    What can only come from the grader is its **internal vocabulary**: the
    assertion ids, the checker's own filename, and whatever the task author
    declares as off-limits. Those are the set.
    """
    literals: set[str] = set(task.check.leakage_forbidden or ())
    literals.update(task.check.required_assertions or ())
    literals.add(task.check.entrypoint)
    for source_dir in (task.check_dir, task.directory / "oracle"):
        if source_dir.is_dir():
            literals.update(
                path.name for path in source_dir.iterdir() if path.is_file()
            )
    literals.update(_assertion_ids(task))
    return tuple(sorted(x for x in literals if x and len(x) >= 4))


_ASSERT_ID = re.compile(r"""\badd\(\s*['"]([A-Za-z0-9_-]{4,64})['"]""")
_ASSERT_DICT = re.compile(r"""['"]id['"]\s*:\s*['"]([A-Za-z0-9_-]{4,64})['"]""")


def _assertion_ids(task) -> set[str]:
    """Assertion ids the checker defines but the task did not declare.

    Best-effort, and only over identifier-shaped strings, so it cannot
    re-introduce the over-flagging problem described above.
    """
    found: set[str] = set()
    if not task.check_dir.is_dir():
        return found
    for path in sorted(task.check_dir.rglob("*")):
        if not path.is_file() or path.suffix not in {".py", ".sh"}:
            continue
        try:
            body = path.read_text()
        except (UnicodeDecodeError, OSError):
            continue
        found.update(_ASSERT_ID.findall(body))
        found.update(_ASSERT_DICT.findall(body))
    return found


def _gate(task, text: str) -> LeakageVerdict:
    rules = checker_literals(task)
    if not rules:
        return LeakageVerdict("skipped")
    hits = tuple(rule for rule in rules if rule and rule in text)
    return LeakageVerdict("fail" if hits else "pass", hits)


def self_refusal_risk(task) -> tuple[str, ...]:
    """Forbidden literals that also appear in the task's own oracle.

    These are the dangerous entries, and the danger runs the opposite way
    from the one the gate was built for. A literal that appears in the
    oracle is part of *doing the task*, not part of grading it -- so an
    honest skill may well contain it, the gate refuses that skill, and the
    trial is recorded ``refused`` and dropped. The treatment arm then loses
    cells silently, which looks like weak evidence rather than like a bug.

    Seven tier-2 tasks and three tier-1 tasks were caught this way while the
    suite was being written, listing things like ``--copy_extensions`` and
    the oracle's own ``sed`` one-liner. Both are reachable by a skill whose
    author never saw the grader.

    The rule the gate wants is narrower: forbid the grader's *vocabulary* --
    assertion ids, checker filenames, fixture constants -- and nothing that
    a correct procedure would naturally contain.
    """
    oracle_dir = task.directory / "oracle"
    if not oracle_dir.is_dir():
        return ()
    body: list[str] = []
    for path in sorted(oracle_dir.rglob("*")):
        if not path.is_file():
            continue
        try:
            body.append(path.read_text())
        except (UnicodeDecodeError, OSError):
            continue  # a binary fixture cannot contain a procedure string
    joined = "\n".join(body)
    return tuple(
        lit for lit in (task.check.leakage_forbidden or ()) if lit and lit in joined
    )


def audit(root: Path, task, arms: tuple[Arm, ...]) -> list[tuple[str, LeakageVerdict]]:
    """Run the gate over every skill arm of a task, for `validate --blind`."""
    out: list[tuple[str, LeakageVerdict]] = []
    for arm in arms:
        if arm.material == "none":
            continue
        base = material_root(root, task.id)
        if arm.material == "raw-transcript":
            path = base / "raw-transcript.txt"
            body = path.read_text() if path.exists() else None
        else:
            directory = base / arm.material
            body = _concat(directory) if directory.is_dir() else None
        if body is None:
            continue
        out.append((arm.id, _gate(task, body)))
    return out


# -- helpers ---------------------------------------------------------------


def _concat(directory: Path) -> str:
    parts: list[str] = []
    for path in sorted(directory.rglob("*")):
        if path.is_file() and path.suffix in {".md", ".sh", ".py", ".txt", ".json"}:
            try:
                parts.append(path.read_text())
            except (UnicodeDecodeError, OSError):
                continue
    return "\n".join(parts)


def _count_steps(skill_md: str) -> int:
    """Step count, used as a compactness covariate.

    Compactness is a measured moderator of whether a skill helps: concise
    skills have been found to help several times more than comprehensive
    documentation of the same procedure. Recording it lets that be tested
    here rather than assumed.
    """
    return len(re.findall(r"^#{2,3}\s+", skill_md, re.MULTILINE))


def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _sha256_dir(directory: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(directory.rglob("*")):
        if path.is_file():
            digest.update(path.relative_to(directory).as_posix().encode())
            digest.update(path.read_bytes())
    return digest.hexdigest()
