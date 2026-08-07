use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use vts_extract::install::apply::{apply, ApplyOptions};
use vts_extract::install::package::skill_name_of;
use vts_extract::install::plan_install;
use vts_extract::install::roots::{detect, known_agents, unknown_agents};

/// Where the skill is installed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Scope {
    /// Every agent installed for this user (`~/.agents/skills`, …).
    Global,
    /// Only this project (`./.agents/skills`, …).
    Project,
}

/// Install a compiled skill package for every agent detected on the
/// machine: one real copy in the canonical root, relative symlinks
/// elsewhere.
pub fn run(package: &str, scope: Scope, only: &[String], force: bool, dry_run: bool) -> Result<()> {
    let unknown = unknown_agents(only);
    if !unknown.is_empty() {
        bail!(
            "unknown agent(s) in --only: {} (known: {})",
            unknown.join(", "),
            known_agents().join(", ")
        );
    }

    let package = Path::new(package);
    let skill_name = skill_name_of(package)?;
    let base = base_dir(scope)?;
    let states = detect(&base, &skill_name, only);
    let plan = plan_install(&skill_name, &states, force)?;
    let report = apply(
        &plan,
        package,
        ApplyOptions {
            symlinks: true,
            dry_run,
        },
    )?;
    println!("{report}");
    Ok(())
}

fn base_dir(scope: Scope) -> Result<PathBuf> {
    match scope {
        Scope::Global => dirs::home_dir().context("cannot locate the home directory"),
        Scope::Project => std::env::current_dir().context("cannot locate the current directory"),
    }
}
