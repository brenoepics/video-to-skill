//! Skill compiler: Procedure IR → installable skill package.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use vts_extract::compile::{
    compile, Evidence, OnScreenArtifact, ProcedureIr, ScriptFile, SourceRef, Step,
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-compile-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A fake bundle containing the frame files the IR references.
fn fake_bundle(dir: &Path) -> PathBuf {
    let bundle = dir.join("bundle");
    fs::create_dir_all(bundle.join("frames")).unwrap();
    fs::write(bundle.join("frames/shot009.jpg"), b"jpeg-bytes").unwrap();
    bundle
}

fn valid_ir() -> ProcedureIr {
    ProcedureIr {
        schema_version: 1,
        skill_name: "vim-basics".into(),
        title: "Vim Basics".into(),
        description: "Use when editing files with Vim: quitting, saving, navigation.".into(),
        overview: "Vim fundamentals from a tutorial video.".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: "https://youtube.example/watch?v=abc".into(),
            title: Some("Vim in 100 Seconds".into()),
            duration_secs: 712.0,
        },
        steps: vec![
            Step {
                id: "01-quit-vim".into(),
                goal: "Quit Vim, discarding changes if needed".into(),
                actions: "Type `:q` (unmodified) or `:q!` (discard changes).".into(),
                success_criteria: "Back at the shell prompt.".into(),
                confidence: "high".into(),
                caveats: None,
                evidence: vec![Evidence {
                    timestamp_secs: 54.0,
                    frame: Some("frames/shot009.jpg".into()),
                    quote: Some("quit, discard changes".into()),
                    source: None,
                }],
                scripts: vec![ScriptFile {
                    name: "quit-vim.sh".into(),
                    contents: "#!/bin/sh\n# In vim: :q!\n".into(),
                }],
                variants: vec![],
            },
            Step {
                id: "02-paste-clipboard".into(),
                goal: "Paste from the system clipboard".into(),
                actions: "Register syntax unverified — transcript says \"+P\".".into(),
                success_criteria: "Clipboard content appears in the buffer.".into(),
                confidence: "low".into(),
                caveats: Some("Exact keystrokes were not visually confirmed.".into()),
                evidence: vec![Evidence {
                    timestamp_secs: 112.0,
                    frame: None,
                    quote: Some("entering +P into Vim".into()),
                    source: None,
                }],
                scripts: vec![],
                variants: vec![],
            },
        ],
        artifacts: vec![],
        gaps: vec!["22 of 27 keyframes not inspected".into()],
        history: vec![],
    }
}

#[test]
fn compile_produces_a_spec_compliant_package() {
    let dir = scratch("happy");
    let bundle = fake_bundle(&dir);
    let out = dir.join("skill");

    compile(&valid_ir(), &bundle, &out).unwrap();

    let skill_md = fs::read_to_string(out.join("SKILL.md")).unwrap();
    assert!(skill_md.starts_with("---\n"), "frontmatter required");
    assert!(skill_md.contains("name: vim-basics"));
    assert!(skill_md.contains("description: "));
    assert!(skill_md.contains("01-quit-vim"), "step index present");

    let step = fs::read_to_string(out.join("steps/01-quit-vim.md")).unwrap();
    assert!(step.contains("[t=0:54]"), "timestamp provenance: {step}");
    assert!(step.contains(":q!"));
    assert!(step.contains("Back at the shell prompt."));

    assert_eq!(
        fs::read(out.join("references/frames/shot009.jpg")).unwrap(),
        b"jpeg-bytes",
        "evidence frame copied into the package"
    );
    let script = out.join("scripts/quit-vim.sh");
    assert!(fs::read_to_string(&script).unwrap().contains(":q!"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&script).unwrap().permissions().mode();
        assert_ne!(mode & 0o111, 0, "scripts are executable");
    }

    let provenance: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("provenance.json")).unwrap()).unwrap();
    assert_eq!(provenance["steps"].as_array().unwrap().len(), 2);
    assert_eq!(
        provenance["source"]["original"],
        "https://youtube.example/watch?v=abc"
    );
}

#[test]
fn low_confidence_steps_are_visibly_flagged() {
    let dir = scratch("lowconf");
    let bundle = fake_bundle(&dir);
    let out = dir.join("skill");

    compile(&valid_ir(), &bundle, &out).unwrap();

    let skill_md = fs::read_to_string(out.join("SKILL.md")).unwrap();
    assert!(
        skill_md.contains("LOW CONFIDENCE"),
        "index must flag it: {skill_md}"
    );
    let step = fs::read_to_string(out.join("steps/02-paste-clipboard.md")).unwrap();
    assert!(step.contains("LOW CONFIDENCE"));
    assert!(step.contains("not visually confirmed"));
}

#[test]
fn steps_without_evidence_are_rejected() {
    let dir = scratch("no-evidence");
    let bundle = fake_bundle(&dir);
    let mut ir = valid_ir();
    ir.steps[0].evidence.clear();

    let err = compile(&ir, &bundle, &dir.join("skill")).unwrap_err();

    assert!(err.to_string().contains("01-quit-vim"), "got: {err}");
    assert!(err.to_string().contains("evidence"), "got: {err}");
}

