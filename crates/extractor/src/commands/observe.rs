//! `vts-extract observe` — read-only GUI postconditions for the verifier.
//!
//! This runs OUTSIDE the sandbox on purpose: it executes nothing from the
//! video. It reads an application's accessibility tree and the
//! filesystem, prints what it saw, and exits 0 (every check holds),
//! 1 (a check failed) or 2 (nothing could be observed — no permission,
//! no such application, no backend on this platform, or an invalid
//! check). The sandbox policy is untouched; performing a tutorial's GUI
//! actions on the host is still not a thing this tool does.

use std::path::Path;

use anyhow::{bail, Result};
use vts_extract::a11y::check::Check;
use vts_extract::a11y::observe::{observe, Report};
use vts_extract::compile::checks::{needs_workspace, substitute_workspace, WORKSPACE_PLACEHOLDER};

pub fn run(checks: &[String], dump: bool, workspace: Option<&str>) -> Result<()> {
    let code = match attempt(checks, dump, workspace) {
        Ok(report) => {
            println!("{}", serde_json::to_string_pretty(&report)?);
            i32::from(!report.passed)
        }
        Err(error) => {
            eprintln!("observe: could not observe — {error:#}");
            2
        }
    };
    std::process::exit(code)
}

fn attempt(checks: &[String], dump: bool, workspace: Option<&str>) -> Result<Report> {
    if checks.is_empty() {
        bail!("nothing to check — pass --check '<json>' (repeatable; an object or an array)");
    }
    let mut declared = Vec::new();
    for raw in checks {
        declared.extend(Check::parse_many(raw)?);
    }
    let observed = match workspace {
        Some(dir) => declared
            .iter()
            .map(|check| substitute_workspace(check, Path::new(dir)))
            .collect::<Result<Vec<_>>>()?,
        None if declared.iter().any(needs_workspace) => bail!(
            "a file_exists path names {WORKSPACE_PLACEHOLDER} — pass --workspace <dir>, the \
             throwaway workspace the step ran in"
        ),
        None => declared.clone(),
    };
    let mut report = observe(&observed, dump)?;
    // Report each check as the step declares it, so the record matches
    // the package; `observed` carries the concrete path.
    for (entry, check) in report.checks.iter_mut().zip(declared) {
        entry.check = check;
    }
    Ok(report)
}
