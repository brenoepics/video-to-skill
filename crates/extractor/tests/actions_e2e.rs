//! `extract --actions` end to end on a synthetic GUI walkthrough: the
//! real cursor tracker gates the clips, a scripted runner stands in for
//! the models, and the click lands in the timeline segment it happened
//! in. Unix only (the runner is a shell script); needs libx264 for the
//! fixture, like the cursor tests.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]
#![cfg(unix)]

#[path = "frames_cursor/fixture.rs"]
mod fixture;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use vts_extract::actions::{load_actions, Action, Button};
use vts_extract::deps::idm::{model_dir, model_ready};
use vts_extract::deps::registry::idm_models;
use vts_extract::deps::verification_marker;
use vts_extract::timeline::load_timeline;

/// Place empty files with markers for every pinned file, the state a
/// verified install leaves behind, without the 33 GB.
fn fake_install(data: &Path) {
    for model in idm_models() {
        let dir = model_dir(&model, data);
        fs::create_dir_all(&dir).unwrap();
        for file in model.files {
            let path = dir.join(file.name);
            fs::write(&path, b"").unwrap();
            fs::write(
                verification_marker(&path),
                format!("sha256:{}\n", file.sha256),
            )
            .unwrap();
        }
        assert!(model_ready(&model, data));
    }
}

/// Answers the detector with one click 7 s into the first clip, and the
/// parameterizer with a left click; logs every batch it is sent.
fn runner(dir: &Path) -> PathBuf {
    let path = dir.join("idm-runner");
    let detect = r#"{"responses":[{"id":"w000","text":"[{\"action_type\": \"click\", \"start_time\": 7.0, \"end_time\": 7.5}]"}]}"#;
    let identify = r#"{"responses":[{"id":"w000-a00","text":"<tool_call>{\"name\": \"computer_use\", \"arguments\": {\"action\": \"left_click\", \"coordinate\": [10, 10]}}</tool_call>"}]}"#;
    fs::write(
        &path,
        format!(
            "#!/bin/sh\ninput=$(cat)\nprintf '%s\\n' \"$input\" >> \"$(dirname \"$0\")/batches.jsonl\"\n\
             case \"$input\" in\n  *'\"stage\":\"detect\"'*) printf '%s' '{detect}' ;;\n  *) printf '%s' '{identify}' ;;\nesac\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn a_click_in_a_gui_walkthrough_reaches_the_timeline() {
    if !fixture::has_libx264() {
        eprintln!("skipping: ffmpeg has no libx264 encoder");
        return;
    }
    let dir = fixture::scratch("actions-e2e");
    let video = fixture::walkthrough_fixture(&dir);
    let data = dir.join("data");
    fake_install(&data);
    let runner = runner(&dir);
    let bundle = dir.join("bundle");

    let out = Command::new(env!("CARGO_BIN_EXE_vts-extract"))
        .args(["extract", video.to_str().unwrap(), "--out"])
        .arg(&bundle)
        .arg("--actions")
        .arg("--actions-runner")
        .arg(&runner)
        .env("VTS_DATA_DIR", &data)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let batches = fs::read_to_string(dir.join("batches.jsonl")).unwrap();
    let batches: Vec<serde_json::Value> = batches
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(batches.len(), 2, "one model load per stage");
    let [detector, parameterizer] = idm_models();
    assert_eq!(
        batches[0]["model_dir"],
        model_dir(&detector, &data).to_str().unwrap()
    );
    assert_eq!(
        batches[1]["model_dir"],
        model_dir(&parameterizer, &data).to_str().unwrap()
    );
    let windows = batches[0]["requests"].as_array().unwrap().len();
    assert_eq!(
        windows, 2,
        "the 16 s with a pointer, in two clips; never the slide"
    );

    let track = load_actions(&bundle).unwrap();
    assert_eq!(track.events.len(), 1, "{track:?}");
    assert!(matches!(
        track.events[0].action,
        Action::Click {
            button: Button::Left,
            count: 1,
            at: Some(_)
        }
    ));
    let timeline = load_timeline(&bundle).unwrap();
    assert_eq!(timeline.segments[0].actions, track.events);
    assert!(timeline.segments[1..].iter().all(|s| s.actions.is_empty()));
}
