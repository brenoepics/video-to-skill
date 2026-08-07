//! Executing a [`Plan`]: one real copy, then a relative symlink per
//! remaining root, with a full copy as the fallback where symlinks are
//! not available (locked-down filesystems, Windows without developer
//! mode).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::package::{copy_tree, remove_existing};
use super::report::{LinkOutcome, Method, Report};
use super::Plan;

#[derive(Debug, Clone, Copy)]
pub struct ApplyOptions {
    /// Try symlinks first. Injectable so the copy fallback is testable
    /// without a hostile filesystem.
    pub symlinks: bool,
    /// Produce the report without writing anything.
    pub dry_run: bool,
}

impl Default for ApplyOptions {
    fn default() -> Self {
        Self {
            symlinks: true,
            dry_run: false,
        }
    }
}

/// Install `package` according to `plan`.
///
/// Order matters: the canonical copy lands first so every link created
/// afterwards resolves immediately.
pub fn apply(plan: &Plan, package: &Path, options: ApplyOptions) -> Result<Report> {
    refuse_self_install(plan, package)?;
    let canonical = if options.dry_run {
        plan.canonical.path.clone()
    } else {
        install_canonical(plan, package)?
    };

    let mut links = Vec::with_capacity(plan.links.len());
    for link in &plan.links {
        let method = if options.dry_run {
            if options.symlinks {
                Method::Symlink
            } else {
                Method::Copy
            }
        } else {
            place(&link.path, &link.relative_target, package, options)?
        };
        links.push(LinkOutcome {
            agent: link.agent.clone(),
            path: link.path.clone(),
            relative_target: link.relative_target.clone(),
            method,
            replaced: link.replaces,
        });
    }

    Ok(Report {
        skill_name: plan.skill_name.clone(),
        canonical_agent: plan.canonical.agent.clone(),
        canonical,
        replaced: plan.canonical.replaces,
        links,
        skips: plan.skips.clone(),
        dry_run: options.dry_run,
    })
}

/// Refuse to install a package over itself.
///
/// Every destination is cleared before it is written, so a `--package`
/// that *is* one of those destinations would be deleted and then copied
/// from its own grave — silently emptying it.
fn refuse_self_install(plan: &Plan, package: &Path) -> Result<()> {
    let Ok(source) = std::fs::canonicalize(package) else {
        return Ok(());
    };
    let destinations =
        std::iter::once(&plan.canonical.path).chain(plan.links.iter().map(|l| &l.path));
    for destination in destinations {
        if std::fs::canonicalize(destination).is_ok_and(|d| d == source) {
            bail!(
                "{} is already installed at that exact path — nothing to do \
                 (pass --package pointing at the freshly compiled package instead)",
                package.display()
            );
        }
    }
    Ok(())
}

/// Write the one real copy, replacing any previous package wholesale so
/// files dropped by a new version cannot linger.
fn install_canonical(plan: &Plan, package: &Path) -> Result<PathBuf> {
    let destination = &plan.canonical.path;
    remove_existing(destination)?;
    copy_tree(package, destination).with_context(|| {
        format!(
            "installing {} into {}",
            package.display(),
            destination.display()
        )
    })?;
    Ok(std::fs::canonicalize(destination).unwrap_or_else(|_| destination.clone()))
}

/// Point one root at the canonical package, falling back to a copy when
/// the symlink is refused.
fn place(
    path: &Path,
    relative_target: &Path,
    package: &Path,
    options: ApplyOptions,
) -> Result<Method> {
    remove_existing(path)?;
    if options.symlinks && symlink_dir(relative_target, path).is_ok() {
        return Ok(Method::Symlink);
    }
    // Leave nothing half-made behind before falling back.
    remove_existing(path)?;
    copy_tree(package, path)
        .with_context(|| format!("copying the package into {}", path.display()))?;
    Ok(Method::Copy)
}

#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}
