//! `vts-extract observe` — read-only GUI postconditions for the verifier.
//!
//! This runs OUTSIDE the sandbox on purpose: it executes nothing from the
//! video. It reads an application's accessibility tree and the
//! filesystem, prints what it saw, and exits 0 (every check holds),
//! 1 (a check failed) or 2 (nothing could be observed — no permission,
//! no such application, no backend on this platform, or an invalid
//! check). The sandbox policy is untouched; performing a tutorial's GUI
//! actions on the host is still not a thing this tool does.

use anyhow::{bail, Result};
use vts_extract::a11y::check::Check;
use vts_extract::a11y::observe::{observe, Report};

pub fn run(checks: &[String], dump: bool) -> Result<()> {
    let code = match attempt(checks, dump) {
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

fn attempt(checks: &[String], dump: bool) -> Result<Report> {
    if checks.is_empty() {
        bail!("nothing to check — pass --check '<json>' (repeatable; an object or an array)");
    }
    let mut parsed = Vec::new();
    for raw in checks {
        parsed.extend(Check::parse_many(raw)?);
    }
    observe(&parsed, dump)
}
