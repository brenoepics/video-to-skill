//! Checksum-verified install: fetch to a partial file, verify, then
//! atomically promote. The `.verified` marker is written last, so a crash
//! at any point leaves either nothing or a complete, verified tool.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::{verification_marker, Tool};

/// How a downloaded artifact is packaged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Archive {
    /// The download is the tool itself.
    None,
    /// A zip whose named member is the tool (ffmpeg builds ship zipped).
    Zip { member: String },
}

/// Everything needed to install one tool: where from, and the expected
/// digest of the download.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub tool: Tool,
    pub url: String,
    /// Hex sha256 of the downloaded artifact (pre-extraction).
    pub sha256: String,
    pub archive: Archive,
}

/// Transport seam — the only place that touches the network, so tests
/// substitute canned bytes and failures.
pub trait Fetcher {
    fn fetch(&self, url: &str, dest: &Path) -> Result<()>;
}

/// Download, verify, and install `spec` into `data_dir`.
/// Returns the final tool path. Safe to re-run after any failure.
pub fn install(spec: &ToolSpec, fetcher: &dyn Fetcher, data_dir: &Path) -> Result<PathBuf> {
    let final_path = spec.tool.install_path(data_dir);
    let dir = final_path
        .parent()
        .context("install path has no parent directory")?;
    fs::create_dir_all(dir)?;

    let partial = final_path.with_extension("partial");
    remove_if_exists(&partial)?;

    let outcome = fetch_verify_promote(spec, fetcher, &partial, &final_path);
    if outcome.is_err() {
        // Restartable: never leave a partial or an unverified final behind.
        remove_if_exists(&partial)?;
        remove_if_exists(&final_path)?;
    }
    outcome?;
    Ok(final_path)
}

fn fetch_verify_promote(
    spec: &ToolSpec,
    fetcher: &dyn Fetcher,
    partial: &Path,
    final_path: &Path,
) -> Result<()> {
    fetcher
        .fetch(&spec.url, partial)
        .with_context(|| format!("downloading {}", spec.url))?;

    let actual = crate::hashing::sha256_hex_of_file(partial)?;
    if !actual.eq_ignore_ascii_case(&spec.sha256) {
        bail!(
            "checksum mismatch for {}: expected {}, got {actual}",
            spec.tool.file_name(),
            spec.sha256
        );
    }

    match &spec.archive {
        Archive::None => {}
        Archive::Zip { member } => extract_zip_member(partial, member)?,
    }

    if spec.tool.is_executable() {
        make_executable(partial)?;
    }
    fs::rename(partial, final_path)?;
    fs::write(
        verification_marker(final_path),
        format!("sha256:{actual}\n"),
    )?;
    Ok(())
}

/// Replace the zip at `path` with its extracted `member`, in place.
/// Uses the system `unzip` (bundled with macOS) — consistent with the
/// project's orchestrate-binaries approach, and spares a zip dependency.
fn extract_zip_member(path: &Path, member: &str) -> Result<()> {
    let staging = path.with_extension("unzip");
    remove_dir_if_exists(&staging)?;
    fs::create_dir_all(&staging)?;
    let status = std::process::Command::new("unzip")
        .arg("-o")
        .arg(path)
        .arg(member)
        .arg("-d")
        .arg(&staging)
        .output()
        .context("running system unzip")?;
    if !status.status.success() {
        bail!("unzip failed: {}", String::from_utf8_lossy(&status.stderr));
    }
    let extracted = staging.join(member);
    fs::rename(&extracted, path).context("promoting extracted archive member")?;
    remove_dir_if_exists(&staging)?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}
