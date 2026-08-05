use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use vts_extract::compile::{self, ProcedureIr};

/// Compile a Procedure IR (authored by the analyzing agent) into an
/// installable skill package.
pub fn run(bundle: &str, ir_path: &str, out: &str) -> Result<()> {
    let raw = fs::read_to_string(ir_path).with_context(|| {
        format!("no Procedure IR at {ir_path} — write it first (see references/ir-format.md)")
    })?;
    let ir: ProcedureIr =
        serde_json::from_str(&raw).context("Procedure IR is not valid JSON for schema v1")?;
    compile::compile(&ir, Path::new(bundle), Path::new(out))?;
    println!(
        "skill package written: {out}/ ({} steps{})",
        ir.steps.len(),
        if ir.steps.iter().any(|s| s.confidence == "low") {
            ", low-confidence steps flagged"
        } else {
            ""
        }
    );
    println!(
        "install: move or copy it into ~/.claude/skills/{}",
        ir.skill_name
    );
    Ok(())
}
