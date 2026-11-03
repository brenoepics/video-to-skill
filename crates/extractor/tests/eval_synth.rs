//! Gold-video synthesis through real ffmpeg: a committed recipe renders
//! to a video of the declared length and size, and the extractor's
//! frame track and timeline run on it. Narration needs macOS `say`;
//! elsewhere the video is silent.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::bundle::{ingest, load_manifest};
use vts_extract::eval::gold::load_gold_set;
use vts_extract::eval::score::score_bundle;
use vts_extract::eval::synth::render_video;
use vts_extract::frames::{extract_frame_track, FrameConfig};
use vts_extract::timeline::assemble;
use vts_extract::tools::Toolchain;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-eval-synth-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn toolchain() -> Toolchain {
    Toolchain {
        ffmpeg: PathBuf::from("ffmpeg"),
        ffprobe: PathBuf::from("ffprobe"),
        whisper_model: None,
        whisper_vad: None,
    }
}

fn probe(media: &Path, entries: &str) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", entries, "-of", "csv=p=0"])
        .arg(media)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn a_gold_recipe_renders_and_runs_through_the_frame_pipeline() {
    let dir = scratch("render");
    let gold = load_gold_set(&Path::new(env!("CARGO_MANIFEST_DIR")).join("eval")).unwrap();
    let video = &gold[0];

    let rendered = render_video(&video.recipe, &toolchain(), &dir.join("render")).unwrap();

    let duration: f64 = probe(&rendered.video, "format=duration").parse().unwrap();
    assert!(
        (duration - video.recipe.duration_secs).abs() < 0.6,
        "duration {duration} vs recipe {}",
        video.recipe.duration_secs
    );
    let size = probe(&rendered.video, "stream=width,height");
    assert!(
        size.contains(&format!("{},{}", video.recipe.width, video.recipe.height)),
        "{size}"
    );
    assert_eq!(rendered.narrated, cfg!(target_os = "macos"));

    let bundle = dir.join("bundle");
    ingest(&rendered.video, &bundle, &toolchain()).unwrap();
    assert!(load_manifest(&bundle).unwrap().media.has_audio);
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();
    let timeline = assemble(&bundle).unwrap();

    let report = score_bundle(video, &timeline, None);
    assert!(report.segments >= 1);
    assert_eq!(
        report.counts.boundaries_gold,
        video.recipe.change_times().len() as u64
    );
    assert_eq!(report.counts.steps_gold, video.steps.len() as u64);
    assert!(report.transcript_segments.is_none());
}
