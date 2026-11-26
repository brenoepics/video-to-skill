//! Shared fixtures for the action-track tests: a real 30 s video bundle
//! whose frame track is replaced by hand-written screen states, and a
//! scripted inverse-dynamics backend that records what it was asked.

// Test support: unwrap is fine here.
#![allow(clippy::unwrap_used, dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use vts_extract::actions::{Batch, IdmBackend, Stage};
use vts_extract::bundle::ingest;
use vts_extract::frames::{FrameTrack, Keyframe, Shot, FRAMES_FILE};
use vts_extract::tools::Toolchain;

pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-actions-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn toolchain() -> Toolchain {
    Toolchain {
        ffmpeg: PathBuf::from("ffmpeg"),
        ffprobe: PathBuf::from("ffprobe"),
        whisper_model: None,
        whisper_vad: None,
    }
}

/// A 30 s, 320x240 test pattern, ingested, with `shots` as its frame
/// track: `(start, end, cursor_presence)` each.
pub fn bundle_with_shots(dir: &Path, shots: &[(f64, f64, Option<f64>)]) -> PathBuf {
    let media = dir.join("screen.mp4");
    let res = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=10:duration=30",
            "-c:v",
            "mpeg4",
            "-q:v",
            "5",
        ])
        .arg(&media)
        .output()
        .unwrap();
    assert!(
        res.status.success(),
        "fixture generation failed: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    let bundle = dir.join("bundle");
    ingest(&media, &bundle, &toolchain()).unwrap();
    let track = FrameTrack {
        schema_version: 1,
        shots: shots
            .iter()
            .enumerate()
            .map(|(index, &(start_secs, end_secs, cursor_presence))| Shot {
                index,
                start_secs,
                end_secs,
                motion_density: 0.0,
                keyframe: Keyframe {
                    timestamp_secs: start_secs,
                    path: format!("frames/shot{index:03}.jpg"),
                    native_path: None,
                    hash: "0".repeat(16),
                },
                interior_keyframes: Vec::new(),
                changed_region: None,
                cursor_presence,
                cursor: None,
            })
            .collect(),
    };
    fs::write(
        bundle.join(FRAMES_FILE),
        serde_json::to_vec_pretty(&track).unwrap(),
    )
    .unwrap();
    bundle
}

/// What the backend saw of one request, including whether its clip was
/// on disk at the time.
#[derive(Debug, Clone)]
pub struct Seen {
    pub id: String,
    pub clip: PathBuf,
    pub clip_existed: bool,
    pub fps: f64,
    pub resized_width: u32,
    pub resized_height: u32,
    pub system: String,
}

type Answer = Box<dyn Fn(Stage, &str) -> Option<String> + Sync>;

/// Answers each request from a script keyed by stage and request id.
pub struct Scripted {
    answer: Answer,
    calls: Mutex<Vec<(Stage, PathBuf, Vec<Seen>)>>,
}

impl Scripted {
    pub fn new(answer: impl Fn(Stage, &str) -> Option<String> + Sync + 'static) -> Scripted {
        Scripted {
            answer: Box::new(answer),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> Vec<(Stage, PathBuf, Vec<Seen>)> {
        self.calls.lock().unwrap().clone()
    }
}

impl IdmBackend for Scripted {
    fn run(&self, batch: &Batch<'_>) -> anyhow::Result<Vec<Option<String>>> {
        let seen = batch
            .requests
            .iter()
            .map(|r| Seen {
                id: r.id.clone(),
                clip: r.clip.clone(),
                clip_existed: r.clip.is_file(),
                fps: r.fps,
                resized_width: r.resized_width,
                resized_height: r.resized_height,
                system: r.system.to_string(),
            })
            .collect();
        self.calls
            .lock()
            .unwrap()
            .push((batch.stage, batch.model_dir.to_path_buf(), seen));
        Ok(batch
            .requests
            .iter()
            .map(|r| (self.answer)(batch.stage, &r.id))
            .collect())
    }
}
