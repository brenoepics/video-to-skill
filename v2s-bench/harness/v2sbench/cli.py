"""Command-line entry point.

    v2sbench doctor            what this machine can actually run
    v2sbench tasks             list the suite
    v2sbench validate          schemas, consistency, leakage, oracle, trivial-agent
    v2sbench render            render the hermetic videos from recipes
    v2sbench probe             measure per-task headroom (what the model knows cold)
    v2sbench run               execute trials
    v2sbench report            build the report bundle the site renders
    v2sbench verify-results    recompute every published figure from raw records
    v2sbench power             sample size / minimum detectable effect
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from . import __version__
from . import arms as arms_mod
from . import record as record_mod
from . import tasks as tasks_mod


def repo_root(start: Path | None = None) -> Path:
    """Find the v2s-bench root by looking for its schemas directory."""
    here = (start or Path(__file__)).resolve()
    for candidate in [here, *here.parents]:
        if (candidate / "schemas" / "task.schema.json").exists():
            return candidate
    return Path.cwd()


def _load(args) -> tuple[Path, list]:
    root = Path(args.root).resolve() if args.root else repo_root()
    tiers = tuple(args.tier) if getattr(args, "tier", None) else tasks_mod.TIERS
    strict = not getattr(args, "no_validate", False)
    suite = tasks_mod.load_suite(root, tiers=tiers, validate=strict)
    if getattr(args, "task", None):
        wanted = set(args.task)
        suite = [t for t in suite if t.id in wanted]
        missing = wanted - {t.id for t in suite}
        if missing:
            raise SystemExit(f"unknown task id(s): {', '.join(sorted(missing))}")
    return root, suite


# -- commands --------------------------------------------------------------


def cmd_doctor(args) -> int:
    from .sandbox import available_backends

    root = Path(args.root).resolve() if args.root else repo_root()
    print(f"v2sbench {__version__}")
    print(f"root            {root}")
    print(f"harness digest  {record_mod.harness_digest()}")
    print("sandbox backends:")
    for name, state in available_backends().items():
        print(f"  {name:12} {state}")
    try:
        suite = tasks_mod.load_suite(root, validate=False)
        print(f"tasks           {len(suite)} loaded")
    except (ValueError, OSError) as exc:
        print(f"tasks           FAILED: {exc}")
    version = tasks_mod.suite_version(root) if (root / "tasks").exists() else "n/a"
    print(f"suite version   {version}")
    return 0


def cmd_tasks(args) -> int:
    root, suite = _load(args)
    if args.json:
        print(json.dumps([_task_row(t) for t in suite], indent=2))
        return 0
    if not suite:
        print("no tasks found. The suite lives under tasks/<tier>/<id>/.")
        return 0
    width = max(len(t.id) for t in suite)
    print(f"{'id':<{width}}  {'tier':<12} {'disc':<5} {'fb':<5} {'video':<9} title")
    for t in suite:
        print(
            f"{t.id:<{width}}  {t.tier:<12} {t.discoverability:<5} "
            f"{t.feedback_bandwidth:<5} {t.video.tier:<9} {t.title}"
        )
    print(f"\n{len(suite)} tasks · suite version {tasks_mod.suite_version(root)}")
    return 0


def _task_row(t) -> dict:
    return {
        "id": t.id, "tier": t.tier, "title": t.title, "goal": t.goal,
        "knowledge_gap": t.knowledge_gap.summary,
        "discoverability": t.discoverability,
        "feedback_bandwidth": t.feedback_bandwidth,
        "saturated": t.saturated,
        "video_tier": t.video.tier, "video_ref": t.video.ref,
        "license": t.license, "attribution": t.attribution,
    }


def cmd_validate(args) -> int:
    from . import materials
    from .runner import RunConfig, oracle_gate, trivial_agent_gate

    root, suite = _load(args)
    failures: list[str] = []
    recorded: dict[str, dict] = {}
    print(f"loaded {len(suite)} tasks (schema + consistency: ok)")

    if args.blind:
        print("\nleakage rules (a rule must not forbid the task's own procedure):")
        risky = 0
        for task in suite:
            overlap = materials.self_refusal_risk(task)
            if overlap:
                risky += 1
                failures.append(f"{task.id}: leakage rule matches its own oracle")
                print(
                    f"  FAIL {task.id}: {list(overlap)} appear(s) in this task's own "
                    "oracle, so an honest skill would be refused and the trial dropped"
                )
        print(f"  {len(suite) - risky}/{len(suite)} task(s) have safe leakage rules")

        print("\nleakage gate (materials must not contain checker literals):")
        checked = 0
        for task in suite:
            for arm_id, verdict in materials.audit(root, task, arms_mod.ALL_ARMS):
                checked += 1
                if verdict.status == "fail":
                    failures.append(f"{task.id}/{arm_id}: {verdict.describe()}")
                    print(f"  FAIL {task.id}/{arm_id}: {verdict.describe()}")
        print(f"  {checked} materials checked")
        if not checked:
            print("  (no materials built yet — run `v2sbench build-materials`)")
        leaked = sum(1 for f in failures if "material" in f or "/A" in f)
        recorded["leakage"] = {
            "status": "skipped" if not checked else ("fail" if leaked else "pass"),
            "detail": f"{checked} materials checked",
            "passed": checked - leaked,
            "total": checked,
        }

    if args.oracle or args.trivial:
        config = RunConfig(
            root=root, model="none", backend=args.sandbox,
            allow_unsandboxed=args.allow_unsandboxed,
        )
        version = tasks_mod.suite_version(root)
        if args.oracle:
            print("\noracle gate (every task's own solution must pass):")
            broken, errored = oracle_gate(config, suite, suite_version=version)
            failures += [f"oracle failed: {t}" for t in broken]
            failures += [f"oracle could not run: {t}" for t, _ in errored]
            passed = len(suite) - len(broken) - len(errored)
            print(f"  {passed}/{len(suite)} oracles passed")
            for t in broken:
                print(f"  FAIL  {t} — the oracle ran and did not satisfy the checker")
            for t, detail in errored:
                print(f"  ERROR {t} — the environment did not come up: {detail}")
            recorded["oracle"] = {
                "status": "pass" if not broken and not errored else "fail",
                "detail": f"{passed}/{len(suite)} oracles satisfied their checker",
                "passed": passed,
                "total": len(suite),
            }
        if args.trivial:
            print("\ntrivial-agent gate (a do-nothing agent must score zero):")
            passed = trivial_agent_gate(config, suite, suite_version=version)
            failures += [f"trivial agent passed: {t}" for t in passed]
            print(f"  {len(passed)} task(s) passed by a null agent")
            for t in passed:
                print(f"  FAIL {t} — this checker can be satisfied without doing the task")
            recorded["trivial-agent"] = {
                "status": "pass" if not passed else "fail",
                "detail": f"{len(passed)} of {len(suite)} checkers satisfiable by a null agent",
                "passed": len(suite) - len(passed),
                "total": len(suite),
            }

    if recorded:
        _record_gates(root, recorded)

    print()
    if failures:
        print(f"{len(failures)} integrity failure(s)")
        return 1
    print("all requested gates passed")
    return 0


def _record_gates(root: Path, recorded: dict[str, dict]) -> None:
    """Persist gate outcomes so the results page can show them.

    The pre-registration promises the gates are published beside the
    results, and they are what separates a working suite from a defective
    one. Without this they lived only in CI logs.
    """
    path = root / "results" / "gates.json"
    existing = json.loads(path.read_text()) if path.exists() else {}
    existing.update(recorded)
    existing["_recorded_at"] = record_mod.utcnow()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(existing, indent=2, sort_keys=True) + "\n")
    print(f"\nrecorded gate status to {path}")


def cmd_run(args) -> int:
    from .agent.factory import create as create_agent
    from .runner import RunConfig, run

    root, suite = _load(args)
    if not suite:
        raise SystemExit("no tasks to run")
    adapter = create_agent(args.agent, model=args.model)
    config = RunConfig(
        root=root,
        model=args.model or "none",
        armset=arms_mod.armset(args.arms),
        seeds=tuple(range(args.seeds)) if args.seeds else (0,),
        backend=args.sandbox,
        allow_unsandboxed=args.allow_unsandboxed,
        max_usd=args.max_usd,
        stop_on_error=args.stop_on_error,
        strict_materials=not args.lenient_materials,
    )
    planned = len(suite) * len(config.armset) * len(config.seeds)
    print(
        f"run {config.run_id}: {len(suite)} tasks x {len(config.armset)} arms "
        f"x {len(config.seeds)} seeds = {planned} trials"
    )
    print(f"  agent {adapter.name} · model {config.model} · sandbox {config.backend}")
    if args.dry_run:
        from .runner import schedule

        for task, arm, seed in schedule(suite, config.armset, config.seeds):
            print(f"  {task.id:<28} {arm.id} seed={seed}")
        return 0

    def progress(outcome) -> None:
        rec = outcome.record
        mark = {"ok": "ok", "error": "ER", "refused": "RF"}.get(outcome.status, outcome.status)
        success = rec["outcome"]["success"]
        print(
            f"  [{mark}] {rec['task']['id']:<28} {rec['arm']} seed={rec['seed']} "
            f"success={success}"
        )

    summary = run(
        config, suite, adapter,
        suite_version=tasks_mod.suite_version(root), on_trial=progress,
    )
    print("\n" + summary.line())
    print(f"records: {summary.path}")
    return 0


def cmd_report(args) -> int:
    from .report.build import build_report

    root = Path(args.root).resolve() if args.root else repo_root()
    bundle = build_report(root, results_dir=Path(args.results) if args.results else None)
    out = Path(args.out) if args.out else root / "results" / "report.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(bundle, indent=2) + "\n")
    status = bundle["status"]
    print(f"{status['kind']}: {status['headline']}")
    print(f"wrote {out}")
    return 0


def cmd_verify_results(args) -> int:
    from .report.build import verify

    root = Path(args.root).resolve() if args.root else repo_root()
    problems = verify(root, Path(args.report) if args.report else root / "results" / "report.json")
    if problems:
        for problem in problems:
            print(f"MISMATCH {problem}")
        return 1
    print("every figure in the report recomputes from the raw records")
    return 0


def cmd_power(args) -> int:
    from .stats.power import power_table

    grids = [(n, k) for n in args.tasks for k in args.seeds]
    rows = power_table(
        grids=grids,
        base_rate=args.base_rate,
        icc=args.icc,
        rho=args.rho,
        power=args.power,
        alpha=args.alpha,
        resamples=args.resamples,
        trials=args.trials,
        seed=args.seed,
    )
    by_cell = {(r.n_tasks, r.n_seeds): r for r in rows}
    print(f"| N tasks | {' | '.join(f'K={k}' for k in args.seeds)} |")
    print(f"|---|{'---|' * len(args.seeds)}")
    for n in args.tasks:
        cells = [f"{by_cell[(n, k)].mde * 100:.1f}" for k in args.seeds]
        print(f"| {n} | {' | '.join(cells)} |")
    print(
        f"\nMinimum detectable effect, percentage points. "
        f"alpha={args.alpha} power={args.power} base={args.base_rate} "
        f"ICC={args.icc} rho={args.rho}"
    )
    print(
        "Simulated, not derived: lower --resamples/--trials run faster and "
        "wobble more. Every cell uses identical settings by construction."
    )
    sample = rows[0]
    print(
        f"Effective trials per task at K={sample.n_seeds}, ICC={sample.icc}: "
        f"{sample.effective_trials:.2f} "
        "(1/(1+(K-1)*ICC); correlated trials are worth less than their count)"
    )
    return 0


def cmd_probe(args) -> int:
    from .probe import probe_task, saturated, write_probes

    root, suite = _load(args)
    if not args.model:
        raise SystemExit("--model is required: a probe score belongs to one model")
    results = []
    for task in suite:
        result = probe_task(task, model=args.model, repeats=args.repeats)
        results.append(result)
        flag = " SATURATED" if saturated(result, task) else ""
        print(
            f"  {task.id:<30} known={result.known:.2f} band={result.band():<6}"
            f" ({result.correct}/{result.repeats - result.errors}){flag}"
        )
    path = write_probes(root, results, model=args.model)
    print(f"\nwrote {path}")
    print(
        "Exclusion needs BOTH a high probe score and high feedback bandwidth: a task "
        "the model can recite but whose environment gives no corrective signal is the "
        "knowing-doing case this benchmark most wants to keep."
    )
    return 0


def cmd_build_materials(args) -> int:
    from .pipeline import plan, preflight

    root, suite = _load(args)
    problems = preflight(args.extractor)
    if problems and not args.dry_run:
        print("cannot build materials:")
        for problem in problems:
            print(f"  - {problem}")
        print("\nRe-run with --dry-run to see the plan without these preconditions.")
        return 1
    for task in suite:
        video = root / ".cache" / "videos" / f"{task.video.ref}.mp4"
        stages = plan(
            task, root=root, video=video,
            extractor=args.extractor, model=args.model or "",
        )
        print(stages.describe())
        print()
    if args.dry_run:
        print("dry run: nothing executed.")
        if problems:
            print("outstanding preconditions:")
            for problem in problems:
                print(f"  - {problem}")
    return 0


def cmd_render(args) -> int:
    from .render import render

    root = Path(args.root).resolve() if args.root else repo_root()
    out_dir = Path(args.out) if args.out else root / ".cache" / "videos"
    out_dir.mkdir(parents=True, exist_ok=True)
    recipes = sorted((root / "videos" / "recipes").glob("*.json"))
    if args.only:
        recipes = [p for p in recipes if p.stem in set(args.only)]
    if not recipes:
        print("no recipes found under videos/recipes/")
        return 1
    for path in recipes:
        result = render(path, out_dir / f"{path.stem}.mp4")
        print(f"{path.stem}: {result}")
    return 0


# -- parser ----------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="v2sbench", description=__doc__.splitlines()[0])
    parser.add_argument("--version", action="version", version=f"v2sbench {__version__}")
    parser.add_argument("--root", help="v2s-bench root (auto-detected)")
    sub = parser.add_subparsers(dest="command", required=True)

    def common(p, *, selection: bool = True) -> None:
        if selection:
            p.add_argument("--tier", action="append", choices=list(tasks_mod.TIERS))
            p.add_argument("--task", action="append")
            p.add_argument("--no-validate", action="store_true")

    def sandboxed(p) -> None:
        p.add_argument("--sandbox", default="docker", choices=["docker", "local", "bubblewrap"])
        p.add_argument(
            "--allow-unsandboxed", action="store_true",
            help="permit the local backend: no isolation, not valid for a published figure",
        )

    p = sub.add_parser("doctor", help="what this machine can run")
    p.set_defaults(func=cmd_doctor)

    p = sub.add_parser("tasks", help="list the suite")
    common(p)
    p.add_argument("--json", action="store_true")
    p.set_defaults(func=cmd_tasks)

    p = sub.add_parser("validate", help="integrity gates")
    common(p)
    sandboxed(p)
    p.add_argument("--blind", action="store_true", help="leakage gate over built materials")
    p.add_argument("--oracle", action="store_true", help="every task's oracle must pass")
    p.add_argument("--trivial", action="store_true", help="a null agent must score zero")
    p.add_argument("--all", dest="all_gates", action="store_true")
    p.set_defaults(func=cmd_validate)

    p = sub.add_parser("run", help="execute trials")
    common(p)
    sandboxed(p)
    p.add_argument("--agent", default="stub", choices=["stub", "claude-code"])
    p.add_argument("--model", help="full model id; aliases are refused")
    p.add_argument("--arms", default="core", help="armset name or comma list, e.g. A0,A1")
    p.add_argument("--seeds", type=int, default=3)
    p.add_argument("--max-usd", type=float, help="stop the run when estimated cost reaches this")
    p.add_argument("--stop-on-error", action="store_true")
    p.add_argument("--lenient-materials", action="store_true",
                   help="skip arms whose material is missing instead of failing")
    p.add_argument("--dry-run", action="store_true", help="print the schedule and stop")
    p.set_defaults(func=cmd_run)

    p = sub.add_parser("report", help="build the report bundle")
    p.add_argument("--results", help="results directory (default results/runs)")
    p.add_argument("--out")
    p.set_defaults(func=cmd_report)

    p = sub.add_parser("verify-results", help="recompute published figures from raw records")
    p.add_argument("--report")
    p.set_defaults(func=cmd_verify_results)

    p = sub.add_parser("power", help="sample size and minimum detectable effect")
    p.add_argument("--tasks", type=int, nargs="+", default=[50, 75, 100, 150])
    p.add_argument("--seeds", type=int, nargs="+", default=[1, 3, 5, 10])
    p.add_argument("--base-rate", type=float, default=0.35)
    p.add_argument("--icc", type=float, default=0.5)
    p.add_argument("--rho", type=float, default=0.6)
    p.add_argument("--alpha", type=float, default=0.05)
    p.add_argument("--power", type=float, default=0.8)
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--resamples", type=int, default=200)
    p.add_argument("--trials", type=int, default=150)
    p.set_defaults(func=cmd_power)

    p = sub.add_parser("probe", help="measure per-task headroom (what the model knows cold)")
    common(p)
    p.add_argument("--model", help="full model id; a probe score belongs to one model")
    p.add_argument("--repeats", type=int, default=10)
    p.set_defaults(func=cmd_probe)

    p = sub.add_parser(
        "build-materials", help="run the v2s pipeline to produce each arm's material"
    )
    common(p)
    p.add_argument("--extractor", default="vts-extract")
    p.add_argument("--model", help="model for the agent-driven analysis stage")
    p.add_argument("--dry-run", action="store_true", help="print the stage plan and stop")
    p.set_defaults(func=cmd_build_materials)

    p = sub.add_parser("render", help="render hermetic videos from recipes")
    p.add_argument("--only", action="append")
    p.add_argument("--out")
    p.set_defaults(func=cmd_render)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if getattr(args, "all_gates", False):
        args.blind = args.oracle = args.trivial = True
    try:
        return int(args.func(args) or 0)
    except KeyboardInterrupt:
        print("\ninterrupted", file=sys.stderr)
        return 130


if __name__ == "__main__":
    raise SystemExit(main())
