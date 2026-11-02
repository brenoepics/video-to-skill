//! Public persistence seams replace complete JSON artifacts on every save.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

use vts_extract::bundle::{
    load_manifest, load_transcript, save_manifest, save_transcript, Manifest, MediaInfo,
    SourceInfo, Tracks, Transcript,
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-atomic-write-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn manifest(original: &str) -> Manifest {
    Manifest {
        schema_version: 1,
        source: SourceInfo {
            original: original.into(),
            media_path: "video.mp4".into(),
            sha256: "ab".repeat(32),
            title: None,
        },
        media: MediaInfo {
            container: "mp4".into(),
            duration_secs: 1.0,
            width: 320,
            height: 240,
            fps: 10.0,
            has_audio: false,
            language: None,
        },
        tracks: Tracks::default(),
        notes: vec![],
    }
}

#[test]
fn saving_a_manifest_twice_atomically_replaces_the_first_version() {
    let dir = scratch("manifest");
    save_manifest(&dir, &manifest("first.mp4")).unwrap();

    save_manifest(&dir, &manifest("second.mp4")).unwrap();

    assert_eq!(load_manifest(&dir).unwrap().source.original, "second.mp4");
    assert!(!dir.join("manifest.json.tmp").exists());
}

#[test]
fn saving_a_transcript_twice_atomically_replaces_the_first_version() {
    let dir = scratch("transcript");
    let first = Transcript {
        schema_version: 1,
        language: Some("en".into()),
        segments: vec![],
    };
    let second = Transcript {
        schema_version: 1,
        language: Some("pt".into()),
        segments: vec![],
    };
    save_transcript(&dir, &first).unwrap();

    save_transcript(&dir, &second).unwrap();

    assert_eq!(load_transcript(&dir).unwrap(), second);
    assert!(!dir.join("transcript.json.tmp").exists());
}
