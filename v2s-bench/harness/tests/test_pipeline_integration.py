"""End-to-end: a synthetic task through sandbox, agent, checker, records, report.

This is the test that proves the harness is wired together: it builds a
whole miniature benchmark in a temp directory -- a tool, a task, a checker,
an oracle, a skill -- and runs real trials through the real code paths with
the stub agent and the local sandbox.

It also pins the two behaviours the benchmark's integrity rests on:
the checker is absent from the workspace while the agent runs, and mock
records can never be aggregated into a published figure.
"""

from __future__ import annotations

import json
import shutil
from pathlib import Path

import pytest

from v2sbench import arms as arms_mod
from v2sbench import record as record_mod
from v2sbench import tasks as tasks_mod
from v2sbench.agent.stub import StubAgent
from v2sbench.report.build import build_report, verify
from v2sbench.runner import RunConfig, oracle_gate, run, trivial_agent_gate

REAL_ROOT = Path(__file__).resolve().parents[2]

TOOL = """#!/usr/bin/env python3
"A tiny two-phase tool: nothing is recorded until it is sealed with the grant."
import json, pathlib, sys

STATE = pathlib.Path(".widget/state.json")

def load():
    return json.loads(STATE.read_text()) if STATE.exists() else {"grant": None, "sealed": False}

def save(state):
    STATE.parent.mkdir(parents=True, exist_ok=True)
    STATE.write_text(json.dumps(state, indent=1))

def main(argv):
    if not argv or argv[0] in {"-h", "--help"}:
        print("usage: widget open | widget seal --grant <token>")
        return 0
    state = load()
    if argv[0] == "open":
        state["grant"] = "g-7731"
        save(state)
        print("opened; grant written to .widget/state.json")
        return 0
    if argv[0] == "seal":
        if "--grant" not in argv:
            print("sealed without a grant (draft only)")
            state["sealed"] = False
            save(state)
            return 0
        token = argv[argv.index("--grant") + 1]
        if token != state.get("grant"):
            print("grant mismatch", file=sys.stderr)
            return 1
        state["sealed"] = True
        save(state)
        print("sealed")
        return 0
    print("unknown subcommand", file=sys.stderr)
    return 2

if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
"""

CHECK = """#!/usr/bin/env python3
import json, pathlib, sys

workdir = pathlib.Path(sys.argv[1])
state_path = workdir / ".widget" / "state.json"
assertions = []

def add(aid, passed, detail, required=True):
    assertions.append({"id": aid, "passed": bool(passed), "required": required, "detail": detail})

if not state_path.exists():
    add("state-exists", False, "no .widget/state.json")
    add("sealed", False, "no state to inspect")
else:
    state = json.loads(state_path.read_text())
    add("state-exists", True, "state present")
    add("sealed", state.get("sealed") is True, f"sealed={state.get('sealed')!r}")

print(json.dumps({"assertions": assertions}))
"""

ORACLE = """#!/bin/sh
set -e
widget open
widget seal --grant g-7731
"""

SKILL = """---
name: widget-sealing
description: Use when sealing a widget drop so the ledger records it.
metadata:
  video-to-skill/verified-steps: "2/2"
---

# Sealing a widget drop

## Open the drop [t=0:04]

Run `widget open`. This writes a grant token into `.widget/state.json`.

## Seal it with the grant [t=0:21]

Sealing without the grant silently leaves the drop as a draft. Pass it:

```sh
widget seal --grant g-7731
```
"""

TASK = """schema_version: 1
id: widget-seal
tier: t1-novel-cli
title: Seal a widget drop
goal: >
  Using the `widget` tool available on this system, get the widget drop into a
  state the ledger records as sealed. Leave the working directory otherwise
  untouched.
discoverability: poor
feedback_bandwidth: silent
knowledge_gap:
  summary: Sealing requires the grant token from state.json; without it the seal silently no-ops.
  probe_question: What argument does `widget seal` require for the seal to be recorded?
  probe_answer_pattern: "--grant"
  taught_at: [21.0]
environment:
  image: python:3.11-slim
  workdir: /work
  network: none
budget:
  max_turns: 8
  wall_secs: 60
oracle:
  entrypoint: solve.sh
  modality: shell
check:
  entrypoint: check.py
  required_assertions: [state-exists, sealed]
  key_actions: ["widget open", "widget seal .*--grant"]
  forbidden_actions: ["rm -rf /"]
  leakage_forbidden: ["check.py", "state-exists"]
video:
  tier: hermetic
  ref: widget-seal
"""


