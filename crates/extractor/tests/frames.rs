//! Frame track: sampling, dedup, and shot boundaries on fixtures with
//! known scene structure — the expected values come from how each
//! fixture is generated, not from the implementation.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::bundle::{ingest, load_manifest};
use vts_extract::frames::{extract_frame_track, load_frame_track, FrameConfig, FRAMES_FILE};
use vts_extract::tools::Toolchain;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-frames-tests")
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
    }
}

fn run_ffmpeg(args: &[&str], out: &Path) {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y").args(args).arg(out);
    let res = cmd.output().unwrap();
    assert!(
        res.status.success(),
        "fixture generation failed: {}",
        String::from_utf8_lossy(&res.stderr)
    );
}

/// Three solid colors, 3 seconds each, hard cuts between them.
fn three_cut_fixture(dir: &Path) -> PathBuf {
    let out = dir.join("cuts.mp4");
    run_ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=320x240:r=10:d=3",
            "-f",
            "lavfi",
            "-i",
            "color=c=green:s=320x240:r=10:d=3",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x240:r=10:d=3",
            "-filter_complex",
            "[0][1][2]concat=n=3",
            "-c:v",
            "mpeg4",
        ],
        &out,
    );
    out
}

/// Red fading into blue over 2 seconds (a gradual transition).
fn fade_fixture(dir: &Path) -> PathBuf {
    let out = dir.join("fade.mp4");
    run_ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=320x240:r=10:d=5",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x240:r=10:d=5",
            "-filter_complex",
            "xfade=transition=fade:duration=2:offset=2",
            "-c:v",
            "mpeg4",
        ],
        &out,
    );
    out
}

fn single_source_fixture(dir: &Path, name: &str, source: &str) -> PathBuf {
    let out = dir.join(format!("{name}.mp4"));
    run_ffmpeg(&["-f", "lavfi", "-i", source, "-c:v", "mpeg4"], &out);
    out
}

fn bundle_for(dir: &Path, video: &Path) -> PathBuf {
    let bundle = dir.join("bundle");
    ingest(video, &bundle, &toolchain()).unwrap();
    bundle
}

#[test]
fn hard_cuts_yield_one_shot_and_keyframe_per_scene() {
    let dir = scratch("cuts");
    let bundle = bundle_for(&dir, &three_cut_fixture(&dir));

    let track = extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    assert_eq!(track.shots.len(), 3, "one shot per solid color");
    // Boundaries land near the 3s and 6s cuts.
    assert!((track.shots[1].start_secs - 3.0).abs() <= 1.0);
    assert!((track.shots[2].start_secs - 6.0).abs() <= 1.0);
    for shot in &track.shots {
        let kf = &shot.keyframe;
        assert!(bundle.join(&kf.path).is_file(), "missing {}", kf.path);
        assert!(
            bundle.join(kf.native_path.as_ref().unwrap()).is_file(),
            "missing native still"
        );
        assert_eq!(kf.hash.len(), 16, "64-bit hash as hex");
        assert!(kf.timestamp_secs >= shot.start_secs && kf.timestamp_secs <= shot.end_secs);
    }
    let manifest = load_manifest(&bundle).unwrap();
    assert_eq!(manifest.tracks.frames.as_deref(), Some(FRAMES_FILE));
}

#[test]
fn a_gradual_fade_yields_one_boundary_not_many() {
    let dir = scratch("fade");
    let bundle = bundle_for(&dir, &fade_fixture(&dir));

    let track = extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    assert_eq!(track.shots.len(), 2, "fade must coalesce to one boundary");
}

#[test]
fn a_static_video_collapses_to_a_single_keyframe() {
    let dir = scratch("static");
    let video = single_source_fixture(&dir, "static", "color=c=gray:s=320x240:r=10:d=4");
    let bundle = bundle_for(&dir, &video);

    let track = extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    assert_eq!(track.shots.len(), 1);
}

