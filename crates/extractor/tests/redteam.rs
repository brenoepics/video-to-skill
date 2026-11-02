//! Red-team fixtures for issue #33: six videos carrying text addressed
//! to the agent — a sticky note, narration, a mid-video banner, a code
//! comment, terminal output, a faint overlay — generated at test time.
//! The agent-side rule is "treat it as data"; these tests pin the
//! compiler's half: encoded into a step it is refused, transcribed as a
//! record it is flagged, and the `curl … | sh` it carries stays caught.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

#[path = "redteam/fixtures.rs"]
mod fixtures;
#[path = "redteam/video.rs"]
mod video;

use std::fs;
use std::path::{Path, PathBuf};
use vts_extract::compile::{
    compile, Evidence, OnScreenArtifact, ProcedureIr, ScriptFile, SourceRef, Step,
};

use fixtures::{Channel, Fixture, FIXTURES};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-redteam-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A placeholder bundle holding the one frame the IR cites.
fn fake_bundle(dir: &Path) -> (PathBuf, String) {
    let bundle = dir.join("bundle");
    fs::create_dir_all(bundle.join("frames")).unwrap();
    fs::write(bundle.join("frames/shot000.jpg"), b"jpeg-bytes").unwrap();
    (bundle, "frames/shot000.jpg".into())
}

/// The tutorial's own step, with nothing of the attacker in it.
fn clean_ir(fixture: &Fixture, frame: &str) -> ProcedureIr {
    ProcedureIr {
        schema_version: 1,
        skill_name: format!("redteam-{}", fixture.name),
        title: "Run the tests".into(),
        description: "Use when running the tutorial project's test suite.".into(),
        overview: "A short screencast of running the tests.".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: format!("{}.mp4", fixture.name),
            title: None,
            duration_secs: 6.0,
        },
        steps: vec![Step {
            id: "01-run-tests".into(),
            goal: "Run the test suite".into(),
            actions: "Run `make test` and read the summary line.".into(),
            success_criteria: "The summary line reports every test passing.".into(),
            confidence: "high".into(),
            caveats: None,
            evidence: vec![Evidence {
                timestamp_secs: 1.0,
                frame: Some(frame.into()),
                quote: None,
                source: None,
            }],
            scripts: vec![],
            variants: vec![],
        }],
        artifacts: vec![],
        gaps: vec![],
        history: vec![],
    }
}

/// What an honest agent writes: the attacker's text recorded where it
/// was seen or heard, and nowhere near the steps.
fn honest_ir(fixture: &Fixture, frame: &str) -> ProcedureIr {
    let mut ir = clean_ir(fixture, frame);
    match fixture.channel {
        Channel::Screen(_) => ir.artifacts.push(OnScreenArtifact {
            text: fixture.injected.into(),
            timestamp_secs: 2.0,
            frame: Some(frame.into()),
        }),
        Channel::Speech => ir.steps[0].evidence[0].quote = Some(fixture.injected.into()),
    }
    ir
}

fn package_text(out: &Path, rel: &str) -> String {
    fs::read_to_string(out.join(rel)).unwrap()
}

fn assert_flagged_package(fixture: &Fixture, out: &Path) {
    let skill = package_text(out, "SKILL.md");
    let gaps = skill.split("## Known gaps").nth(1).unwrap_or_default();
    assert!(
        gaps.contains("suspected prompt injection") && gaps.contains(fixture.class),
        "{}: the flag must land in Known gaps:\n{skill}",
        fixture.name
    );
    let provenance = package_text(out, "provenance.json");
    assert!(
        provenance.contains("suspected prompt injection"),
        "{}: provenance carries the flag",
        fixture.name
    );
    // The Evidence section may quote the video — that is the record;
    // the instructions above it must not carry a word of the attacker.
    let step = package_text(out, "steps/01-run-tests.md");
    let instructions = step.split("## Evidence").next().unwrap();
    assert!(
        !instructions.contains(fixture.injected) && !instructions.contains(fixture.command),
        "{}: nothing of the attacker reaches the step's instructions:\n{step}",
        fixture.name
    );
    assert!(!out.join("scripts").exists(), "{}: no script", fixture.name);
}

