"""Behavioural tests for zep.

The happy path and, just as importantly, the failure mode the benchmark
leans on: sealing without the grant must succeed *and* leave a different
record, because that is the superficially-correct wrong procedure the
task checkers have to be able to tell apart.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ZEP = str(Path(__file__).resolve().parent / "zep")


def run(cwd: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, ZEP, *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )


def grant(cwd: Path, name: str) -> str:
    return (cwd / ".zep" / "drops" / name / "grant").read_text().strip()


def sealed(cwd: Path, name: str) -> dict:
    return json.loads((cwd / ".zep" / "drops" / name / "sealed.json").read_text())


def test_version_is_fixed(tmp_path: Path) -> None:
    out = run(tmp_path, "--version")
    assert out.returncode == 0
    assert out.stdout.strip() == "zep 0.4.2"


def test_open_prints_the_grant_and_writes_it_beside_the_drop(tmp_path: Path) -> None:
    out = run(tmp_path, "open", "rc-7")
    assert out.returncode == 0
    nonce = grant(tmp_path, "rc-7")
    assert len(nonce) == 12
    assert "grant: " + nonce in out.stdout
    assert ".zep/drops/rc-7/grant" in out.stdout


def test_grant_is_deterministic_per_name(tmp_path: Path) -> None:
    other = tmp_path / "second"
    other.mkdir()
    run(tmp_path, "open", "rc-7")
    run(other, "open", "rc-7")
    assert grant(tmp_path, "rc-7") == grant(other, "rc-7")


def test_full_seal_records_an_anchor(tmp_path: Path) -> None:
    run(tmp_path, "open", "rc-7")
    run(tmp_path, "put", "rc-7", "port", "8080")
    out = run(tmp_path, "seal", "rc-7", "--grant", grant(tmp_path, "rc-7"))
    assert out.returncode == 0
    record = sealed(tmp_path, "rc-7")
    assert record["state"] == "sealed"
    assert len(record["anchor"]) == 16
    assert "anchor: " + record["anchor"] in out.stdout
    ledger = json.loads((tmp_path / ".zep" / "ledger.json").read_text())
    assert ledger["drops"]["rc-7"]["state"] == "sealed"


def test_sealing_without_the_grant_succeeds_but_stays_provisional(tmp_path: Path) -> None:
    """The key failure mode: exit 0, plausible output, different state."""
    run(tmp_path, "open", "rc-7")
    run(tmp_path, "put", "rc-7", "port", "8080")
    out = run(tmp_path, "seal", "rc-7")
    assert out.returncode == 0
    assert "provisional" in out.stdout
    record = sealed(tmp_path, "rc-7")
    assert record["state"] == "provisional"
    assert record["anchor"] is None


def test_a_provisional_drop_can_be_upgraded_with_the_grant(tmp_path: Path) -> None:
    run(tmp_path, "open", "rc-7")
    run(tmp_path, "put", "rc-7", "port", "8080")
    run(tmp_path, "seal", "rc-7")
    out = run(tmp_path, "seal", "rc-7", "--grant", grant(tmp_path, "rc-7"))
    assert out.returncode == 0
    assert sealed(tmp_path, "rc-7")["state"] == "sealed"


def test_a_wrong_grant_is_refused(tmp_path: Path) -> None:
    run(tmp_path, "open", "rc-7")
    run(tmp_path, "put", "rc-7", "port", "8080")
    out = run(tmp_path, "seal", "rc-7", "--grant", "000000000000")
    assert out.returncode == 4
    assert "grant mismatch" in out.stderr
    assert not (tmp_path / ".zep" / "drops" / "rc-7" / "sealed.json").exists()


def test_the_documented_token_flag_does_not_exist(tmp_path: Path) -> None:
    """--help advertises --token; seal does not accept it. Deliberate."""
    assert "--token" in run(tmp_path, "--help").stdout
    run(tmp_path, "open", "rc-7")
    run(tmp_path, "put", "rc-7", "port", "8080")
    out = run(tmp_path, "seal", "rc-7", "--token", grant(tmp_path, "rc-7"))
    assert out.returncode == 2
    assert "unknown option: --token" in out.stderr


def test_staging_into_a_sealed_drop_is_refused(tmp_path: Path) -> None:
    run(tmp_path, "open", "rc-7")
    run(tmp_path, "put", "rc-7", "port", "8080")
    run(tmp_path, "seal", "rc-7", "--grant", grant(tmp_path, "rc-7"))
    out = run(tmp_path, "put", "rc-7", "region", "eu-west")
    assert out.returncode == 3
    assert "is sealed" in out.stderr


def test_sealing_an_empty_drop_is_refused(tmp_path: Path) -> None:
    run(tmp_path, "open", "rc-7")
    out = run(tmp_path, "seal", "rc-7", "--grant", grant(tmp_path, "rc-7"))
    assert out.returncode == 3
    assert "nothing staged" in out.stderr


def test_chaining_requires_a_real_sealed_anchor(tmp_path: Path) -> None:
    run(tmp_path, "open", "rc-7")
    run(tmp_path, "put", "rc-7", "port", "8080")
    run(tmp_path, "seal", "rc-7", "--grant", grant(tmp_path, "rc-7"))
    anchor = sealed(tmp_path, "rc-7")["anchor"]

    bad = run(tmp_path, "open", "rc-8", "--after", "0" * 16)
    assert bad.returncode == 4
    assert "unknown anchor" in bad.stderr
    assert not (tmp_path / ".zep" / "drops" / "rc-8").exists()

    good = run(tmp_path, "open", "rc-8", "--after", anchor)
    assert good.returncode == 0
    run(tmp_path, "put", "rc-8", "note", "fix")
    run(tmp_path, "seal", "rc-8", "--grant", grant(tmp_path, "rc-8"))
    assert sealed(tmp_path, "rc-8")["parent_anchor"] == anchor


def test_a_provisional_drop_cannot_be_chained_from(tmp_path: Path) -> None:
    run(tmp_path, "open", "rc-7")
    run(tmp_path, "put", "rc-7", "port", "8080")
    run(tmp_path, "seal", "rc-7")
    out = run(tmp_path, "open", "rc-8", "--after", "0" * 16)
    assert out.returncode == 4


def test_paths_outside_the_work_directory_are_refused(tmp_path: Path) -> None:
    out = run(tmp_path, "show", "../escape")
    assert out.returncode == 2
    assert out.stdout == ""
