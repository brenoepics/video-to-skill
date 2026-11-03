//! Deterministic gold-video generation: a recipe's stills are rendered
//! to PPM files, strung together with ffmpeg's concat demuxer at the
//! recipe's timings, and muxed with the narration track.

pub mod audio;
pub mod canvas;
pub mod font;
pub mod screens;
pub mod terminal;

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use self::canvas::Canvas;
use self::terminal::TerminalState;
use crate::eval::recipe::{Recipe, Screen};
use crate::tools::Toolchain;

/// A generated gold video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub video: PathBuf,
    /// False when the narration track is silence (`say` unavailable).
    pub narrated: bool,
}

/// Render `recipe` into `work/<id>.mp4`, leaving the stills and clips
/// beside it.
pub fn render_video(recipe: &Recipe, tools: &Toolchain, work: &Path) -> Result<Rendered> {
    recipe.validate()?;
    fs::create_dir_all(work)?;
    let mut names = Vec::new();
    for (idx, canvas) in render_stills(recipe).iter().enumerate() {
        let name = format!("still{idx:03}.ppm");
        fs::write(work.join(&name), canvas.to_ppm())?;
        names.push(name);
    }
    let list = work.join("stills.ffconcat");
    fs::write(&list, concat_list(&names, &recipe.still_durations()))?;
    let narration = audio::narrate(recipe, tools, work)?;

    let video = work.join(format!("{}.mp4", recipe.id));
    let mut command = Command::new(&tools.ffmpeg);
    command
        .args(["-y", "-v", "error", "-f", "concat", "-safe", "0", "-i"])
        .arg(&list);
    if narration.is_available() {
        for clip in &narration.clips {
            command.arg("-i").arg(clip);
        }
        command.args([
            "-filter_complex",
            &audio::mix_filter(&narration.delays_ms, 1),
        ]);
        command.args(["-map", "0:v", "-map", "[a]"]);
    } else {
        command.args(["-f", "lavfi", "-i", "anullsrc=r=22050:cl=mono"]);
        command.args(["-map", "0:v", "-map", "1:a"]);
    }
    let output = command
        .args(["-r", &recipe.fps.to_string(), "-pix_fmt", "yuv420p"])
        .args(["-c:v", "mpeg4", "-q:v", "3", "-c:a", "aac"])
        .args(["-t", &format!("{:.3}", recipe.duration_secs)])
        .arg(&video)
        .output()
        .context("running ffmpeg to render a gold video")?;
    if !output.status.success() {
        bail!(
            "rendering {} failed: {}",
            recipe.id,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(Rendered {
        video,
        narrated: narration.is_available(),
    })
}

/// Every still of `recipe` as a canvas, in order. The terminal persists
/// across stills; other screens are drawn from their own description.
#[must_use]
pub fn render_stills(recipe: &Recipe) -> Vec<Canvas> {
    let (w, h) = (recipe.width, recipe.height);
    let mut terminal = TerminalState::default();
    recipe
        .stills
        .iter()
        .map(|still| match &still.screen {
            Screen::Terminal(op) => {
                terminal.apply(op);
                screens::terminal(w, h, &terminal)
            }
            Screen::Dialog(dialog) => screens::dialog(w, h, dialog),
            Screen::Slide(slide) => screens::slide(w, h, slide),
            Screen::Desktop(desktop) => screens::desktop(w, h, desktop),
        })
        .collect()
}

/// The terminal lines visible during each still, with the still's step
/// label — what a viewer could read off the screen at that moment.
#[must_use]
pub fn shown_lines(recipe: &Recipe) -> Vec<(Option<String>, Vec<String>)> {
    let mut terminal = TerminalState::default();
    recipe
        .stills
        .iter()
        .map(|still| {
            let lines = match &still.screen {
                Screen::Terminal(op) => {
                    terminal.apply(op);
                    terminal.visible().to_vec()
                }
                Screen::Dialog(d) => d.rows.clone(),
                Screen::Slide(s) => s.bullets.clone(),
                Screen::Desktop(_) => Vec::new(),
            };
            (still.step.clone(), lines)
        })
        .collect()
}

/// An ffconcat script showing each file for its duration. The last file
/// is repeated without a duration: the demuxer otherwise drops the
/// final `duration` directive.
#[must_use]
pub fn concat_list(names: &[String], durations: &[f64]) -> String {
    let mut out = String::from("ffconcat version 1.0\n");
    for (name, duration) in names.iter().zip(durations) {
        // Writing into a String cannot fail.
        let _ = writeln!(out, "file {name}\nduration {duration:.3}");
    }
    if let Some(last) = names.last() {
        let _ = writeln!(out, "file {last}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{concat_list, render_stills, shown_lines};
    use crate::eval::recipe::Recipe;

    fn recipe() -> Recipe {
        serde_json::from_str(
            r#"{
              "schema_version": 1, "id": "demo", "title": "Demo",
              "width": 320, "height": 180, "fps": 10, "duration_secs": 9.0,
              "stills": [
                {"t": 0.0, "terminal": {}},
                {"t": 2.0, "step": "01-init", "terminal": {"type": "git init"}},
                {"t": 5.0, "step": "02-recap", "slide": {"title": "Recap"}},
                {"t": 7.0, "step": "03-back", "terminal": {"output": ["ok"]}}
              ]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn concat_list_repeats_the_last_file_without_a_duration() {
        let list = concat_list(&["a.ppm".into(), "b.ppm".into()], &[2.0, 3.5]);
        assert_eq!(
            list,
            "ffconcat version 1.0\nfile a.ppm\nduration 2.000\nfile b.ppm\nduration 3.500\nfile b.ppm\n"
        );
    }

    #[test]
    fn stills_render_one_canvas_each_at_the_recipe_size() {
        let canvases = render_stills(&recipe());
        assert_eq!(canvases.len(), 4);
        assert!(canvases.iter().all(|c| c.width == 320 && c.height == 180));
        assert_ne!(canvases[0], canvases[1], "typing changes the picture");
    }

    #[test]
    fn the_terminal_persists_across_a_slide() {
        let shown = shown_lines(&recipe());
        assert_eq!(
            shown[1],
            (Some("01-init".into()), vec!["$ git init".into()])
        );
        assert!(
            shown[2].1.is_empty(),
            "a slide without bullets shows no lines"
        );
        assert_eq!(
            shown[3].1,
            vec!["$ git init".to_string(), "ok".into(), "$ ".into()]
        );
    }
}