#[test]
fn bad_skill_names_and_missing_frames_are_rejected() {
    let dir = scratch("invalid");
    let bundle = fake_bundle(&dir);

    let mut bad_name = valid_ir();
    bad_name.skill_name = "Bad Name!".into();
    let err = compile(&bad_name, &bundle, &dir.join("s1")).unwrap_err();
    assert!(err.to_string().contains("kebab-case"), "got: {err}");

    let mut ghost_frame = valid_ir();
    ghost_frame.steps[0].evidence[0].frame = Some("frames/nope.jpg".into());
    let err = compile(&ghost_frame, &bundle, &dir.join("s2")).unwrap_err();
    assert!(err.to_string().contains("nope.jpg"), "got: {err}");
}

#[test]
fn colliding_frame_basenames_are_rejected() {
    let dir = scratch("collide");
    let bundle = fake_bundle(&dir);
    fs::create_dir_all(bundle.join("rewatch/at-a")).unwrap();
    fs::write(bundle.join("rewatch/at-a/shot009.jpg"), b"other-bytes").unwrap();
    let mut ir = valid_ir();
    ir.steps[1].evidence[0].frame = Some("rewatch/at-a/shot009.jpg".into());

    let err = compile(&ir, &bundle, &dir.join("skill")).unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains("shot009.jpg"), "names the basename: {msg}");
    assert!(msg.contains("frames/shot009.jpg"), "names one path: {msg}");
    assert!(
        msg.contains("rewatch/at-a/shot009.jpg"),
        "names the other path: {msg}"
    );
}

#[test]
fn artifact_frames_join_the_collision_check() {
    let dir = scratch("collide-artifact");
    let bundle = fake_bundle(&dir);
    fs::create_dir_all(bundle.join("rewatch/at-b")).unwrap();
    fs::write(bundle.join("rewatch/at-b/shot009.jpg"), b"other-bytes").unwrap();
    let mut ir = valid_ir();
    ir.artifacts.push(OnScreenArtifact {
        text: "npm install -g vim".into(),
        timestamp_secs: 61.0,
        frame: Some("rewatch/at-b/shot009.jpg".into()),
    });

    let err = compile(&ir, &bundle, &dir.join("skill")).unwrap_err();

    assert!(err.to_string().contains("shot009.jpg"), "got: {err}");
}

#[test]
fn the_same_frame_cited_twice_is_not_a_collision() {
    let dir = scratch("same-frame-twice");
    let bundle = fake_bundle(&dir);
    let mut ir = valid_ir();
    ir.steps[1].evidence[0].frame = Some("frames/shot009.jpg".into());

    compile(&ir, &bundle, &dir.join("skill")).unwrap();
}

#[test]
fn compilation_is_pure_and_deterministic() {
    let dir = scratch("pure");
    let bundle = fake_bundle(&dir);
    let (a, b) = (dir.join("a"), dir.join("b"));

    compile(&valid_ir(), &bundle, &a).unwrap();
    compile(&valid_ir(), &bundle, &b).unwrap();

    for rel in [
        "SKILL.md",
        "steps/01-quit-vim.md",
        "steps/02-paste-clipboard.md",
        "provenance.json",
        "scripts/quit-vim.sh",
    ] {
        assert_eq!(
            fs::read(a.join(rel)).unwrap(),
            fs::read(b.join(rel)).unwrap(),
            "{rel} differs between identical compiles"
        );
    }
}

#[test]
fn variants_and_fold_history_are_rendered() {
    use vts_extract::compile::{FoldRecord, StepVariant};
    let dir = scratch("variants");
    let bundle = fake_bundle(&dir);
    let mut ir = valid_ir();
    ir.steps[0].variants.push(StepVariant {
        actions: "Press ZQ (normal mode shortcut)".into(),
        source_original: "https://youtube.example/watch?v=other".into(),
        evidence: vec![Evidence {
            timestamp_secs: 30.0,
            frame: None,
            quote: Some("ZQ shown on screen".into()),
            source: Some("https://youtube.example/watch?v=other".into()),
        }],
        note: None,
    });
    ir.history.push(FoldRecord {
        source: SourceRef {
            original: "https://youtube.example/watch?v=other".into(),
            title: Some("Another Vim Video".into()),
            duration_secs: 300.0,
        },
        date: "2026-08-05".into(),
        confirmed: vec!["02-paste-clipboard".into()],
        added: vec![],
        variants: vec!["01-quit-vim".into()],
    });

    compile(&ir, &bundle, &dir.join("skill")).unwrap();

    let step = fs::read_to_string(dir.join("skill/steps/01-quit-vim.md")).unwrap();
    assert!(step.contains("Variant"), "variant section: {step}");
    assert!(step.contains("ZQ"));
    assert!(step.contains("watch?v=other"), "variant cites its source");
    let skill_md = fs::read_to_string(dir.join("skill/SKILL.md")).unwrap();
    assert!(skill_md.contains("Sources"), "sources section: {skill_md}");
    assert!(skill_md.contains("Another Vim Video"));
    assert!(skill_md.contains("2026-08-05"));
}

#[test]
fn ir_round_trips_through_disk() {
    let dir = scratch("roundtrip");
    let ir = valid_ir();
    let path = dir.join("procedure.json");
    fs::write(&path, serde_json::to_string_pretty(&ir).unwrap()).unwrap();

    let loaded: ProcedureIr = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

    assert_eq!(loaded, ir);
}
