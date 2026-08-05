mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Local extraction engine for video-to-skill.
///
/// Does the deterministic, non-LLM work: media ingest, scene/slide detection,
/// keyframe dedup, timestamped transcription. Emits an `extraction/` bundle
/// (timeline.json + frames/ + transcript) that the orchestrating agent distills
/// into a skill. Everything runs locally; the video never leaves the machine.
#[derive(Parser)]
#[command(name = "vts-extract", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Verify local dependencies (ffmpeg, yt-dlp, whisper weights) and report what's missing
    Check {
        /// Download missing dependencies into the app data dir
        #[arg(long)]
        fix: bool,
    },
    /// Run the full extraction pipeline on a video file or URL
    Extract {
        /// Path to a video file, or a URL yt-dlp understands
        input: String,
        /// Output directory for the extraction bundle
        #[arg(long, default_value = "extraction")]
        out: String,
    },
    /// Export frames at the given timestamps (seconds or MM:SS) for agent inspection
    FrameAt {
        /// Extraction bundle directory
        #[arg(long, default_value = "extraction")]
        bundle: String,
        /// One or more timestamps, e.g. "754" or "12:34"
        #[arg(required = true, num_args(1..))]
        timestamps: Vec<String>,
    },
    /// Compile a Procedure IR (procedure.json) into an installable skill package
    Compile {
        /// Extraction bundle directory (frame references resolve against it)
        #[arg(long, default_value = "extraction")]
        bundle: String,
        /// Path to the Procedure IR JSON
        #[arg(long)]
        ir: String,
        /// Output directory for the skill package
        #[arg(long)]
        out: String,
    },
    /// Fold a new video's Procedure IR into an existing skill package
    Merge {
        /// Existing generated skill package directory
        #[arg(long)]
        skill: String,
        /// New video's Procedure IR JSON
        #[arg(long)]
        ir: String,
        /// New video's extraction bundle (for its frames)
        #[arg(long)]
        bundle: String,
        /// Output directory for the merged package
        #[arg(long, default_value = "merged-skill")]
        out: String,
        /// Show the fold plan without writing anything
        #[arg(long)]
        dry_run: bool,
    },
    /// Record a verification report against a generated skill package
    Verify {
        /// Generated skill package directory
        #[arg(long)]
        skill: String,
        /// Path to the verification report JSON
        #[arg(long)]
        report: String,
    },
    /// Export densely-sampled frames for a time range (agent "re-watch")
    Clip {
        #[arg(long, default_value = "extraction")]
        bundle: String,
        /// Start timestamp, e.g. "12:34"
        start: String,
        /// End timestamp, e.g. "13:02"
        end: String,
        /// Frames per second to sample
        #[arg(long, default_value_t = 2.0)]
        fps: f32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { fix } => commands::check::run(fix),
        Command::Extract { input, out } => commands::extract::run(&input, &out),
        Command::FrameAt { bundle, timestamps } => commands::frame::frame_at(&bundle, &timestamps),
        Command::Compile { bundle, ir, out } => commands::compile::run(&bundle, &ir, &out),
        Command::Verify { skill, report } => commands::verify::run(&skill, &report),
        Command::Merge {
            skill,
            ir,
            bundle,
            out,
            dry_run,
        } => commands::merge::run(&skill, &ir, &bundle, &out, dry_run),
        Command::Clip {
            bundle,
            start,
            end,
            fps,
        } => commands::frame::clip(&bundle, &start, &end, fps),
    }
}
