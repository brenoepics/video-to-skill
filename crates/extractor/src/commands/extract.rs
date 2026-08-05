use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use vts_extract::asr::{self, whisper::WhisperEngine};
use vts_extract::bundle::{self, YtdlpDownloader};
use vts_extract::deps::{self, Env, Presence, Tool};
use vts_extract::frames::{self, FrameConfig};
use vts_extract::timeline;
use vts_extract::tools::Toolchain;

/// The full extraction pipeline: ingest → transcript → frames → timeline.
/// A failed optional stage (e.g. missing whisper model) degrades with a
/// warning instead of aborting — the timeline records the gap.
pub fn run(input: &str, out: &str) -> Result<()> {
    let env = Env::from_system();
    let tools = Toolchain::resolve(&env)?;
    let bundle_dir = Path::new(out);

    let started = Instant::now();
    if input.starts_with("http") {
        println!("downloading …");
    }
    let report = bundle::ingest_any(input, bundle_dir, &tools, &LazyYtdlp { env })?;
    let m = &report.manifest;
    println!(
        "{} {out}/ ({:.1}s)",
        if report.created {
            "bundle created:"
        } else {
            "bundle reused:"
        },
        started.elapsed().as_secs_f32()
    );
    println!(
        "  {} · {:.1}s · {}x{} @ {:.3}fps · audio: {}",
        m.media.container,
        m.media.duration_secs,
        m.media.width,
        m.media.height,
        m.media.fps,
        if m.media.has_audio { "yes" } else { "no" },
    );

    transcribe_stage(bundle_dir, &tools);

    let started = Instant::now();
    println!("analyzing frames …");
    let track = frames::extract_frame_track(bundle_dir, &tools, &FrameConfig::default())?;
    let busiest = track
        .shots
        .iter()
        .map(|s| s.motion_density)
        .fold(0.0f64, f64::max);
    println!(
        "  {} shots · peak motion {:.3} ({:.1}s)",
        track.shots.len(),
        busiest,
        started.elapsed().as_secs_f32()
    );

    let started = Instant::now();
    let tl = timeline::assemble(bundle_dir)?;
    println!(
        "timeline assembled: {} segments ({:.1}s)",
        tl.segments.len(),
        started.elapsed().as_secs_f32()
    );
    for note in &tl.notes {
        println!("  ! {note}");
    }
    Ok(())
}

/// Resolves yt-dlp only when a URL actually needs downloading, so
/// local-file extraction never requires it.
struct LazyYtdlp {
    env: Env,
}

impl bundle::VideoDownloader for LazyYtdlp {
    fn download(&self, url: &str, dest_dir: &Path) -> Result<bundle::DownloadedVideo> {
        let ytdlp = deps::probe(&self.env)
            .into_iter()
            .find(|s| s.tool == Tool::Ytdlp && s.presence != Presence::Missing)
            .and_then(|s| s.location)
            .context("yt-dlp is missing — run `vts-extract check --fix` first")?;
        YtdlpDownloader::new(ytdlp).download(url, dest_dir)
    }
}

/// Transcription is best-effort: without the model (or on engine failure)
/// the pipeline continues and the gap surfaces in the timeline notes.
fn transcribe_stage(bundle_dir: &Path, tools: &Toolchain) {
    let Some(model) = tools.whisper_model.clone() else {
        println!(
            "! whisper model missing — skipping transcription (run `vts-extract check --fix`)"
        );
        return;
    };
    let started = Instant::now();
    println!("transcribing …");
    let result =
        asr::extract_audio_track(bundle_dir, tools, &WhisperEngine::new(model), &mut |pct| {
            if pct > 0 {
                println!("  … {pct}%");
            }
        });
    match result {
        Ok(t) => match (t.segments.len(), t.language.as_deref().unwrap_or("unknown")) {
            (0, _) => println!("  no speech found (see manifest notes)"),
            (n, lang) => println!(
                "  {n} segments · language: {lang} ({:.1}s)",
                started.elapsed().as_secs_f32()
            ),
        },
        Err(err) => println!("! transcription failed, continuing without it: {err:#}"),
    }
}
