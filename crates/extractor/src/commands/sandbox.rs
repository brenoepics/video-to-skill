//! `vts-extract sandbox` — the execution boundary for video-derived
//! commands. The verification protocol runs every step through this.

use std::path::Path;

use anyhow::{bail, Result};
use vts_extract::sandbox;

/// Run `command` confined to `workspace`. The child's exit code becomes
/// this process's exit code, so a verifier reads pass/fail directly.
pub fn run(workspace: &str, command: &[String], print_policy: bool) -> Result<()> {
    let dir = Path::new(workspace);
    if print_policy {
        println!(
            "mechanism: {}",
            sandbox::detect().map_or(
                "NONE — execution would be refused",
                sandbox::Mechanism::name
            )
        );
        println!("{}", sandbox::policy_text(dir)?);
        return Ok(());
    }
    if command.is_empty() {
        bail!("nothing to run — pass the command after `--`, or use --print-policy");
    }
    let Some(mechanism) = sandbox::detect() else {
        // surface the same refusal the library raises, before the
        // workspace is even resolved
        sandbox::run(dir, command)?;
        unreachable!("run() refuses when no mechanism is available");
    };
    eprintln!(
        "sandbox: {} — no network, writes confined to {workspace}",
        mechanism.name()
    );
    let code = sandbox::run(dir, command)?;
    std::process::exit(code);
}
