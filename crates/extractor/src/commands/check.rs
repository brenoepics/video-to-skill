use std::path::Path;

use anyhow::Result;
use vts_extract::deps::{self, fetch::HttpFetcher, registry, Env, Presence, Tool, ToolStatus};

/// Dependency doctor: report every managed tool, and with `fix` bootstrap
/// the missing ones into the app data dir (never a system location).
/// Plain `check` never touches the network.
///
/// Downloading is consent-gated: `--fix` alone discloses exactly what
/// would be fetched and stops. Nothing leaves or enters the machine
/// until the user has seen the inventory and `--yes` is passed.
pub fn run(fix: bool, yes: bool, print_downloads: bool) -> Result<()> {
    let exe = std::env::current_exe().ok();
    println!("{}", identity_line(exe.as_deref()));
    let env = Env::from_system();
    println!("data dir: {}\n", env.data_dir.display());

    if print_downloads {
        print_inventory(&registry::inventory());
        return Ok(());
    }

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
        println!("\nRun `vts-extract check --fix` to see what it would download.");
        return Ok(());
    }

    let pending: Vec<_> = missing
        .iter()
        .filter_map(|s| registry::spec_for(s.tool))
        .collect();
    if !yes {
        println!("\n{} would be downloaded:\n", plural(pending.len()));
        print_inventory(&pending);
        println!(
            "\nNothing was downloaded. Show this list to the user, then re-run with\n\
             `vts-extract check --fix --yes` to approve it. Tools already on PATH are\n\
             never downloaded — installing ffmpeg/yt-dlp yourself avoids them entirely."
        );
        return Ok(());
    }

    for status in missing {
        bootstrap(status.tool, &env)?;
    }
    println!("\nAll dependencies ready.");
    Ok(())
}

fn plural(n: usize) -> String {
    if n == 1 {
        "1 artifact".to_string()
    } else {
        format!("{n} artifacts")
    }
}

/// Disclose each artifact's exact origin and pinned digest. Integrity
/// does not depend on trusting the host: a compromised origin cannot
/// change the bytes without failing this checksum.
fn print_inventory(specs: &[deps::bootstrap::ToolSpec]) {
    for spec in specs {
        println!("  {}", label(spec.tool));
        println!("    url    {}", spec.url);
        println!("    sha256 {}", spec.sha256);
    }
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

/// One-line self identity: name, crate version, and the running executable's
/// path — printed first so the caller can confirm *which* binary answered.
fn identity_line(exe: Option<&Path>) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let path = exe.map_or_else(|| "<unknown path>".to_string(), |p| p.display().to_string());
    format!("vts-extract {version} — {path}")
}

fn label(tool: Tool) -> &'static str {
    match tool {
        Tool::Ffmpeg => "ffmpeg",
        Tool::Ffprobe => "ffprobe",
        Tool::Ytdlp => "yt-dlp",
        Tool::WhisperModel => "whisper model (ggml-base)",
    }
}

#[cfg(test)]
mod tests {
    use super::identity_line;
    use std::path::Path;

    #[test]
    fn identity_line_names_binary_version_and_path() {
        let line = identity_line(Some(Path::new("/opt/tools/vts-extract")));
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(
            line,
            format!("vts-extract {version} — /opt/tools/vts-extract")
        );
    }

    #[test]
    fn identity_line_without_path_still_reports_version() {
        let line = identity_line(None);
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(line, format!("vts-extract {version} — <unknown path>"));
    }
}
