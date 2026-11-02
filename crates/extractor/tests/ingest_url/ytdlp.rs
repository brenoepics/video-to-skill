use std::fs;
use std::path::{Path, PathBuf};

use vts_extract::bundle::{ingest_any, VideoDownloader, YtdlpDownloader};

use super::{scratch, toolchain};

const FFMPEG_LOCATION_LOG: &str = "ffmpeg-locations.txt";

/// YouTube often rejects merged-stream downloads with a 403; the
/// downloader must fall back to a combined single stream on its own.
/// Fake `yt-dlp`: fails any call without an explicit `-f`, succeeds
/// with one, and records the `FFmpeg` path from every attempt.
#[test]
fn ytdlp_hardens_retries_and_receives_ffmpeg_location() {
    let dir = scratch("fallback");
    let fake = fake_ytdlp(&dir);
    let ffmpeg = dir.join("managed tools").join("ffmpeg executable");

    let media = YtdlpDownloader::new(fake, ffmpeg.clone())
        .download("https://youtube.example/watch?v=x", &dir)
        .unwrap();

    assert!(media.path.ends_with("source.mp4"));
    assert_eq!(media.title.as_deref(), Some("Fallback Video"));
    let locations = fs::read_to_string(dir.join(FFMPEG_LOCATION_LOG)).unwrap();
    assert_eq!(
        locations.lines().collect::<Vec<_>>(),
        vec![ffmpeg.to_string_lossy(), ffmpeg.to_string_lossy(),]
    );
}

#[cfg(unix)]
fn fake_ytdlp(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let fake = dir.join("yt-dlp");
    fs::write(
        &fake,
        r#"#!/bin/bash
out=""; title=""; fmt=""; ffmpeg_location=""
ignore_config=""; no_plugin_dirs=""; no_remote_components=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -o) out="$2"; shift 2;;
    --ffmpeg-location) ffmpeg_location="$2"; shift 2;;
    --print-to-file) title="$3"; shift 3;;
    -f) fmt="$2"; shift 2;;
    --ignore-config) ignore_config=1; shift;;
    --no-plugin-dirs) no_plugin_dirs=1; shift;;
    --no-remote-components) no_remote_components=1; shift;;
    *) shift;;
  esac
done
if [[ -z "$ignore_config" || -z "$no_plugin_dirs" || -z "$no_remote_components" || -z "$ffmpeg_location" ]]; then
  echo "ERROR: hardened yt-dlp flags missing" >&2
  exit 2
fi
printf '%s\n' "$ffmpeg_location" >> "$(dirname "$0")/ffmpeg-locations.txt"
if [[ -z "$fmt" ]]; then
  echo "ERROR: unable to download video data: HTTP Error 403: Forbidden" >&2
  exit 1
fi
echo "Fallback Video" > "$title"
echo fake-media > "${out/\%(ext)s/mp4}"
"#,
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    fake
}

#[cfg(windows)]
fn fake_ytdlp(dir: &Path) -> PathBuf {
    let fake = dir.join("yt-dlp.cmd");
    fs::write(
        &fake,
        r#"@echo off
setlocal EnableDelayedExpansion
set "out="
set "title="
set "fmt="
set "ffmpeg_location="
set "ignore_config="
set "no_plugin_dirs="
set "no_remote_components="
:loop
if "%~1"=="" goto parsed
if "%~1"=="-o" (set "out=%~2" & shift & shift & goto loop)
if "%~1"=="--ffmpeg-location" (set "ffmpeg_location=%~2" & shift & shift & goto loop)
if "%~1"=="--print-to-file" (set "title=%~3" & shift & shift & shift & goto loop)
if "%~1"=="-f" (set "fmt=%~2" & shift & shift & goto loop)
if "%~1"=="--ignore-config" (set "ignore_config=1" & shift & goto loop)
if "%~1"=="--no-plugin-dirs" (set "no_plugin_dirs=1" & shift & goto loop)
if "%~1"=="--no-remote-components" (set "no_remote_components=1" & shift & goto loop)
shift
goto loop
:parsed
if not defined ignore_config (echo ERROR: hardened flags missing 1>&2 & exit /b 2)
if not defined no_plugin_dirs (echo ERROR: hardened flags missing 1>&2 & exit /b 2)
if not defined no_remote_components (echo ERROR: hardened flags missing 1>&2 & exit /b 2)
if not defined ffmpeg_location (echo ERROR: FFmpeg location missing 1>&2 & exit /b 2)
for %%F in ("!out!") do echo !ffmpeg_location!>>"%%~dpFffmpeg-locations.txt"
if not defined fmt (echo ERROR: HTTP Error 403: Forbidden 1>&2 & exit /b 1)
>"!title!" echo Fallback Video
for %%F in ("!out!") do >"%%~dpFsource.mp4" echo fake-media
"#,
    )
    .unwrap();
    fake
}

/// Real yt-dlp download of the yt-dlp project's own tiny test video.
/// Run with: `cargo test --test ingest_url -- --ignored`
#[test]
#[ignore = "requires network and the bootstrapped yt-dlp"]
fn real_youtube_download_end_to_end() {
    use vts_extract::deps::{probe, Env, Presence, Tool};

    let env = Env::from_system();
    let ytdlp = probe(&env)
        .into_iter()
        .find(|s| s.tool == Tool::Ytdlp && s.presence != Presence::Missing)
        .and_then(|s| s.location)
        .expect("run `vts-extract check --fix --yes` first");
    let tools = toolchain();
    let downloader = YtdlpDownloader::new(ytdlp, tools.ffmpeg.clone());

    let dir = scratch("real-yt");
    let bundle = dir.join("bundle");
    let report = ingest_any(
        "https://www.youtube.com/watch?v=jNQXAC9IVRw",
        &bundle,
        &tools,
        &downloader,
    )
    .unwrap();

    let m = &report.manifest;
    assert!(m.source.title.is_some(), "title should be recorded");
    assert!(m.media.duration_secs > 5.0);
    assert!(m.media.width > 0);
}
