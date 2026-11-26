//! The spec's reference validator (`skills-ref`) accepts what the
//! compiler writes, and the compile command learns the source media's
//! hash from the bundle manifest.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::bundle::{save_manifest, Manifest, MediaInfo, PlatformPriors, SourceInfo, Tracks};
use vts_extract::compile::{compile, load_ir, Evidence, ProcedureIr, ScriptFile, SourceRef, Step};

/// The pinned reference validator: `skills-ref` 0.1.1 from pypi.org, whose
/// console script is `agentskills`. An installed binary is preferred;
/// otherwise `uvx` or `pipx run` fetch exactly that version on first use.
const VALIDATOR_SPEC: &str = "skills-ref==0.1.1";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-compile-validator-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

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
        license: None,
        compatibility: Some("Requires vim 9 on macOS or Linux.".into()),
        overview: "Vim fundamentals from a tutorial video.".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: "https://youtube.example/watch?v=abc".into(),
            title: Some("Vim in 100 Seconds".into()),
            duration_secs: 712.0,
            sha256: None,
        },
        steps: vec![Step {
            id: "01-quit-vim".into(),
            goal: "Quit Vim, discarding changes if needed".into(),
            actions: "Type `:q!`.".into(),
            success_criteria: "Back at the shell prompt.".into(),
            confidence: "high".into(),
            caveats: None,
            evidence: vec![Evidence {
                timestamp_secs: 54.0,
                frame: Some("frames/shot009.jpg".into()),
                quote: Some("quit".into()),
                source: None,
            }],
            scripts: vec![ScriptFile {
                name: "quit-vim.sh".into(),
                contents: "#!/bin/sh\n# In vim: :q!\n".into(),
            }],
            variants: vec![],
            preconditions: vec![],
            parameters: vec![],
            postcondition: None,
            failure_notes: vec![],
            usage_count: 0,
            verdict: None,
        }],
        artifacts: vec![],
        gaps: vec![],
        history: vec![],
        task_graph: vts_extract::compile::TaskGraph::default(),
    }
}

fn manifest(sha256: &str) -> Manifest {
    Manifest {
        schema_version: 1,
        source: SourceInfo {
            original: "https://youtube.example/watch?v=abc".into(),
            media_path: "/abs/v.mp4".into(),
            sha256: sha256.into(),
            title: None,
        },
        media: MediaInfo {
            container: "mp4".into(),
            duration_secs: 712.0,
            width: 1280,
            height: 720,
            fps: 30.0,
            has_audio: true,
            language: None,
        },
        tracks: Tracks::default(),
        platform: PlatformPriors::default(),
        notes: vec![],
    }
}

#[test]
fn loading_an_ir_fills_the_source_hash_from_the_bundle_manifest() {
    let dir = scratch("load-ir");
    let bundle = fake_bundle(&dir);
    let mut ir = valid_ir();
    let path = dir.join("procedure.json");
    fs::write(&path, serde_json::to_string_pretty(&ir).unwrap()).unwrap();

    assert_eq!(
        load_ir(&path, &bundle).unwrap().source.sha256,
        None,
        "no manifest, no hash"
    );

    save_manifest(&bundle, &manifest(&"cd".repeat(32))).unwrap();
    assert_eq!(
        load_ir(&path, &bundle).unwrap().source.sha256.as_deref(),
        Some("cd".repeat(32).as_str()),
        "the bundle manifest is the source of truth for the media hash"
    );

    ir.source.sha256 = Some("ef".repeat(32));
    fs::write(&path, serde_json::to_string_pretty(&ir).unwrap()).unwrap();
    assert_eq!(
        load_ir(&path, &bundle).unwrap().source.sha256.as_deref(),
        Some("ef".repeat(32).as_str()),
        "an authored hash is kept"
    );
}

/// The reference validator as an argv prefix, or `None` when no way to
/// run the pinned version exists on this machine.
fn validator() -> Option<Vec<String>> {
    let candidates: [&[&str]; 4] = [
        &["agentskills"],
        &["skills-ref"],
        &["uvx", "--from", VALIDATOR_SPEC, "agentskills"],
        &["pipx", "run", "--spec", VALIDATOR_SPEC, "agentskills"],
    ];
    candidates
        .iter()
        .find(|argv| {
            Command::new(argv[0])
                .args(&argv[1..])
                .arg("--version")
                .output()
                .is_ok_and(|o| o.status.success())
        })
        .map(|argv| argv.iter().map(ToString::to_string).collect())
}

#[test]
fn the_reference_validator_accepts_a_compiled_package() {
    let Some(argv) = validator() else {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI must run the pinned validator: install {VALIDATOR_SPEC}, uv or pipx"
        );
        eprintln!("skipping: no way to run {VALIDATOR_SPEC} (install it, uv or pipx)");
        return;
    };
    let dir = scratch("validator");
    let bundle = fake_bundle(&dir);
    let package = dir.join("vim-basics");
    let mut ir = valid_ir();
    ir.description = r#"Use when: "quitting" vim — see C:\path."#.into();
    ir.source.sha256 = Some("ab".repeat(32));
    compile(&ir, &bundle, &package).unwrap();
    let run = |dir: &Path| {
        Command::new(&argv[0])
            .args(&argv[1..])
            .arg("validate")
            .arg(dir)
            .output()
            .unwrap()
    };

    let accepted = run(&package);
    assert!(
        accepted.status.success(),
        "validator rejected the package:\n{}{}",
        String::from_utf8_lossy(&accepted.stdout),
        String::from_utf8_lossy(&accepted.stderr)
    );

    // Prove the validator is live: the same package under another name
    // breaks the name == directory rule.
    let renamed = package.with_file_name("renamed");
    fs::rename(&package, &renamed).unwrap();
    let rejected = run(&renamed);
    assert!(!rejected.status.success(), "a misnamed directory must fail");
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("must match skill name"),
        "{}",
        String::from_utf8_lossy(&rejected.stderr)
    );
}
