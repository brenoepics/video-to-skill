//! Stage the current package before removing obsolete files from the old one.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};

use super::{cleanup, ProcedureIr};

static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn emit(ir: &ProcedureIr, bundle_dir: &Path, out_dir: &Path) -> Result<()> {
    let obsolete = cleanup::obsolete_files(ir, out_dir)?;
    let current = cleanup::owned_files(ir)?;
    let metadata = [Path::new("SKILL.md"), Path::new("provenance.json")];
    for path in current.iter().map(PathBuf::as_path).chain(metadata) {
        cleanup::check_parents(out_dir, path)?;
    }
    let staging = Staging::new(out_dir)?;
    super::emit::emit(ir, bundle_dir, &staging.path)?;
    // Remove before promotion: old and new spellings can address the same
    // file on a case-insensitive filesystem. Sources inside the old package
    // have already been copied into staging, so cleanup cannot erase inputs.
    cleanup::remove_obsolete(out_dir, &obsolete)?;
    for path in current.iter().map(PathBuf::as_path).chain(metadata) {
        let destination = out_dir.join(path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(staging.path.join(path), &destination)
            .with_context(|| format!("promoting compiled file {}", path.display()))?;
    }
    Ok(())
}

struct Staging {
    path: PathBuf,
}

impl Staging {
    fn new(out_dir: &Path) -> Result<Self> {
        fs::create_dir_all(out_dir)?;
        let root = fs::canonicalize(out_dir).context("resolving package directory")?;
        let parent = root
            .parent()
            .context("package directory has no parent for staging")?;
        loop {
            let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            // A killed process bypasses Drop: keep any orphan outside the
            // installable tree, beside the canonical package directory.
            let path = parent.join(format!(".compile-{}-{sequence}.tmp", std::process::id()));
            #[cfg(unix)]
            let created = {
                use std::os::unix::fs::DirBuilderExt;
                fs::DirBuilder::new().mode(0o700).create(&path)
            };
            #[cfg(not(unix))]
            let created = fs::create_dir(&path);
            match created {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("creating package staging directory"),
            }
        }
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        // Only a private directory successfully created above is removed;
        // no unrelated files in the package are enumerated or cleaned up.
        let _ = fs::remove_dir_all(&self.path);
    }
}
