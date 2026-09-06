//! Reading and copying a skill package on disk.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

/// The marker that makes a directory a skill package.
pub const MANIFEST: &str = "SKILL.md";

/// The name `package` will be installed under: its SKILL.md frontmatter
/// `name:` when present, else the directory's own name.
///
/// Doubles as validation — a directory without a SKILL.md is not a
/// skill package and is refused.
pub fn skill_name_of(package: &Path) -> Result<String> {
    let manifest = package.join(MANIFEST);
    if !manifest.is_file() {
        bail!(
            "{} is not a skill package — no {MANIFEST} in it (point --package at a compiled package dir)",
            package.display()
        );
    }
    let text =
        fs::read_to_string(&manifest).with_context(|| format!("reading {}", manifest.display()))?;
    let name = frontmatter_name(&text)
        .or_else(|| {
            package
                .file_name()
                .and_then(|n| n.to_str())
                .map(ToString::to_string)
        })
        .with_context(|| format!("cannot name a skill from {}", package.display()))?;
    crate::safe_path::validate_skill_name(&name)?;
    Ok(name)
}

/// `name:` from a leading `---` frontmatter block, if there is one.
fn frontmatter_name(text: &str) -> Option<String> {
    let mut lines = text.lines();
    if lines.next()?.trim_end_matches('\r') != "---" {
        return None;
    }
    let mut name = None;
    for raw in lines {
        let line = raw.trim_end_matches('\r');
        if line == "---" {
            return name;
        }
        if let Some(value) = line.strip_prefix("name:").map(str::trim) {
            if !value.is_empty() {
                name = Some(value.to_string());
            }
        }
    }
    None
}

/// Recursively copy `from` into `to`, creating `to` and preserving the
/// tree's shape. Symlinks inside the package are followed (a package is
/// self-contained by construction).
pub fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to).with_context(|| format!("creating {}", to.display()))?;
    for entry in fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&source, &destination)?;
        } else {
            fs::copy(&source, &destination)
                .with_context(|| format!("copying {}", source.display()))?;
        }
    }
    Ok(())
}

/// Remove whatever occupies `path` — file, directory, or symlink —
/// leaving the name free. A no-op when nothing is there.
pub fn remove_existing(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    // A symlink to a directory is not a directory: unlink it, never
    // recurse through it into the canonical copy.
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        fs::remove_file(path).with_context(|| format!("removing {}", path.display()))
    } else {
        fs::remove_dir_all(path).with_context(|| format!("removing {}", path.display()))
    }
}
