//! Reproducible benchmark suite behind `vts-extract bench`.
//!
//! A fixed set of generated fixtures runs through the real pipeline so
//! numbers stay comparable across machines and releases. Rendering is
//! split into pure functions (`report`, `splice`) so the markdown output
//! is testable without running any benchmark.

pub mod fixtures;
pub mod machine;
pub mod report;
pub mod splice;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::asr::{self, whisper::WhisperEngine};
use crate::bundle;
use crate::deps::Env;
use crate::frames::{self, FrameConfig};
use crate::rewatch;
use crate::timeline;
use crate::tools::Toolchain;

/// Everything one benchmark run measured, ready for rendering.
#[derive(Debug, Clone, Serialize)]
pub struct BenchReport {
    pub schema_version: u32,
    pub machine: Machine,
    /// `vts-extract` crate version the numbers were measured with.
    pub version: String,
    pub results: Vec<BenchResult>,
}

/// The hardware and OS the benchmark ran on.
#[derive(Debug, Clone, Serialize)]
pub struct Machine {
    /// CPU/SoC name, e.g. "Apple M2 Pro"; "unknown" when undetectable.
    pub chip: String,
    pub logical_cores: usize,
    /// OS and architecture, e.g. "macos/aarch64".
    pub os: String,
}

/// One benchmark's measurements.
#[derive(Debug, Clone, Serialize)]
pub struct BenchResult {
    /// Stable identifier, e.g. "asr-45s".
    pub id: String,
    pub label: String,
    pub wall_secs: f64,
    /// Duration of the media the benchmark processed, when applicable.
    pub media_secs: Option<f64>,
    /// `media_secs / wall_secs` — above 1.0 means faster than realtime.
    pub realtime_factor: Option<f64>,
    /// Per-stage wall times, in execution order.
    pub stages: Vec<(String, f64)>,
}

/// A finished suite run: the report plus any benchmarks that were
/// skipped (with the reason), e.g. the ASR bench off macOS.
#[derive(Debug)]
pub struct SuiteOutcome {
    pub report: BenchReport,
    pub skipped: Vec<String>,
}

/// Run the fixed benchmark suite against the real pipeline. `progress`
/// receives one human-readable line per long-running step.
pub fn run_suite(progress: &mut dyn FnMut(&str)) -> Result<SuiteOutcome> {
    let tools = Toolchain::resolve(&Env::from_system())?;
    let model = tools
        .whisper_model
        .clone()
        .context("whisper model weights are missing — run `vts-extract check --fix` first")?;
    let work = scratch_dir()?;

    let mut results = Vec::new();
    let mut skipped = Vec::new();
    if cfg!(target_os = "macos") {
        progress("asr-45s: generating ~45s speech fixture …");
        results.push(bench_asr(&tools, &model, &work, progress)?);
    } else {
        skipped
            .push("asr-45s skipped: generating the speech fixture needs macOS `say`".to_string());
    }

    progress("extract-120s: generating 120s 720p fixture …");
    let (extract, bundle_dir) = bench_extract(&tools, &model, &work, progress)?;
    results.push(extract);

    progress("frames-batch-5: exporting 5 frames …");
    results.push(bench_frames_batch(&tools, &bundle_dir)?);

    fs::remove_dir_all(&work).ok();
    Ok(SuiteOutcome {
        report: BenchReport {
            schema_version: 1,
            machine: machine::detect(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            results,
        },
        skipped,
    })
}

/// Transcribe generated speech through the real whisper engine
/// (measures demux + recognition).
fn bench_asr(
    tools: &Toolchain,
    model: &Path,
    work: &Path,
    progress: &mut dyn FnMut(&str),
) -> Result<BenchResult> {
    let media = fixtures::speech_mp4(tools, work)?;
    let bundle_dir = work.join("asr-bundle");
    let ingest = bundle::ingest(&media, &bundle_dir, tools)?;
    let media_secs = ingest.manifest.media.duration_secs;

    progress("asr-45s: transcribing …");
    let engine = WhisperEngine::new(model.to_path_buf());
    let started = Instant::now();
    asr::extract_audio_track(&bundle_dir, tools, &engine, &mut |_| {})?;
    let wall_secs = started.elapsed().as_secs_f64();

    Ok(BenchResult {
        id: "asr-45s".into(),
        label: "Transcribe ~45s of generated speech".into(),
        wall_secs,
        media_secs: Some(media_secs),
        realtime_factor: realtime(media_secs, wall_secs),
        stages: Vec::new(),
    })
}

/// The full extract pipeline on the 120s fixture, timing each stage.
/// Returns the bundle so the batch-frame bench can reuse it.
fn bench_extract(
    tools: &Toolchain,
    model: &Path,
    work: &Path,
    progress: &mut dyn FnMut(&str),
) -> Result<(BenchResult, PathBuf)> {
    let media = fixtures::testsrc_mp4(tools, work, 120)?;
    let bundle_dir = work.join("extract-bundle");
    let mut stages = Vec::new();
    let total = Instant::now();

    let started = Instant::now();
    let ingest = bundle::ingest(&media, &bundle_dir, tools)?;
    stages.push(("ingest".to_string(), started.elapsed().as_secs_f64()));
    let media_secs = ingest.manifest.media.duration_secs;

    progress("extract-120s: transcribing …");
    let engine = WhisperEngine::new(model.to_path_buf());
    let started = Instant::now();
    asr::extract_audio_track(&bundle_dir, tools, &engine, &mut |_| {})?;
    stages.push(("transcribe".to_string(), started.elapsed().as_secs_f64()));

    progress("extract-120s: analyzing frames …");
    let started = Instant::now();
    frames::extract_frame_track(&bundle_dir, tools, &FrameConfig::default())?;
    stages.push(("frames".to_string(), started.elapsed().as_secs_f64()));

    let started = Instant::now();
    timeline::assemble(&bundle_dir)?;
    stages.push(("timeline".to_string(), started.elapsed().as_secs_f64()));

    let wall_secs = total.elapsed().as_secs_f64();
    let result = BenchResult {
        id: "extract-120s".into(),
        label: "Full extract of a 120s 720p video".into(),
        wall_secs,
        media_secs: Some(media_secs),
        realtime_factor: realtime(media_secs, wall_secs),
        stages,
    };
    Ok((result, bundle_dir))
}

/// Batch frame export at 5 timestamps from the extract bundle.
fn bench_frames_batch(tools: &Toolchain, bundle_dir: &Path) -> Result<BenchResult> {
    let timestamps = ["10", "30", "50", "70", "90"];
    let started = Instant::now();
    rewatch::frames_at(bundle_dir, tools, &timestamps)?;
    Ok(BenchResult {
        id: "frames-batch-5".into(),
        label: "Batch export of 5 frames".into(),
        wall_secs: started.elapsed().as_secs_f64(),
        media_secs: None,
        realtime_factor: None,
        stages: Vec::new(),
    })
}

fn realtime(media_secs: f64, wall_secs: f64) -> Option<f64> {
    (wall_secs > 0.0).then(|| media_secs / wall_secs)
}

fn scratch_dir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("vts-bench-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).context("clearing the bench scratch dir")?;
    }
    fs::create_dir_all(&dir).context("creating the bench scratch dir")?;
    Ok(dir)
}
