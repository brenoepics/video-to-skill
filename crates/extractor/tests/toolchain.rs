//! Toolchain resolution: which binaries and weights a pipeline run gets.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use vts_extract::deps::{registry, Env, Tool};
use vts_extract::tools::Toolchain;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-toolchain-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_executable(dir: &Path, name: &str) {
    let path = dir.join(name);
    fs::write(&path, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A model file with the marker a checksum-verified install of the
/// currently pinned artifact would leave beside it.
fn verified_model(models: &Path, tool: Tool) -> PathBuf {
    fs::create_dir_all(models).unwrap();
    let path = models.join(tool.file_name());
    fs::write(&path, b"weights").unwrap();
    let digest = registry::spec_for(tool).unwrap().sha256;
    fs::write(
        models.join(format!("{}.verified", tool.file_name())),
        format!("sha256:{digest}"),
    )
    .unwrap();
    path
}

#[test]
fn resolve_picks_up_the_vad_model_beside_the_whisper_weights() {
    let bin = scratch("vad-bin");
    let data = scratch("vad-data");
    fake_executable(&bin, "ffmpeg");
    fake_executable(&bin, "ffprobe");
    let whisper = verified_model(&data.join("models"), Tool::WhisperModel);
    let vad = verified_model(&data.join("models"), Tool::WhisperVad);

    let tools = Toolchain::resolve(&Env {
        path_dirs: vec![bin],
        data_dir: data,
    })
    .unwrap();

    assert_eq!(tools.whisper_model, Some(whisper));
    assert_eq!(tools.whisper_vad, Some(vad));
}

#[test]
fn resolve_leaves_vad_unset_when_only_whisper_weights_are_installed() {
    let bin = scratch("novad-bin");
    let data = scratch("novad-data");
    fake_executable(&bin, "ffmpeg");
    fake_executable(&bin, "ffprobe");
    verified_model(&data.join("models"), Tool::WhisperModel);

    let tools = Toolchain::resolve(&Env {
        path_dirs: vec![bin],
        data_dir: data,
    })
    .unwrap();

    assert!(tools.whisper_model.is_some());
    assert_eq!(tools.whisper_vad, None, "no VAD model means plain whisper");
}

#[test]
fn resolve_names_the_missing_executable() {
    let err = Toolchain::resolve(&Env {
        path_dirs: vec![scratch("missing-bin")],
        data_dir: scratch("missing-data"),
    })
    .unwrap_err();

    assert!(err.to_string().contains("ffmpeg"), "got: {err}");
}
