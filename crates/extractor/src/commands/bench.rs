use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use vts_extract::bench::{self, report, splice::splice, BenchReport, BenchResult};

/// Run the benchmark suite and print the results table. With
/// `--write-docs <repo-root>` the generated markdown is spliced into
/// `BENCHMARK.md` and `README.md` between their bench markers.
pub fn run(write_docs: Option<&str>) -> Result<()> {
    let outcome = bench::run_suite(&mut |line| println!("{line}"))?;
    print_table(&outcome.report);
    for note in &outcome.skipped {
        println!("! {note}");
    }
    if let Some(root) = write_docs {
        write_docs_into(Path::new(root), &outcome.report)?;
    }
    Ok(())
}

fn print_table(report: &BenchReport) {
    println!();
    println!(
        "{:<16} {:<38} {:>9} {:>9}",
        "id", "label", "wall", "realtime"
    );
    for result in &report.results {
        println!(
            "{:<16} {:<38} {:>8.1}s {:>9}",
            result.id,
            result.label,
            result.wall_secs,
            realtime_cell(result)
        );
        if !result.stages.is_empty() {
            let stages = result
                .stages
                .iter()
                .map(|(name, secs)| format!("{name} {secs:.1}s"))
                .collect::<Vec<_>>()
                .join(" · ");
            println!("{:<16} {stages}", "");
        }
    }
    println!(
        "\nmachine: {} · {} logical cores · {} · vts-extract {}",
        report.machine.chip, report.machine.logical_cores, report.machine.os, report.version
    );
}

fn realtime_cell(result: &BenchResult) -> String {
    result
        .realtime_factor
        .map_or_else(|| "—".to_string(), |f| format!("{f:.1}x"))
}

fn write_docs_into(root: &Path, report: &BenchReport) -> Result<()> {
    let date = today();
    splice_file(
        &root.join("BENCHMARK.md"),
        report::RESULTS_BEGIN,
        report::RESULTS_END,
        &report::render_results_markdown(report, &date),
    )?;
    splice_file(
        &root.join("README.md"),
        report::SUMMARY_BEGIN,
        report::SUMMARY_END,
        &report::render_readme_summary(report),
    )?;
    println!(
        "docs updated: {} and {}",
        root.join("BENCHMARK.md").display(),
        root.join("README.md").display()
    );
    Ok(())
}

fn splice_file(path: &Path, begin: &str, end: &str, replacement: &str) -> Result<()> {
    let document = fs::read_to_string(path)
        .with_context(|| format!("cannot read {} — does it exist?", path.display()))?;
    let updated = splice(&document, begin, end, replacement)
        .with_context(|| format!("{} has no usable bench markers", path.display()))?;
    fs::write(path, updated).with_context(|| format!("writing {}", path.display()))
}

fn today() -> String {
    std::process::Command::new("date")
        .arg("+%F")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown-date".to_string())
}
