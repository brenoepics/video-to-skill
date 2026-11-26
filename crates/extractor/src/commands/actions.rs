//! The opt-in action stages of `check` and `extract`: disclosing and
//! installing the VideoAgentTrek weights, and labelling a bundle's
//! screen recording with them. Kept apart so the default commands read
//! exactly as before.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use vts_extract::actions::{self, Models, SubprocessRunner};
use vts_extract::deps::fetch::LargeFileFetcher;
use vts_extract::deps::idm::{install_model, model_dir, model_ready};
use vts_extract::deps::registry::{idm_models, IdmModel};
use vts_extract::deps::Env;
use vts_extract::tools::Toolchain;

/// A 3.9 GB shard over a slow link still finishes inside this.
const SHARD_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);

pub fn label(model: &IdmModel) -> String {
    let stage = if model.role == "detector" {
        "action detector"
    } else {
        "action parameterizer"
    };
    format!("{stage} ({})", model.label())
}

/// One `check` line per model; returns the ones still missing.
pub fn report(env: &Env) -> Vec<IdmModel> {
    let mut missing = Vec::new();
    for model in idm_models() {
        let dir = model_dir(&model, &env.data_dir);
        if model_ready(&model, &env.data_dir) {
            println!("✓ {} (installed) — {}", label(&model), dir.display());
        } else {
            println!("✗ {} (missing, opt-in)", label(&model));
            missing.push(model);
        }
    }
    missing
}

/// Every file of `models`, with its URL, size and pin.
pub fn print_inventory(models: &[IdmModel]) {
    for model in models {
        println!(
            "  {} — {} files, {}",
            label(model),
            model.files.len(),
            gigabytes(model.total_bytes())
        );
        for file in model.files {
            println!("    url    {}", model.url(file));
            println!("    sha256 {} ({} bytes)", file.sha256, file.size);
        }
    }
}

/// The opt-in section of `check --print-downloads`.
pub fn print_downloads() {
    let models = idm_models();
    let total: u64 = models.iter().map(IdmModel::total_bytes).sum();
    println!(
        "\nOpt-in, only with --actions ({}, Apache-2.0):",
        gigabytes(total)
    );
    print_inventory(&models);
}

pub fn total(models: &[IdmModel]) -> String {
    gigabytes(models.iter().map(IdmModel::total_bytes).sum())
}

/// Download every missing file of `models`; one failure per model.
pub fn install(models: &[IdmModel], env: &Env) -> Vec<String> {
    let largest = models
        .iter()
        .flat_map(|m| m.files.iter().map(|f| f.size))
        .max()
        .unwrap_or(0);
    let fetcher = LargeFileFetcher {
        max_bytes: largest,
        timeout: SHARD_TIMEOUT,
    };
    let mut failures = Vec::new();
    for model in models {
        println!("\ndownloading {} …", label(model));
        match install_model(model, &fetcher, &env.data_dir) {
            Ok(dir) => println!("✓ {} installed — {}", label(model), dir.display()),
            Err(error) => failures.push(format!("{}: {error:#}", label(model))),
        }
    }
    failures
}

fn gigabytes(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)] // display only
    let gb = bytes as f64 / 1e9;
    format!("{gb:.1} GB")
}

/// `extract --actions`: label the bundle, or note why it could not be.
pub fn extract_stage(bundle_dir: &Path, tools: &Toolchain, env: &Env, runner: &str) {
    let [detector, parameterizer] = idm_models();
    let outcome =
        if model_ready(&detector, &env.data_dir) && model_ready(&parameterizer, &env.data_dir) {
            run(bundle_dir, tools, env, runner, (&detector, &parameterizer))
        } else {
            Err(anyhow::anyhow!(
                "VideoAgentTrek models not installed — run `vts-extract check --fix --actions`"
            ))
        };
    if let Err(error) = outcome {
        match actions::record_unavailable(bundle_dir, &format!("{error:#}")) {
            Ok(note) => println!("! {note}"),
            Err(err) => println!("! actions: {error:#}; also failed to note it: {err:#}"),
        }
    }
}

fn run(
    bundle_dir: &Path,
    tools: &Toolchain,
    env: &Env,
    runner: &str,
    (detector, parameterizer): (&IdmModel, &IdmModel),
) -> Result<()> {
    let started = Instant::now();
    println!("labelling GUI actions ({runner}) …");
    let detector_dir: PathBuf = model_dir(detector, &env.data_dir);
    let parameterizer_dir = model_dir(parameterizer, &env.data_dir);
    let backend = SubprocessRunner {
        program: PathBuf::from(runner),
    };
    let models = Models {
        detector: &detector_dir,
        parameterizer: &parameterizer_dir,
    };
    let track = actions::extract_action_track(bundle_dir, tools, &backend, &models)?;
    if track.windows.is_empty() {
        println!("  no screen-recording segment (cursor presence below 0.8 throughout)");
        return Ok(());
    }
    println!(
        "  {} events across {} clips ({:.1}s)",
        track.events.len(),
        track.windows.len(),
        started.elapsed().as_secs_f32()
    );
    for note in &track.notes {
        println!("  ! {note}");
    }
    Ok(())
}
