//! Quality evaluation behind `vts-extract eval`: a synthetic gold set of
//! short tutorial videos, each generated deterministically from a recipe
//! and labelled with a hand-written Procedure IR, is run through the
//! real pipeline and scored. The extractor emits no steps itself, so the
//! gated metrics are what it alone determines — segment boundaries
//! against the recipe's visual changes, step recovery by timestamp `IoU`,
//! gold commands spoken in the transcript — while the agent-dependent
//! ones (package steps, verbatim commands, verification) are scored
//! only when compiled packages are supplied, and reported as not scored
//! otherwise.

pub mod gate;
pub mod gold;
pub mod metrics;
pub mod package;
pub mod pool;
pub mod recipe;
pub mod report;
pub mod score;
pub mod stats;
pub mod synth;
pub mod types;

pub use pool::{metrics_from, Unavailable};
pub use types::{Counts, EvalReport, GoldSummary, Metric, Outcome, VideoReport};

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::asr::{self, whisper::WhisperEngine};
use crate::bench::machine;
use crate::bundle;
use crate::deps::Env;
use crate::frames::{self, FrameConfig};
use crate::timeline;
use crate::tools::Toolchain;

/// Gold set location relative to the repository root.
pub const DEFAULT_GOLD_DIR: &str = "crates/extractor/eval";
pub const BASELINE_FILE: &str = "baseline.json";
/// Two thumbnail ticks at the default 1 fps sampling: a change at x.5 s
/// shows on tick x+1 and a steady-state detector confirms it on x+2.
pub const BOUNDARY_TOLERANCE_SECS: f64 = 2.0;
pub const STEP_IOU_THRESHOLD: f64 = 0.5;
/// Largest drop of a metric's lower bound below the baseline that CI
/// tolerates before failing.
pub const DEFAULT_MAX_DROP: f64 = 0.05;

#[derive(Debug, Clone, Default)]
pub struct EvalOptions {
    pub gold_dir: PathBuf,
    /// Compiled packages to score, one `<gold id>/` directory each.
    pub packages_dir: Option<PathBuf>,
    /// Keep generated videos and bundles here instead of a temp dir.
    pub keep_dir: Option<PathBuf>,
    /// Restrict to these gold ids; empty means all.
    pub only: Vec<String>,
}

/// Generate, extract and score the whole gold set.
pub fn run_eval(options: &EvalOptions, progress: &mut dyn FnMut(&str)) -> Result<EvalReport> {
    let tools = Toolchain::resolve(&Env::from_system())?;
    let engine = WhisperEngine::from_toolchain(&tools);
    let mut gold = gold::load_gold_set(&options.gold_dir)?;
    if !options.only.is_empty() {
        gold.retain(|g| options.only.contains(&g.id));
        if gold.is_empty() {
            bail!("no gold video matches --only {:?}", options.only);
        }
    }
    let (work, keep) = match &options.keep_dir {
        Some(dir) => (dir.clone(), true),
        None => (scratch_dir()?, false),
    };
    let mut pooled = Counts::default();
    let mut videos = Vec::new();
    let mut all_narrated = true;
    for video in &gold {
        progress(&format!(
            "{}: rendering {:.0}s video …",
            video.id, video.recipe.duration_secs
        ));
        let rendered = synth::render_video(&video.recipe, &tools, &work.join(&video.id))?;
        all_narrated &= rendered.narrated;
        progress(&format!("{}: extracting …", video.id));
        let mut report = extract_and_score(video, &rendered.video, &tools, engine.as_ref(), &work)?;
        if let Some(dir) = options.packages_dir.as_ref().map(|d| d.join(&video.id)) {
            if dir.is_dir() {
                let (ir, verification) = package::load_package(&dir)?;
                score::score_package(video, &ir, verification.as_ref(), &mut report);
            }
        }
        pooled.add(&report.counts);
        videos.push(report);
    }
    if !keep {
        fs::remove_dir_all(&work).ok();
    }
    let unavailable = unavailable_reasons(options, engine.is_some(), all_narrated, &videos);
    Ok(EvalReport {
        schema_version: 1,
        version: env!("CARGO_PKG_VERSION").to_string(),
        generated_on: crate::calendar::today_utc().context("reading the system clock")?,
        machine: machine::detect(engine.as_ref().map_or("none", WhisperEngine::device)),
        tolerance_secs: BOUNDARY_TOLERANCE_SECS,
        iou_threshold: STEP_IOU_THRESHOLD,
        gold_set: summarize(&gold),
        metrics: metrics_from(&pooled, &unavailable),
        videos,
        notes: Vec::new(),
    })
}

fn extract_and_score(
    video: &gold::GoldVideo,
    media: &Path,
    tools: &Toolchain,
    engine: Option<&WhisperEngine>,
    work: &Path,
) -> Result<VideoReport> {
    let bundle_dir = work.join(&video.id).join("bundle");
    bundle::ingest(media, &bundle_dir, tools)?;
    if let Some(engine) = engine {
        asr::extract_audio_track(&bundle_dir, tools, engine, &mut |_| {})?;
    }
    frames::extract_frame_track(&bundle_dir, tools, &FrameConfig::default())?;
    let timeline = timeline::assemble(&bundle_dir)?;
    let transcript = engine
        .map(|_| bundle::load_transcript(&bundle_dir))
        .transpose()?;
    Ok(score::score_bundle(video, &timeline, transcript.as_ref()))
}

fn unavailable_reasons(
    options: &EvalOptions,
    engine: bool,
    narrated: bool,
    videos: &[VideoReport],
) -> Unavailable {
    let transcript = if !engine {
        Some("whisper weights missing — run `vts-extract check --fix --yes`".to_string())
    } else if narrated {
        None
    } else {
        Some("narration needs macOS `say`; the videos were rendered silent".to_string())
    };
    let packages = if options.packages_dir.is_none() {
        Some("no compiled packages (--packages): step authoring needs the agent".to_string())
    } else if videos.iter().all(|v| v.packages_scored == 0) {
        Some("no <gold id>/provenance.json found under --packages".to_string())
    } else {
        None
    };
    let verification = match &packages {
        Some(reason) => Some(format!("{reason}; verification needs the sandboxed agent")),
        None if videos.iter().all(|v| v.verifications_scored == 0) => {
            Some("no verification.json in the packages: run `vts-extract verify` first".to_string())
        }
        None => None,
    };
    Unavailable {
        transcript,
        packages,
        verification,
    }
}

fn summarize(gold: &[gold::GoldVideo]) -> GoldSummary {
    GoldSummary {
        videos: gold.len() as u64,
        steps: gold.iter().map(|g| g.steps.len() as u64).sum(),
        commands: gold.iter().map(|g| g.command_count() as u64).sum(),
        changes: gold
            .iter()
            .map(|g| g.recipe.change_times().len() as u64)
            .sum(),
    }
}

fn scratch_dir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("vts-eval-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).context("clearing the eval scratch dir")?;
    }
    fs::create_dir_all(&dir).context("creating the eval scratch dir")?;
    Ok(dir)
}
