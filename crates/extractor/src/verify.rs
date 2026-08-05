//! Verification records for generated skill packages.
//!
//! The verification itself is performed by a sandboxed sub-agent (see
//! references/verification-protocol.md); this module owns the typed
//! report, validates it against the package, and stamps an honest badge
//! into the generated SKILL.md.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::compile::ProcedureIr;

pub const VERIFICATION_FILE: &str = "verification.json";
const BADGE_START: &str = "<!-- verification-badge -->";
const BADGE_END: &str = "<!-- /verification-badge -->";

/// Allowed per-step outcomes: `pass`, `fail`, `skipped` (safety policy),
/// `unverifiable` (cannot be checked in a sandbox).
const OUTCOMES: &[&str] = &["pass", "fail", "skipped", "unverifiable"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    pub schema_version: u32,
    /// True only when every executed step met its success criteria.
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
}

/// Validate `report` against the package at `skill_dir`, write
/// `verification.json`, and stamp the badge into SKILL.md (replacing any
/// previous badge).
pub fn record_verification(skill_dir: &Path, report: &VerificationReport) -> Result<()> {
    if report.schema_version != 1 {
        bail!(
            "unsupported report schema_version {}",
            report.schema_version
        );
    }
    let provenance: ProcedureIr = serde_json::from_str(
        &fs::read_to_string(skill_dir.join("provenance.json"))
            .context("not a generated skill package (missing provenance.json)")?,
    )
    .context("package provenance.json is not valid")?;

    let known: Vec<&str> = provenance.steps.iter().map(|s| s.id.as_str()).collect();
    for step in &report.steps {
        if !known.contains(&step.id.as_str()) {
            bail!("report cites unknown step '{}'", step.id);
        }
        if !OUTCOMES.contains(&step.outcome.as_str()) {
            bail!(
                "step '{}': outcome '{}' is not one of {OUTCOMES:?}",
                step.id,
                step.outcome
            );
        }
    }

    fs::write(
        skill_dir.join(VERIFICATION_FILE),
        serde_json::to_string_pretty(report)?,
    )?;
    stamp_badge(skill_dir, report)
}

fn stamp_badge(skill_dir: &Path, report: &VerificationReport) -> Result<()> {
    let path = skill_dir.join("SKILL.md");
    let skill_md = fs::read_to_string(&path).context("package has no SKILL.md")?;

    let counts = |outcome: &str| report.steps.iter().filter(|s| s.outcome == outcome).count();
    let body = if report.verified {
        format!(
            "> ✅ **Verified by execution** — {} passed, {} skipped by safety policy, {} unverifiable in a sandbox. Details: [verification.json](verification.json).",
            counts("pass"),
            counts("skipped"),
            counts("unverifiable"),
        )
    } else {
        format!(
            "> ⚠ **NOT verified** — {}. Details: [verification.json](verification.json).",
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
    fs::write(&path, stamped)?;
    Ok(())
}
