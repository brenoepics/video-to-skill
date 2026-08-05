use anyhow::Result;
use vts_extract::deps::{self, fetch::HttpFetcher, registry, Env, Presence, Tool, ToolStatus};

/// Dependency doctor: report every managed tool, and with `fix` bootstrap
/// the missing ones into the app data dir (never a system location).
/// Plain `check` never touches the network.
pub fn run(fix: bool) -> Result<()> {
    let env = Env::from_system();
    println!("data dir: {}\n", env.data_dir.display());

    let mut missing: Vec<ToolStatus> = Vec::new();
    for status in deps::probe(&env) {
        report(&status);
        if status.presence == Presence::Missing {
            missing.push(status);
        }
    }

    if missing.is_empty() {
        println!("\nAll dependencies ready.");
        return Ok(());
    }
    if !fix {
        println!("\nRun `vts-extract check --fix` to download the missing ones.");
        return Ok(());
    }

    for status in missing {
        bootstrap(status.tool, &env)?;
    }
    println!("\nAll dependencies ready.");
    Ok(())
}

fn report(status: &ToolStatus) {
    let (mark, note) = match status.presence {
        Presence::OnPath => ("✓", "on PATH"),
        Presence::Installed => ("✓", "installed"),
        Presence::Missing => ("✗", "missing"),
    };
    let location = status
        .location
        .as_ref()
        .map(|p| format!(" — {}", p.display()))
        .unwrap_or_default();
    println!("{mark} {} ({note}){location}", label(status.tool));
}

fn bootstrap(tool: Tool, env: &Env) -> Result<()> {
    let Some(spec) = registry::spec_for(tool) else {
        println!(
            "! no pinned download for {} on this platform — please install it manually",
            label(tool)
        );
        return Ok(());
    };
    println!("\ndownloading {} …", label(tool));
    let installed = deps::bootstrap::install(&spec, &HttpFetcher, &env.data_dir)?;
    println!("✓ {} installed — {}", label(tool), installed.display());
    Ok(())
}

fn label(tool: Tool) -> &'static str {
    match tool {
        Tool::Ffmpeg => "ffmpeg",
        Tool::Ffprobe => "ffprobe",
        Tool::Ytdlp => "yt-dlp",
        Tool::WhisperModel => "whisper model (ggml-base)",
    }
}
