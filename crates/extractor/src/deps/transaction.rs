//! Same-directory transactional promotion for managed tools.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::bootstrap::{
    artifact_directory, artifact_marker, marker_matches, remove_file_if_exists, temporary_path,
};
use super::{verification_marker, Tool};

#[derive(Debug)]
pub(crate) struct StagedTool {
    pub(crate) tool: Tool,
    pub(crate) path: PathBuf,
}

struct Promotion {
    staged: PathBuf,
    final_path: PathBuf,
    staged_marker: PathBuf,
    final_marker: PathBuf,
    backup: PathBuf,
    backup_marker: PathBuf,
    binary_state: PromotionState,
    marker_state: PromotionState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PromotionState {
    Absent,
    Present,
    BackedUp,
    Promoted,
}

pub(crate) fn promote(
    staged: &[StagedTool],
    sha256: &str,
    data_dir: &Path,
    shared_archive: bool,
) -> Result<()> {
    if shared_archive {
        return promote_shared(staged, sha256, data_dir);
    }
    let mut promotions = prepare_promotions(staged, sha256, data_dir)?;
    if let Err(error) = apply(&mut promotions) {
        let rollback = rollback(&promotions);
        cleanup_staging(&promotions);
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(error.context(format!(
                "also failed to restore the previous installation: {rollback_error:#}"
            ))),
        };
    }
    cleanup_backups(&promotions)?;
    cleanup_staging(&promotions);
    Ok(())
}

/// Publish every member of one archive with a single directory rename.
/// The content-addressed destination is immutable, so concurrent installers
/// can safely reuse the winner instead of replacing it.
fn promote_shared(staged: &[StagedTool], sha256: &str, data_dir: &Path) -> Result<()> {
    let final_dir = artifact_directory(data_dir, sha256);
    let parent = final_dir
        .parent()
        .context("shared artifact path has no parent")?;
    fs::create_dir_all(parent)?;

    if shared_directory_ready(&final_dir, staged, sha256) {
        return Ok(());
    }
    if final_dir.exists() {
        anyhow::bail!(
            "shared artifact directory is incomplete: {}",
            final_dir.display()
        );
    }

    let staging_dir = create_staging_directory(&final_dir)?;
    let prepared = prepare_shared_directory(&staging_dir, staged, sha256);
    if let Err(error) = prepared {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(error);
    }

    match fs::rename(&staging_dir, &final_dir) {
        Ok(()) => Ok(()),
        Err(_) if shared_directory_ready(&final_dir, staged, sha256) => {
            let _ = fs::remove_dir_all(&staging_dir);
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging_dir);
            Err(error).context("publishing the shared tool artifact")
        }
    }
}

fn create_staging_directory(final_dir: &Path) -> Result<PathBuf> {
    loop {
        let candidate = temporary_path(final_dir, "prepared");
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn prepare_shared_directory(staging_dir: &Path, staged: &[StagedTool], sha256: &str) -> Result<()> {
    for item in staged {
        let destination = staging_dir.join(item.tool.file_name());
        fs::rename(&item.path, &destination).context("staging a shared tool")?;
    }
    write_marker(&artifact_marker(staging_dir), sha256)?;
    Ok(())
}

fn shared_directory_ready(dir: &Path, staged: &[StagedTool], sha256: &str) -> bool {
    marker_matches(&artifact_marker(dir), sha256)
        && staged
            .iter()
            .all(|item| dir.join(item.tool.file_name()).is_file())
}

fn prepare_promotions(
    staged: &[StagedTool],
    sha256: &str,
    data_dir: &Path,
) -> Result<Vec<Promotion>> {
    let mut promotions = Vec::with_capacity(staged.len());
    for item in staged {
        let final_path = item.tool.install_path(data_dir);
        let final_marker = verification_marker(&final_path);
        let staged_marker = temporary_path(&final_marker, "prepared");
        if let Err(error) = write_marker(&staged_marker, sha256) {
            let _ = remove_file_if_exists(&staged_marker);
            cleanup_staging(&promotions);
            return Err(error);
        }
        promotions.push(Promotion {
            staged: item.path.clone(),
            backup: temporary_path(&final_path, "backup"),
            backup_marker: temporary_path(&final_marker, "backup"),
            binary_state: existing_state(&final_path),
            marker_state: existing_state(&final_marker),
            final_path,
            staged_marker,
            final_marker,
        });
    }
    Ok(promotions)
}

fn existing_state(path: &Path) -> PromotionState {
    if path.exists() {
        PromotionState::Present
    } else {
        PromotionState::Absent
    }
}

fn write_marker(path: &Path, sha256: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    writeln!(file, "sha256:{sha256}")?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

fn apply(promotions: &mut [Promotion]) -> Result<()> {
    for promotion in promotions.iter_mut() {
        if promotion.marker_state == PromotionState::Present {
            fs::rename(&promotion.final_marker, &promotion.backup_marker)
                .context("backing up the verification marker")?;
            promotion.marker_state = PromotionState::BackedUp;
        }
    }
    for promotion in promotions.iter_mut() {
        if promotion.binary_state == PromotionState::Present {
            fs::rename(&promotion.final_path, &promotion.backup)
                .context("backing up the installed tool")?;
            promotion.binary_state = PromotionState::BackedUp;
        }
    }
    for promotion in promotions.iter_mut() {
        fs::rename(&promotion.staged, &promotion.final_path)
            .context("promoting the verified tool")?;
        promotion.binary_state = PromotionState::Promoted;
    }
    for promotion in promotions.iter_mut() {
        fs::rename(&promotion.staged_marker, &promotion.final_marker)
            .context("promoting the verification marker")?;
        promotion.marker_state = PromotionState::Promoted;
    }
    Ok(())
}

fn rollback(promotions: &[Promotion]) -> Result<()> {
    for promotion in promotions.iter().rev() {
        if promotion.marker_state == PromotionState::Promoted {
            remove_file_if_exists(&promotion.final_marker)?;
        }
        if promotion.binary_state == PromotionState::Promoted {
            remove_file_if_exists(&promotion.final_path)?;
        }
        if promotion.backup.exists() {
            fs::rename(&promotion.backup, &promotion.final_path)
                .context("restoring the previous tool")?;
        }
        if promotion.backup_marker.exists() {
            fs::rename(&promotion.backup_marker, &promotion.final_marker)
                .context("restoring the previous verification marker")?;
        }
    }
    Ok(())
}

fn cleanup_backups(promotions: &[Promotion]) -> Result<()> {
    for promotion in promotions {
        remove_file_if_exists(&promotion.backup)?;
        remove_file_if_exists(&promotion.backup_marker)?;
    }
    Ok(())
}

fn cleanup_staging(promotions: &[Promotion]) {
    for promotion in promotions {
        let _ = remove_file_if_exists(&promotion.staged);
        let _ = remove_file_if_exists(&promotion.staged_marker);
    }
}
