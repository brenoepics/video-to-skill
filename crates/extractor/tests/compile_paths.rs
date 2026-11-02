//! The compiler accepts only portable paths confined to its input/output roots.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use vts_extract::compile::{
    compile, Evidence, OnScreenArtifact, ProcedureIr, ScriptFile, SourceRef, Step,
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-compile-path-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn bundle(root: &Path) -> PathBuf {
    let bundle = root.join("bundle");
    fs::create_dir_all(bundle.join("frames")).unwrap();
    fs::write(bundle.join("frames/shot001.jpg"), b"frame").unwrap();
    bundle
}

fn valid_ir() -> ProcedureIr {
    ProcedureIr {
        schema_version: 1,
        skill_name: "safe-skill".into(),
        title: "Safe Skill".into(),
        description: "Use for testing safe paths.".into(),
        overview: "Overview".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: "fixture.mp4".into(),
            title: None,
            duration_secs: 1.0,
        },
        steps: vec![Step {
            id: "01-safe-step".into(),
            goal: "Exercise a safe step".into(),
            actions: "Do the safe action.".into(),
            success_criteria: "It succeeds.".into(),
            confidence: "high".into(),
            caveats: None,
            evidence: vec![Evidence {
                timestamp_secs: 0.0,
                frame: Some("frames/shot001.jpg".into()),
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

fn add_second_step(ir: &mut ProcedureIr) {
    ir.steps.push(Step {
        id: "02-second-step".into(),
        goal: "Exercise another safe step".into(),
        actions: "Do the second action.".into(),
        success_criteria: "It also succeeds.".into(),
        confidence: "high".into(),
        caveats: None,
        evidence: vec![Evidence {
            timestamp_secs: 0.5,
            frame: None,
            quote: Some("second step".into()),
            source: None,
        }],
        scripts: vec![],
        variants: vec![],
    });
}

#[test]
fn frame_parent_traversal_is_rejected_before_any_output_is_written() {
    let root = scratch("frame-parent");
    let bundle = bundle(&root);
    fs::write(root.join("outside.jpg"), b"private").unwrap();
    let mut ir = valid_ir();
    ir.steps[0].evidence[0].frame = Some("../outside.jpg".into());
    let out = root.join("out");

    let error = compile(&ir, &bundle, &out).unwrap_err().to_string();

    assert!(error.contains("bundle-relative"), "{error}");
    assert!(!out.exists(), "validation must happen before emission");
}

#[test]
fn windows_separators_and_drive_paths_are_rejected_on_every_host() {
    let root = scratch("windows-paths");
    let bundle = bundle(&root);
    for unsafe_path in [r"frames\shot001.jpg", r"C:\secrets\frame.jpg"] {
        let mut ir = valid_ir();
        ir.steps[0].evidence[0].frame = Some(unsafe_path.into());

        let error = compile(&ir, &bundle, &root.join("out"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("bundle-relative"), "{unsafe_path}: {error}");
    }
}

#[test]
fn artifact_frames_receive_the_same_confinement_validation_as_step_evidence() {
    let root = scratch("artifact");
    let bundle = bundle(&root);
    fs::write(root.join("outside.jpg"), b"private").unwrap();
    let mut ir = valid_ir();
    ir.artifacts.push(OnScreenArtifact {
        text: "visible".into(),
        timestamp_secs: 0.5,
        frame: Some("../outside.jpg".into()),
    });

    let error = compile(&ir, &bundle, &root.join("out"))
        .unwrap_err()
        .to_string();

    assert!(error.contains("artifact frame"), "{error}");
    assert!(error.contains("bundle-relative"), "{error}");
}

#[test]
fn script_names_must_be_one_portable_file_name() {
    let root = scratch("script");
    let bundle = bundle(&root);
    for unsafe_name in ["../outside.sh", r"..\outside.ps1", "C:/outside.ps1"] {
        let mut ir = valid_ir();
        ir.steps[0].scripts.push(ScriptFile {
            name: unsafe_name.into(),
            contents: "echo safe".into(),
        });

        let error = compile(&ir, &bundle, &root.join("out"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("script name"), "{unsafe_name}: {error}");
    }
    assert!(!root.join("outside.sh").exists());
    assert!(!root.join("outside.ps1").exists());
}

#[test]
fn windows_reserved_script_names_are_rejected_portably() {
    let root = scratch("reserved");
    let bundle = bundle(&root);
    for name in ["CON.ps1", "COM¹.cmd", "COM².cmd", "COM³.cmd", "LPT¹.ps1"] {
        let mut ir = valid_ir();
        ir.steps[0].scripts.push(ScriptFile {
            name: name.into(),
            contents: "Write-Output safe".into(),
        });

        let error = compile(&ir, &bundle, &root.join("out"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("reserved"), "{name}: {error}");
    }
}

#[test]
fn colliding_frame_basenames_are_rejected() {
    let root = scratch("colliding-frames");
    let bundle = bundle(&root);
    fs::create_dir_all(bundle.join("rewatch/at-a")).unwrap();
    fs::write(bundle.join("rewatch/at-a/shot001.jpg"), b"other").unwrap();
    let mut ir = valid_ir();
    add_second_step(&mut ir);
    ir.steps[1].evidence[0].frame = Some("rewatch/at-a/shot001.jpg".into());

    let error = compile(&ir, &bundle, &root.join("out"))
        .unwrap_err()
        .to_string();

    assert!(error.contains("shot001.jpg"), "{error}");
    assert!(error.contains("frames/shot001.jpg"), "{error}");
    assert!(error.contains("rewatch/at-a/shot001.jpg"), "{error}");
}

#[test]
fn artifact_frames_join_the_collision_check() {
    let root = scratch("colliding-artifact-frame");
    let bundle = bundle(&root);
    fs::create_dir_all(bundle.join("rewatch/at-b")).unwrap();
    fs::write(bundle.join("rewatch/at-b/shot001.jpg"), b"other").unwrap();
    let mut ir = valid_ir();
    ir.artifacts.push(OnScreenArtifact {
        text: "visible command".into(),
        timestamp_secs: 0.5,
        frame: Some("rewatch/at-b/shot001.jpg".into()),
    });

    let error = compile(&ir, &bundle, &root.join("out"))
        .unwrap_err()
        .to_string();

    assert!(error.contains("shot001.jpg"), "{error}");
}

#[test]
fn frame_basenames_that_only_differ_by_case_are_rejected() {
    let root = scratch("colliding-frame-case");
    let bundle = bundle(&root);
    fs::create_dir_all(bundle.join("rewatch/at-c")).unwrap();
    fs::write(bundle.join("rewatch/at-c/SHOT001.JPG"), b"other").unwrap();
    let mut ir = valid_ir();
    add_second_step(&mut ir);
    ir.steps[1].evidence[0].frame = Some("rewatch/at-c/SHOT001.JPG".into());

    let error = compile(&ir, &bundle, &root.join("out"))
        .unwrap_err()
        .to_string();

    assert!(error.contains("case-insensitive"), "{error}");
    assert!(error.contains("shot001.jpg"), "{error}");
    assert!(error.contains("SHOT001.JPG"), "{error}");
}

#[test]
fn colliding_script_names_are_rejected() {
    let root = scratch("colliding-scripts");
    let bundle = bundle(&root);
    let mut ir = valid_ir();
    add_second_step(&mut ir);
    ir.steps[0].scripts.push(ScriptFile {
        name: "safe.ps1".into(),
        contents: "first".into(),
    });

    for (name, expected) in [
        ("SAFE.PS1", "case-insensitive"),
        ("safe.ps1", "different contents"),
    ] {
        let mut candidate = ir.clone();
        candidate.steps[1].scripts.push(ScriptFile {
            name: name.into(),
            contents: "second".into(),
        });
        let error = compile(&candidate, &bundle, &root.join(name))
            .unwrap_err()
            .to_string();
        assert!(error.contains("script name collision"), "{error}");
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn the_same_frame_cited_twice_is_not_a_collision() {
    let root = scratch("same-frame-twice");
    let bundle = bundle(&root);
    let mut ir = valid_ir();
    add_second_step(&mut ir);
    ir.steps[1].evidence[0].frame = Some("frames/shot001.jpg".into());

    compile(&ir, &bundle, &root.join("out")).unwrap();
}
