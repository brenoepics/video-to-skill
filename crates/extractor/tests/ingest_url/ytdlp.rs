use std::fs;

use vts_extract::bundle::{
    ingest_any, CaptionTrack, Chapter, SkipRange, VideoDownloader, YtdlpDownloader,
};

use super::fake_ytdlp::{fake_ytdlp, write_canned_sidecars};
use super::{scratch, toolchain};

const FFMPEG_LOCATION_LOG: &str = "ffmpeg-locations.txt";
const ARGS_LOG: &str = "args.txt";

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

/// The platform's free priors are requested on every attempt: creator
/// captions only (never auto-generated ones), the info json for
/// chapters, and `SponsorBlock` *marks* — never removal, so `[t=]` keeps
/// pointing at the original timeline.
#[test]
fn ytdlp_requests_creator_captions_chapters_and_sponsorblock_marks() {
    let dir = scratch("priors-flags");
    let fake = fake_ytdlp(&dir);

    YtdlpDownloader::new(fake, dir.join("ffmpeg"))
        .download("https://youtube.example/watch?v=x", &dir)
        .unwrap();

    let args = fs::read_to_string(dir.join(ARGS_LOG)).unwrap();
    let args: Vec<&str> = args.lines().collect();
    assert!(args.contains(&"--write-subs"), "{args:?}");
    assert!(!args.contains(&"--write-auto-subs"), "{args:?}");
    assert_eq!(flag_value(&args, "--sub-langs"), Some("all,-live_chat"));
    assert_eq!(flag_value(&args, "--sub-format"), Some("json3/vtt"));
    assert!(args.contains(&"--write-info-json"), "{args:?}");
    assert_eq!(
        flag_value(&args, "--sponsorblock-mark"),
        Some("sponsor,intro,outro,selfpromo")
    );
    assert!(!args.contains(&"--sponsorblock-remove"), "{args:?}");
    assert!(args.contains(&"--no-plugin-dirs"), "{args:?}");
    assert!(args.contains(&"--no-remote-components"), "{args:?}");
}

/// yt-dlp writes one subtitle file per language plus `source.info.json`
/// beside the media. The downloader keeps the track in the spoken
/// language, discards the rest and the info json, and hands back the
/// chapters and `SponsorBlock` ranges it read.
#[test]
fn ytdlp_keeps_the_spoken_language_track_and_reads_platform_priors() {
    let dir = scratch("priors");
    let fake = fake_ytdlp(&dir);
    write_canned_sidecars(&dir);

    let media = YtdlpDownloader::new(fake, dir.join("ffmpeg"))
        .download("https://youtube.example/watch?v=x", &dir)
        .unwrap();

    let platform = &media.platform;
    assert_eq!(
        platform.captions,
        Some(CaptionTrack {
            path: "source.en.json3".into(),
            language: "en".into(),
            format: "json3".into(),
            available_languages: vec!["de".into(), "en".into()],
        })
    );
    assert!(dir.join("source.en.json3").is_file());
    assert!(
        !dir.join("source.de.vtt").exists(),
        "unused tracks are dropped"
    );
    assert!(
        !dir.join("source.info.json").exists(),
        "info json is not kept"
    );
    assert_eq!(
        platform.chapters,
        vec![
            chapter(0.0, 45.0, "Intro"),
            chapter(45.0, 200.0, "Install the CLI"),
            chapter(200.0, 300.0, "Wrap up"),
        ]
    );
    assert_eq!(
        platform.skips,
        vec![skip(12.5, 40.0, "sponsor"), skip(280.0, 300.0, "outro")]
    );
    assert!(media.notes.is_empty(), "{:?}", media.notes);
}

/// `SponsorBlock` is a third-party service; when its API is unreachable
/// yt-dlp aborts the whole download. The downloader retries without the
/// marks and says so, rather than failing the ingest.
#[test]
fn ytdlp_retries_without_sponsorblock_when_the_api_is_unreachable() {
    let dir = scratch("sponsorblock-down");
    let fake = fake_ytdlp(&dir);
    write_canned_sidecars(&dir);
    fs::write(dir.join("sponsorblock-down"), b"").unwrap();

    let media = YtdlpDownloader::new(fake, dir.join("ffmpeg"))
        .download("https://youtube.example/watch?v=x", &dir)
        .unwrap();

    assert!(media.path.ends_with("source.mp4"));
    assert_eq!(media.platform.chapters.len(), 3, "chapters still arrive");
    assert!(media.platform.captions.is_some());
    assert!(
        media.notes.iter().any(|n| n.contains("SponsorBlock")),
        "{:?}",
        media.notes
    );
    let args = fs::read_to_string(dir.join(ARGS_LOG)).unwrap();
    let marks = args.lines().filter(|l| *l == "--sponsorblock-mark").count();
    assert_eq!(
        marks, 1,
        "marks are dropped after the first SponsorBlock failure"
    );
}

fn flag_value<'a>(args: &[&'a str], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| *a == flag)
        .and_then(|i| args.get(i + 1).copied())
}

fn chapter(start: f64, end: f64, title: &str) -> Chapter {
    Chapter {
        start_secs: start,
        end_secs: end,
        title: title.into(),
    }
}

fn skip(start: f64, end: f64, category: &str) -> SkipRange {
    SkipRange {
        start_secs: start,
        end_secs: end,
        category: category.into(),
    }
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
    // "Me at the zoo" carries creator captions in English and German.
    let captions = m.platform.captions.as_ref().expect("creator captions");
    assert_eq!(captions.language, "en");
    assert!(bundle.join(&captions.path).is_file());
}
