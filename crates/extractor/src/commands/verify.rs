use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use vts_extract::verify::{record_verification, VerificationReport};

/// Record a verification report against a generated skill package and
/// stamp the badge into its SKILL.md.
pub fn run(skill: &str, report_path: &str) -> Result<()> {
    let raw = fs::read_to_string(report_path)
        .with_context(|| format!("no verification report at {report_path}"))?;
    let report: VerificationReport =
        serde_json::from_str(&raw).context("verification report is not valid for schema v1")?;
    record_verification(Path::new(skill), &report)?;
    println!(
        "verification recorded: {} ({} step outcomes) — badge stamped in {skill}/SKILL.md",
        if report.verified {
            "VERIFIED"
        } else {
            "NOT VERIFIED"
        },
        report.steps.len()
    );
    Ok(())
}
