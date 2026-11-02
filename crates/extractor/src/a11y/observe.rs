//! Orchestration behind `vts-extract observe`: read each application's
//! tree once, evaluate every check, and build the report.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::check::{Check, Target};
use super::eval::evaluate;
use super::tree::Snapshot;

#[cfg(test)]
mod tests;

/// The substrate name recorded for `file_exists` checks.
pub const FILESYSTEM_SUBSTRATE: &str = "filesystem";

/// What `observe` prints: one entry per requested check, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// True only when every check passed.
    pub passed: bool,
    pub checks: Vec<CheckReport>,
    /// The trees that were read, when dumping was requested.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snapshots: Vec<Snapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckReport {
    pub check: Check,
    /// Which substrate answered: the GUI backend's name or `filesystem`.
    pub substrate: String,
    pub passed: bool,
    pub observed: String,
    /// What a pass does not establish; the verifier copies this into the
    /// step's detail.
    pub not_proved: String,
}

/// Observe `checks` with this platform's backend.
pub fn observe(checks: &[Check], dump: bool) -> Result<Report> {
    observe_with(checks, dump, backend::read)
}

/// Observe `checks`, reading application trees through `read` — one
/// read per distinct application. Exposed so the whole report path can
/// be exercised without a GUI.
pub fn observe_with(
    checks: &[Check],
    dump: bool,
    mut read: impl FnMut(Option<&str>) -> Result<Snapshot>,
) -> Result<Report> {
    let mut snapshots: Vec<(Option<String>, Snapshot)> = Vec::new();
    let mut results = Vec::with_capacity(checks.len());
    for check in checks {
        let (verdict, substrate) = match check.target() {
            Target::File(_) => (evaluate(check, None), FILESYSTEM_SUBSTRATE),
            Target::Gui(app) => {
                let known = snapshots.iter().position(|(key, _)| key.as_deref() == app);
                let index = if let Some(index) = known {
                    index
                } else {
                    let snapshot = read(app).with_context(|| {
                        format!(
                            "{}: could not observe {}",
                            check.kind(),
                            app.unwrap_or("the frontmost application")
                        )
                    })?;
                    snapshots.push((app.map(str::to_string), snapshot));
                    snapshots.len() - 1
                };
                (
                    evaluate(check, Some(&snapshots[index].1)),
                    backend::SUBSTRATE,
                )
            }
        };
        results.push(CheckReport {
            check: check.clone(),
            substrate: substrate.to_string(),
            passed: verdict.passed,
            observed: verdict.observed,
            not_proved: verdict.not_proved.to_string(),
        });
    }
    Ok(Report {
        passed: results.iter().all(|result| result.passed),
        checks: results,
        snapshots: if dump {
            snapshots
                .into_iter()
                .map(|(_, snapshot)| snapshot)
                .collect()
        } else {
            Vec::new()
        },
    })
}

#[cfg(target_os = "macos")]
mod backend {
    pub use crate::a11y::macos::{snapshot as read, SUBSTRATE};
}

#[cfg(not(target_os = "macos"))]
mod backend {
    use anyhow::{bail, Result};

    use crate::a11y::tree::Snapshot;

    pub const SUBSTRATE: &str = "none";

    pub fn read(_app: Option<&str>) -> Result<Snapshot> {
        bail!(
            "GUI observation has no backend on this platform yet (macOS AXUIElement only; \
             Windows UI Automation and AT-SPI are designed in issue #37) — mark the step \
             unverifiable with this reason"
        )
    }
}
