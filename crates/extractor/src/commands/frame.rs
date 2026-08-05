use std::path::Path;

use anyhow::Result;
use vts_extract::deps::Env;
use vts_extract::rewatch;
use vts_extract::tools::Toolchain;

/// Export one frame per timestamp, all validated before any export.
/// Output is machine-readable: `t=<secs>\t<path>` per line, in input
/// order, ready for direct agent consumption.
pub fn frame_at(bundle: &str, timestamps: &[String]) -> Result<()> {
    let tools = Toolchain::resolve(&Env::from_system())?;
    let frames = rewatch::frames_at(Path::new(bundle), &tools, timestamps)?;
    for frame in &frames {
        println!("t={:.3}\t{}", frame.timestamp_secs, frame.path.display());
    }
    Ok(())
}

/// Densely sample `start..end` at `fps` — the agent's "re-watch" tool.
pub fn clip(bundle: &str, start: &str, end: &str, fps: f32) -> Result<()> {
    let tools = Toolchain::resolve(&Env::from_system())?;
    let result = rewatch::clip(Path::new(bundle), &tools, start, end, f64::from(fps))?;
    if result.truncated {
        eprintln!(
            "warning: request exceeded the {}-frame cap and was truncated",
            rewatch::CLIP_FRAME_CAP
        );
    }
    for frame in &result.frames {
        println!("t={:.3}\t{}", frame.timestamp_secs, frame.path.display());
    }
    Ok(())
}
