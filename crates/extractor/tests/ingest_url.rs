//! URL ingest: routing, the downloader seam, and the shared bundle path.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::bundle::{ingest_any, load_manifest, DownloadedVideo, VideoDownloader};
use vts_extract::tools::Toolchain;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-url-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn toolchain() -> Toolchain {
    Toolchain {
        ffmpeg: PathBuf::from("ffmpeg"),
        ffprobe: PathBuf::from("ffprobe"),
        whisper_model: None,
    }
}

fn generate_video(path: &Path) {
    let res = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=320x240:rate=10",
            "-c:v",
            "mpeg4",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(res.status.success());
}

/// Writes a real (generated) video into the destination, like yt-dlp
/// would; or fails in configurable ways.
enum FakeDownloader {
    Working { title: &'static str },
    PrivateVideo,
    DiesMidDownload,
    MustNotBeCalled,
}

impl VideoDownloader for FakeDownloader {
    fn download(&self, _url: &str, dest_dir: &Path) -> anyhow::Result<DownloadedVideo> {
        match self {
            FakeDownloader::Working { title } => {
                let path = dest_dir.join("source.mp4");
                generate_video(&path);
                Ok(DownloadedVideo {
                    path,
                    title: Some((*title).to_string()),
                })
            }
            FakeDownloader::PrivateVideo => anyhow::bail!("Private video. Sign in if..."),
            FakeDownloader::DiesMidDownload => {
                fs::write(dest_dir.join("source.mp4"), b"partial garbage").unwrap();
                anyhow::bail!("connection reset")
            }
            FakeDownloader::MustNotBeCalled => panic!("downloader must not be called"),
        }
    }
}

#[test]
fn url_ingest_downloads_into_the_bundle_and_records_provenance() {
    let dir = scratch("happy");
    let bundle = dir.join("bundle");

    let report = ingest_any(
        "https://youtube.example/watch?v=abc",
        &bundle,
        &toolchain(),
        &FakeDownloader::Working {
            title: "How to Test Things",
        },
    )
    .unwrap();

    let m = &report.manifest;
    assert_eq!(m.source.original, "https://youtube.example/watch?v=abc");
    assert_eq!(m.source.title.as_deref(), Some("How to Test Things"));
    // Media lives inside the bundle and went through the same probe path.
    // (Canonicalize: macOS temp dirs resolve /var → /private/var.)
    assert!(Path::new(&m.source.media_path).starts_with(fs::canonicalize(&bundle).unwrap()));
    assert_eq!((m.media.width, m.media.height), (320, 240));
    assert_eq!(load_manifest(&bundle).unwrap(), *m);
}

#[test]
fn local_paths_bypass_the_downloader_entirely() {
    let dir = scratch("local");
    let video = dir.join("local.mp4");
    generate_video(&video);

    let report = ingest_any(
        video.to_str().unwrap(),
        &dir.join("bundle"),
        &toolchain(),
        &FakeDownloader::MustNotBeCalled,
    )
    .unwrap();

    assert!(report.created);
    assert!(report.manifest.source.title.is_none());
}

#[test]
fn download_failure_leaves_no_bundle_behind() {
    let dir = scratch("private");
    let bundle = dir.join("bundle");

    let err = ingest_any(
        "https://youtube.example/watch?v=private",
        &bundle,
        &toolchain(),
        &FakeDownloader::PrivateVideo,
    )
    .unwrap_err();

    assert!(err.to_string().contains("Private"), "got: {err}");
    assert!(load_manifest(&bundle).is_err(), "no manifest may exist");
}

#[test]
fn interrupted_download_is_restartable() {
    let dir = scratch("retry");
    let bundle = dir.join("bundle");
    let url = "https://youtube.example/watch?v=flaky";

    let first = ingest_any(url, &bundle, &toolchain(), &FakeDownloader::DiesMidDownload);
    assert!(first.is_err());
    assert!(load_manifest(&bundle).is_err());

    let second = ingest_any(
        url,
        &bundle,
        &toolchain(),
        &FakeDownloader::Working { title: "Recovered" },
    )
    .unwrap();
    assert!(second.created);
    assert_eq!(second.manifest.source.title.as_deref(), Some("Recovered"));
}

#[test]
fn reingesting_the_same_url_reuses_the_bundle() {
    let dir = scratch("reuse");
    let bundle = dir.join("bundle");
    let url = "https://youtube.example/watch?v=abc";
    let dl = FakeDownloader::Working { title: "Once" };

    let first = ingest_any(url, &bundle, &toolchain(), &dl).unwrap();
    assert!(first.created);
    let second = ingest_any(url, &bundle, &toolchain(), &FakeDownloader::MustNotBeCalled).unwrap();
    assert!(!second.created);
    assert_eq!(first.manifest, second.manifest);
}

/// YouTube often rejects merged-stream downloads with a 403; the
/// downloader must fall back to a combined single stream on its own.
/// Fake `yt-dlp`: fails any call without an explicit `-f`, succeeds
/// with one.
#[test]
fn ytdlp_downloader_retries_with_combined_stream_on_failure() {
    use std::os::unix::fs::PermissionsExt;
    use vts_extract::bundle::YtdlpDownloader;

    let dir = scratch("fallback");
    let fake = dir.join("yt-dlp");
    fs::write(
        &fake,
        r#"#!/bin/bash
out=""; title=""; fmt=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -o) out="$2"; shift 2;;
    --print-to-file) title="$3"; shift 3;;
    -f) fmt="$2"; shift 2;;
    *) shift;;
  esac
done
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

    let media = YtdlpDownloader::new(fake)
        .download("https://youtube.example/watch?v=x", &dir)
        .unwrap();

    assert!(media.path.ends_with("source.mp4"));
    assert_eq!(media.title.as_deref(), Some("Fallback Video"));
}

/// Real yt-dlp download of the yt-dlp project's own tiny test video.
/// Run with: `cargo test --test ingest_url -- --ignored`
#[test]
#[ignore = "requires network and the bootstrapped yt-dlp"]
fn real_youtube_download_end_to_end() {
    use vts_extract::bundle::YtdlpDownloader;
    use vts_extract::deps::{probe, Env, Presence, Tool};

    let ytdlp = probe(&Env::from_system())
        .into_iter()
        .find(|s| s.tool == Tool::Ytdlp && s.presence != Presence::Missing)
        .and_then(|s| s.location)
        .expect("run `vts-extract check --fix` first");

    let dir = scratch("real-yt");
    let bundle = dir.join("bundle");
    let report = ingest_any(
        "https://www.youtube.com/watch?v=jNQXAC9IVRw",
        &bundle,
        &toolchain(),
        &YtdlpDownloader::new(ytdlp),
    )
    .unwrap();

    let m = &report.manifest;
    assert!(m.source.title.is_some(), "title should be recorded");
    assert!(m.media.duration_secs > 5.0);
    assert!(m.media.width > 0);
}
