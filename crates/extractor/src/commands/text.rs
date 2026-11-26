use std::path::Path;

use anyhow::{anyhow, Result};
use vts_extract::bundle::load_manifest;
use vts_extract::ocr::{self, ScreenTextLine};

/// Print the screen text track as `t=<secs>\t<frame>\t<text>` lines,
/// filtered by `grep` (a case-insensitive regex) when given — the
/// agent's way to find the frame that shows a command without viewing
/// every keyframe.
pub fn run(bundle: &str, grep: Option<&str>) -> Result<()> {
    let bundle_dir = Path::new(bundle);
    let track = ocr::load_screen_text(bundle_dir).map_err(|err| absent_track(bundle_dir, err))?;
    let lines: Vec<&ScreenTextLine> = match grep {
        Some(pattern) => ocr::grep(&track, &ocr::compile_pattern(pattern)?),
        None => track.lines.iter().collect(),
    };
    for line in &lines {
        println!("{}", ocr::format_line(line));
    }
    if lines.is_empty() {
        eprintln!(
            "no on-screen text {} across {} keyframes",
            grep.map_or_else(
                || "was recognised".to_string(),
                |p| format!("matched '{p}'")
            ),
            track.frames_read
        );
    }
    Ok(())
}

/// When the artifact is missing, the manifest usually says why (no
/// engine on this platform); surface that instead of a bare file error.
fn absent_track(bundle_dir: &Path, err: anyhow::Error) -> anyhow::Error {
    let note = load_manifest(bundle_dir).ok().and_then(|manifest| {
        manifest
            .notes
            .iter()
            .find_map(|note| note.strip_prefix("screen_text: ").map(str::to_string))
    });
    match note {
        Some(reason) => anyhow!("no screen text track in {}: {reason}", bundle_dir.display()),
        None => err,
    }
}
