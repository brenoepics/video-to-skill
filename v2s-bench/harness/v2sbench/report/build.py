"""Build the report bundle, and re-derive it to check a published one.

Two guarantees live here.

**Mock data cannot become a figure.** Records written by the stub agent,
the oracle gate or the trivial-agent gate are stamped ``is_mock``. If any
are present the bundle's status becomes ``mock``, which the site renders
with a watermark and no figures; if mock and real records are mixed, the
build refuses outright rather than quietly averaging them.

**Every figure is recomputable.** ``verify`` reads a published bundle,
recomputes it from the raw records and reports any disagreement, so the
numbers on the site are auditable rather than asserted.
"""

from __future__ import annotations

import json
from dataclasses import asdict
from pathlib import Path
from typing import Any

from .. import __version__
from .. import arms as arms_mod
from .. import record as record_mod
from .. import stats
from .. import tasks as tasks_mod

SCHEMA_VERSION = 1

#: Below this many paired trials per comparison, a run is a pilot: real,
#: but under-powered for the effect sizes the literature reports.
PILOT_PAIRS = 150

STATUS_TEXT = {
    "no-results": (
        "No measurements yet",
        "The suite, harness and analysis are in place; no trials have been run. "
        "Nothing on this page is a result.",
    ),
    "mock": (
        "Mock data — not a measurement",
        "These figures come from the stub agent, which calls no model. They exist "
        "only to exercise the pipeline and must not be read as findings.",
    ),
    "pilot": (
        "Pilot run — under-powered",
        "Real trials, but too few to resolve the effect sizes this benchmark cares "
        "about. Intervals are wide and verdicts are mostly inconclusive by design.",
    ),
    "measured": ("Measured", "Trials run under the frozen pre-registration."),
}


def build_report(
    root: Path, *, results_dir: Path | None = None, resamples: int = 10000, seed: int = 0
) -> dict[str, Any]:
    results_dir = results_dir or root / "results" / "runs"
    records = record_mod.read_all(results_dir) if results_dir.exists() else []
    suite = tasks_mod.load_suite(root, validate=False)
    probes = _load_probes(root)

    # Raises when mock and measured records appear together. Letting it
    # propagate is the point: a published figure must never average a
    # measurement with a fixture.
    if records:
        stats.refuse_mixed(records)

    is_mock = bool(records) and all(
        r.get("provenance", {}).get("is_mock") for r in records
    )
    kind = _status_kind(records, is_mock)
    headline, detail = STATUS_TEXT[kind]

    bundle: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "generated_at": record_mod.utcnow(),
        "status": {
            "kind": kind,
            "headline": headline,
            "detail": detail,
            "prereg_frozen": _prereg_frozen(root),
        },
        "provenance": _provenance(root, records, suite, is_mock),
        "arms": [],
        "comparisons": [],
        "strata": [],
        "tasks": [_task_entry(t, probes.get(t.id)) for t in suite],
        "cells": [],
        "failure_modes": [],
    }

    if kind == "no-results":
        return bundle

    bundle["arms"] = [asdict(a) for a in stats.analyse_arms(records)]
    bundle["cells"] = [asdict(c) for c in stats.analyse_cells(records)]
    bundle["comparisons"] = [
        asdict(c)
        for c in stats.analyse_comparisons(
            records, arms_mod.COMPARISONS, resamples=resamples, seed=seed
        )
    ]
    bundle["strata"] = [
        asdict(s)
        for dimension in ("headroom_band", "discoverability", "tier", "video_tier")
        for s in stats.analyse_strata(
            records, dimension, "A1", "A0", resamples=resamples, seed=seed
        )
    ]
    return bundle


def _status_kind(records: list[dict[str, Any]], is_mock: bool) -> str:
    if not records:
        return "no-results"
    if is_mock:
        return "mock"
    ok = sum(1 for r in records if r.get("outcome", {}).get("status") == "ok")
    return "pilot" if ok < PILOT_PAIRS else "measured"


def _provenance(
    root: Path, records: list[dict[str, Any]], suite: list[Any], is_mock: bool
) -> dict[str, Any]:
    provs = [r.get("provenance", {}) for r in records]
    costs = [
        float(r.get("metrics", {}).get("usd_cost") or 0.0)
        for r in records
    ]
    excluded = sum(
        1 for r in records if r.get("outcome", {}).get("status") != "ok"
    )
    return {
        "harness_version": __version__,
        "suite_version": tasks_mod.suite_version(root) if (root / "tasks").exists() else None,
        "suite_commit": _first(provs, "suite_commit"),
        "prereg_commit": _first(provs, "prereg_commit"),
        "models": sorted({p.get("model") for p in provs if p.get("model")}),
        "run_ids": sorted({r.get("run_id") for r in records if r.get("run_id")}),
        "trials_total": len(records),
        "trials_excluded": excluded,
        "usd_total": round(sum(costs), 4) if any(costs) else None,
        "video_tiers": sorted({r.get("video", {}).get("tier") for r in records if r.get("video")}),
        "is_mock": is_mock,
    }


