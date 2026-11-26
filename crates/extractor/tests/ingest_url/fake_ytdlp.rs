//! A stand-in `yt-dlp` for the downloader tests: a script that checks
//! the hardening flags, 403s until an explicit `-f` arrives, writes the
//! sidecars a real download leaves behind, and logs every argument.

use std::fs;
use std::path::{Path, PathBuf};

/// What yt-dlp leaves beside the media under `-o source.%(ext)s`: one
/// creator-caption file per language in the shape YouTube serves (one
/// `utf8` segment per cue, newlines inside it, no per-word offsets) and
/// the info json as written *before* post-processing — the creator's
/// `chapters` and the raw `sponsorblock_chapters` side by side.
pub fn write_canned_sidecars(dir: &Path) {
    let canned = dir.join("canned");
    fs::create_dir_all(&canned).unwrap();
    fs::write(
        canned.join("source.en.json3"),
        r#"{"wireMagic":"pb3","pens":[{}],"wsWinStyles":[{}],"wpWinPositions":[{}],"events":[
{"tStartMs":0,"dDurationMs":300000,"id":1,"wpWinPosId":1,"wsWinStyleId":1},
{"tStartMs":1200,"dDurationMs":2160,"segs":[{"utf8":"Welcome to the tutorial, in front of the\nterminal"}]},
{"tStartMs":5318,"dDurationMs":2656,"segs":[{"utf8":"run cargo build"}]}
]}"#,
    )
    .unwrap();
    fs::write(
        canned.join("source.de.vtt"),
        "WEBVTT\nKind: captions\nLanguage: de\n\n00:00:01.200 --> 00:00:03.360\nWillkommen\n",
    )
    .unwrap();
    fs::write(
        canned.join("source.info.json"),
        r#"{"id":"x","title":"Fallback Video","language":"en","duration":300.0,
"chapters":[
{"start_time":0.0,"end_time":45.0,"title":"Intro"},
{"start_time":45.0,"end_time":200.0,"title":"Install the CLI"},
{"start_time":200.0,"end_time":300.0,"title":"Wrap up"}],
"sponsorblock_chapters":[
{"start_time":280.0,"end_time":300.0,"category":"outro","title":"Endcards/Credits","type":"skip","_categories":[["outro",280.0,300.0,"Endcards/Credits"]]},
{"start_time":12.5,"end_time":40.0,"category":"sponsor","title":"Sponsor","type":"skip","_categories":[["sponsor",12.5,40.0,"Sponsor"]]}],
"formats":[{"format_id":"18","url":"https://example.invalid/not-kept"}]}"#,
    )
    .unwrap();
}

#[cfg(unix)]
pub fn fake_ytdlp(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let fake = dir.join("yt-dlp");
    fs::write(
        &fake,
        r#"#!/bin/bash
here="$(dirname "$0")"
out=""; title=""; fmt=""; ffmpeg_location=""; sponsorblock=""
ignore_config=""; no_plugin_dirs=""; no_remote_components=""
printf '%s\n' "$@" >> "$here/args.txt"
while [[ $# -gt 0 ]]; do
  case "$1" in
    -o) out="$2"; shift 2;;
    --ffmpeg-location) ffmpeg_location="$2"; shift 2;;
    --print-to-file) title="$3"; shift 3;;
    -f) fmt="$2"; shift 2;;
    --ignore-config) ignore_config=1; shift;;
    --no-plugin-dirs) no_plugin_dirs=1; shift;;
    --no-remote-components) no_remote_components=1; shift;;
    --sponsorblock-mark) sponsorblock="$2"; shift 2;;
    *) shift;;
  esac
done
if [[ -z "$ignore_config" || -z "$no_plugin_dirs" || -z "$no_remote_components" || -z "$ffmpeg_location" ]]; then
  echo "ERROR: hardened yt-dlp flags missing" >&2
  exit 2
fi
printf '%s\n' "$ffmpeg_location" >> "$here/ffmpeg-locations.txt"
dest="$(dirname "$out")"
# SponsorBlock runs after filtering, before anything is written.
if [[ -n "$sponsorblock" && -e "$here/sponsorblock-down" ]]; then
  echo "ERROR: Postprocessing: Unable to communicate with SponsorBlock API: timed out" >&2
  exit 1
fi
# Subtitles and the info json land before the media request, so they
# are on disk even when that request then 403s.
if [[ -d "$here/canned" ]]; then cp "$here"/canned/* "$dest"/; fi
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
pub fn fake_ytdlp(dir: &Path) -> PathBuf {
    let fake = dir.join("yt-dlp.cmd");
    fs::write(
        &fake,
        r#"@echo off
setlocal EnableDelayedExpansion
set "here=%~dp0"
set "out="
set "title="
set "fmt="
set "ffmpeg_location="
set "sponsorblock="
set "ignore_config="
set "no_plugin_dirs="
set "no_remote_components="
:loop
if "%~1"=="" goto parsed
>>"%here%args.txt" echo %~1
if "%~1"=="-o" (set "out=%~2" & shift & shift & goto loop)
if "%~1"=="--ffmpeg-location" (set "ffmpeg_location=%~2" & shift & shift & goto loop)
if "%~1"=="--print-to-file" (set "title=%~3" & shift & shift & shift & goto loop)
if "%~1"=="-f" (set "fmt=%~2" & shift & shift & goto loop)
if "%~1"=="--ignore-config" (set "ignore_config=1" & shift & goto loop)
if "%~1"=="--no-plugin-dirs" (set "no_plugin_dirs=1" & shift & goto loop)
if "%~1"=="--no-remote-components" (set "no_remote_components=1" & shift & goto loop)
if "%~1"=="--sponsorblock-mark" (>>"%here%args.txt" echo %~2& set "sponsorblock=%~2" & shift & shift & goto loop)
shift
goto loop
:parsed
if not defined ignore_config (echo ERROR: hardened flags missing 1>&2 & exit /b 2)
if not defined no_plugin_dirs (echo ERROR: hardened flags missing 1>&2 & exit /b 2)
if not defined no_remote_components (echo ERROR: hardened flags missing 1>&2 & exit /b 2)
if not defined ffmpeg_location (echo ERROR: FFmpeg location missing 1>&2 & exit /b 2)
>>"%here%ffmpeg-locations.txt" echo !ffmpeg_location!
for %%F in ("!out!") do set "dest=%%~dpF"
if defined sponsorblock if exist "%here%sponsorblock-down" (echo ERROR: Unable to communicate with SponsorBlock API 1>&2 & exit /b 1)
if exist "%here%canned" copy /y "%here%canned\*" "!dest!" >nul
if not defined fmt (echo ERROR: HTTP Error 403: Forbidden 1>&2 & exit /b 1)
>"!title!" echo Fallback Video
>"!dest!source.mp4" echo fake-media
"#,
    )
    .unwrap();
    fake
}
