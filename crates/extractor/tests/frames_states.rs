//! Evidence-frame placement on a synthetic screencast: every screen
//! change yields one state whose keyframe is the first steady second
//! after the change, a slow scroll is folded into its surrounding state,
//! and a hard cut is still a boundary. The expected values follow from
//! how the fixture is drawn, not from the implementation.
//! Skipped (with a note) where ffmpeg has no libx264 encoder.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

#[path = "frames_states/fixture.rs"]
mod fixture;

use std::fs;
use vts_extract::bundle::ingest;
use vts_extract::frames::{extract_frame_track, FrameConfig, FRAMES_FILE};

use fixture::{
    has_libx264, scratch, screencast_fixture, toolchain, CUT_AT, LINE_APPEARS_AT, SCROLL_FROM,
    SCROLL_TO,
};

#[test]
fn keyframes_land_on_the_first_steady_second_after_each_change() {
    if !has_libx264() {
        eprintln!("skipping: ffmpeg has no libx264 encoder");
        return;
    }
    let dir = scratch("screencast");
    let video = screencast_fixture(&dir);
    let bundle = dir.join("bundle");
    ingest(&video, &bundle, &toolchain()).unwrap();

    let track = extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    let keyframes: Vec<f64> = track
        .shots
        .iter()
        .map(|s| s.keyframe.timestamp_secs)
        .collect();
    // One state per appended line, its evidence frame being the first
    // whole second after the line appeared; the opening screen and the
    // hard cut are states too; the scroll adds none.
    let mut expected = vec![0.0];
    expected.extend(LINE_APPEARS_AT.iter().map(|t| t.ceil()));
    expected.push(CUT_AT.ceil());
    assert_eq!(keyframes, expected, "evidence timestamps: {keyframes:?}");

    let starts: Vec<f64> = track.shots.iter().map(|s| s.start_secs).collect();
    assert!(
        !starts
            .iter()
            .any(|&t| t > LINE_APPEARS_AT[2].ceil() && t < CUT_AT),
        "the scroll must not cut: {starts:?}"
    );
    let scrolled = &track.shots[3];
    assert!(scrolled.start_secs <= SCROLL_FROM && scrolled.end_secs >= SCROLL_TO + 1.0);
    assert!((track.shots[4].start_secs - CUT_AT.ceil()).abs() < f64::EPSILON);
    for shot in &track.shots {
        assert!(bundle.join(&shot.keyframe.path).is_file());
        assert!(bundle
            .join(shot.keyframe.native_path.as_ref().unwrap())
            .is_file());
    }

    // Same media, same track, byte for byte.
    let first = fs::read(bundle.join(FRAMES_FILE)).unwrap();
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();
    let second = fs::read(bundle.join(FRAMES_FILE)).unwrap();
    assert_eq!(first, second, "frames.json must be byte-identical");
}
