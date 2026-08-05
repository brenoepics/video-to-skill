//! Detection: which tools are present, where, and what to do about the rest.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use vts_extract::deps::{probe, Env, Presence, Tool};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-deps-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_executable(dir: &Path, name: &str) {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn env_with(path_dirs: Vec<PathBuf>, data_dir: PathBuf) -> Env {
    Env {
        path_dirs,
        data_dir,
    }
}

#[test]
fn tool_on_path_is_reported_found_on_path() {
    let bin = scratch("path-bin");
    let data = scratch("path-data");
    fake_executable(&bin, "ffmpeg");

    let statuses = probe(&env_with(vec![bin.clone()], data));

    let ffmpeg = statuses.iter().find(|s| s.tool == Tool::Ffmpeg).unwrap();
    assert_eq!(ffmpeg.presence, Presence::OnPath);
    assert_eq!(ffmpeg.location, Some(bin.join("ffmpeg")));
}

#[test]
fn verified_tool_in_data_dir_is_reported_installed() {
    let bin = scratch("data-bin");
    let data = scratch("data-data");
    let tools = data.join("tools");
    fake_executable(&tools, "yt-dlp");
    // The verification marker is only written after a checksum-verified install.
    fs::write(tools.join("yt-dlp.verified"), "sha256:abc").unwrap();

    let statuses = probe(&env_with(vec![bin], data.clone()));

    let ytdlp = statuses.iter().find(|s| s.tool == Tool::Ytdlp).unwrap();
    assert_eq!(ytdlp.presence, Presence::Installed);
    assert_eq!(ytdlp.location, Some(tools.join("yt-dlp")));
}

#[test]
fn unverified_file_in_data_dir_is_not_counted_present() {
    let bin = scratch("corrupt-bin");
    let data = scratch("corrupt-data");
    // File exists (e.g. interrupted download) but has no verification marker.
    fake_executable(&data.join("tools"), "yt-dlp");

    let statuses = probe(&env_with(vec![bin], data));

    let ytdlp = statuses.iter().find(|s| s.tool == Tool::Ytdlp).unwrap();
    assert_eq!(ytdlp.presence, Presence::Missing);
}

#[test]
fn whisper_model_is_detected_by_verified_file_not_executability() {
    let bin = scratch("model-bin");
    let data = scratch("model-data");
    let models = data.join("models");
    fs::create_dir_all(&models).unwrap();
    fs::write(models.join("ggml-base.bin"), b"weights").unwrap();
    fs::write(models.join("ggml-base.bin.verified"), "sha256:abc").unwrap();

    let statuses = probe(&env_with(vec![bin], data.clone()));

    let model = statuses
        .iter()
        .find(|s| s.tool == Tool::WhisperModel)
        .unwrap();
    assert_eq!(model.presence, Presence::Installed);
    assert_eq!(model.location, Some(models.join("ggml-base.bin")));
}

#[test]
fn missing_everywhere_reports_missing_with_all_tools_covered() {
    let statuses = probe(&env_with(vec![scratch("none-bin")], scratch("none-data")));

    assert_eq!(statuses.len(), 4);
    assert!(statuses.iter().any(|s| s.tool == Tool::Ffprobe));
    assert!(statuses.iter().all(|s| s.presence == Presence::Missing));
    assert!(statuses.iter().all(|s| s.location.is_none()));
}

#[test]
fn path_lookup_ignores_non_executable_files() {
    let bin = scratch("nonexec-bin");
    let data = scratch("nonexec-data");
    fs::write(bin.join("ffmpeg"), "not a binary").unwrap();

    let statuses = probe(&env_with(vec![bin], data));

    let ffmpeg = statuses.iter().find(|s| s.tool == Tool::Ffmpeg).unwrap();
    assert_eq!(ffmpeg.presence, Presence::Missing);
}
