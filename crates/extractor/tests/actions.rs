//! The opt-in action track: which segments the cursor gate hands to the
//! inverse-dynamics models, how their answers become typed events, and
//! that nothing changes for a bundle that never opted in.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

#[path = "actions/support.rs"]
mod support;

use std::fs;

use support::{bundle_with_shots, scratch, toolchain, Scripted};
use vts_extract::actions::{
    extract_action_track, load_actions, Action, Button, Models, Point, Stage, ACTIONS_FILE,
};
use vts_extract::bundle::load_manifest;
use vts_extract::deps::registry::{IDM_DETECTOR, IDM_PARAMETERIZER};
use vts_extract::timeline::{assemble, TIMELINE_FILE};

/// 0-14s pointer on screen, 14-20s slides, 20-30s pointer again.
const SHOTS: [(f64, f64, Option<f64>); 3] = [
    (0.0, 14.0, Some(0.95)),
    (14.0, 20.0, Some(0.1)),
    (20.0, 30.0, Some(0.9)),
];

const CLICK_AT_2S: &str = r#"Sure. [{"action_type": "click", "start_time": 2.0, "end_time": 2.5}]"#;
const LEFT_CLICK: &str = "<tool_call>\n{\"name\": \"computer_use\", \"arguments\": \
     {\"action\": \"left_click\", \"coordinate\": [154, 63]}}\n</tool_call>";

fn models(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let detector = dir.join("s1");
    let parameterizer = dir.join("s2");
    fs::create_dir_all(&detector).unwrap();
    fs::create_dir_all(&parameterizer).unwrap();
    (detector, parameterizer)
}

#[test]
fn only_screen_recording_segments_reach_the_detector_in_ten_second_clips() {
    let dir = scratch("gate");
    let bundle = bundle_with_shots(&dir, &SHOTS);
    let (s1, s2) = models(&dir);
    let backend = Scripted::new(|_, _| None);

    let track = extract_action_track(
        &bundle,
        &toolchain(),
        &backend,
        &Models {
            detector: &s1,
            parameterizer: &s2,
        },
    )
    .unwrap();

    let windows: Vec<(f64, f64)> = track
        .windows
        .iter()
        .map(|w| (w.start_secs, w.end_secs))
        .collect();
    assert_eq!(windows, [(0.0, 10.0), (10.0, 14.0), (20.0, 30.0)]);
    let calls = backend.calls();
    assert_eq!(calls.len(), 1, "no detection, so stage 2 never runs");
    let (stage, model_dir, requests) = &calls[0];
    assert_eq!(*stage, Stage::Detect);
    assert_eq!(model_dir, &s1);
    assert_eq!(requests.len(), 3);
    for request in requests {
        assert!(request.clip_existed, "{}", request.clip.display());
        assert!((request.fps - 2.0).abs() < f64::EPSILON);
        assert_eq!(request.resized_width % 28, 0);
        assert_eq!(request.resized_height % 28, 0);
    }
    assert_eq!(track.events, []);
}