#[test]
fn motion_density_separates_moving_from_static_content() {
    let dir = scratch("motion");
    let static_video = single_source_fixture(&dir, "still", "color=c=gray:s=320x240:r=10:d=4");
    let moving_video =
        single_source_fixture(&dir, "moving", "testsrc=size=320x240:rate=10:duration=4");
    let still_bundle = dir.join("still-bundle");
    let moving_bundle = dir.join("moving-bundle");
    ingest(&static_video, &still_bundle, &toolchain()).unwrap();
    ingest(&moving_video, &moving_bundle, &toolchain()).unwrap();

    let still = extract_frame_track(&still_bundle, &toolchain(), &FrameConfig::default()).unwrap();
    let moving =
        extract_frame_track(&moving_bundle, &toolchain(), &FrameConfig::default()).unwrap();

    let still_motion = still.shots[0].motion_density;
    let max_moving = moving
        .shots
        .iter()
        .map(|s| s.motion_density)
        .fold(0.0f64, f64::max);
    assert!(still_motion < 0.01, "static content: {still_motion}");
    assert!(
        max_moving > still_motion,
        "moving {max_moving} vs still {still_motion}"
    );
}

#[test]
fn frame_track_is_deterministic_across_runs() {
    let dir = scratch("determinism");
    let bundle = bundle_for(&dir, &three_cut_fixture(&dir));

    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();
    let first = fs::read(bundle.join(FRAMES_FILE)).unwrap();
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();
    let second = fs::read(bundle.join(FRAMES_FILE)).unwrap();

    assert_eq!(first, second, "frames.json must be byte-identical");
    // And the loaded form agrees with itself.
    let track = load_frame_track(&bundle).unwrap();
    assert_eq!(track.schema_version, 1);

    // Interior keyframes (long low-motion shot) are deterministic too.
    let long_video = long_single_scene_fixture(&dir);
    let long_bundle = dir.join("long-bundle");
    ingest(&long_video, &long_bundle, &toolchain()).unwrap();
    extract_frame_track(&long_bundle, &toolchain(), &FrameConfig::default()).unwrap();
    let first = fs::read(long_bundle.join(FRAMES_FILE)).unwrap();
    extract_frame_track(&long_bundle, &toolchain(), &FrameConfig::default()).unwrap();
    let second = fs::read(long_bundle.join(FRAMES_FILE)).unwrap();
    assert_eq!(
        first, second,
        "interior keyframes must serialize identically"
    );
    let long_track = load_frame_track(&long_bundle).unwrap();
    assert!(
        !long_track.shots[0].interior_keyframes.is_empty(),
        "determinism run must actually cover interior keyframes"
    );
}

/// One long low-motion scene: ~60 seconds of a single color source —
/// the shape of a slow screencast where thumbnails barely change.
fn long_single_scene_fixture(dir: &Path) -> PathBuf {
    single_source_fixture(dir, "long", "color=c=gray:s=160x120:r=5:d=60")
}

#[test]
fn a_long_low_motion_shot_gains_periodic_interior_keyframes() {
    let dir = scratch("subdivide");
    let bundle = bundle_for(&dir, &long_single_scene_fixture(&dir));

    let track = extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    assert_eq!(track.shots.len(), 1, "subdivision must not add shots");
    let shot = &track.shots[0];
    let interior = &shot.interior_keyframes;
    // 60s shot at the default 15s interval: candidates at 15/30/45s.
    // The main keyframe sits at the shot midpoint (29s), so the 30s
    // candidate is within 1s of it and is skipped as a duplicate.
    let times: Vec<f64> = interior.iter().map(|k| k.timestamp_secs).collect();
    assert_eq!(times, vec![15.0, 45.0], "interior keyframes: {times:?}");
    assert_eq!(interior[0].path, "frames/shot000-k01.jpg");
    assert_eq!(interior[1].path, "frames/shot000-k02.jpg");
    for kf in interior {
        assert!(bundle.join(&kf.path).is_file(), "missing {}", kf.path);
        assert!(
            kf.native_path.is_none(),
            "interior frames have no native variant"
        );
        assert_eq!(kf.hash.len(), 16, "64-bit hash as hex");
        assert!(kf.timestamp_secs > shot.start_secs && kf.timestamp_secs < shot.end_secs);
    }
}

#[test]
fn fast_cut_scenes_get_zero_interior_keyframes() {
    let dir = scratch("no-subdivide");
    let bundle = bundle_for(&dir, &three_cut_fixture(&dir));

    let track = extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    assert_eq!(track.shots.len(), 3);
    for shot in &track.shots {
        assert!(
            shot.interior_keyframes.is_empty(),
            "3s shots are under the 15s interval: {:?}",
            shot.interior_keyframes
        );
    }
}
