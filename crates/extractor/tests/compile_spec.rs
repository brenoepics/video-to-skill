//! Agent Skills spec compliance of the emitted package
//! (<https://agentskills.io/specification>): the frontmatter carries
//! exactly the six spec fields, the name equals the directory, and the
//! spec's field limits are enforced before anything is written.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use vts_extract::compile::{compile, Evidence, ProcedureIr, ScriptFile, SourceRef, Step};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-compile-spec-tests")
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

fn step(id: &str, scripts: Vec<ScriptFile>) -> Step {
    Step {
        id: id.into(),
        goal: format!("do {id}"),
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
        scripts,
        variants: vec![],
        preconditions: vec![],
        parameters: vec![],
        postcondition: None,
        checks: vec![],
        failure_notes: vec![],
        usage_count: 0,
        verdict: None,
    }
}

fn script(name: &str, shebang: &str) -> ScriptFile {
    ScriptFile {
        name: name.into(),
        contents: format!("{shebang}\n# In vim: :q!\n"),
    }
}

fn valid_ir() -> ProcedureIr {
    ProcedureIr {
        schema_version: 1,
        skill_name: "vim-basics".into(),
        title: "Vim Basics".into(),
        description: "Use when editing files with Vim: quitting, saving, navigation.".into(),
        license: Some("CC-BY-4.0".into()),
        compatibility: Some("Requires vim 9 on macOS or Linux.".into()),
        overview: "Vim fundamentals from a tutorial video.".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: "https://youtube.example/watch?v=abc".into(),
            title: Some("Vim in 100 Seconds".into()),
            duration_secs: 712.0,
            sha256: Some("ab".repeat(32)),
        },
        steps: vec![
            step("01-quit-vim", vec![script("quit-vim.sh", "#!/bin/sh")]),
            step("02-save-file", vec![]),
        ],
        artifacts: vec![],
        gaps: vec![],
        history: vec![],
        task_graph: vts_extract::compile::TaskGraph::default(),
    }
}

/// Lines of the leading `---` block, delimiters excluded.
fn frontmatter(skill_md: &str) -> Vec<String> {
    let mut lines = skill_md.lines();
    assert_eq!(lines.next(), Some("---"), "frontmatter must open the file");
    lines
        .take_while(|line| *line != "---")
        .map(str::to_string)
        .collect()
}

fn top_level_keys(frontmatter: &[String]) -> Vec<String> {
    frontmatter
        .iter()
        .filter(|line| !line.starts_with(' '))
        .map(|line| line.split(':').next().unwrap().to_string())
        .collect()
}

fn compiled(name: &str, ir: &ProcedureIr) -> String {
    let dir = scratch(name);
    let bundle = fake_bundle(&dir);
    let out = dir.join(&ir.skill_name);
    compile(ir, &bundle, &out).unwrap();
    fs::read_to_string(out.join("SKILL.md")).unwrap()
}

fn assert_line(frontmatter: &[String], wanted: &str, skill_md: &str) {
    assert!(
        frontmatter.contains(&wanted.to_string()),
        "missing frontmatter line {wanted:?} in:\n{skill_md}"
    );
}

