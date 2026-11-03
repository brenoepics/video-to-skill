//! Verification records for generated skill packages.
//!
//! The verification itself is performed by a sandboxed sub-agent (see
//! references/verification-protocol.md); this module owns the typed
//! report, validates it against the package, and stamps an honest badge
//! into the generated SKILL.md.

use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::compile::{frontmatter, repairs, ProcedureIr, Verdict, SCHEMA_VERSION};

pub const VERIFICATION_FILE: &str = "verification.json";
const BADGE_START: &str = "<!-- verification-badge -->";
const BADGE_END: &str = "<!-- /verification-badge -->";

/// Allowed per-step outcomes: `pass`, `fail`, `skipped` (safety policy),
/// `unverifiable` (cannot be checked in a sandbox).
const OUTCOMES: &[&str] = &["pass", "fail", "skipped", "unverifiable"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    pub schema_version: u32,
    /// True only when every executed step met its success criteria AND
    /// nothing was skipped/unverifiable. When false, the badge derives
    /// partial (no `fail` outcomes) vs failed from the step outcomes.
    pub verified: bool,
    /// Verify/repair attempts consumed (protocol caps this at 2 repairs).
    pub attempts: u32,
    /// One-line human summary; shown in the badge when not verified.
    pub summary: String,
    pub steps: Vec<StepOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepOutcome {
    pub id: String,
    pub outcome: String,
    pub detail: String,
    /// External signals cited by this step's repairs — the `Repair N: …
    /// Signal: …` lines of its caveats — filled in by
    /// [`record_verification`] from the package's provenance.
    #[serde(default)]
    pub repairs: Vec<String>,
}

/// Validate `report` against the package at `skill_dir`, carry the
/// repair signals its steps cite into it, write `verification.json`, and
/// stamp the badge into SKILL.md (replacing any previous badge). Returns
/// the report as written.
pub fn record_verification(
    skill_dir: &Path,
    report: &VerificationReport,
) -> Result<VerificationReport> {
    if report.schema_version != 1 {
        bail!(
            "unsupported report schema_version {}",
            report.schema_version
        );
    }
    let mut provenance: ProcedureIr = serde_json::from_str(
        &fs::read_to_string(skill_dir.join("provenance.json"))
            .context("not a generated skill package (missing provenance.json)")?,
    )
    .context("package provenance.json is not valid")?;

    let mut recorded = report.clone();
    for step in &mut recorded.steps {
        let Some(known) = provenance.steps.iter().find(|s| s.id == step.id) else {
            bail!("report cites unknown step '{}'", step.id);
        };
        if !OUTCOMES.contains(&step.outcome.as_str()) {
            bail!(
                "step '{}': outcome '{}' is not one of {OUTCOMES:?}",
                step.id,
                step.outcome
            );
        }
        let cited =
            repairs::cited_signals(known.caveats.as_deref().unwrap_or("")).map_err(|line| {
                anyhow!(
                    "step '{}': the package records a repair without an external signal \
                     ('{line}') — fix the IR and recompile; the compiler refuses it",
                    step.id
                )
            })?;
        for signal in cited {
            if !step.repairs.contains(&signal) {
                step.repairs.push(signal);
            }
        }
    }
    let cited: usize = recorded.steps.iter().map(|s| s.repairs.len()).sum();
    if report.attempts > 1 && cited == 0 {
        bail!(
            "report claims {} repair(s) (attempts: {}) but no step records one — every \
             repair must be a 'Repair N: <change>. Signal: <external signal>' line in the \
             caveats of the step it changed (see references/verification-protocol.md)",
            report.attempts - 1,
            report.attempts
        );
    }

    fs::write(
        skill_dir.join(VERIFICATION_FILE),
        serde_json::to_string_pretty(&recorded)?,
    )?;

    let path = skill_dir.join("SKILL.md");
    let skill_md = fs::read_to_string(&path).context("package has no SKILL.md")?;
    let stamped = stamp_badge(&skill_md, &recorded)?;
    // The frontmatter's machine-readable counterpart of the badge:
    // passed/total, so a catalogue can rank skills without parsing prose.
    let passed = recorded
        .steps
        .iter()
        .filter(|s| s.outcome == "pass")
        .count();
    let stamped = frontmatter::set_metadata(
        &stamped,
        frontmatter::METADATA_VERIFIED_STEPS,
        &format!("{passed}/{}", provenance.steps.len()),
    )
    .context("recompile the package with this version of vts-extract first")?;
    fs::write(&path, stamped)?;

    record_verdicts(&mut provenance, &recorded);
    fs::write(
        skill_dir.join("provenance.json"),
        serde_json::to_string_pretty(&provenance)?,
    )?;
    Ok(recorded)
}

/// Fold the report into the package's own record of its steps: the
/// latest verdict, one more execution for every step that ran, and
/// unsolved failures as failure notes.
fn record_verdicts(provenance: &mut ProcedureIr, report: &VerificationReport) {
    for outcome in &report.steps {
        let Some(step) = provenance.steps.iter_mut().find(|s| s.id == outcome.id) else {
            continue;
        };
        if matches!(outcome.outcome.as_str(), "pass" | "fail") {
            step.usage_count += 1;
        }
        if outcome.outcome == "fail" {
            let note = format!("Unsolved: {}", outcome.detail);
            if !step.failure_notes.contains(&note) {
                step.failure_notes.push(note);
            }
        }
        step.verdict = Some(Verdict {
            outcome: outcome.outcome.clone(),
            detail: outcome.detail.clone(),
            repairs: outcome.repairs.clone(),
        });
    }
    provenance.schema_version = SCHEMA_VERSION;
}

fn stamp_badge(skill_md: &str, report: &VerificationReport) -> Result<String> {
    let counts = |outcome: &str| report.steps.iter().filter(|s| s.outcome == outcome).count();
    let any_failed = report.steps.iter().any(|s| s.outcome == "fail");
    // A skill that needed repairing says so: the signals are in the report.
    let repaired = match report.attempts.saturating_sub(1) {
        0 => String::new(),
        1 => " after 1 repair".to_string(),
        n => format!(" after {n} repairs"),
    };
    let body = if report.verified {
        format!(
            "> ✅ **Verified by execution**{repaired} — {} passed, {} skipped by safety policy, {} unverifiable in a sandbox. Details: [verification.json](verification.json).",
            counts("pass"),
            counts("skipped"),
            counts("unverifiable"),
        )
    } else if !any_failed {
        // Nothing that ran diverged; the skill is only partially exercised.
        format!(
            "> 🟡 **Partially verified**{repaired} — no executed step failed: {} passed, {} skipped by safety policy, {} unverifiable in a sandbox. {}. Details: [verification.json](verification.json).",
            counts("pass"),
            counts("skipped"),
            counts("unverifiable"),
            report.summary,
        )
    } else {
        format!(
            "> ⚠ **NOT verified**{repaired} — {}. Details: [verification.json](verification.json).",
            report.summary
        )
    };
    let badge = format!("{BADGE_START}\n{body}\n{BADGE_END}");

    let stamped =
        if let (Some(start), Some(end)) = (skill_md.find(BADGE_START), skill_md.find(BADGE_END)) {
            let mut out = String::new();
            out.push_str(&skill_md[..start]);
            out.push_str(&badge);
            out.push_str(&skill_md[end + BADGE_END.len()..]);
            out
        } else {
            // Insert right after the frontmatter's closing delimiter.
            let after = skill_md[3..]
                .find("---")
                .map(|i| i + 3 + 3)
                .context("SKILL.md has no frontmatter to stamp after")?;
            format!(
                "{}\n\n{badge}{}",
                skill_md[..after].trim_end(),
                &skill_md[after..]
            )
        };
    Ok(stamped)
}
