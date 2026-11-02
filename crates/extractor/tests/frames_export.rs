//! Batch keyframe export: many frames per call, run concurrently, with
//! results indistinguishable from exporting each frame on its own.
//! (Split from tests/frames.rs to honor the source-file line limit.)

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::frames::export::{export_frames, worker_count, FrameRequest};
use vts_extract::tools::Toolchain;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-frames-export-tests")
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

/// Three solid colors, 3 seconds each, hard cuts between them.
fn three_cut_fixture(dir: &Path) -> PathBuf {
    let out = dir.join("cuts.mp4");
    let res = Command::new("ffmpeg")
        .args([
            "-y",
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
        ])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "fixture generation failed: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    out
}

fn request(timestamp_secs: f64, dest: &Path, max_width: Option<u32>) -> FrameRequest {
    FrameRequest {
        timestamp_secs,
        dest: dest.to_path_buf(),
        max_width,
    }
}

#[test]
fn batch_export_writes_every_request_byte_identically_to_single_exports() {
    let dir = scratch("batch");
    let media = three_cut_fixture(&dir);
    // More requests than workers, two asking for the same frame, one at
    // native size.
    let requests: Vec<FrameRequest> = (0..12)
        .map(|i| {
            let t = f64::from(i % 3) * 3.0 + 1.0;
            request(t, &dir.join(format!("b{i:02}.jpg")), Some(160))
        })
        .chain([request(7.0, &dir.join("native.jpg"), None)])
        .collect();

    export_frames(&toolchain(), &media, &requests).unwrap();

    for r in &requests {
        assert!(r.dest.is_file(), "missing {}", r.dest.display());
    }
    // Same timestamp + width → same bytes; different timestamps differ.
    let red = fs::read(dir.join("b00.jpg")).unwrap();
    assert_eq!(red, fs::read(dir.join("b03.jpg")).unwrap());
    assert_ne!(red, fs::read(dir.join("b01.jpg")).unwrap());
    // Concurrent export produces exactly what a lone export would.
    let single = dir.join("single.jpg");
    export_frames(&toolchain(), &media, &[request(1.0, &single, Some(160))]).unwrap();
    assert_eq!(red, fs::read(&single).unwrap());
}

#[test]
fn batch_export_reports_the_first_failing_request_in_input_order() {
    let dir = scratch("batch-fail");
    let media = three_cut_fixture(&dir);
    // Unwritable destinations (missing parent dir) at 5s and 2s; 1s is fine.
    let requests = [
        request(1.0, &dir.join("ok.jpg"), Some(160)),
        request(5.0, &dir.join("missing-a/x.jpg"), Some(160)),
        request(2.0, &dir.join("missing-b/y.jpg"), Some(160)),
    ];

    let err = export_frames(&toolchain(), &media, &requests).unwrap_err();

    assert!(err.to_string().contains("5.000"), "got: {err}");
    assert!(
        dir.join("ok.jpg").is_file(),
        "independent requests still run"
    );
}

#[test]
fn empty_batches_are_a_no_op() {
    let dir = scratch("empty");
    let media = three_cut_fixture(&dir);

    export_frames(&toolchain(), &media, &[]).unwrap();
}

#[test]
fn worker_count_uses_half_the_cores_bounded_by_work_and_a_cap() {
    // ffmpeg already threads each decode, so one process per two cores.
    assert_eq!(worker_count(10, 100), 5);
    assert_eq!(worker_count(3, 100), 1);
    assert_eq!(worker_count(1, 100), 1);
    assert_eq!(
        worker_count(64, 100),
        8,
        "capped so 4K decodes don't thrash"
    );
    assert_eq!(worker_count(10, 2), 2, "never more workers than requests");
    assert_eq!(worker_count(10, 0), 0);
}