@pytest.fixture
def bench_root(tmp_path: Path) -> Path:
    root = tmp_path / "bench"
    (root / "schemas").mkdir(parents=True)
    for schema in (REAL_ROOT / "schemas").glob("*.json"):
        shutil.copy(schema, root / "schemas" / schema.name)

    task_dir = root / "tasks" / "t1-novel-cli" / "widget-seal"
    (task_dir / "check").mkdir(parents=True)
    (task_dir / "oracle").mkdir(parents=True)
    (task_dir / "task.yaml").write_text(TASK)
    (task_dir / "check" / "check.py").write_text(CHECK)
    (task_dir / "oracle" / "solve.sh").write_text(ORACLE)

    tool = root / "tools" / "widget" / "widget.py"
    tool.parent.mkdir(parents=True)
    tool.write_text(TOOL)
    tool.chmod(0o755)

    skill = root / "artifacts" / "skills" / "widget-seal" / "v2s-skill"
    skill.mkdir(parents=True)
    (skill / "SKILL.md").write_text(SKILL)
    return root


def _config(root: Path, **kw) -> RunConfig:
    defaults = dict(
        root=root, model="stub-model", backend="local", allow_unsandboxed=True,
        seeds=(0,), armset=("A0", "A1"),
    )
    defaults.update(kw)
    return RunConfig(**defaults)


def test_suite_loads_and_validates(bench_root: Path) -> None:
    suite = tasks_mod.load_suite(bench_root)
    assert [t.id for t in suite] == ["widget-seal"]
    assert suite[0].feedback_bandwidth == "silent"
    assert suite[0].oracle.entrypoint == "solve.sh"


def test_oracle_passes_the_checker(bench_root: Path) -> None:
    """The oracle gate: a task whose own solution fails is a broken task.

    The gate separates the two ways an oracle can not-pass, because they
    point at different bugs: `failed` means the task or its checker is
    wrong, `errored` means the environment did not come up.
    """
    suite = tasks_mod.load_suite(bench_root)
    failed, errored = oracle_gate(_config(bench_root), suite, suite_version="test")
    assert errored == [], f"the environment did not come up: {errored}"
    assert failed == [], "the oracle must be able to pass its own checker"


def test_trivial_agent_scores_zero(bench_root: Path) -> None:
    """A do-nothing agent must not pass. Suites have shipped that could."""
    suite = tasks_mod.load_suite(bench_root)
    assert trivial_agent_gate(_config(bench_root), suite, suite_version="test") == []


def test_paired_run_writes_records_for_every_arm(bench_root: Path) -> None:
    suite = tasks_mod.load_suite(bench_root)
    summary = run(_config(bench_root), suite, StubAgent(), suite_version="test")

    assert summary.written == 2, summary.line()
    assert summary.errors == 0, summary.line()
    records = record_mod.read_all(bench_root / "results" / "runs")
    by_arm = {r["arm"]: r for r in records}
    assert set(by_arm) == {"A0", "A1"}

    # The stub acts only on material it was given, so this asserts the
    # plumbing, not a finding: material reached A1 and did not reach A0.
    assert by_arm["A0"]["outcome"]["success"] is False
    assert by_arm["A1"]["outcome"]["success"] is True
    assert by_arm["A1"]["metrics"]["key_action_hit"] is True
    assert by_arm["A1"]["material"]["verified_steps"] == "2/2"
    assert by_arm["A1"]["material"]["sha256"]
    assert all(r["provenance"]["is_mock"] for r in records)


def test_checker_is_absent_while_the_agent_runs(bench_root: Path) -> None:
    """The single most important isolation property in the harness.

    If the checker were readable from the workdir, an agent could satisfy
    the assertions directly and the benchmark would measure its grader.
    """
    suite = tasks_mod.load_suite(bench_root)
    seen: list[str] = []

    class Peeking(StubAgent):
        def run(self, **kw):  # type: ignore[override]
            box = kw["sandbox"]
            listing = box.exec(["sh", "-lc", f"find {box.workdir} -type f"])
            seen.append(listing.stdout)
            return super().run(**kw)

    run(_config(bench_root), suite, Peeking(), suite_version="test")
    assert seen, "the probing agent never ran"
    for listing in seen:
        assert "check.py" not in listing
        assert "solve.sh" not in listing


