//! End-to-end run of the real benchmark suite. Ignored by default: it
//! generates fixtures with ffmpeg (and `say` on macOS) and transcribes
//! through real whisper weights, taking minutes.
//!
//! Prerequisites: ffmpeg/ffprobe on PATH and bootstrapped whisper
//! weights (`vts-extract check --fix`). Run with:
//!
//!   cargo test --test bench -- --ignored

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use vts_extract::bench;

#[test]
#[ignore = "runs the full 120s benchmark; needs ffmpeg + whisper weights (vts-extract check --fix)"]
fn suite_measures_the_contracted_benchmarks() {
    let mut progress_lines = Vec::new();

    let outcome = bench::run_suite(&mut |line| progress_lines.push(line.to_string())).unwrap();

    let report = &outcome.report;
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.version, env!("CARGO_PKG_VERSION"));
    assert!(report.machine.logical_cores > 0);
    assert!(!report.machine.chip.is_empty());
    assert!(!progress_lines.is_empty(), "suite must report progress");

    let extract = report
        .results
        .iter()
        .find(|r| r.id == "extract-120s")
        .expect("extract-120s result");
    let media_secs = extract.media_secs.expect("extract media duration");
    assert!(
        (media_secs - 120.0).abs() < 2.0,
        "fixture should be ~120s, got {media_secs}"
    );
    assert!(extract.wall_secs > 0.0);
    assert!(extract.realtime_factor.expect("realtime factor") > 0.0);
    let stage_names: Vec<&str> = extract
        .stages
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(stage_names, ["ingest", "transcribe", "frames", "timeline"]);
    assert!(extract.stages.iter().all(|(_, secs)| *secs >= 0.0));

    let batch = report
        .results
        .iter()
        .find(|r| r.id == "frames-batch-5")
        .expect("frames-batch-5 result");
    assert!(batch.wall_secs > 0.0);
    assert!(batch.realtime_factor.is_none());

    if cfg!(target_os = "macos") {
        let asr = report
            .results
            .iter()
            .find(|r| r.id == "asr-45s")
            .expect("asr-45s result on macOS");
        assert!(asr.media_secs.expect("asr media duration") > 30.0);
        assert!(asr.realtime_factor.expect("asr realtime factor") > 0.0);
    } else {
        assert!(
            outcome.skipped.iter().any(|s| s.contains("asr-45s")),
            "non-macOS runs must record the asr skip: {:?}",
            outcome.skipped
        );
    }
}
