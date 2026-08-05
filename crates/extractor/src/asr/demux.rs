//! Audio demux: media file → mono 16 kHz wav (what whisper.cpp expects).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::tools::Toolchain;

pub fn to_wav(tools: &Toolchain, media: &Path, bundle_dir: &Path) -> Result<PathBuf> {
    let wav = bundle_dir.join("audio-16k.wav");
    let output = std::process::Command::new(&tools.ffmpeg)
        .args(["-y", "-v", "error", "-i"])
        .arg(media)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "wav"])
        .arg(&wav)
        .output()
        .context("running ffmpeg for audio demux")?;
    if !output.status.success() {
        bail!(
            "audio demux failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(wav)
}
