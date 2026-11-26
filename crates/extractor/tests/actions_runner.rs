//! The subprocess backend speaks the runner protocol: one JSON batch on
//! stdin, one JSON answer set on stdout, answers matched by request id.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use vts_extract::actions::{Batch, ClipRequest, IdmBackend, Stage, SubprocessRunner};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-actions-runner-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A runner that saves its stdin beside itself and prints `stdout`.
fn runner(dir: &Path, stdout: &str, exit: i32) -> PathBuf {
    let path = dir.join("runner");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\ncat > \"$(dirname \"$0\")/batch.json\"\nprintf '%s' '{stdout}'\nexit {exit}\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn request(id: &str) -> ClipRequest {
    ClipRequest {
        id: id.into(),
        clip: PathBuf::from(format!("/clips/{id}.mp4")),
        fps: 2.0,
        resized_width: 308,
        resized_height: 252,
        system: "system turn",
        prompt: "user turn",
        max_new_tokens: 256,
    }
}

#[test]
fn the_runner_gets_the_whole_batch_and_answers_by_id() {
    let dir = scratch("answers");
    let program = runner(
        &dir,
        r#"{"responses": [{"id": "w001", "text": "[]"}, {"id": "w000", "text": "an answer"}]}"#,
        0,
    );
    let requests = [request("w000"), request("w001"), request("w002")];
    let model_dir = dir.join("model");

    let answers = SubprocessRunner { program }
        .run(&Batch {
            stage: Stage::Detect,
            model_dir: &model_dir,
            requests: &requests,
        })
        .unwrap();

    assert_eq!(
        answers,
        [Some("an answer".to_string()), Some("[]".to_string()), None]
    );
    let sent: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("batch.json")).unwrap()).unwrap();
    assert_eq!(sent["protocol"], 1);
    assert_eq!(sent["stage"], "detect");
    assert_eq!(sent["model_dir"], model_dir.to_str().unwrap());
    assert_eq!(sent["requests"].as_array().unwrap().len(), 3);
    let first = &sent["requests"][0];
    assert_eq!(first["id"], "w000");
    assert_eq!(first["clip"], "/clips/w000.mp4");
    assert_eq!(first["fps"], 2.0);
    assert_eq!(first["resized_width"], 308);
    assert_eq!(first["resized_height"], 252);
    assert_eq!(first["system"], "system turn");
    assert_eq!(first["prompt"], "user turn");
    assert_eq!(first["max_new_tokens"], 256);
}

#[test]
fn a_failing_runner_fails_the_stage() {
    let dir = scratch("fails");
    let program = runner(&dir, "", 3);
    let requests = [request("w000")];

    let error = SubprocessRunner { program }
        .run(&Batch {
            stage: Stage::Identify,
            model_dir: &dir,
            requests: &requests,
        })
        .unwrap_err();

    assert!(format!("{error:#}").contains("failed"), "{error:#}");
}
