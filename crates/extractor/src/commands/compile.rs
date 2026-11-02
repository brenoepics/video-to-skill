use std::path::Path;

use anyhow::Result;
use vts_extract::compile;

/// Compile a Procedure IR (authored by the analyzing agent) into an
/// installable skill package. `out` must be named after the skill.
pub fn run(bundle: &str, ir_path: &str, out: &str) -> Result<()> {
    let ir = compile::load_ir(Path::new(ir_path), Path::new(bundle))?;
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
        "install: vts-extract install --package {out}   # prints the plan; add --yes to apply"
    );
    Ok(())
}
