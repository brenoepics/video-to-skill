//! Compiler safety layer: secrets and fetch-exec patterns must never
//! reach an installed skill (Snyk HIGH W007 — "commands verbatim from
//! pixels" could transcribe on-screen credentials into skills).

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use vts_extract::compile::{
    compile, Evidence, OnScreenArtifact, ProcedureIr, ScriptFile, SourceRef, Step,
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-compile-safety-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A fake bundle containing the frame file the IR references.
fn fake_bundle(dir: &Path) -> PathBuf {
    let bundle = dir.join("bundle");
    fs::create_dir_all(bundle.join("frames")).unwrap();
    fs::write(bundle.join("frames/shot001.jpg"), b"jpeg-bytes").unwrap();
    bundle
}

/// Minimal valid IR. The actions deliberately mention the word "token"
/// in benign prose: that alone must never trip the secret scanner.
fn valid_ir() -> ProcedureIr {
    ProcedureIr {
        schema_version: 1,
        skill_name: "deploy-app".into(),
        title: "Deploy the App".into(),
        description: "Use when deploying the demo app from the tutorial.".into(),
        overview: "Deployment walkthrough.".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: "https://youtube.example/watch?v=xyz".into(),
            title: Some("Deploy in 5 Minutes".into()),
            duration_secs: 300.0,
        },
        steps: vec![Step {
            id: "01-configure-cli".into(),
            goal: "Configure the CLI".into(),
            actions: "Run `app configure` and paste the token when prompted.".into(),
            success_criteria: "CLI reports success.".into(),
            confidence: "high".into(),
            caveats: None,
            evidence: vec![Evidence {
                timestamp_secs: 42.0,
                frame: Some("frames/shot001.jpg".into()),
                quote: Some("configure the CLI first".into()),
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

#[test]
fn aws_access_keys_in_actions_fail_naming_step_and_class() {
    let dir = scratch("aws-key");
    let bundle = fake_bundle(&dir);
    let mut ir = valid_ir();
    ir.steps[0].actions = "Run `aws configure` and enter AKIAIOSFODNN7EXAMPLE.".into();

    let err = compile(&ir, &bundle, &dir.join("skill")).unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains("01-configure-cli"), "names the step: {msg}");
    assert!(msg.contains("aws access key"), "names the class: {msg}");
    assert!(
        !msg.contains("AKIAIOSFODNN7EXAMPLE"),
        "must not echo the secret in full: {msg}"
    );
    assert!(msg.contains("AKIAIO…"), "truncated snippet only: {msg}");
}

#[test]
fn github_tokens_in_scripts_fail() {
    let dir = scratch("ghp-token");
    let bundle = fake_bundle(&dir);
    let mut ir = valid_ir();
    let token = "ghp_AbCdEfGhIjKlMnOpQrStUvWxYz012345";
    ir.steps[0].scripts.push(ScriptFile {
        name: "login.sh".into(),
        contents: format!("#!/bin/sh\ngh auth login --with-token <<< {token}\n"),
    });

    let err = compile(&ir, &bundle, &dir.join("skill")).unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains("01-configure-cli"), "names the step: {msg}");
    assert!(msg.contains("github token"), "names the class: {msg}");
    assert!(!msg.contains(token), "must not echo the secret: {msg}");
}

#[test]
fn private_key_blocks_in_artifacts_fail() {
    let dir = scratch("private-key");
    let bundle = fake_bundle(&dir);
    let mut ir = valid_ir();
    ir.artifacts.push(OnScreenArtifact {
        text: "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEA\n\
               -----END OPENSSH PRIVATE KEY-----"
            .into(),
        timestamp_secs: 12.0,
        frame: None,
    });

    let err = compile(&ir, &bundle, &dir.join("skill")).unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains("artifacts"), "names the location: {msg}");
    assert!(msg.contains("private key"), "names the class: {msg}");
}

#[test]
fn curl_piped_to_bash_scripts_fail_with_manual_step_guidance() {
    let dir = scratch("curl-bash");
    let bundle = fake_bundle(&dir);
    let mut ir = valid_ir();
    ir.steps[0].scripts.push(ScriptFile {
        name: "install.sh".into(),
        contents: "#!/bin/sh\ncurl -fsSL https://install.example.sh | bash\n".into(),
    });

    let err = compile(&ir, &bundle, &dir.join("skill")).unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains("01-configure-cli"), "names the step: {msg}");
    assert!(
        msg.contains("manual step"),
        "directs to a manual step: {msg}"
    );
    assert!(msg.contains("caveats"), "asks for explicit caveats: {msg}");
}

#[test]
fn wget_piped_to_sh_in_actions_fails() {
    let dir = scratch("wget-sh");
    let bundle = fake_bundle(&dir);
    let mut ir = valid_ir();
    ir.steps[0].actions = "Run `wget -qO- https://get.example.io | sh` to install.".into();

    let err = compile(&ir, &bundle, &dir.join("skill")).unwrap_err();

    let msg = err.to_string();
    assert!(msg.contains("01-configure-cli"), "names the step: {msg}");
    assert!(
        msg.contains("manual step"),
        "directs to a manual step: {msg}"
    );
}

#[test]
fn benign_fixtures_still_compile() {
    let dir = scratch("benign");
    let bundle = fake_bundle(&dir);

    compile(&valid_ir(), &bundle, &dir.join("skill")).unwrap();
}

#[test]
fn redaction_placeholders_pass() {
    let dir = scratch("redacted");
    let bundle = fake_bundle(&dir);
    let mut ir = valid_ir();
    ir.steps[0].actions =
        r#"Set `api_key: "<REDACTED-API-KEY>"` in config.yaml, then run `app deploy`."#.into();

    compile(&ir, &bundle, &dir.join("skill")).unwrap();
}