def test_leakage_gate_refuses_an_answer_key(bench_root: Path) -> None:
    """A skill naming the checker's internals is refused, not scored."""
    skill = bench_root / "artifacts" / "skills" / "widget-seal" / "v2s-skill" / "SKILL.md"
    skill.write_text(SKILL + "\n<!-- make check.py pass the state-exists assertion -->\n")

    suite = tasks_mod.load_suite(bench_root)
    summary = run(_config(bench_root, armset=("A1",)), suite, StubAgent(), suite_version="test")

    assert summary.refused == 1, summary.line()
    rec = record_mod.read_all(bench_root / "results" / "runs")[0]
    assert rec["outcome"]["status"] == "refused"
    assert rec["material"]["blind_check"] == "fail"


def test_missing_material_is_an_error_not_a_substitution(bench_root: Path) -> None:
    """A2's material does not exist; the harness must not quietly use A1's."""
    suite = tasks_mod.load_suite(bench_root)
    summary = run(_config(bench_root, armset=("A2",)), suite, StubAgent(), suite_version="test")
    assert summary.errors == 1
    rec = record_mod.read_all(bench_root / "results" / "runs")[0]
    assert "build-materials" in rec["outcome"]["error_detail"]


def test_report_marks_stub_results_as_mock_and_verifies(bench_root: Path) -> None:
    suite = tasks_mod.load_suite(bench_root)
    run(_config(bench_root), suite, StubAgent(), suite_version="test")

    bundle = build_report(bench_root, resamples=200)
    assert bundle["status"]["kind"] == "mock"
    assert bundle["provenance"]["is_mock"] is True
    assert "not a measurement" in bundle["status"]["headline"].lower()

    report_path = bench_root / "results" / "report.json"
    report_path.write_text(json.dumps(bundle, indent=2))
    assert verify(bench_root, report_path) == []


def test_report_refuses_to_mix_mock_and_real(bench_root: Path) -> None:
    """The guard against fabricated numbers becoming the headline."""
    suite = tasks_mod.load_suite(bench_root)
    run(_config(bench_root), suite, StubAgent(), suite_version="test")

    shard = next((bench_root / "results" / "runs").glob("*.jsonl"))
    records = [json.loads(line) for line in shard.read_text().splitlines()]
    records[0]["provenance"]["is_mock"] = False
    shard.write_text("\n".join(json.dumps(r) for r in records) + "\n")

    with pytest.raises(ValueError, match="refusing to aggregate"):
        build_report(bench_root)


def test_empty_results_render_as_no_results(bench_root: Path) -> None:
    bundle = build_report(bench_root)
    assert bundle["status"]["kind"] == "no-results"
    assert bundle["arms"] == []
    assert bundle["comparisons"] == []
    assert len(bundle["tasks"]) == 1


def test_arm_order_rotates_so_no_arm_is_always_first(bench_root: Path) -> None:
    from v2sbench.runner import schedule

    suite = tasks_mod.load_suite(bench_root)
    order = [(a.id, s) for _, a, s in schedule(suite, ("A0", "A1"), (0, 1))]
    assert order == [("A0", 0), ("A1", 0), ("A1", 1), ("A0", 1)]


def test_every_record_validates_against_the_schema(bench_root: Path) -> None:
    suite = tasks_mod.load_suite(bench_root)
    run(_config(bench_root), suite, StubAgent(), suite_version="test")
    validator = record_mod.validator(bench_root)
    for rec in record_mod.read_all(bench_root / "results" / "runs"):
        assert list(validator.iter_errors(rec)) == []


def test_comparisons_cover_every_declared_hypothesis() -> None:
    """The report must not silently drop a pre-registered hypothesis."""
    assert [c.hypothesis for c in arms_mod.COMPARISONS] == ["H1", "H3", "H4", "H5", "H6", "H8"]
    for comparison in arms_mod.COMPARISONS:
        arms_mod.resolve(comparison.treatment)
        arms_mod.resolve(comparison.reference)