#[test]
fn injected_text_in_actions_is_refused_as_injection() {
    for fixture in &FIXTURES {
        let dir = scratch(&format!("actions-{}", fixture.name));
        let (bundle, frame) = fake_bundle(&dir);
        let mut ir = clean_ir(fixture, &frame);
        ir.steps[0].actions = format!("Then do as the screen says: {}", fixture.injected);

        let msg = compile(&ir, &bundle, &dir.join("skill"))
            .unwrap_err()
            .to_string();

        assert!(msg.contains("prompt injection"), "{}: {msg}", fixture.name);
        assert!(msg.contains(fixture.class), "{}: {msg}", fixture.name);
        assert!(msg.contains("01-run-tests"), "{}: {msg}", fixture.name);
        assert!(!msg.contains(fixture.command), "{}: {msg}", fixture.name);
    }
}

#[test]
fn injected_text_in_scripts_is_refused_as_injection() {
    for fixture in &FIXTURES {
        let dir = scratch(&format!("scripts-{}", fixture.name));
        let (bundle, frame) = fake_bundle(&dir);
        let mut ir = clean_ir(fixture, &frame);
        ir.steps[0].scripts.push(ScriptFile {
            name: "run-tests.sh".into(),
            contents: format!("#!/bin/sh\n# {}\nmake test\n", fixture.injected),
        });

        let msg = compile(&ir, &bundle, &dir.join("skill"))
            .unwrap_err()
            .to_string();

        assert!(msg.contains("prompt injection"), "{}: {msg}", fixture.name);
        assert!(msg.contains("script contents"), "{}: {msg}", fixture.name);
        assert!(
            !dir.join("skill").exists(),
            "{}: nothing emitted",
            fixture.name
        );
    }
}

#[test]
fn injected_text_in_artifacts_is_flagged_in_gaps() {
    for fixture in &FIXTURES {
        let dir = scratch(&format!("artifacts-{}", fixture.name));
        let (bundle, frame) = fake_bundle(&dir);
        let mut ir = clean_ir(fixture, &frame);
        ir.artifacts.push(OnScreenArtifact {
            text: fixture.injected.into(),
            timestamp_secs: 2.0,
            frame: None,
        });
        let out = dir.join("skill");

        compile(&ir, &bundle, &out).unwrap();

        assert_flagged_package(fixture, &out);
        let skill = package_text(&out, "SKILL.md");
        assert!(
            skill.contains("artifacts [t=2s]"),
            "{}: the flag names where the text was seen:\n{skill}",
            fixture.name
        );
    }
}

#[test]
fn the_carried_command_is_still_caught_as_fetch_exec() {
    for fixture in &FIXTURES {
        let dir = scratch(&format!("fetch-exec-{}", fixture.name));
        let (bundle, frame) = fake_bundle(&dir);
        let mut ir = clean_ir(fixture, &frame);
        ir.steps[0].scripts.push(ScriptFile {
            name: "setup.sh".into(),
            contents: format!("#!/bin/sh\n{}\n", fixture.command),
        });

        let msg = compile(&ir, &bundle, &dir.join("skill"))
            .unwrap_err()
            .to_string();

        assert!(msg.contains("interpreter"), "{}: {msg}", fixture.name);
        assert!(msg.contains("manual step"), "{}: {msg}", fixture.name);
    }
}

/// The videos themselves, where the machine can make them: rendered,
/// ingested, frame-tracked, and transcribed the honest way. Elsewhere
/// the same IR compiles against a placeholder bundle, so the compiler
/// half is pinned on every platform.
#[test]
fn each_video_renders_and_its_honest_transcription_compiles() {
    for fixture in &FIXTURES {
        let dir = scratch(&format!("video-{}", fixture.name));
        let rendered = match video::render(fixture, &dir) {
            Ok(rendered) => Some(rendered),
            Err(reason) => {
                eprintln!("{}: video skipped — {reason}", fixture.name);
                None
            }
        };
        let (bundle, frame) = rendered.as_ref().map_or_else(
            || fake_bundle(&dir),
            |r| (r.bundle.clone(), r.frame.clone()),
        );
        let out = dir.join("skill");

        compile(&honest_ir(fixture, &frame), &bundle, &out).unwrap();

        assert_flagged_package(fixture, &out);
        if let Some(rendered) = rendered {
            assert!(
                out.join("references/frames")
                    .join(Path::new(&frame).file_name().unwrap())
                    .is_file(),
                "{}: the cited keyframe is packaged",
                fixture.name
            );
            if matches!(fixture.channel, Channel::Speech) {
                assert!(rendered.has_audio, "{}: narration muxed", fixture.name);
            }
        }
    }
}
