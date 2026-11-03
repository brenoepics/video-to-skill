//! URL ingest: routing, the downloader seam, and the shared bundle path.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use vts_extract::bundle::{
    ingest_any, load_manifest, CaptionTrack, Chapter, DownloadedVideo, PlatformPriors, SkipRange,
    VideoDownloader,
};
use vts_extract::tools::Toolchain;

#[path = "ingest_url/fake_ytdlp.rs"]
mod fake_ytdlp;
#[path = "ingest_url/ytdlp.rs"]
mod ytdlp;

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
        whisper_vad: None,
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
    Working {
        title: &'static str,
    },
    /// Like `Working`, plus everything the platform offered beside the
    /// media: a creator caption file, chapters and `SponsorBlock` ranges.
    WithPriors,
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
                    platform: PlatformPriors::default(),
                    notes: vec![],
                })
            }
            FakeDownloader::WithPriors => {
                let path = dest_dir.join("source.mp4");
                generate_video(&path);
                fs::write(
                    dest_dir.join("source.en.vtt"),
                    "WEBVTT\n\n00:00:00.500 --> 00:00:01.800\nHello there\n",
                )
                .unwrap();
                Ok(DownloadedVideo {
                    path,
                    title: Some("With Priors".into()),
                    platform: PlatformPriors {
                        captions: Some(CaptionTrack {
                            path: "source.en.vtt".into(),
                            language: "en".into(),
                            format: "vtt".into(),
                            available_languages: vec!["en".into()],
                        }),
                        chapters: vec![Chapter {
                            start_secs: 0.0,
                            end_secs: 2.0,
                            title: "Intro".into(),
                        }],
                        skips: vec![SkipRange {
                            start_secs: 1.5,
                            end_secs: 2.0,
                            category: "outro".into(),
                        }],
                    },
                    notes: vec!["ingest: SponsorBlock marks unavailable".into()],
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

/// Captions, chapters and `SponsorBlock` ranges the downloader found are
/// recorded in the manifest, and the caption file stays in the bundle.
#[test]
fn url_ingest_records_platform_priors_in_the_manifest() {
    let dir = scratch("priors-manifest");
    let bundle = dir.join("bundle");

    let report = ingest_any(
        "https://youtube.example/watch?v=priors",
        &bundle,
        &toolchain(),
        &FakeDownloader::WithPriors,
    )
    .unwrap();

    let m = &report.manifest;
    let captions = m.platform.captions.as_ref().unwrap();
    assert_eq!(captions.language, "en");
    assert!(bundle.join(&captions.path).is_file());
    assert_eq!(m.platform.chapters.len(), 1);
    assert_eq!(m.platform.chapters[0].title, "Intro");
    assert_eq!(m.platform.skips.len(), 1);
    assert_eq!(m.platform.skips[0].category, "outro");
    assert!(
        m.notes.iter().any(|n| n.contains("SponsorBlock")),
        "{:?}",
        m.notes
    );
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
    assert_eq!(report.manifest.platform, PlatformPriors::default());
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
