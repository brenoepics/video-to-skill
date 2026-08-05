//! Media metadata via ffprobe's JSON output.

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use super::MediaInfo;

#[derive(Deserialize)]
struct ProbeOutput {
    format: Option<ProbeFormat>,
    #[serde(default)]
    streams: Vec<ProbeStream>,
}

#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

#[derive(Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
}

/// Probe `media` with ffprobe. Fails with a "corrupt" error when ffprobe
/// can't parse the file or it has no video stream.
pub fn probe(ffprobe: &Path, media: &Path, container: &str) -> Result<MediaInfo> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(media)
        .output()
        .context("running ffprobe")?;
    if !output.status.success() {
        bail!(
            "'{}' could not be read as video — the file may be corrupt",
            media.display()
        );
    }
    let parsed: ProbeOutput =
        serde_json::from_slice(&output.stdout).context("parsing ffprobe output")?;

    let video = parsed
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"));
    let Some(video) = video else {
        bail!(
            "'{}' could not be read as video — the file may be corrupt or audio-only",
            media.display()
        );
    };
    let has_audio = parsed
        .streams
        .iter()
        .any(|s| s.codec_type.as_deref() == Some("audio"));

    Ok(MediaInfo {
        container: container.to_string(),
        duration_secs: parsed
            .format
            .and_then(|f| f.duration)
            .and_then(|d| d.parse().ok())
            .unwrap_or(0.0),
        width: video.width.unwrap_or(0),
        height: video.height.unwrap_or(0),
        fps: video
            .r_frame_rate
            .as_deref()
            .and_then(parse_fraction)
            .unwrap_or(0.0),
        has_audio,
        language: None,
    })
}

/// ffprobe rates come as fractions like "10/1" or "30000/1001".
fn parse_fraction(raw: &str) -> Option<f64> {
    let (num, den) = raw.split_once('/')?;
    let num: f64 = num.parse().ok()?;
    let den: f64 = den.parse().ok()?;
    (den != 0.0).then_some(num / den)
}

#[cfg(test)]
mod tests {
    use super::parse_fraction;

    #[test]
    fn parses_ffprobe_rate_fractions() {
        assert_eq!(parse_fraction("10/1"), Some(10.0));
        let ntsc = parse_fraction("30000/1001").unwrap();
        assert!((ntsc - 29.97).abs() < 0.01);
        assert_eq!(parse_fraction("10"), None);
        assert_eq!(parse_fraction("1/0"), None);
    }
}
