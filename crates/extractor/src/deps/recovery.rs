//! Recovery of incomplete immutable shared-tool artifacts.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::bootstrap::{
    artifact_directory, artifact_is_ready, artifact_marker, marker_matches, temporary_path,
    ToolSpec,
};

/// Move a corrupt bundle aside before downloading its replacement. A unique
/// sibling keeps recovery atomic and lets concurrent installers reuse a winner.
pub(crate) fn quarantine_incomplete_shared(
    spec: &ToolSpec,
    data_dir: &Path,
) -> Result<Option<PathBuf>> {
    let final_dir = artifact_directory(data_dir, &spec.sha256);
    if !final_dir.exists() || artifact_is_ready(spec, data_dir) {
        return Ok(None);
    }

    loop {
        let quarantine = temporary_path(&final_dir, "incomplete");
        if quarantine.exists() {
            continue;
        }
        match fs::rename(&final_dir, &quarantine) {
            Ok(()) if directory_is_ready(&quarantine, spec) => {
                return restore_concurrent_winner(&final_dir, quarantine, spec, data_dir);
            }
            Ok(()) => return Ok(Some(quarantine)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_error) if artifact_is_ready(spec, data_dir) => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "quarantining incomplete shared artifact {}",
                        final_dir.display()
                    )
                });
            }
        }
    }
}

fn restore_concurrent_winner(
    final_dir: &Path,
    quarantine: PathBuf,
    spec: &ToolSpec,
    data_dir: &Path,
) -> Result<Option<PathBuf>> {
    match fs::rename(&quarantine, final_dir) {
        Ok(()) => Ok(None),
        Err(_error) if artifact_is_ready(spec, data_dir) => {
            let _ = fs::remove_dir_all(quarantine);
            Ok(None)
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "restoring concurrently published shared artifact from {}",
                quarantine.display()
            )
        }),
    }
}

fn directory_is_ready(dir: &Path, spec: &ToolSpec) -> bool {
    marker_matches(&artifact_marker(dir), &spec.sha256)
        && spec
            .provided_tools()
            .into_iter()
            .all(|tool| dir.join(tool.file_name()).is_file())
}
