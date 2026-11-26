//! The action stage from the command line: off unless asked for, its
//! 33 GB of weights disclosed separately, and a missing model a note,
//! not a failure.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

#[path = "actions/support.rs"]
mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use support::scratch;
use vts_extract::bundle::load_manifest;

fn vts(data_dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vts-extract"))
        .args(args)
        .env("VTS_DATA_DIR", data_dir)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Four seconds of a test pattern: no pointer anywhere.
fn short_video(dir: &Path) -> PathBuf {
    let media = dir.join("pattern.mp4");
    let res = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=10:duration=4",
        ])
        .args(["-c:v", "mpeg4", "-q:v", "5"])
        .arg(&media)
        .output()
        .unwrap();
    assert!(res.status.success());
    media
}

#[test]
fn a_default_extraction_writes_no_action_artifact() {
    let dir = scratch("cli-default");
    let media = short_video(&dir);
    let bundle = dir.join("bundle");

    let out = vts(
        &dir.join("data"),
        &[
            "extract",
            media.to_str().unwrap(),
            "--out",
            bundle.to_str().unwrap(),
        ],
    );

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!bundle.join("actions.json").exists());
    assert!(!bundle.join("actions").exists());
    for file in ["manifest.json", "timeline.json"] {
        let raw = fs::read_to_string(bundle.join(file)).unwrap();
        assert!(!raw.contains("\"actions\""), "{file}: {raw}");
        assert!(!raw.contains("actions:"), "{file}: {raw}");
    }
}

#[test]
fn asking_for_actions_without_a_runner_is_refused_up_front() {
    let dir = scratch("cli-no-runner");

    let out = vts(&dir, &["extract", "missing.mp4", "--actions"]);

    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--actions-runner"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn missing_models_leave_a_note_instead_of_failing_the_extraction() {
    let dir = scratch("cli-no-models");
    let media = short_video(&dir);
    let bundle = dir.join("bundle");

    let out = vts(
        &dir.join("data"),
        &[
            "extract",
            media.to_str().unwrap(),
            "--out",
            bundle.to_str().unwrap(),
            "--actions",
            "--actions-runner",
            "/nonexistent/runner",
        ],
    );

    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!bundle.join("actions.json").exists());
    let notes = load_manifest(&bundle).unwrap().notes;
    assert!(
        notes
            .iter()
            .any(|n| n.starts_with("actions:") && n.contains("check --fix --actions")),
        "{notes:?}"
    );
}

#[test]
fn the_idm_weights_are_disclosed_only_on_request_and_never_fetched_without_yes() {
    let dir = scratch("cli-check");
    let data = dir.join("data");

    let plain = stdout(&vts(&data, &["check"]));
    assert!(!plain.contains("VideoAgentTrek"), "{plain}");

    let report = stdout(&vts(&data, &["check", "--actions"]));
    assert!(report.contains("✗ action detector"), "{report}");
    assert!(report.contains("✗ action parameterizer"), "{report}");

    let disclosure = stdout(&vts(&data, &["check", "--fix", "--actions"]));
    assert!(disclosure.contains("33.2 GB"), "{disclosure}");
    assert_eq!(
        disclosure
            .matches("https://huggingface.co/xlangai/VideoAgentTrek-IDM-")
            .count(),
        28,
        "{disclosure}"
    );
    assert!(
        disclosure.contains("check --fix --actions --yes"),
        "{disclosure}"
    );
    assert!(!data.join("models").join("videoagenttrek").exists());

    let inventory = stdout(&vts(&data, &["check", "--print-downloads"]));
    assert!(inventory.contains("only with --actions"), "{inventory}");
    assert!(
        inventory.contains("VideoAgentTrek-IDM-s2-7B"),
        "{inventory}"
    );
}
