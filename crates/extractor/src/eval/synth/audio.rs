//! Narration for gold videos: each cue is spoken by macOS `say` into
//! its own file, then every clip is delayed to its cue time and mixed
//! into one track. Elsewhere (or without `say`) the video gets silence,
//! and the caller reports transcript metrics as not scored.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::eval::recipe::Recipe;
use crate::tools::Toolchain;

/// Pinned so every machine narrates with the same voice and pace.
const VOICE: &str = "Samantha";
const WORDS_PER_MINUTE: &str = "175";

/// Spoken cues ready to mix: one clip per cue with its delay.
#[derive(Debug, Clone, Default)]
pub struct Narration {
    pub clips: Vec<PathBuf>,
    pub delays_ms: Vec<u64>,
}

impl Narration {
    /// Whether any cue was actually spoken.
    #[must_use]
    pub fn is_available(&self) -> bool {
        !self.clips.is_empty()
    }
}

/// Speak every cue of `recipe` into `work`. Returns an empty narration
/// when `say` is unavailable; fails when a cue runs into the next one.
pub fn narrate(recipe: &Recipe, tools: &Toolchain, work: &Path) -> Result<Narration> {
    if recipe.narration.is_empty() || !say_available() {
        return Ok(Narration::default());
    }
    let mut narration = Narration::default();
    for (idx, cue) in recipe.narration.iter().enumerate() {
        let clip = work.join(format!("cue{idx:03}.aiff"));
        speak(&cue.text, &clip)?;
        let spoken_secs = probe_duration_secs(tools, &clip)?;
        let next = recipe
            .narration
            .get(idx + 1)
            .map_or(recipe.duration_secs, |c| c.t);
        if cue.t + spoken_secs > next + 0.05 {
            bail!(
                "recipe {}: cue at {}s takes {spoken_secs:.1}s and runs into the next cue at {next}s",
                recipe.id,
                cue.t
            );
        }
        narration.clips.push(clip);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // validated ≥ 0
        narration.delays_ms.push((cue.t * 1000.0).round() as u64);
    }
    Ok(narration)
}

/// ffmpeg filtergraph mixing inputs `first_input..` (one per delay) into
/// `[a]`, padded with silence so the track outlasts the video.
#[must_use]
pub fn mix_filter(delays_ms: &[u64], first_input: usize) -> String {
    // Writing into a String cannot fail.
    let mut graph = String::new();
    let mut labels = String::new();
    for (idx, delay) in delays_ms.iter().enumerate() {
        let _ = write!(
            graph,
            "[{}:a]adelay={delay}|{delay}[d{idx}];",
            first_input + idx
        );
        let _ = write!(labels, "[d{idx}]");
    }
    if delays_ms.len() == 1 {
        graph.push_str("[d0]apad[a]");
    } else {
        let _ = write!(
            graph,
            "{labels}amix=inputs={}:duration=longest:normalize=0,apad[a]",
            delays_ms.len()
        );
    }
    graph
}

fn say_available() -> bool {
    cfg!(target_os = "macos")
        && Command::new("say")
            .arg("-v")
            .arg("?")
            .output()
            .is_ok_and(|o| o.status.success())
}

fn speak(text: &str, clip: &Path) -> Result<()> {
    let pinned = Command::new("say")
        .args(["-v", VOICE, "-r", WORDS_PER_MINUTE, "-o"])
        .arg(clip)
        .arg(text)
        .output()
        .context("running macOS `say`")?;
    if pinned.status.success() {
        return Ok(());
    }
    // The pinned voice is missing on this machine: use the default one.
    let fallback = Command::new("say")
        .args(["-r", WORDS_PER_MINUTE, "-o"])
        .arg(clip)
        .arg(text)
        .output()
        .context("running macOS `say`")?;
    if !fallback.status.success() {
        bail!(
            "`say` failed: {}",
            String::from_utf8_lossy(&fallback.stderr)
        );
    }
    Ok(())
}

fn probe_duration_secs(tools: &Toolchain, media: &Path) -> Result<f64> {
    let output = Command::new(&tools.ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(media)
        .output()
        .context("running ffprobe on a narration clip")?;
    if !output.status.success() {
        bail!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .context("ffprobe printed no duration")
}

#[cfg(test)]
mod tests {
    use super::mix_filter;

    #[test]
    fn two_cues_are_delayed_mixed_and_padded() {
        assert_eq!(
            mix_filter(&[1000, 8000], 1),
            "[1:a]adelay=1000|1000[d0];[2:a]adelay=8000|8000[d1];\
             [d0][d1]amix=inputs=2:duration=longest:normalize=0,apad[a]"
        );
    }

    #[test]
    fn a_single_cue_skips_the_mixer() {
        assert_eq!(mix_filter(&[500], 1), "[1:a]adelay=500|500[d0];[d0]apad[a]");
    }
}
