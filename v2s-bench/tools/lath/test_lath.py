"""Behavioural tests for lath.

Two properties the benchmark depends on: `set` really does take the
value before the key (and the swapped call silently stages nonsense
rather than erroring), and precedence really is first-stacked-wins.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

LATH = str(Path(__file__).resolve().parent / "lath")


def run(cwd: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, LATH, *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )


def tree(tmp_path: Path) -> Path:
    run(tmp_path, "init")
    return tmp_path


def layer_lines(cwd: Path, name: str) -> list[str]:
    return (cwd / ".lath" / "layers" / (name + ".kv")).read_text().splitlines()


def test_version_is_fixed(tmp_path: Path) -> None:
    assert run(tmp_path, "--version").stdout.strip() == "lath 2.3.0"


def test_commands_refuse_to_run_outside_a_lath_tree(tmp_path: Path) -> None:
    out = run(tmp_path, "set", "base", "8080", "port")
    assert out.returncode == 6
    assert "not a lath tree" in out.stderr


def test_set_stores_the_value_before_the_key(tmp_path: Path) -> None:
    cwd = tree(tmp_path)
    assert run(cwd, "set", "base", "8080", "port").returncode == 0
    assert layer_lines(cwd, "base") == ["port\t8080"]


def test_the_swapped_order_is_accepted_and_stages_the_wrong_thing(tmp_path: Path) -> None:
    """The failure mode the checkers must catch: no error, wrong record."""
    cwd = tree(tmp_path)
    out = run(cwd, "set", "base", "port", "8080")
    assert out.returncode == 0
    assert layer_lines(cwd, "base") == ["8080\tport"]


def test_help_documents_the_argument_order(tmp_path: Path) -> None:
    """lath is the good-discoverability tool; the fact must really be there."""
    top = run(tmp_path, "--help").stdout
    assert "set <layer> <value> <key>" in top
    topic = run(tmp_path, "help", "set").stdout
    assert "VALUE comes BEFORE the KEY" in topic
    assert "lath set base 8080 port" in topic
    assert "FIRST layer stacked wins" in run(tmp_path, "help", "stack").stdout


def test_first_stacked_layer_wins_a_conflict(tmp_path: Path) -> None:
    cwd = tree(tmp_path)
    run(cwd, "set", "base", "30", "timeout")
    run(cwd, "set", "prod", "60", "timeout")
    run(cwd, "stack", "prod")
    run(cwd, "stack", "base")
    assert run(cwd, "flatten", "--out", "out.json").returncode == 0
    assert json.loads((cwd / "out.json").read_text())["timeout"] == "60"


def test_the_reversed_stack_order_silently_picks_the_other_value(tmp_path: Path) -> None:
    cwd = tree(tmp_path)
    run(cwd, "set", "base", "30", "timeout")
    run(cwd, "set", "prod", "60", "timeout")
    run(cwd, "stack", "base")
    run(cwd, "stack", "prod")
    run(cwd, "flatten", "--out", "out.json")
    assert json.loads((cwd / "out.json").read_text())["timeout"] == "30"


def test_flatten_fingerprint_is_deterministic_and_path_independent(tmp_path: Path) -> None:
    cwd = tree(tmp_path)
    run(cwd, "set", "base", "8080", "port")
    run(cwd, "stack", "base")
    first = run(cwd, "flatten", "--out", "a.json").stdout
    second = run(cwd, "flatten", "--out", "deep/b.json").stdout
    pick = lambda text: [l for l in text.splitlines() if l.startswith("fingerprint:")][0]
    assert pick(first) == pick(second)
    record = json.loads((cwd / ".lath" / "last-flatten.json").read_text())
    assert pick(first).split()[1] == record["fingerprint"]
    assert record["out"] == "deep/b.json"


def test_flatten_requires_an_out_path_and_a_stacked_layer(tmp_path: Path) -> None:
    cwd = tree(tmp_path)
    run(cwd, "set", "base", "8080", "port")
    assert run(cwd, "flatten").returncode == 2
    assert run(cwd, "flatten", "--out", "x.json").returncode == 3
    assert not (cwd / "x.json").exists()


def test_a_hand_edited_layer_makes_flatten_refuse(tmp_path: Path) -> None:
    cwd = tree(tmp_path)
    run(cwd, "set", "base", "8080", "port")
    run(cwd, "stack", "base")
    (cwd / ".lath" / "layers" / "base.kv").write_text("port\t9999\n")
    out = run(cwd, "flatten", "--out", "x.json")
    assert out.returncode == 7
    assert "edited outside lath" in out.stderr


def test_unset_and_unstack(tmp_path: Path) -> None:
    cwd = tree(tmp_path)
    run(cwd, "set", "base", "8080", "port")
    run(cwd, "set", "base", "eu-west", "region")
    run(cwd, "unset", "base", "port")
    assert layer_lines(cwd, "base") == ["region\teu-west"]
    run(cwd, "stack", "base")
    assert run(cwd, "unstack", "base").returncode == 0
    assert run(cwd, "layers").stdout.startswith("nothing stacked")


def test_paths_outside_the_work_directory_are_refused(tmp_path: Path) -> None:
    cwd = tree(tmp_path)
    run(cwd, "set", "base", "8080", "port")
    run(cwd, "stack", "base")
    out = run(cwd, "flatten", "--out", "../escape.json")
    assert out.returncode == 5
    assert not (tmp_path.parent / "escape.json").exists()
