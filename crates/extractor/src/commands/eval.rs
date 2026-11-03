use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use vts_extract::eval::{self, gate, report, EvalOptions, EvalReport};

/// Command-line options of `vts-extract eval`.
pub struct Args<'a> {
    pub gold: &'a str,
    pub out: &'a str,
    pub summary: Option<&'a str>,
    pub baseline: Option<&'a str>,
    pub max_drop: f64,
    pub packages: Option<&'a str>,
    pub keep: Option<&'a str>,
    pub only: &'a [String],
}

/// Run the quality eval, write the JSON artifact (and the markdown
/// summary when asked), and fail when a metric's lower bound drops more
/// than `max_drop` below the baseline.
pub fn run(args: &Args<'_>) -> Result<()> {
    let options = EvalOptions {
        gold_dir: PathBuf::from(args.gold),
        packages_dir: args.packages.map(PathBuf::from),
        keep_dir: args.keep.map(PathBuf::from),
        only: args.only.to_vec(),
    };
    let current = eval::run_eval(&options, &mut |line| println!("{line}"))?;
    write(
        Path::new(args.out),
        &serde_json::to_string_pretty(&current)?,
    )?;
    println!("results written to {}", args.out);

    let comparison = args
        .baseline
        .map(|path| -> Result<gate::Comparison> {
            let baseline = load_baseline(Path::new(path))?;
            Ok(gate::compare(&current, &baseline, args.max_drop))
        })
        .transpose()?;
    let markdown = report::render_summary_markdown(&current, comparison.as_ref());
    println!("\n{markdown}");
    if let Some(path) = args.summary {
        write(Path::new(path), &markdown)?;
    }
    if let Some(comparison) = comparison {
        let regressions = comparison.regressions();
        if !regressions.is_empty() {
            bail!(
                "quality regression against {}:\n  {}",
                args.baseline.unwrap_or_default(),
                regressions.join("\n  ")
            );
        }
        println!(
            "gate: no metric dropped more than {:.3} below the baseline",
            args.max_drop
        );
    }
    Ok(())
}

fn load_baseline(path: &Path) -> Result<EvalReport> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("cannot read the baseline at {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("{} is not an eval report", path.display()))
}

fn write(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}
