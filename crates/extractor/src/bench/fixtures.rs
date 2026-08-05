//! Deterministic benchmark fixtures, generated on the fly: macOS `say`
//! speech for the ASR bench, ffmpeg `testsrc` + sine for the pipeline
//! bench. Fixed parameters keep numbers comparable across runs.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::tools::Toolchain;

/// ~45 seconds of narration at `say`'s default speaking rate.
const SPEECH_SCRIPT: &str = "Welcome to this benchmark narration. In this recording we walk \
    through a complete tutorial, step by step, so the transcription engine has realistic \
    speech to work with. First, open the terminal and create a new project directory. \
    Second, initialize the repository and add a readme file describing the goal. Third, \
    install the dependencies and verify that the build completes without warnings. Fourth, \
    write a small failing test, then make it pass with the simplest change that works. \
    Fifth, refactor the code, keeping every test green along the way. Sixth, commit the \
    work with a clear message and push the branch for review. Finally, open a pull \
    request, wait for the checks, and merge once everything passes. That concludes the \
    narrated portion of this benchmark recording. Thank you for listening.";

/// Generate ~45s of spoken narration with macOS `say`, muxed into an mp4.
/// Only meaningful on macOS — callers skip the ASR bench elsewhere.
pub fn speech_mp4(tools: &Toolchain, dir: &Path) -> Result<PathBuf> {
    let aiff = dir.join("speech.aiff");
    run(
        Command::new("say").arg("-o").arg(&aiff).arg(SPEECH_SCRIPT),
        "generating speech with macOS `say`",
    )?;
    let mp4 = dir.join("speech.mp4");
    run(
        Command::new(&tools.ffmpeg)
            .args(["-y", "-v", "error"])
            .args(["-f", "lavfi", "-i", "color=c=black:s=320x240:r=10", "-i"])
            .arg(&aiff)
            .args(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
            .arg(&mp4),
        "muxing generated speech into mp4",
    )?;
    Ok(mp4)
}

/// Generate a `testsrc` video with a sine audio track: 1280x720 @ 30fps
/// for `duration_secs` seconds.
pub fn testsrc_mp4(tools: &Toolchain, dir: &Path, duration_secs: u32) -> Result<PathBuf> {
    let mp4 = dir.join(format!("testsrc-{duration_secs}s.mp4"));
    let video = format!("testsrc=duration={duration_secs}:size=1280x720:rate=30");
    let audio = format!("sine=frequency=440:duration={duration_secs}");
    run(
        Command::new(&tools.ffmpeg)
            .args(["-y", "-v", "error"])
            .args(["-f", "lavfi", "-i", &video])
            .args(["-f", "lavfi", "-i", &audio])
            .args(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
            .arg(&mp4),
        "generating the testsrc benchmark video",
    )?;
    Ok(mp4)
}

fn run(command: &mut Command, what: &str) -> Result<()> {
    let output = command.output().with_context(|| what.to_string())?;
    if !output.status.success() {
        bail!("{what} failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}
