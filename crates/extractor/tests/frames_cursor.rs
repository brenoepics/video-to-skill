//! Cursor-centric evidence on a synthetic GUI walkthrough: the frame
//! track records how much of each state the cursor was on screen and,
//! for each state change in a stretch with a cursor, exports native
//! crops around the cursor just before and just after the change. The
//! expected values follow from how the fixture is drawn.
//! Skipped (with a note) where ffmpeg has no libx264 encoder.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

#[path = "frames_cursor/fixture.rs"]
mod fixture;

use std::fs;
use std::path::Path;
use std::process::Command;
use vts_extract::bundle::ingest;
use vts_extract::frames::{extract_frame_track, FrameConfig, RegionCrop, FRAMES_FILE};
use vts_extract::timeline::assemble;

use fixture::{
    has_libx264, scratch, toolchain, walkthrough_fixture, BOXES, CURSOR_AT, CURSOR_H, CURSOR_W,
    NATIVE_H, NATIVE_W,
};

/// The image decoded to 8-bit gray, row-major, with its width.
fn decode_gray(path: &Path) -> (Vec<u8>, usize) {
    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-show_entries", "stream=width", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .unwrap();
    let width = String::from_utf8_lossy(&probe.stdout)
        .trim()
        .parse()
        .unwrap();
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    (out.stdout, width)
}

/// Gray level of native pixel `(x, y)` inside a crop.
fn pixel(crop: &RegionCrop, gray: &[u8], width: usize, x: usize, y: usize) -> u8 {
    let (cx, cy) = (crop.x as usize, crop.y as usize);
    assert!(
        x >= cx && y >= cy && x < cx + crop.width as usize && y < cy + crop.height as usize,
        "({x}, {y}) lies outside {crop:?}"
    );
    gray[(y - cy) * width + (x - cx)]
}

fn contains(crop: &RegionCrop, x: usize, y: usize) -> bool {
    (crop.x as usize..(crop.x + crop.width) as usize).contains(&x)
        && (crop.y as usize..(crop.y + crop.height) as usize).contains(&y)
}

#[test]
fn state_changes_with_a_cursor_get_crops_around_it_and_presence_is_recorded() {
    if !has_libx264() {
        eprintln!("skipping: ffmpeg has no libx264 encoder");
        return;
    }
    let dir = scratch("walkthrough");
    let video = walkthrough_fixture(&dir);
    let bundle = dir.join("bundle");
    ingest(&video, &bundle, &toolchain()).unwrap();

    let track = extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    // Opening window, one state per opened box, the slide after the cut.
    let starts: Vec<f64> = track.shots.iter().map(|s| s.start_secs).collect();
    assert_eq!(starts, vec![0.0, 8.0, 13.0, 16.0], "{starts:?}");

    // The cursor is on screen for the whole window stretch (it sits
    // still before its first move, too) and never on the slide.
    let presence: Vec<f64> = track
        .shots
        .iter()
        .map(|s| s.cursor_presence.unwrap())
        .collect();
    assert!(
        presence[..3].iter().all(|&p| p >= 0.8),
        "cursor presence per state: {presence:?}"
    );
    assert!(presence[3] < 0.2, "the slide has no cursor: {presence:?}");

    // Each click's state change: crops centred where the cursor was
    // just before the change, one of the moment before and one of the
    // settled result, both at native pixel size.
    for (i, &(x, y, w, h, _)) in BOXES.iter().enumerate() {
        let shot = &track.shots[i + 1];
        let cursor = shot
            .cursor
            .as_ref()
            .unwrap_or_else(|| panic!("state {} has no cursor crops: {shot:?}", i + 1));
        let (tip_x, tip_y) = CURSOR_AT[i + 1].0;
        assert!(
            cursor.x.abs_diff(u32::try_from(tip_x).unwrap()) <= u32::try_from(CURSOR_W).unwrap()
                && cursor.y.abs_diff(u32::try_from(tip_y).unwrap())
                    <= u32::try_from(CURSOR_H).unwrap(),
            "cursor fix ({}, {}) is not at the tip ({tip_x}, {tip_y})",
            cursor.x,
            cursor.y
        );
        assert_eq!(
            cursor.before.path,
            format!("frames/shot{:03}-cursor-before.jpg", i + 1)
        );
        assert_eq!(
            cursor.after.path,
            format!("frames/shot{:03}-cursor-after.jpg", i + 1)
        );
        assert!(cursor.before.timestamp_secs < shot.start_secs);
        assert!((cursor.after.timestamp_secs - shot.keyframe.timestamp_secs).abs() < 1e-9);
        for crop in [&cursor.before, &cursor.after] {
            assert!(contains(crop, tip_x, tip_y), "{crop:?} misses the cursor");
            assert!(crop.width < u32::try_from(NATIVE_W).unwrap());
            assert!(crop.height < u32::try_from(NATIVE_H).unwrap());
            let (gray, width) = decode_gray(&bundle.join(&crop.path));
            assert_eq!(
                (width, gray.len() / width),
                (crop.width as usize, crop.height as usize)
            );
            // The arrow's body is dark a few pixels in from its tip.
            assert!(pixel(crop, &gray, width, tip_x + 2, tip_y + 6) < 80);
        }
        // The box that opened shows in the after crop and not before.
        let (bx, by) = (x + 20, y + 20);
        assert!(bx < x + w && by < y + h);
        let (before, bw) = decode_gray(&bundle.join(&cursor.before.path));
        let (after, aw) = decode_gray(&bundle.join(&cursor.after.path));
        assert!(
            pixel(&cursor.before, &before, bw, bx, by) > 200,
            "box {i} open too early"
        );
        let opened = pixel(&cursor.after, &after, aw, bx, by);
        assert!(
            (100..=160).contains(&opened),
            "box {i} missing after: {opened}"
        );
    }
    assert!(
        track.shots[0].cursor.is_none(),
        "nothing changed into the opening screen"
    );

    // The timeline carries presence and the crops beside the keyframe.
    let timeline = assemble(&bundle).unwrap();
    for (segment, shot) in timeline.segments.iter().zip(&track.shots) {
        assert_eq!(segment.cursor_presence, shot.cursor_presence);
        assert_eq!(segment.cursor, shot.cursor);
    }

    // Same media, same track, byte for byte.
    let first = fs::read(bundle.join(FRAMES_FILE)).unwrap();
    extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();
    assert_eq!(first, fs::read(bundle.join(FRAMES_FILE)).unwrap());
}
