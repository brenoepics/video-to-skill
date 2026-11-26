//! Reading a compiled package back for scoring: the IR it was compiled
//! from (`provenance.json`) and the verification report, if any.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::compile::ProcedureIr;
use crate::verify::{VerificationReport, VERIFICATION_FILE};

/// The package's IR and its verification report when one was recorded.
pub fn load_package(dir: &Path) -> Result<(ProcedureIr, Option<VerificationReport>)> {
    let provenance = dir.join("provenance.json");
    let ir: ProcedureIr = serde_json::from_str(
        &fs::read_to_string(&provenance)
            .with_context(|| format!("not a compiled package: {}", provenance.display()))?,
    )
    .with_context(|| format!("{} is not a valid IR", provenance.display()))?;
    let verification_path = dir.join(VERIFICATION_FILE);
    let verification = if verification_path.is_file() {
        Some(
            serde_json::from_str(&fs::read_to_string(&verification_path)?)
                .with_context(|| format!("{} is not valid", verification_path.display()))?,
        )
    } else {
        None
    };
    Ok((ir, verification))
}
