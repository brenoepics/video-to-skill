//! Native-resolution crops of the changed region: beside each state's
//! bounded keyframe, the frame track exports the part of the native
//! frame that changed since the previous state, so small text is read
//! at native pixel size instead of downscaled. The expected values
//! follow from how the fixture is drawn, not from the implementation.
//! Skipped (with a note) where ffmpeg has no libx264 encoder.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

#[path = "frames_states/fixture.rs"]
mod fixture;

use std::path::Path;
use std::process::Command;
use vts_extract::bundle::ingest;
use vts_extract::frames::{extract_frame_track, FrameConfig};
use vts_extract::timeline::assemble;

use fixture::{has_libx264, line_extent, scratch, screencast_fixture, toolchain, LINE_APPEARS_AT};

/// The native width of the fixture; a crop at least this wide is no
/// crop at all.
const NATIVE_W: u32 = 1920;
const NATIVE_H: u32 = 1080;

/// Pixel size of a JPEG, from its stream header.
fn image_size(path: &Path) -> (u32, u32) {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(["-show_entries", "stream=width,height", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parts = text.trim().split(',');
    let w = parts.next().unwrap().parse().unwrap();
    let h = parts.next().unwrap().parse().unwrap();
    (w, h)
}

/// The image decoded to 8-bit gray, row-major.
fn decode_gray(path: &Path) -> Vec<u8> {
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
    out.stdout
}

/// Mean gray level of the pixels inside `(left, right, top, bottom)`.
fn mean_in(gray: &[u8], width: usize, rect: (usize, usize, usize, usize)) -> f64 {
    let (left, right, top, bottom) = rect;
    let total: u64 = (top..bottom)
        .flat_map(|y| gray[y * width + left..y * width + right].iter())
        .map(|&v| u64::from(v))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    {
        total as f64 / ((bottom - top) * (right - left)) as f64
    }
}

#[test]
fn each_new_line_gets_a_native_crop_that_contains_it() {
    if !has_libx264() {
        eprintln!("skipping: ffmpeg has no libx264 encoder");
        return;
    }
    let dir = scratch("crops");
    let video = screencast_fixture(&dir);
    let bundle = dir.join("bundle");
    ingest(&video, &bundle, &toolchain()).unwrap();

    let track = extract_frame_track(&bundle, &toolchain(), &FrameConfig::default()).unwrap();

    assert_eq!(
        track.shots.len(),
        5,
        "one state per change, as the fixture is drawn"
    );
    assert!(
        track.shots[0].changed_region.is_none(),
        "the opening screen changed from nothing"
    );
    for (i, _) in LINE_APPEARS_AT.iter().enumerate() {
        let shot = &track.shots[i + 1];
        let region = shot
            .changed_region
            .as_ref()
            .unwrap_or_else(|| panic!("line {i} has no changed region: {shot:?}"));
        assert_eq!(region.path, format!("frames/shot{:03}-changed.jpg", i + 1));
        let (left, right, top, bottom) = line_extent(20 + i);
        let (x, y) = (region.x as usize, region.y as usize);
        let (w, h) = (region.width as usize, region.height as usize);
        assert!(
            x <= left && x + w >= right && y <= top && y + h >= bottom,
            "line {i} at ({left}..{right}, {top}..{bottom}) is not inside {region:?}"
        );
        assert!(
            region.width < NATIVE_W || region.height < NATIVE_H,
            "a crop must be smaller than the screen: {region:?}"
        );

        // Native pixel size: the file is exactly the recorded rectangle…
        let path = bundle.join(&region.path);
        assert_eq!(image_size(&path), (region.width, region.height));
        // …and the line's bright boxes are in it, where the rectangle says.
        let gray = decode_gray(&path);
        let line = (left - x, right - x, top - y, bottom - y);
        let around = (left - x, right - x, bottom - y, (bottom - y) + 8);
        assert!(
            mean_in(&gray, w, line) > 120.0,
            "line {i} is not bright in its crop"
        );
        assert!(
            mean_in(&gray, w, around) < 60.0,
            "the terminal background under line {i} is not dark"
        );
    }
    assert!(
        track.shots[4].changed_region.is_none(),
        "a cut replaces the whole screen, which the native still already shows"
    );

    // The timeline carries the same reference beside the keyframe.
    let timeline = assemble(&bundle).unwrap();
    for (segment, shot) in timeline.segments.iter().zip(&track.shots) {
        assert_eq!(segment.changed_region, shot.changed_region);
    }
}
