//! Rendering the red-team fixtures at test time: ffmpeg `drawtext` for
//! on-screen text, macOS `say` for narration, then the real ingest and
//! frame track so the IR can cite frames that exist. Nothing binary is
//! committed; a machine without the tools skips the video and says so.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::bundle::ingest;
use vts_extract::frames::{extract_frame_track, FrameConfig};
use vts_extract::tools::Toolchain;

use super::fixtures::{wrap, Channel, Fixture, Layout};

const SIZE: &str = "640x360";
const SECS: u32 = 6;

pub struct Rendered {
    pub bundle: PathBuf,
    /// Bundle-relative path of the first keyframe.
    pub frame: String,
    pub has_audio: bool,
}

fn toolchain() -> Toolchain {
    Toolchain {
        ffmpeg: PathBuf::from("ffmpeg"),
        ffprobe: PathBuf::from("ffprobe"),
        whisper_model: None,
        whisper_vad: None,
    }
}

/// Render the fixture into `dir`, ingest it and extract its frame
/// track. `Err` names what the machine lacks (ffmpeg with `drawtext`
/// and a monospace font, or `say`): the caller skips the video and
/// still runs every compiler assertion.
pub fn render(fixture: &Fixture, dir: &Path) -> Result<Rendered, String> {
    let video = dir.join(format!("{}.mp4", fixture.name));
    match fixture.channel {
        Channel::Screen(layout) => {
            drawtext_available()?;
            draw(fixture, layout, dir, &video)?;
        }
        Channel::Speech => speak(fixture, dir, &video)?,
    }
    let bundle = dir.join("bundle");
    let report = ingest(&video, &bundle, &toolchain()).map_err(|e| e.to_string())?;
    let track = extract_frame_track(&bundle, &toolchain(), &FrameConfig::default())
        .map_err(|e| e.to_string())?;
    let frame = track
        .shots
        .first()
        .map(|shot| shot.keyframe.path.clone())
        .ok_or("no keyframe extracted")?;
    Ok(Rendered {
        bundle,
        frame,
        has_audio: report.manifest.media.has_audio,
    })
}

/// A one-frame probe with the same font spec the fixtures use: the
/// Homebrew ffmpeg ships without `drawtext`, and a build with it still
/// needs fontconfig to resolve `monospace`.
fn drawtext_available() -> Result<(), String> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("color=c=black:s=64x64:r=1:d=1")
        .args([
            "-vf",
            "drawtext=text=probe:font=monospace",
            "-frames:v",
            "1",
        ])
        .args(["-f", "null", "-"])
        .output()
        .map_err(|e| format!("ffmpeg not runnable: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "ffmpeg cannot draw text: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Text lives in files, so no character of the injection needs
/// filtergraph escaping; ffmpeg runs inside `dir` to keep the paths
/// relative (a `C:` drive letter would otherwise need escaping too).
fn draw(fixture: &Fixture, layout: Layout, dir: &Path, video: &Path) -> Result<(), String> {
    let text = |name: &str, body: &str| -> Result<(), String> {
        fs::write(dir.join(name), body).map_err(|e| e.to_string())
    };
    let mono = "font=monospace";
    let (background, graph) = match layout {
        Layout::StickyNote => {
            text("legit.txt", fixture.legit)?;
            text("note.txt", &wrap(fixture.injected, 36))?;
            ("0x2b2b2b", format!(
                "drawtext=textfile=legit.txt:{mono}:fontsize=18:fontcolor=0xd0d0d0:x=24:y=24:line_spacing=8,\
                 drawbox=x=300:y=60:w=330:h=150:color=0xf5e663:t=fill,\
                 drawtext=textfile=note.txt:{mono}:fontsize=14:fontcolor=black:x=312:y=80:line_spacing=6"
            ))
        }
        Layout::Banner => {
            text("legit.txt", fixture.legit)?;
            text("banner.txt", &wrap(fixture.injected, 44))?;
            ("0x1e1e2e", format!(
                "drawtext=textfile=legit.txt:{mono}:fontsize=28:fontcolor=white:x=24:y=180,\
                 drawbox=x=0:y=0:w=iw:h=110:color=0xc0392b:t=fill:enable='between(t,2,4)',\
                 drawtext=textfile=banner.txt:{mono}:fontsize=16:fontcolor=white:x=20:y=28:line_spacing=6:enable='between(t,2,4)'"
            ))
        }
        Layout::EditorComment => {
            let mut lines: Vec<&str> = fixture.legit.lines().collect();
            lines.insert(2, fixture.injected);
            let numbered: Vec<String> = lines
                .iter()
                .enumerate()
                .map(|(i, line)| format!("{:>2}  {line}", i + 1))
                .collect();
            text("editor.txt", &numbered.join("\n"))?;
            ("0x1e1e1e", format!(
                "drawbox=x=0:y=0:w=44:h=ih:color=0x252526:t=fill,\
                 drawtext=textfile=editor.txt:{mono}:fontsize=15:fontcolor=0xd4d4d4:x=12:y=20:line_spacing=10"
            ))
        }
        Layout::TerminalOutput => {
            text(
                "terminal.txt",
                &format!("{}\n{}\n$ ", fixture.legit, fixture.injected),
            )?;
            ("black", format!(
                "drawtext=textfile=terminal.txt:{mono}:fontsize=16:fontcolor=0x33ff33:x=16:y=16:line_spacing=8"
            ))
        }
        Layout::Perturbed => {
            text("legit.txt", fixture.legit)?;
            text("faint.txt", fixture.injected)?;
            (
                "0x1e1e2e",
                format!(
                    "drawtext=textfile=legit.txt:{mono}:fontsize=28:fontcolor=white:x=24:y=120,\
                 drawtext=textfile=faint.txt:{mono}:fontsize=14:fontcolor=white@0.08:x=40:y=250,\
                 noise=alls=10:allf=t"
                ),
            )
        }
    };
    let source = format!("color=c={background}:s={SIZE}:r=10:d={SECS}");
    run(Command::new("ffmpeg")
        .current_dir(dir)
        .args(["-y", "-v", "error", "-f", "lavfi", "-i", &source])
        .args(["-vf", &graph, "-c:v", "mpeg4"])
        .arg(video))
}

fn speak(fixture: &Fixture, dir: &Path, video: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("narration needs macOS `say`".into());
    }
    let aiff = dir.join("speech.aiff");
    run(Command::new("say")
        .arg("-o")
        .arg(&aiff)
        .arg(fixture.injected))?;
    let source = format!("color=c=black:s={SIZE}:r=10");
    run(Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "lavfi", "-i", &source, "-i"])
        .arg(&aiff)
        .args(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
        .arg(video))
}

fn run(command: &mut Command) -> Result<(), String> {
    let out = command
        .output()
        .map_err(|e| format!("{} not runnable: {e}", command.get_program().display()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} failed: {}",
            command.get_program().display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}
