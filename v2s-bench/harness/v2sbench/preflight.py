"""Check that a run will work, before it costs anything.

This exists for the case where someone gets one attempt. A benchmark run
fails late and expensively: a missing image is discovered on trial 1, a
missing material on trial 2, an unreadable tool on trial 40, and an
alias-pinned model not until someone reads the records a week later. Every
one of those is cheap to detect up front.

Nothing here calls a model or starts a trial. It reports, and the report is
the thing: a run that preflights clean is not guaranteed to succeed, but
every failure mode we know how to name has been ruled out.
"""

from __future__ import annotations

import shutil
import subprocess
from collections.abc import Iterable
from dataclasses import dataclass, field
from pathlib import Path

from . import arms as arms_mod
from . import materials as materials_mod

OK, WARN, FAIL, SKIP = "ok", "warn", "FAIL", "skip"

#: Mid-range per-trial cost from docs/cost.md, used only to show a figure
#: before spending. Deliberately the midpoint, not the optimistic end.
USD_PER_TRIAL = 0.45


@dataclass
class Check:
    name: str
    status: str
    detail: str
    fix: str = ""


@dataclass
class Report:
    checks: list[Check] = field(default_factory=list)
    trials: int = 0
    usd_estimate: float = 0.0

    def add(self, name: str, status: str, detail: str, fix: str = "") -> None:
        self.checks.append(Check(name, status, detail, fix))

    @property
    def failures(self) -> list[Check]:
        return [c for c in self.checks if c.status == FAIL]

    @property
    def warnings(self) -> list[Check]:
        return [c for c in self.checks if c.status == WARN]

    def ready(self) -> bool:
        return not self.failures


def run_preflight(
    root: Path,
    tasks: Iterable,
    *,
    armset: tuple[str, ...],
    seeds: int,
    model: str | None,
    backend: str,
    adapter: str,
) -> Report:
    report = Report()
    tasks = list(tasks)

    _check_sandbox(report, backend)
    _check_images(report, tasks, backend)
    _check_agent(report, adapter, model)
    _check_extractor(report)
    _check_materials(report, root, tasks, armset)
    _check_leakage(report, tasks)
    _check_disk(report)
    _estimate(report, tasks, armset, seeds, adapter)
    return report


# -- individual checks -----------------------------------------------------


def _check_sandbox(report: Report, backend: str) -> None:
    if backend == "local":
        report.add(
            "sandbox",
            WARN,
            "the local backend has no isolation and no network restriction",
            "results from it are recorded as `local` and are not valid for a "
            "published figure; use --sandbox docker for a real run",
        )
        return
    if backend != "docker":
        report.add("sandbox", FAIL, f"backend {backend!r} is not implemented")
        return
    if shutil.which("docker") is None:
        report.add("sandbox", FAIL, "docker is not on PATH", "install Docker")
        return
    probe = subprocess.run(["docker", "info"], capture_output=True, text=True)
    if probe.returncode != 0:
        report.add(
            "sandbox",
            FAIL,
            "the docker CLI is present but no daemon is reachable",
            "start Docker; a run cannot proceed without it",
        )
        return
    report.add("sandbox", OK, "docker daemon reachable")


def _check_images(report: Report, tasks: list, backend: str) -> None:
    wanted = sorted({t.environment.image for t in tasks})
    if backend != "docker" or shutil.which("docker") is None:
        report.add("images", SKIP, f"{len(wanted)} image(s) declared; not checked")
        return
    missing: list[str] = []
    for image in wanted:
        probe = subprocess.run(
            ["docker", "image", "inspect", image], capture_output=True, text=True
        )
        if probe.returncode != 0:
            missing.append(image)
    if missing:
        report.add(
            "images",
            FAIL,
            f"{len(missing)} of {len(wanted)} declared image(s) not present: "
            + ", ".join(missing),
            "build them from env/<tier>/Dockerfile, or `docker pull` the public ones. "
            "See docs/running-a-pilot.md",
        )
        return
    report.add("images", OK, f"all {len(wanted)} declared image(s) present locally")
    _check_images_nonroot(report, wanted)


def _check_images_nonroot(report: Report, images: list[str]) -> None:
    """The discoverability stratum depends on the agent not being root.

    Six tier-1 tasks hide their tool's source with `chmod 0111`, which does
    nothing against root. An image running the agent as root silently
    converts those tasks from `poor` to `good` discoverability, and the
    stratum stops measuring what it claims to.
    """
    rooted: list[str] = []
    for image in images:
        probe = subprocess.run(
            ["docker", "run", "--rm", "--entrypoint", "id", image, "-u"],
            capture_output=True,
            text=True,
            timeout=120,
        )
        if probe.returncode == 0 and probe.stdout.strip() == "0":
            rooted.append(image)
    if rooted:
        report.add(
            "images run as non-root",
            FAIL,
            "the agent would run as root in: " + ", ".join(rooted),
            "root can read a tool that `chmod 0111` was meant to hide, which "
            "turns every `discoverability: poor` task into a `good` one. Add a "
            "USER line to the image",
        )
        return
    report.add("images run as non-root", OK, "no image runs the agent as root")