def _first(provs: list[dict[str, Any]], key: str) -> Any:
    for prov in provs:
        if prov.get(key):
            return prov[key]
    return None


def _task_entry(task: Any, probe: float | None) -> dict[str, Any]:
    return {
        "id": task.id,
        "tier": task.tier,
        "title": task.title,
        "goal": task.goal,
        "knowledge_gap": task.knowledge_gap.summary,
        "discoverability": task.discoverability,
        "probe_known": probe,
        "headroom_band": task.headroom_band(probe),
        "saturated": task.saturated,
        "video_tier": task.video.tier,
        "video_license": task.license,
        "video_attribution": task.attribution,
    }


def _load_probes(root: Path) -> dict[str, float]:
    path = root / "results" / "probes.json"
    if not path.exists():
        return {}
    payload = json.loads(path.read_text())
    return {k: float(v) for k, v in payload.get("probe_known", {}).items()}


def _prereg_frozen(root: Path) -> bool:
    doc = root / "docs" / "preregistration.md"
    if not doc.exists():
        return False
    head = doc.read_text()[:600]
    return "**Frozen on:** _(unset)_" not in head


# -- auditing --------------------------------------------------------------


def verify(root: Path, report_path: Path, *, results_dir: Path | None = None) -> list[str]:
    """Recompute the bundle and list every figure that disagrees.

    This is the promise that makes the published numbers checkable: a
    reader runs it and either gets silence or a list of discrepancies.
    """
    if not report_path.exists():
        return [f"no report at {report_path}"]
    published = json.loads(report_path.read_text())
    recomputed = build_report(root, results_dir=results_dir)
    problems: list[str] = []

    if published.get("status", {}).get("kind") != recomputed["status"]["kind"]:
        problems.append(
            f"status.kind: published {published.get('status', {}).get('kind')!r} "
            f"vs recomputed {recomputed['status']['kind']!r}"
        )

    for section, key in (("arms", "id"), ("comparisons", "hypothesis"), ("cells", None)):
        problems += _compare_section(published, recomputed, section, key)

    for field in ("trials_total", "trials_excluded", "models"):
        pub = published.get("provenance", {}).get(field)
        rec = recomputed["provenance"].get(field)
        if pub != rec:
            problems.append(f"provenance.{field}: published {pub!r} vs recomputed {rec!r}")
    return problems


_NUMERIC = ("rate", "diff", "ci_lo", "ci_hi", "p_value", "p_adjusted", "wilson_lo", "wilson_hi")


def _compare_section(
    published: dict[str, Any], recomputed: dict[str, Any], section: str, key: str | None
) -> list[str]:
    problems: list[str] = []
    pub_rows = published.get(section) or []
    rec_rows = recomputed.get(section) or []
    if len(pub_rows) != len(rec_rows):
        problems.append(f"{section}: published {len(pub_rows)} rows vs recomputed {len(rec_rows)}")
    index = _index(rec_rows, key)
    for row in pub_rows:
        match = index.get(_row_key(row, key))
        if match is None:
            problems.append(f"{section}: published row {_row_key(row, key)} not reproducible")
            continue
        for field in _NUMERIC:
            if field in row and not _close(row.get(field), match.get(field)):
                problems.append(
                    f"{section}[{_row_key(row, key)}].{field}: "
                    f"published {row.get(field)!r} vs recomputed {match.get(field)!r}"
                )
        if row.get("verdict") and row.get("verdict") != match.get("verdict"):
            problems.append(
                f"{section}[{_row_key(row, key)}].verdict: "
                f"published {row['verdict']!r} vs recomputed {match.get('verdict')!r}"
            )
    return problems


def _index(rows: list[dict[str, Any]], key: str | None) -> dict[Any, dict[str, Any]]:
    return {_row_key(row, key): row for row in rows}


def _row_key(row: dict[str, Any], key: str | None) -> Any:
    if key:
        return row.get(key)
    return (row.get("task"), row.get("arm"))


def _close(a: Any, b: Any, tol: float = 1e-9) -> bool:
    if a is None or b is None:
        return a == b
    try:
        return abs(float(a) - float(b)) <= tol
    except (TypeError, ValueError):
        return a == b
