//! Portable path validation for data that can be influenced by a video.
//!
//! Validation uses one syntax on every host: `/` separates relative path
//! components. Rejecting Windows syntax even on Unix keeps a package safe
//! when it is later moved between operating systems.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Validate an installed skill directory name.
pub fn validate_skill_name(name: &str) -> Result<()> {
    if !is_kebab(name) || windows_reserved(name) {
        bail!(
            "skill name '{name}' must be safe kebab-case (lowercase letters, digits, hyphens, and not a reserved Windows name)"
        );
    }
    Ok(())
}

/// Validate a step id, which also becomes a Markdown file name.
pub fn validate_step_id(id: &str) -> Result<()> {
    if !is_kebab(id) || windows_reserved(id) {
        bail!(
            "step id '{id}' must be safe kebab-case (lowercase letters, digits, hyphens, and not a reserved Windows name)"
        );
    }
    Ok(())
}

/// Validate one script file name; directories are deliberately forbidden.
pub fn validate_script_name(name: &str) -> Result<()> {
    if name.contains('/') || name.contains('\\') || name.is_empty() || invalid_windows_chars(name) {
        bail!("script name '{name}' must be one portable file name");
    }
    if name == "." || name == ".." || name.ends_with('.') || name.ends_with(' ') {
        bail!("script name '{name}' must be one portable file name");
    }
    if windows_reserved(name) {
        bail!("script name '{name}' is a reserved Windows file name");
    }
    Ok(())
}

/// Resolve a portable relative reference and prove its target is a file
/// contained by `bundle_dir`, including after following symlinks.
pub fn resolve_bundle_file(bundle_dir: &Path, reference: &str) -> Result<PathBuf> {
    validate_bundle_reference(reference)?;
    let root = fs::canonicalize(bundle_dir)
        .with_context(|| format!("cannot resolve bundle directory {}", bundle_dir.display()))?;
    let candidate = root.join(reference);
    let resolved = fs::canonicalize(&candidate).with_context(|| {
        format!("bundle-relative frame '{reference}' does not name an existing file")
    })?;
    if !resolved.starts_with(&root) || !resolved.is_file() {
        bail!("bundle-relative frame '{reference}' must stay inside the bundle");
    }
    Ok(resolved)
}

/// Return the final component of a validated bundle-relative reference.
pub fn portable_basename(reference: &str) -> Result<&str> {
    validate_bundle_reference(reference)?;
    Ok(reference
        .rsplit_once('/')
        .map_or(reference, |(_, name)| name))
}

fn validate_bundle_reference(reference: &str) -> Result<()> {
    if reference.is_empty() || reference.contains('\\') || reference.starts_with('/') {
        bail!("frame '{reference}' must be a safe bundle-relative path using '/' separators");
    }
    for component in reference.split('/') {
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.ends_with('.')
            || component.ends_with(' ')
            || invalid_windows_chars(component)
            || windows_reserved(component)
        {
            bail!("frame '{reference}' must be a safe bundle-relative path using '/' separators");
        }
    }
    Ok(())
}

fn is_kebab(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

fn invalid_windows_chars(value: &str) -> bool {
    value
        .chars()
        .any(|c| c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
}

fn windows_reserved(value: &str) -> bool {
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || reserved_numbered_name(&stem, "COM")
        || reserved_numbered_name(&stem, "LPT")
}

fn reserved_numbered_name(stem: &str, prefix: &str) -> bool {
    stem.strip_prefix(prefix).is_some_and(|suffix| {
        matches!(
            suffix,
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
        )
    })
}