#[test]
fn a_detected_click_becomes_a_typed_event_on_the_video_timeline() {
    let dir = scratch("click");
    let bundle = bundle_with_shots(&dir, &SHOTS);
    let (s1, s2) = models(&dir);
    let backend = Scripted::new(|stage, id| match (stage, id) {
        (Stage::Detect, "w002") => Some(CLICK_AT_2S.to_string()),
        (Stage::Identify, _) => Some(LEFT_CLICK.to_string()),
        _ => None,
    });

    let track = extract_action_track(
        &bundle,
        &toolchain(),
        &backend,
        &Models {
            detector: &s1,
            parameterizer: &s2,
        },
    )
    .unwrap();

    let calls = backend.calls();
    assert_eq!(calls.len(), 2);
    let (stage, model_dir, requests) = &calls[1];
    assert_eq!(*stage, Stage::Identify);
    assert_eq!(model_dir, &s2);
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert!(request.clip_existed);
    assert!(request.system.contains("computer_use"));

    assert_eq!(track.events.len(), 1);
    let event = &track.events[0];
    assert!(
        (event.start_secs - 22.0).abs() < 1e-9,
        "{}",
        event.start_secs
    );
    assert!((event.end_secs - 22.5).abs() < 1e-9);
    let expected_x = (154.0 / f64::from(request.resized_width) * 10_000.0).round() / 10_000.0;
    let expected_y = (63.0 / f64::from(request.resized_height) * 10_000.0).round() / 10_000.0;
    assert_eq!(
        event.action,
        Action::Click {
            button: Button::Left,
            count: 1,
            at: Some(Point {
                x: expected_x,
                y: expected_y
            }),
        }
    );
    assert!((event.confidence - 0.82).abs() < f64::EPSILON);
    assert_eq!(event.detected_by, IDM_DETECTOR.label());
    assert_eq!(event.parameters_by, Some(IDM_PARAMETERIZER.label()));

    assert_eq!(load_actions(&bundle).unwrap(), track);
    assert_eq!(
        load_manifest(&bundle).unwrap().tracks.actions.as_deref(),
        Some(ACTIONS_FILE)
    );
}

#[test]
fn an_unreadable_parameter_answer_keeps_the_detected_type_without_parameters() {
    let dir = scratch("fallback");
    let bundle = bundle_with_shots(&dir, &SHOTS);
    let (s1, s2) = models(&dir);
    let backend = Scripted::new(|stage, id| match (stage, id) {
        (Stage::Detect, "w000") => Some(
            r#"[{"action_type": "type", "start_time": "4.0", "end_time": "6.0"},
                {"action_type": "hover", "start_time": 7.0, "end_time": 7.5}]"#
                .to_string(),
        ),
        (Stage::Identify, _) => Some("I could not tell.".to_string()),
        _ => None,
    });

    let track = extract_action_track(
        &bundle,
        &toolchain(),
        &backend,
        &Models {
            detector: &s1,
            parameterizer: &s2,
        },
    )
    .unwrap();

    assert_eq!(track.events.len(), 1, "{:?}", track.events);
    let event = &track.events[0];
    assert_eq!(event.action, Action::Type { text: None });
    assert_eq!(event.parameters_by, None);
    assert!((event.confidence - 0.75).abs() < f64::EPSILON);
    assert!(
        track.notes.iter().any(|note| note.contains("hover")),
        "an unknown action type is reported, not guessed: {:?}",
        track.notes
    );
}

#[test]
fn the_timeline_carries_events_only_for_bundles_that_opted_in() {
    let dir = scratch("timeline");
    let bundle = bundle_with_shots(&dir, &SHOTS);

    assemble(&bundle).unwrap();
    let plain = fs::read_to_string(bundle.join(TIMELINE_FILE)).unwrap();
    let manifest = fs::read_to_string(bundle.join("manifest.json")).unwrap();
    assert!(!plain.contains("\"actions\""), "{plain}");
    assert!(!manifest.contains("\"actions\""), "{manifest}");
    assert!(!bundle.join(ACTIONS_FILE).exists());

    let (s1, s2) = models(&dir);
    let backend = Scripted::new(|stage, id| match (stage, id) {
        (Stage::Detect, "w002") => Some(CLICK_AT_2S.to_string()),
        (Stage::Identify, _) => Some(LEFT_CLICK.to_string()),
        _ => None,
    });
    let models = Models {
        detector: &s1,
        parameterizer: &s2,
    };
    extract_action_track(&bundle, &toolchain(), &backend, &models).unwrap();
    let timeline = assemble(&bundle).unwrap();

    let with_events: Vec<usize> = timeline
        .segments
        .iter()
        .filter(|s| !s.actions.is_empty())
        .map(|s| s.index)
        .collect();
    assert_eq!(
        with_events,
        [2],
        "the click at 22s belongs to the 20-30s segment"
    );
    assert!(matches!(
        timeline.segments[2].actions[0].action,
        Action::Click { .. }
    ));
}
