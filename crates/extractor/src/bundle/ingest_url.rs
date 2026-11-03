//! URL ingest: download into the bundle, then flow through the exact
//! same local-file ingest path (one probe/manifest code path for both).

mod priors;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::{ingest, load_manifest, save_manifest, IngestReport, PlatformPriors};
use crate::tools::Toolchain;

/// No user config, no plugins, no remote components — every attempt.
const HARDENING: [&str; 3] = [
    "--ignore-config",
    "--no-plugin-dirs",
    "--no-remote-components",
];
/// Creator captions only, never `--write-auto-subs`. Every language the
/// creator published is fetched because the spoken language is only
/// known once the info json is read; `priors` then keeps one track.
/// `live_chat` is a stream replay, not captions.
const CAPTION_FLAGS: [&str; 5] = [
    "--write-subs",
    "--sub-langs",
    "all,-live_chat",
    "--sub-format",
    "json3/vtt",
];
/// Mark, never remove: cutting would shift every `[t=]` off the
/// original timeline.
const SPONSORBLOCK_FLAGS: [&str; 2] = ["--sponsorblock-mark", "sponsor,intro,outro,selfpromo"];
const SPONSORBLOCK_DOWN_NOTE: &str =
    "ingest: SponsorBlock lookup failed — downloaded without skip marks";

/// What a downloader hands back: the media file it wrote (inside the
/// bundle), the video's title when known, and what the platform offered
/// beside the media.
#[derive(Debug)]
pub struct DownloadedVideo {
    pub path: PathBuf,
    pub title: Option<String>,
    pub platform: PlatformPriors,
    /// Degradations worth keeping in the manifest notes.
    pub notes: Vec<String>,
}

/// Transport seam for video downloads — tests substitute canned files
/// and failures; production uses [`YtdlpDownloader`].
pub trait VideoDownloader {
    fn download(&self, url: &str, dest_dir: &Path) -> Result<DownloadedVideo>;
}

/// Route any input — URL or local path — to the right ingest.
pub fn ingest_any(
    input: &str,
    bundle_dir: &Path,
    tools: &Toolchain,
    downloader: &dyn VideoDownloader,
) -> Result<IngestReport> {
    if input.starts_with("http://") || input.starts_with("https://") {
        ingest_url(input, bundle_dir, tools, downloader)
    } else {
        ingest(Path::new(input), bundle_dir, tools)
    }
}

fn ingest_url(
    url: &str,
    bundle_dir: &Path,
    tools: &Toolchain,
    downloader: &dyn VideoDownloader,
) -> Result<IngestReport> {
    if bundle_dir.join(super::MANIFEST_FILE).is_file() {
        let existing = load_manifest(bundle_dir)?;
        if existing.source.original != url {
            bail!(
                "'{}' already holds a bundle for a different video ({}) — choose another output directory",
                bundle_dir.display(),
                existing.source.original,
            );
        }
        return Ok(IngestReport {
            manifest: existing,
            created: false,
        });
    }

    fs::create_dir_all(bundle_dir)?;
    let outcome = download_then_ingest(url, bundle_dir, tools, downloader);
    if outcome.is_err() {
        // Restartable: a failed attempt leaves no manifest and no
        // half-downloaded media (or sidecars) behind.
        let _ = fs::remove_file(bundle_dir.join(super::MANIFEST_FILE));
        for entry in fs::read_dir(bundle_dir).into_iter().flatten().flatten() {
            if entry.file_name().to_string_lossy().starts_with("source.") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    outcome
}

fn download_then_ingest(
    url: &str,
    bundle_dir: &Path,
    tools: &Toolchain,
    downloader: &dyn VideoDownloader,
) -> Result<IngestReport> {
    let media = downloader.download(url, bundle_dir)?;
    let mut report = ingest(&media.path, bundle_dir, tools)?;
    // Provenance: the manifest records the URL the user gave, not the
    // temp path the file landed at.
    report.manifest.source.original = url.to_string();
    report.manifest.source.title = media.title;
    report.manifest.platform = media.platform;
    report.manifest.notes.extend(media.notes);
    save_manifest(bundle_dir, &report.manifest)?;
    Ok(report)
}

/// Production downloader: shells out to the bootstrapped yt-dlp.
pub struct YtdlpDownloader {
    ytdlp: PathBuf,
    ffmpeg: PathBuf,
}

impl YtdlpDownloader {
    #[must_use]
    pub fn new(ytdlp: PathBuf, ffmpeg: PathBuf) -> YtdlpDownloader {
        YtdlpDownloader { ytdlp, ffmpeg }
    }
}

impl VideoDownloader for YtdlpDownloader {
    fn download(&self, url: &str, dest_dir: &Path) -> Result<DownloadedVideo> {
        // YouTube intermittently 403s merged (separate video+audio)
        // downloads; a combined single stream usually still works, so
        // retry with one before giving up.
        let format_attempts: [Option<&str>; 2] = [None, Some("b[ext=mp4]/b")];
        let mut sponsorblock = true;
        let mut last_error = None;
        for format in format_attempts {
            let mut outcome = self.attempt(url, dest_dir, format, sponsorblock);
            if sponsorblock
                && outcome
                    .as_ref()
                    .is_err_and(|err| err.to_string().contains("SponsorBlock"))
            {
                // A third-party lookup must never block the download.
                sponsorblock = false;
                outcome = self.attempt(url, dest_dir, format, false);
            }
            match outcome {
                Ok(mut video) => {
                    if !sponsorblock {
                        video.platform.skips.clear();
                        video.notes.push(SPONSORBLOCK_DOWN_NOTE.to_string());
                    }
                    return Ok(video);
                }
                Err(err) => last_error = Some(err),
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("yt-dlp produced no output")))
    }
}

impl YtdlpDownloader {
    fn attempt(
        &self,
        url: &str,
        dest_dir: &Path,
        format: Option<&str>,
        sponsorblock: bool,
    ) -> Result<DownloadedVideo> {
        let title_file = dest_dir.join("title.txt");
        let mut cmd = std::process::Command::new(&self.ytdlp);
        cmd.args(HARDENING)
            .args(["--no-playlist", "--merge-output-format", "mp4", "-o"])
            .arg(dest_dir.join("source.%(ext)s"))
            .arg("--ffmpeg-location")
            .arg(&self.ffmpeg)
            .args(["--print-to-file", "%(title)s"])
            .arg(&title_file)
            .arg("--no-simulate")
            .args(CAPTION_FLAGS)
            .arg("--write-info-json");
        if sponsorblock {
            cmd.args(SPONSORBLOCK_FLAGS);
        }
        if let Some(format) = format {
            cmd.args(["-f", format]);
        }
        cmd.arg(url);
        // A yt-dlp that is still being written (a test stand-in, or a
        // concurrent install) fails to exec with "text file busy" for an
        // instant; wait it out like the PATH probe does.
        let output = crate::deps::retry_while_busy(|| cmd.output()).context("running yt-dlp")?;
        if !output.status.success() {
            bail!(
                "download failed: {}",
                String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .last()
                    .unwrap_or("unknown yt-dlp error")
            );
        }
        let title = fs::read_to_string(&title_file)
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
        let _ = fs::remove_file(&title_file);

        let media = fs::read_dir(dest_dir)?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_stem().is_some_and(|s| s == "source")
                    && p.extension().is_some_and(|e| e != "part")
            })
            .context("yt-dlp reported success but no media file was found")?;
        let platform = priors::collect(dest_dir)?;
        Ok(DownloadedVideo {
            path: media,
            title,
            platform,
            notes: Vec::new(),
        })
    }
}
