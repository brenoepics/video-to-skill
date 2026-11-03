"""Behavioural tests for kiln.

Includes the two properties the benchmark leans on: a lane must be
primed before it fires (and `prime` is the only name for that step), and
a batch holds the LAST hold-count shards in name order, not the first.
Also asserts the help text stays terse -- kiln is the deliberately
poorly-documented tool, and a later "improvement" to its help would
silently change what the discoverability stratum measures.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

KILN = str(Path(__file__).resolve().parent / "kiln")

PROFILE = "# amber lane\nfire-lane: amber\nshard-glob: shards/*.txt\nhold-count: 3\n"


def run(cwd: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, KILN, *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )


def lane(tmp_path: Path, profile: str = PROFILE, count: int = 5) -> Path:
    (tmp_path / "shards").mkdir(parents=True)
    for n in range(1, count + 1):
        (tmp_path / "shards" / ("%02d-s.txt" % n)).write_text("payload %02d\n" % n)
    (tmp_path / "kiln.profile").write_text(profile)
    return tmp_path


def state(cwd: Path) -> dict:
    return json.loads((cwd / ".kiln" / "state.json").read_text())


def rollup(cwd: Path) -> str:
    found = sorted((cwd / "out").glob("*.rollup"))
    assert len(found) == 1
    return found[0].read_text()


def test_version_is_fixed(tmp_path: Path) -> None:
    assert run(tmp_path, "--version").stdout.strip() == "kiln 1.9.0"


def test_help_stays_terse(tmp_path: Path) -> None:
    text = run(tmp_path, "--help").stdout
    assert "prime" in text
    for secret in ("fire-lane", "shard-glob", "hold-count", "kiln.profile"):
        assert secret not in text, secret + " leaked into --help"


def test_firing_an_unprimed_lane_is_refused(tmp_path: Path) -> None:
    cwd = lane(tmp_path)
    out = run(cwd, "fire")
    assert out.returncode == 4
    assert "lane not primed" in out.stderr
    assert not (cwd / "out").exists()


def test_priming_without_a_profile_is_refused(tmp_path: Path) -> None:
    out = run(tmp_path, "prime")
    assert out.returncode == 3
    assert "profile absent" in out.stderr


def test_an_incomplete_profile_is_refused_without_naming_the_keys(tmp_path: Path) -> None:
    cwd = lane(tmp_path, profile="lane: amber\nglob: shards/*.txt\nkeep: 3\n")
    out = run(cwd, "prime")
    assert out.returncode == 3
    assert out.stderr.strip() == "kiln: profile incomplete"
    assert not (cwd / ".kiln").exists()


def test_prime_records_the_lane_and_a_stamped_state(tmp_path: Path) -> None:
    cwd = lane(tmp_path)
    out = run(cwd, "prime")
    assert out.returncode == 0
    assert "primed lane amber" in out.stdout
    recorded = state(cwd)
    assert recorded["lane"] == "amber"
    assert recorded["primed"] is True
    assert recorded["hold"] == 3
    assert len(recorded["stamp"]) == 16


def test_a_batch_holds_the_last_shards_in_name_order(tmp_path: Path) -> None:
    cwd = lane(tmp_path)
    run(cwd, "prime")
    assert run(cwd, "fire").returncode == 0
    text = rollup(cwd)
    assert "shards: 3" in text
    assert "-- 03-s.txt" in text and "-- 05-s.txt" in text
    assert "-- 01-s.txt" not in text and "-- 02-s.txt" not in text


def test_the_batch_id_is_deterministic_across_trees(tmp_path: Path) -> None:
    first = lane(tmp_path / "a")
    second = lane(tmp_path / "b")
    for cwd in (first, second):
        run(cwd, "prime")
        run(cwd, "fire")
    names = [sorted((cwd / "out").iterdir())[0].name for cwd in (first, second)]
    assert names[0] == names[1]
    assert state(first)["last_batch"] == state(second)["last_batch"]


def test_editing_the_profile_makes_the_state_stale(tmp_path: Path) -> None:
    cwd = lane(tmp_path)
    run(cwd, "prime")
    run(cwd, "fire")
    (cwd / "kiln.profile").write_text(PROFILE + "hold-count: 2\n")
    out = run(cwd, "fire")
    assert out.returncode == 5
    assert "state stale" in out.stderr
    assert "stale" in run(cwd, "peek").stdout
    assert run(cwd, "prime").returncode == 0
    assert run(cwd, "fire").returncode == 0


def test_a_fabricated_prior_batch_is_refused(tmp_path: Path) -> None:
    cwd = lane(tmp_path, profile=PROFILE + "prior-batch: deadbeef\n")
    out = run(cwd, "prime")
    assert out.returncode == 7
    assert "unknown prior batch" in out.stderr


def test_a_real_prior_batch_chains_the_next_firing(tmp_path: Path) -> None:
    cwd = lane(tmp_path)
    run(cwd, "prime")
    run(cwd, "fire")
    first = state(cwd)["last_batch"]
    (cwd / "kiln.profile").write_text(PROFILE + "prior-batch: " + first + "\n")
    run(cwd, "prime")
    out = run(cwd, "fire")
    assert out.returncode == 0
    second = state(cwd)["last_batch"]
    assert second != first
    text = (cwd / "out" / ("amber-%s.rollup" % second)).read_text()
    assert ("prior: " + first) in text
    record = json.loads((cwd / ".kiln" / "batches" / (second + ".json")).read_text())
    assert record["prior"] == first


def test_a_glob_outside_the_work_directory_is_refused(tmp_path: Path) -> None:
    cwd = lane(tmp_path, profile="fire-lane: amber\nshard-glob: ../*.txt\nhold-count: 1\n")
    out = run(cwd, "prime")
    assert out.returncode == 8


def test_no_shards_matched_is_refused(tmp_path: Path) -> None:
    cwd = lane(tmp_path, profile="fire-lane: amber\nshard-glob: nope/*.txt\nhold-count: 1\n")
    run(cwd, "prime")
    out = run(cwd, "fire")
    assert out.returncode == 6