def _check_agent(report: Report, adapter: str, model: str | None) -> None:
    if adapter == "stub":
        report.add(
            "agent",
            WARN,
            "the stub agent calls no model",
            "its records are stamped mock and can never become a published "
            "figure; use --agent claude-code for a real run",
        )
        return
    if shutil.which("claude") is None:
        report.add("agent", FAIL, "`claude` is not on PATH")
        return
    version = subprocess.run(
        ["claude", "--version"], capture_output=True, text=True
    ).stdout.strip()
    if not model:
        report.add("agent", FAIL, "no --model given", "pass a full model id")
        return
    if model.lower() in {"opus", "sonnet", "haiku", "fable"}:
        report.add(
            "model id",
            FAIL,
            f"{model!r} is an alias",
            "an alias resolves to a different model over time, which makes the "
            "recorded figure uninterpretable later. Pass a full model id",
        )
        return
    report.add("agent", OK, f"{version or 'claude'} · model {model}")


def _check_extractor(report: Report) -> None:
    """Report which build of v2s is present -- it is the system under test."""
    from .pipeline import extractor_provenance, find_extractor

    binary = find_extractor()
    if not binary:
        report.add(
            "vts-extract",
            WARN,
            "not found; only needed to BUILD materials, not to run trials",
            "build the current source (`cargo build --release` from the "
            "video-to-skill root), not a tagged release -- the extractor is "
            "what is being graded",
        )
        return
    prov = extractor_provenance(binary)
    detail = f"{prov.get('version', '?')}"
    if prov.get("commit"):
        detail += f" @ {prov['commit']}"
    if prov.get("dirty") == "yes":
        report.add(
            "vts-extract",
            WARN,
            f"{detail} (working tree DIRTY)",
            "a dirty build is not reproducible from any commit; commit first "
            "so the result can be attributed to a specific version of v2s",
        )
        return
    report.add("vts-extract", OK, detail)


def _check_materials(
    report: Report, root: Path, tasks: list, armset: tuple[str, ...]
) -> None:
    needed = [arms_mod.resolve(a) for a in armset if arms_mod.resolve(a).requires_artifact]
    if not needed:
        report.add("materials", OK, "this armset needs no built material (baseline only)")
        return

    missing: list[str] = []
    for task in tasks:
        for arm in needed:
            try:
                materials_mod.resolve(root, task, arm, strict=True)
            except materials_mod.MaterialError:
                missing.append(f"{task.id}/{arm.id}")

    total = len(tasks) * len(needed)
    if missing:
        shown = ", ".join(missing[:4])
        more = f" (and {len(missing) - 4} more)" if len(missing) > 4 else ""
        report.add(
            "materials",
            FAIL,
            f"{len(missing)} of {total} arm material(s) missing: {shown}{more}",
            "run `v2sbench build-materials`. Without them the treatment arms "
            "have nothing to inject and the run measures nothing",
        )
        return
    report.add("materials", OK, f"all {total} arm material(s) present")


def _check_leakage(report: Report, tasks: list) -> None:
    risky = [t.id for t in tasks if materials_mod.self_refusal_risk(t)]
    if risky:
        report.add(
            "leakage rules",
            FAIL,
            f"{len(risky)} task(s) forbid their own procedure: " + ", ".join(risky[:4]),
            "an honest skill would be refused and the trial dropped, silently "
            "costing the treatment arm cells. Run `validate --blind`",
        )
        return
    report.add("leakage rules", OK, f"{len(tasks)} task(s) have safe leakage rules")


def _check_disk(report: Report) -> None:
    usage = shutil.disk_usage("/")
    free_gb = usage.free / 1e9
    if free_gb < 5:
        report.add(
            "disk", FAIL, f"{free_gb:.1f}GB free", "images and traces need room"
        )
    elif free_gb < 20:
        report.add("disk", WARN, f"{free_gb:.1f}GB free; images are several GB")
    else:
        report.add("disk", OK, f"{free_gb:.1f}GB free")


def _estimate(
    report: Report, tasks: list, armset: tuple[str, ...], seeds: int, adapter: str
) -> None:
    report.trials = len(tasks) * len(armset) * seeds
    if adapter == "stub":
        report.usd_estimate = 0.0
        report.add("cost", OK, f"{report.trials} trials, $0 (stub agent)")
        return
    report.usd_estimate = report.trials * USD_PER_TRIAL
    report.add(
        "cost",
        OK,
        f"{report.trials} trials, roughly ${report.usd_estimate:.0f} "
        f"at ${USD_PER_TRIAL:.2f}/trial",
        "a midpoint estimate from docs/cost.md, not a quote. Cap it with --max-usd",
    )


def render(report: Report) -> str:
    """Format the report. Failures last, so they are what you read."""
    width = max((len(c.name) for c in report.checks), default=10)
    lines: list[str] = []
    for check in report.checks:
        mark = {OK: " ok ", WARN: "warn", FAIL: "FAIL", SKIP: "skip"}[check.status]
        lines.append(f"  [{mark}] {check.name:<{width}}  {check.detail}")
        if check.fix and check.status in {FAIL, WARN}:
            lines.append(f"         {'':<{width}}  -> {check.fix}")
    lines.append("")
    if report.failures:
        lines.append(
            f"NOT READY — {len(report.failures)} blocking problem(s). "
            "Nothing was spent."
        )
    else:
        warned = f", {len(report.warnings)} warning(s)" if report.warnings else ""
        lines.append(f"READY — {report.trials} trials{warned}.")
    return "\n".join(lines)
