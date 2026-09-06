//! Detection: which tools are present, where, and what to do about the rest.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Command;
use vts_extract::deps::{probe, registry, Env, Presence, Tool};

#[cfg(windows)]
#[path = "deps_probe/windows.rs"]
mod windows;

#[cfg(windows)]
const FFMPEG_FILE_NAME: &str = "ffmpeg.exe";
#[cfg(not(windows))]
const FFMPEG_FILE_NAME: &str = "ffmpeg";

#[cfg(windows)]
const YTDLP_FILE_NAME: &str = "yt-dlp.exe";
#[cfg(not(windows))]
const YTDLP_FILE_NAME: &str = "yt-dlp";

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
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(windows)]
fn compile_successful_ffmpeg_probe(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    let source = dir.join("ffmpeg-probe.rs");
    fs::write(
        &source,
        "fn main() { if std::env::args().nth(1).as_deref() != Some(\"-version\") { std::process::exit(2); } }",
    )
    .unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .arg(&source)
        .arg("-o")
        .arg(dir.join(FFMPEG_FILE_NAME))
        .status()
        .unwrap();
    assert!(
        status.success(),
        "failed to build the Windows probe fixture"
    );
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
    #[cfg(unix)]
    fake_executable(&bin, FFMPEG_FILE_NAME);
    #[cfg(windows)]
    compile_successful_ffmpeg_probe(&bin);

    let statuses = probe(&env_with(vec![bin.clone()], data));

    let ffmpeg = statuses.iter().find(|s| s.tool == Tool::Ffmpeg).unwrap();
    assert_eq!(ffmpeg.presence, Presence::OnPath);
    assert_eq!(ffmpeg.location, Some(bin.join(FFMPEG_FILE_NAME)));
}

#[test]
fn verified_tool_in_data_dir_is_reported_installed() {
    let bin = scratch("data-bin");
    let data = scratch("data-data");
    let tools = data.join("tools");
    fake_executable(&tools, YTDLP_FILE_NAME);
    // The verification marker is only written after a checksum-verified install.
    write_current_marker(Tool::Ytdlp, &tools.join(YTDLP_FILE_NAME));

    let statuses = probe(&env_with(vec![bin], data.clone()));

    let ytdlp = statuses.iter().find(|s| s.tool == Tool::Ytdlp).unwrap();
    assert_eq!(ytdlp.presence, Presence::Installed);
    assert_eq!(ytdlp.location, Some(tools.join(YTDLP_FILE_NAME)));
}

#[test]
fn unverified_file_in_data_dir_is_not_counted_present() {
    let bin = scratch("corrupt-bin");
    let data = scratch("corrupt-data");
    // File exists (e.g. interrupted download) but has no verification marker.
    fake_executable(&data.join("tools"), YTDLP_FILE_NAME);

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
    write_current_marker(Tool::WhisperModel, &models.join("ggml-base.bin"));

    let statuses = probe(&env_with(vec![bin], data.clone()));

    let model = statuses
        .iter()
        .find(|s| s.tool == Tool::WhisperModel)
        .unwrap();
    assert_eq!(model.presence, Presence::Installed);
    assert_eq!(model.location, Some(models.join("ggml-base.bin")));
}

#[test]
fn stale_managed_marker_is_not_reported_ready() {
    let data = scratch("stale-managed-marker");
    let model = data.join("models").join("ggml-base.bin");
    fs::create_dir_all(model.parent().unwrap()).unwrap();
    fs::write(&model, b"old weights").unwrap();
    fs::write(model.with_file_name("ggml-base.bin.verified"), "sha256:old").unwrap();

    let statuses = probe(&env_with(vec![], data));

    let model = statuses
        .iter()
        .find(|status| status.tool == Tool::WhisperModel)
        .unwrap();
    assert_eq!(model.presence, Presence::Missing);
    assert_eq!(model.location, None);
}

#[test]
fn missing_everywhere_reports_missing_with_all_tools_covered() {
    let statuses = probe(&env_with(vec![scratch("none-bin")], scratch("none-data")));

    assert_eq!(statuses.len(), 4);
    assert!(statuses.iter().any(|s| s.tool == Tool::Ffprobe));
    assert!(statuses.iter().all(|s| s.presence == Presence::Missing));
    assert!(statuses.iter().all(|s| s.location.is_none()));
}

