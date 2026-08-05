//! Pinned download sources for every managed tool.
//!
//! Every entry pins an exact version and the sha256 of its artifact —
//! "latest" URLs are never used, so an upstream release can't change what
//! we install. All sources are public and ungated (no accounts, no tokens).
//!
//! Default whisper model: multilingual `base` (142 MB) — fast on Apple
//! Silicon and adequate for tutorial narration; larger models can become
//! configurable later without touching this mechanism.

use super::bootstrap::{Archive, ToolSpec};
use super::Tool;

/// The install spec for `tool` on this platform, or `None` when no
/// trusted static build is pinned for it here (the doctor then points
/// the user at a manual install instead).
#[must_use]
pub fn spec_for(tool: Tool) -> Option<ToolSpec> {
    match tool {
        // Static build from Martin Riedl's ffmpeg build server, pinned to
        // 8.1.2; checksum cross-verified against the published .sha256.
        Tool::Ffmpeg => macos_arm64().then(|| ToolSpec {
            tool,
            url: "https://ffmpeg.martin-riedl.de/download/macos/arm64/1783011502_8.1.2/ffmpeg.zip"
                .into(),
            sha256: "ef1aa60006c7b77ce170c1608c08d8e4ba1c30c5746f2ac986ded932d0ac2c3c".into(),
            archive: Archive::Zip {
                member: "ffmpeg".into(),
            },
        }),
        // Same pinned build as ffmpeg above; checksum from the published .sha256.
        Tool::Ffprobe => macos_arm64().then(|| ToolSpec {
            tool,
            url: "https://ffmpeg.martin-riedl.de/download/macos/arm64/1783011502_8.1.2/ffprobe.zip"
                .into(),
            sha256: "c39787f4af7a3932502d2d48db6f6feaaa836b48a73ef78c32cc3285df61dfaf".into(),
            archive: Archive::Zip {
                member: "ffprobe".into(),
            },
        }),
        // Official release binary (universal2), pinned with the SHA2-256SUMS
        // value published alongside the release.
        Tool::Ytdlp => macos().then(|| ToolSpec {
            tool,
            url: "https://github.com/yt-dlp/yt-dlp/releases/download/2026.07.04/yt-dlp_macos"
                .into(),
            sha256: "498bd0dae17855c599d371d68ec5bafc439a9d8640e838be25c765a9792f261b".into(),
            archive: Archive::None,
        }),
        // ggerganov/whisper.cpp model repo on Hugging Face: public, ungated;
        // checksum from the repo's git-lfs pointer. Platform-independent.
        Tool::WhisperModel => Some(ToolSpec {
            tool,
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin".into(),
            sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe".into(),
            archive: Archive::None,
        }),
    }
}

fn macos() -> bool {
    cfg!(target_os = "macos")
}

fn macos_arm64() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_available_spec_is_pinned_and_verifiable() {
        for tool in Tool::ALL {
            let Some(spec) = spec_for(tool) else { continue };
            assert!(spec.url.starts_with("https://"), "{tool:?}: https only");
            assert!(
                !spec.url.contains("latest"),
                "{tool:?}: versions must be pinned, not 'latest'"
            );
            assert_eq!(spec.sha256.len(), 64, "{tool:?}: full sha256 required");
            assert!(spec.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }
}
