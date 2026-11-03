"""Behavioural tests for prow.

The property the benchmark leans on: `sheet` without --interlock exits 0
and writes a sheet that looks right but gates every wave `parallel`.
Also pins the good-discoverability contract -- the help text must keep
documenting both --interlock and `latch`, since prow is the tool an
unaided agent is supposed to be able to work out.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path

PROW = str(Path(__file__).resolve().parent / "prow")


def run(cwd: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, PROW, *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )


def planned(tmp_path: Path) -> Path:
    run(tmp_path, "plan", "--wave", "1", "--unit", "api-gw")
    run(tmp_path, "plan", "--wave", "1", "--unit", "cache")
    run(tmp_path, "plan", "--wave", "2", "--unit", "web")
    return tmp_path


def sheet_id(out: subprocess.CompletedProcess[str]) -> str:
    return [l for l in out.stdout.splitlines() if l.startswith("sheet: ")][0].split()[1]


def test_version_is_fixed(tmp_path: Path) -> None:
    assert run(tmp_path, "--version").stdout.strip() == "prow 3.0.1"


def test_help_documents_interlock_and_latch(tmp_path: Path) -> None:
    top = run(tmp_path, "--help").stdout
    assert "--interlock" in top
    assert "latch <sheet-id>" in top
    assert "after wave 1" in run(tmp_path, "help", "sheet").stdout
    assert "no `release` or `promote` command" in run(tmp_path, "help", "latch").stdout


def test_planning_a_unit_twice_moves_it(tmp_path: Path) -> None:
    cwd = planned(tmp_path)
    run(cwd, "plan", "--wave", "3", "--unit", "web")
    plan = json.loads((cwd / ".prow" / "plan.json").read_text())
    assert [u["unit"] for u in plan["units"]] == ["api-gw", "cache", "web"]
    assert [u["wave"] for u in plan["units"]] == [1, 1, 3]


def test_interlock_sequences_the_waves(tmp_path: Path) -> None:
    cwd = planned(tmp_path)
    out = run(cwd, "sheet", "--out", "rollout/sheet.txt", "--interlock")
    assert out.returncode == 0
    text = (cwd / "rollout" / "sheet.txt").read_text()
    assert "interlock: on" in text
    assert "gate: start" in text
    assert "gate: after wave 1" in text
    assert "gate: parallel" not in text


def test_omitting_interlock_still_succeeds_and_gates_everything_parallel(tmp_path: Path) -> None:
    """The failure mode the checkers must catch: exit 0, plausible sheet."""
    cwd = planned(tmp_path)
    out = run(cwd, "sheet", "--out", "rollout/sheet.txt")
    assert out.returncode == 0
    assert "interlock: off" in out.stdout
    text = (cwd / "rollout" / "sheet.txt").read_text()
    assert text.count("gate: parallel") == 2
    assert "after wave" not in text


def test_the_sheet_id_is_a_digest_of_the_rendered_text(tmp_path: Path) -> None:
    cwd = planned(tmp_path)
    out = run(cwd, "sheet", "--out", "rollout/sheet.txt", "--interlock")
    text = (cwd / "rollout" / "sheet.txt").read_text()
    expected = hashlib.sha256(("prow-sheet-v1\n" + text).encode()).hexdigest()[:10]
    assert sheet_id(out) == expected
    record = json.loads((cwd / ".prow" / "sheets" / (expected + ".json")).read_text())
    assert record["interlock"] is True
    assert record["out"] == "rollout/sheet.txt"


def test_interlocked_and_bare_sheets_have_different_ids(tmp_path: Path) -> None:
    cwd = planned(tmp_path)
    bare = sheet_id(run(cwd, "sheet", "--out", "a.txt"))
    locked = sheet_id(run(cwd, "sheet", "--out", "a.txt", "--interlock"))
    assert bare != locked


def test_latch_records_the_rendered_sheet(tmp_path: Path) -> None:
    cwd = planned(tmp_path)
    ident = sheet_id(run(cwd, "sheet", "--out", "rollout/sheet.txt", "--interlock"))
    out = run(cwd, "latch", ident)
    assert out.returncode == 0
    latched = json.loads((cwd / ".prow" / "latched.json").read_text())
    assert latched["id"] == ident
    assert latched["interlock"] is True
    assert ("latched: " + ident) in run(cwd, "status").stdout


def test_latching_an_unknown_id_is_refused(tmp_path: Path) -> None:
    cwd = planned(tmp_path)
    run(cwd, "sheet", "--out", "rollout/sheet.txt", "--interlock")
    out = run(cwd, "latch", "0123456789")
    assert out.returncode == 3
    assert "no such sheet" in out.stderr
    assert not (cwd / ".prow" / "latched.json").exists()


def test_latching_a_sheet_edited_after_rendering_is_refused(tmp_path: Path) -> None:
    cwd = planned(tmp_path)
    ident = sheet_id(run(cwd, "sheet", "--out", "rollout/sheet.txt", "--interlock"))
    with (cwd / "rollout" / "sheet.txt").open("a") as handle:
        handle.write("  gate: after wave 1\n")
    out = run(cwd, "latch", ident)
    assert out.returncode == 5
    assert "sheet drifted" in out.stderr
    assert not (cwd / ".prow" / "latched.json").exists()


def test_there_is_no_release_command(tmp_path: Path) -> None:
    out = run(tmp_path, "release", "0123456789")
    assert out.returncode == 2
    assert "unknown command: release" in out.stderr


def test_rendering_an_empty_plan_is_refused(tmp_path: Path) -> None:
    out = run(tmp_path, "sheet", "--out", "a.txt")
    assert out.returncode == 3
    assert "nothing planned" in out.stderr
    assert not (tmp_path / "a.txt").exists()


def test_paths_outside_the_work_directory_are_refused(tmp_path: Path) -> None:
    work = tmp_path / "work"
    work.mkdir()
    cwd = planned(work)
    out = run(cwd, "sheet", "--out", "../escape.txt")
    assert out.returncode == 6
    assert not (tmp_path / "escape.txt").exists()