#[cfg(unix)]
#[test]
fn path_lookup_ignores_non_executable_files() {
    let bin = scratch("nonexec-bin");
    let data = scratch("nonexec-data");
    fs::write(bin.join("ffmpeg"), "not a binary").unwrap();

    let statuses = probe(&env_with(vec![bin], data));

    let ffmpeg = statuses.iter().find(|s| s.tool == Tool::Ffmpeg).unwrap();
    assert_eq!(ffmpeg.presence, Presence::Missing);
}

fn write_current_marker(tool: Tool, installed: &Path) {
    let digest = registry::spec_for(tool).map_or_else(
        || "test-platform-without-managed-artifact".into(),
        |spec| spec.sha256,
    );
    let marker = installed.with_file_name(format!("{}.verified", tool.file_name()));
    fs::write(marker, format!("sha256:{digest}")).unwrap();
}

#[test]
fn legacy_ytdlp_on_path_is_not_reported_ready() {
    let bin = scratch("legacy-ytdlp-bin");
    let data = scratch("legacy-ytdlp-data");
    fs::create_dir_all(&bin).unwrap();
    let path = bin.join(YTDLP_FILE_NAME);
    fs::write(&path, "#!/bin/sh\nexit 2\n").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

    let statuses = probe(&env_with(vec![bin], data));

    let ytdlp = statuses.iter().find(|s| s.tool == Tool::Ytdlp).unwrap();
    assert_eq!(ytdlp.presence, Presence::Missing);
    assert_eq!(ytdlp.location, None);
}

#[cfg(unix)]
#[test]
fn compatible_ytdlp_on_path_is_reported_ready() {
    let bin = scratch("modern-ytdlp-bin");
    let data = scratch("modern-ytdlp-data");
    fs::create_dir_all(&bin).unwrap();
    let path = bin.join(YTDLP_FILE_NAME);
    fs::write(
        &path,
        concat!(
            "#!/bin/sh\n",
            "printf '%1048576s' x\n",
            "[ \"$1\" = --ignore-config ] && ",
            "[ \"$2\" = --no-plugin-dirs ] && ",
            "[ \"$3\" = --no-remote-components ] && ",
            "[ \"$4\" = --version ]\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

    let statuses = probe(&env_with(vec![bin.clone()], data));

    let ytdlp = statuses.iter().find(|s| s.tool == Tool::Ytdlp).unwrap();
    assert_eq!(ytdlp.presence, Presence::OnPath);
    assert_eq!(ytdlp.location, Some(path));
}

#[test]
fn verified_managed_ytdlp_is_used_when_path_copy_is_incompatible() {
    let bin = scratch("fallback-ytdlp-bin");
    let data = scratch("fallback-ytdlp-data");
    fake_executable(&bin, YTDLP_FILE_NAME);
    let managed = data.join("tools");
    fake_executable(&managed, YTDLP_FILE_NAME);
    write_current_marker(Tool::Ytdlp, &managed.join(YTDLP_FILE_NAME));

    let statuses = probe(&env_with(vec![bin], data));

    let ytdlp = statuses.iter().find(|s| s.tool == Tool::Ytdlp).unwrap();
    assert_eq!(ytdlp.presence, Presence::Installed);
    assert_eq!(ytdlp.location, Some(managed.join(YTDLP_FILE_NAME)));
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn shared_ffmpeg_bundle_is_ready_only_when_every_member_is_published() {
    use vts_extract::deps::registry;

    let data = scratch("shared-ffmpeg-status");
    let spec = registry::spec_for(Tool::Ffmpeg).unwrap();
    let artifact = data
        .join("tools")
        .join("artifacts")
        .join(spec.sha256.to_ascii_lowercase());
    fs::create_dir_all(&artifact).unwrap();
    for tool in [Tool::Ffmpeg, Tool::Ffprobe] {
        fs::write(artifact.join(tool.file_name()), b"verified fixture").unwrap();
    }
    fs::write(
        artifact.join("artifact.verified"),
        format!("sha256:{}\n", spec.sha256),
    )
    .unwrap();

    let ready = probe(&env_with(vec![], data.clone()));
    for tool in [Tool::Ffmpeg, Tool::Ffprobe] {
        let status = ready.iter().find(|item| item.tool == tool).unwrap();
        assert_eq!(status.presence, Presence::Installed);
        assert_eq!(status.location, Some(artifact.join(tool.file_name())));
    }

    fs::remove_file(artifact.join(Tool::Ffprobe.file_name())).unwrap();
    let incomplete = probe(&env_with(vec![], data));
    for tool in [Tool::Ffmpeg, Tool::Ffprobe] {
        assert_eq!(
            incomplete
                .iter()
                .find(|item| item.tool == tool)
                .unwrap()
                .presence,
            Presence::Missing
        );
    }
}