#[test]
fn frontmatter_carries_exactly_the_six_spec_fields() {
    let skill_md = compiled("six-fields", &valid_ir());

    let fm = frontmatter(&skill_md);
    assert_eq!(
        top_level_keys(&fm),
        [
            "name",
            "description",
            "license",
            "compatibility",
            "metadata",
            "allowed-tools"
        ],
        "{skill_md}"
    );
    assert_line(&fm, "name: vim-basics", &skill_md);
    assert_line(&fm, r#"license: "CC-BY-4.0""#, &skill_md);
    assert_line(
        &fm,
        r#"compatibility: "Requires vim 9 on macOS or Linux.""#,
        &skill_md,
    );
    let version = env!("CARGO_PKG_VERSION");
    assert_line(
        &fm,
        &format!(r#"  video-to-skill/version: "{version}""#),
        &skill_md,
    );
    assert_line(
        &fm,
        &format!(r#"  video-to-skill/source-sha256: "{}""#, "ab".repeat(32)),
        &skill_md,
    );
    // Nothing is verified at compile time.
    assert_line(&fm, r#"  video-to-skill/verified-steps: "0/2""#, &skill_md);
    assert_line(&fm, "allowed-tools: Bash(scripts/quit-vim.sh:*)", &skill_md);
}

#[test]
fn description_is_a_quoted_yaml_scalar() {
    let mut ir = valid_ir();
    ir.description = r#"Use when: "quitting" vim; see C:\path."#.into();

    let skill_md = compiled("quoted", &ir);

    assert_line(
        &frontmatter(&skill_md),
        r#"description: "Use when: \"quitting\" vim; see C:\\path.""#,
        &skill_md,
    );
}

#[test]
fn unknown_optional_fields_are_omitted_and_license_is_never_invented() {
    let mut ir = valid_ir();
    ir.license = None;
    ir.compatibility = None;
    ir.source.sha256 = None;
    ir.steps[0].scripts.clear();

    let skill_md = compiled("minimal", &ir);

    let fm = frontmatter(&skill_md);
    assert_eq!(
        top_level_keys(&fm),
        ["name", "description", "license", "metadata"],
        "{skill_md}"
    );
    assert_line(
        &fm,
        r#"license: "Unspecified; the source video's terms apply (see provenance.json)""#,
        &skill_md,
    );
    assert!(
        !skill_md.contains("source-sha256"),
        "no hash is known: {skill_md}"
    );
}

#[test]
fn scripts_derive_compatibility_from_their_interpreters() {
    let mut ir = valid_ir();
    ir.compatibility = None;
    ir.steps[1].scripts = vec![
        script("save.py", "#!/usr/bin/env python3"),
        script("also.sh", "#!/bin/sh"),
    ];

    let skill_md = compiled("shebangs", &ir);

    let fm = frontmatter(&skill_md);
    assert_line(
        &fm,
        r#"compatibility: "Requires python3 and sh to run scripts/.""#,
        &skill_md,
    );
    assert_line(
        &fm,
        "allowed-tools: Bash(scripts/quit-vim.sh:*) Bash(scripts/save.py:*) \
         Bash(scripts/also.sh:*)",
        &skill_md,
    );
}

#[test]
fn skill_name_must_match_the_package_directory() {
    let dir = scratch("dir-name");
    let bundle = fake_bundle(&dir);
    let out = dir.join("skill");

    let err = compile(&valid_ir(), &bundle, &out).unwrap_err().to_string();

    assert!(err.contains("vim-basics"), "{err}");
    assert!(err.contains("'skill'"), "{err}");
    assert!(err.contains("directory"), "{err}");
    assert!(!out.exists(), "validation must happen before emission");
}

#[test]
fn names_follow_the_spec_limits() {
    let dir = scratch("name-limits");
    let bundle = fake_bundle(&dir);

    let mut doubled = valid_ir();
    doubled.skill_name = "vim--basics".into();
    let err = compile(&doubled, &bundle, &dir.join("vim--basics"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("consecutive"), "{err}");

    let mut long = valid_ir();
    long.skill_name = "a".repeat(65);
    let err = compile(&long, &bundle, &dir.join("a".repeat(65)))
        .unwrap_err()
        .to_string();
    assert!(err.contains("64"), "{err}");
}

#[test]
fn compatibility_is_capped_at_500_characters_and_script_names_have_no_spaces() {
    let dir = scratch("field-limits");
    let bundle = fake_bundle(&dir);

    let mut long = valid_ir();
    long.compatibility = Some("x".repeat(501));
    let err = compile(&long, &bundle, &dir.join("vim-basics"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("500"), "{err}");

    let mut spaced = valid_ir();
    spaced.steps[0].scripts[0].name = "quit vim.sh".into();
    let err = compile(&spaced, &bundle, &dir.join("vim-basics"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("allowed-tools"), "{err}");
}
