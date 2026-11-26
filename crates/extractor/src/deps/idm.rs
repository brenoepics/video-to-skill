//! Installing the opt-in inverse-dynamics weights: a model is a directory
//! of pinned files (`registry::idm`), each downloaded, checksum-verified,
//! then promoted with its own `.verified` marker. A model is ready only
//! when every file carries a marker for its pin, so an interrupted 16 GB
//! download resumes file by file and never counts as installed.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::bootstrap::{installed_from_artifact, remove_file_if_exists, temporary_path, Fetcher};
use super::install_lock::InstallLock;
use super::registry::{IdmFile, IdmModel};
use super::verification_marker;

/// Where `model` lives inside the app data dir.
#[must_use]
pub fn model_dir(model: &IdmModel, data_dir: &Path) -> PathBuf {
    data_dir
        .join("models")
        .join("videoagenttrek")
        .join(model.dir_name())
}

/// True when every pinned file is installed and verified.
#[must_use]
pub fn model_ready(model: &IdmModel, data_dir: &Path) -> bool {
    let dir = model_dir(model, data_dir);
    model
        .files
        .iter()
        .all(|file| installed_from_artifact(&dir.join(file.name), file.sha256))
}

/// Download and verify every file of `model` that is not installed yet;
/// returns the model directory.
pub fn install_model(model: &IdmModel, fetcher: &dyn Fetcher, data_dir: &Path) -> Result<PathBuf> {
    let dir = model_dir(model, data_dir);
    fs::create_dir_all(&dir)
        .with_context(|| format!("creating model directory {}", dir.display()))?;
    for file in model.files {
        let dest = dir.join(file.name);
        if installed_from_artifact(&dest, file.sha256) {
            continue;
        }
        let _lock = InstallLock::acquire(data_dir)?;
        install_file(&model.url(file), file, fetcher, &dest)
            .with_context(|| format!("installing {} {}", model.repo, file.name))?;
    }
    Ok(dir)
}

fn install_file(url: &str, file: &IdmFile, fetcher: &dyn Fetcher, dest: &Path) -> Result<()> {
    let download = temporary_path(dest, "download");
    remove_file_if_exists(&download)?;
    let result =
        fetch_verified(url, file, fetcher, &download).and_then(|()| promote(&download, dest, file));
    let _ = remove_file_if_exists(&download);
    result
}

fn fetch_verified(url: &str, file: &IdmFile, fetcher: &dyn Fetcher, download: &Path) -> Result<()> {
    fetcher
        .fetch(url, download)
        .with_context(|| format!("downloading {url}"))?;
    let actual = crate::hashing::sha256_hex_of_file(download)?;
    if !actual.eq_ignore_ascii_case(file.sha256) {
        bail!(
            "checksum mismatch for {}: expected {}, got {actual}",
            file.name,
            file.sha256
        );
    }
    Ok(())
}

/// Replace any previous copy (Windows cannot rename over a file), then
/// write the marker last so the probe never sees a half-promoted file.
fn promote(download: &Path, dest: &Path, file: &IdmFile) -> Result<()> {
    let marker = verification_marker(dest);
    remove_file_if_exists(&marker)?;
    remove_file_if_exists(dest)?;
    fs::rename(download, dest)?;
    crate::fsx::write_atomic(&marker, format!("sha256:{}\n", file.sha256).as_bytes())?;
    Ok(())
}
