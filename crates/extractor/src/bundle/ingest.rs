//! Local-file ingest: validate the input, probe it, and lay down the
//! bundle skeleton. Idempotent: re-running on the same source reuses the
//! bundle; a different source is refused rather than overwritten.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use super::{load_manifest, probe_media, save_manifest, Manifest, SourceInfo, Tracks};
use crate::tools::Toolchain;

pub const SUPPORTED_CONTAINERS: &[&str] = &["mp4", "mkv", "webm", "mov"];

#[derive(Debug)]
pub struct IngestReport {
    pub manifest: Manifest,
    /// False when an existing bundle for the same source was reused.
    pub created: bool,
}

/// Ingest a local video file into `bundle_dir`.
pub fn ingest(input: &Path, bundle_dir: &Path, tools: &Toolchain) -> Result<IngestReport> {
    if !input.is_file() {
        bail!("'{}' not found — check the path", input.display());
    }
    let container = container_of(input)?;
    let sha256 = crate::hashing::sha256_hex_of_file(input)?;

    if bundle_dir.join(super::MANIFEST_FILE).is_file() {
        return reuse_existing(input, bundle_dir, &sha256);
    }

    let media = probe_media::probe(&tools.ffprobe, input, container)?;
    let manifest = Manifest {
        schema_version: 1,
        source: SourceInfo {
            original: input.display().to_string(),
            media_path: fs::canonicalize(input)?.display().to_string(),
            sha256,
            title: None,
        },
        media,
        tracks: Tracks::default(),
        notes: Vec::new(),
    };
    save_manifest(bundle_dir, &manifest)?;
    Ok(IngestReport {
        manifest,
        created: true,
    })
}

fn reuse_existing(input: &Path, bundle_dir: &Path, sha256: &str) -> Result<IngestReport> {
    let existing = load_manifest(bundle_dir)
        .with_context(|| format!("'{}' holds a damaged bundle", bundle_dir.display()))?;
    if existing.source.sha256 != sha256 {
        bail!(
            "'{}' already holds a bundle for a different video ({}) — choose another output directory",
            bundle_dir.display(),
            existing.source.original,
        );
    }
    let _ = input;
    Ok(IngestReport {
        manifest: existing,
        created: false,
    })
}

fn container_of(input: &Path) -> Result<&'static str> {
    let ext = input
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    SUPPORTED_CONTAINERS
        .iter()
        .find(|c| **c == ext)
        .copied()
        .with_context(|| {
            format!(
                "Unsupported format '{ext}' — supported: {}",
                SUPPORTED_CONTAINERS.join(", ")
            )
        })
}
