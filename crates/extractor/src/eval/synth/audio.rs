//! Narration for gold videos: each cue is spoken by macOS `say` into
//! its own file, then every clip is delayed to its cue time and mixed
//! into one track. Elsewhere (or without `say`) the video gets silence,
//! and the caller reports transcript metrics as not scored. Each `say`
//! runs under a deadline: it can hang after writing its clip.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::eval::recipe::Recipe;
use crate::tools::Toolchain;

/// Pinned so every machine narrates with the same voice and pace.
const VOICE: &str = "Samantha";
const WORDS_PER_MINUTE: &str = "175";
/// `say -o` has been seen to hang after writing its file in full; a cue
/// is a sentence or two, so a run alive this long is killed.
const SAY_DEADLINE: Duration = Duration::from_secs(60);
const SAY_POLL: Duration = Duration::from_millis(50);

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
    let mut pinned = Command::new("say");
    pinned
        .args(["-v", VOICE, "-r", WORDS_PER_MINUTE, "-o"])
        .arg(clip)
        .arg(text);
    if spoke(&mut pinned, clip, SAY_DEADLINE)? {
        return Ok(());
    }
    // The pinned voice is missing on this machine: use the default one.
    let mut fallback = Command::new("say");
    fallback
        .args(["-r", WORDS_PER_MINUTE, "-o"])
        .arg(clip)
        .arg(text);
    if spoke(&mut fallback, clip, SAY_DEADLINE)? {
        return Ok(());
    }
    bail!("`say` could not speak {text:?} into {}", clip.display())
}

/// Run one `say` and report whether `clip` came out of it. A run still
/// alive at `deadline` is killed; its clip counts when it was written
/// before the hang (the duration probe rejects a truncated one).
fn spoke(say: &mut Command, clip: &Path, deadline: Duration) -> Result<bool> {
    let _ = fs::remove_file(clip);
    Ok(match run_within(say, deadline)? {
        Some(status) => status.success(),
        None => fs::metadata(clip).is_ok_and(|m| m.len() > 0),
    })
}

/// Run `command` to completion, or kill it at `deadline` (`None`).
fn run_within(command: &mut Command, deadline: Duration) -> Result<Option<ExitStatus>> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("running {}", command.get_program().to_string_lossy()))?;
    let until = Instant::now() + deadline;
    loop {
        if let Some(status) = child.try_wait().context("waiting for a subprocess")? {
            return Ok(Some(status));
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(SAY_POLL);
    }
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

#[cfg(all(test, unix))]
mod deadline_tests {
    use std::process::Command;
    use std::time::{Duration, Instant};

    use super::{run_within, spoke};

    #[test]
    fn a_command_that_finishes_reports_its_status() {
        let status = run_within(&mut Command::new("true"), Duration::from_secs(5)).unwrap();
        assert!(status.is_some_and(|s| s.success()));
    }

    #[test]
    fn a_command_past_its_deadline_is_killed() {
        let started = Instant::now();
        let status =
            run_within(Command::new("sleep").arg("30"), Duration::from_millis(200)).unwrap();
        assert!(status.is_none());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_hung_say_still_counts_when_its_clip_was_written() {
        let dir = std::env::temp_dir().join(format!("vts-say-deadline-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("cue.aiff");

        let mut hung = Command::new("sh");
        hung.arg("-c")
            .arg("printf audio > \"$0\"; sleep 30")
            .arg(&clip);
        let mut silent = Command::new("sh");
        silent.arg("-c").arg("sleep 30");

        assert!(spoke(&mut hung, &clip, Duration::from_millis(300)).unwrap());
        assert!(!spoke(&mut silent, &clip, Duration::from_millis(300)).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
