//! Resolved locations of the external binaries the pipeline shells out to.

use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::deps::{self, Env, Presence, Tool};

/// Paths to the binaries a pipeline run needs. Tests construct this
/// directly; production resolves it from the environment.
#[derive(Debug, Clone)]
pub struct Toolchain {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    /// Whisper weights; optional because ingest-only flows don't need them.
    pub whisper_model: Option<PathBuf>,
}

impl Toolchain {
    /// Resolve from PATH + app data dir; errors name what's missing and
    /// how to fix it.
    pub fn resolve(env: &Env) -> Result<Toolchain> {
        let statuses = deps::probe(env);
        let locate = |tool: Tool| -> Result<PathBuf> {
            statuses
                .iter()
                .find(|s| s.tool == tool && s.presence != Presence::Missing)
                .and_then(|s| s.location.clone())
                .map_or_else(
                    || {
                        bail!(
                            "{} is missing — run `vts-extract check --fix` first",
                            tool.file_name()
                        )
                    },
                    Ok,
                )
        };
        Ok(Toolchain {
            ffmpeg: locate(Tool::Ffmpeg)?,
            ffprobe: locate(Tool::Ffprobe)?,
            whisper_model: locate(Tool::WhisperModel).ok(),
        })
    }
}
