//! Checksum-verified, restartable dependency installation.
//!
//! Downloads and extracted files are staged beside their destinations. The
//! verified marker is promoted last, so the probe never observes a partial
//! install as ready.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{bail, Context, Result};

use super::{archive, install_lock::InstallLock, recovery, transaction, verification_marker, Tool};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A tool contained in a ZIP artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveMember {
    pub tool: Tool,
    /// Exact, forward-slash-separated path stored in the archive.
    pub path: String,
}

/// How a downloaded artifact is packaged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Archive {
    /// The download is the tool itself.
    None,
    /// One ZIP can install multiple tools, such as `FFmpeg` and `ffprobe`.
    Zip { members: Vec<ArchiveMember> },
}

/// Everything needed to install one tool from a pinned artifact.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub tool: Tool,
    pub url: String,
    /// Hex SHA-256 of the downloaded artifact, before extraction.
    pub sha256: String,
    pub archive: Archive,
}

impl ToolSpec {
    /// Tools installed by this artifact, in archive order.
    #[must_use]
    pub fn provided_tools(&self) -> Vec<Tool> {
        match &self.archive {
            Archive::None => vec![self.tool],
            Archive::Zip { members } => members.iter().map(|member| member.tool).collect(),
        }
    }

    pub(crate) fn is_shared_archive(&self) -> bool {
        matches!(&self.archive, Archive::Zip { members } if members.len() > 1)
    }

    pub(crate) fn install_path_for(&self, tool: Tool, data_dir: &Path) -> PathBuf {
        if self.is_shared_archive() {
            artifact_directory(data_dir, &self.sha256).join(tool.file_name())
        } else {
            tool.install_path(data_dir)
        }
    }
}

/// Transport seam: tests supply deterministic bytes and failures.
pub trait Fetcher {
    fn fetch(&self, url: &str, dest: &Path) -> Result<()>;
}

/// Download, verify, and install `spec` into `data_dir`.
///
/// Reinstalling is safe on Windows, where `rename` cannot replace an existing
/// file. A failed download or promotion preserves the previous verified tool.
pub fn install(spec: &ToolSpec, fetcher: &dyn Fetcher, data_dir: &Path) -> Result<PathBuf> {
    validate_spec(spec)?;
    let _install_lock = InstallLock::acquire(data_dir)?;
    let requested_path = spec.install_path_for(spec.tool, data_dir);
    if artifact_is_ready(spec, data_dir) {
        return Ok(requested_path);
    }
    let quarantined = if spec.is_shared_archive() {
        recovery::quarantine_incomplete_shared(spec, data_dir)?
    } else {
        None
    };
    if artifact_is_ready(spec, data_dir) {
        cleanup_quarantine(quarantined.as_deref());
        return Ok(requested_path);
    }

    let staging_anchor = if spec.is_shared_archive() {
        spec.tool.install_path(data_dir)
    } else {
        requested_path.clone()
    };
    let install_dir = staging_anchor
        .parent()
        .context("install path has no parent directory")?;
    fs::create_dir_all(install_dir)?;
    let download = temporary_path(&staging_anchor, "download");
    remove_file_if_exists(&download)?;

    let result = prepare_and_promote(spec, fetcher, data_dir, &download);
    let _ = remove_file_if_exists(&download);
    if let Err(error) = result {
        return match quarantined {
            Some(path) => match fs::remove_dir_all(&path) {
                Ok(()) => Err(error.context("discarded the incomplete previous artifact")),
                Err(cleanup) => Err(error.context(format!(
                    "also failed to remove incomplete artifact {}: {cleanup}",
                    path.display()
                ))),
            },
            None => Err(error),
        };
    }

    if !artifact_is_ready(spec, data_dir) {
        bail!(
            "installation finished without a verified {}",
            spec.tool.file_name()
        );
    }
    cleanup_quarantine(quarantined.as_deref());
    Ok(requested_path)
}

fn cleanup_quarantine(path: Option<&Path>) {
    if let Some(path) = path {
        let _ = fs::remove_dir_all(path);
    }
}

fn prepare_and_promote(
    spec: &ToolSpec,
    fetcher: &dyn Fetcher,
    data_dir: &Path,
    download: &Path,
) -> Result<()> {
    fetcher
        .fetch(&spec.url, download)
        .with_context(|| format!("downloading {}", spec.url))?;
    let actual = crate::hashing::sha256_hex_of_file(download)?;
    if !actual.eq_ignore_ascii_case(&spec.sha256) {
        bail!(
            "checksum mismatch for {}: expected {}, got {actual}",
            spec.tool.file_name(),
            spec.sha256
        );
    }

    let staged = match &spec.archive {
        Archive::None => {
            if spec.tool.is_executable() {
                make_executable(download)?;
            }
            vec![transaction::StagedTool {
                tool: spec.tool,
                path: download.to_path_buf(),
            }]
        }
        Archive::Zip { members } => archive::extract(download, members, data_dir)?,
    };
    let result = transaction::promote(&staged, &actual, data_dir, spec.is_shared_archive());
    for item in staged {
        let _ = remove_file_if_exists(&item.path);
    }
    result
}

fn validate_spec(spec: &ToolSpec) -> Result<()> {
    if spec.sha256.len() != 64 || !spec.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid SHA-256 pin for {}", spec.tool.file_name());
    }
    if let Archive::Zip { members } = &spec.archive {
        if !members.iter().any(|member| member.tool == spec.tool) {
            bail!(
                "ZIP specification does not contain {}",
                spec.tool.file_name()
            );
        }
        for (index, member) in members.iter().enumerate() {
            if members[..index]
                .iter()
                .any(|previous| previous.tool == member.tool)
            {
                bail!("ZIP specification contains duplicate tool entries");
            }
        }
    }
    Ok(())
}

pub(crate) fn installed_from_artifact(path: &Path, expected_sha256: &str) -> bool {
    if !path.is_file() {
        return false;
    }
    marker_matches(&verification_marker(path), expected_sha256)
}

pub(crate) fn marker_matches(path: &Path, expected_sha256: &str) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|marker| marker.trim().strip_prefix("sha256:").map(str::to_owned))
        .is_some_and(|digest| digest.eq_ignore_ascii_case(expected_sha256))
}

pub(crate) fn artifact_is_ready(spec: &ToolSpec, data_dir: &Path) -> bool {
    if spec.is_shared_archive() {
        let directory = artifact_directory(data_dir, &spec.sha256);
        return marker_matches(&artifact_marker(&directory), &spec.sha256)
            && spec
                .provided_tools()
                .into_iter()
                .all(|tool| spec.install_path_for(tool, data_dir).is_file());
    }
    installed_from_artifact(&spec.install_path_for(spec.tool, data_dir), &spec.sha256)
}

pub(crate) fn artifact_directory(data_dir: &Path, sha256: &str) -> PathBuf {
    data_dir
        .join("tools")
        .join("artifacts")
        .join(sha256.to_ascii_lowercase())
}

pub(crate) fn artifact_marker(directory: &Path) -> PathBuf {
    directory.join("artifact.verified")
}

pub(crate) fn temporary_path(destination: &Path, role: &str) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = destination
        .file_name()
        .map_or_else(|| "tool".into(), |name| name.to_string_lossy());
    destination.with_file_name(format!(
        ".{name}.{}.{}.{}",
        std::process::id(),
        sequence,
        role
    ))
}

pub(crate) fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(unix)]
pub(crate) fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn make_executable(path: &Path) -> Result<()> {
    path.metadata().map(|_| ()).map_err(Into::into)
}
