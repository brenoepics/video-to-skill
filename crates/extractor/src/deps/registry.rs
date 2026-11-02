//! Pinned download sources for every managed tool.
//!
//! Every entry pins an exact version and the sha256 of its artifact —
//! "latest" URLs are never used, so an upstream release can't change what
//! we install. All sources are public and ungated (no accounts, no tokens).
//!
//! Default whisper model: multilingual `base` (142 MB) — fast on Apple
//! Silicon and adequate for tutorial narration; larger models can become
//! configurable later without touching this mechanism.

use super::bootstrap::{Archive, ArchiveMember, ToolSpec};
use super::Tool;

const WINDOWS_FFMPEG_URL: &str =
    "https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.1.2-essentials_build.zip";
const WINDOWS_FFMPEG_SHA256: &str =
    "db580001caa24ac104c8cb856cd113a87b0a443f7bdf47d8c12b1d740584a2ec";
const WINDOWS_FFMPEG_ROOT: &str = "ffmpeg-8.1.2-essentials_build/bin";
const WINDOWS_YTDLP_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/download/2026.07.04/yt-dlp.exe";
const WINDOWS_YTDLP_SHA256: &str =
    "52fe3c26dcf71fbdc85b528589020bb0b8e383155cfa81b64dd447bbe35e24b8";
const WHISPER_BASE_REVISION: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";

/// The install spec for `tool` on this platform, or `None` when no
/// trusted static build is pinned for it here (the doctor then points
/// the user at a manual install instead).
#[must_use]
pub fn spec_for(tool: Tool) -> Option<ToolSpec> {
    match tool {
        // Static build from Martin Riedl's ffmpeg build server, pinned to
        // 8.1.2; checksum cross-verified against the published .sha256.
        Tool::Ffmpeg | Tool::Ffprobe if windows_x64() => Some(windows_ffmpeg_bundle(tool)),
        Tool::Ffmpeg => macos_arm64().then(|| macos_ffmpeg(tool)),
        // Same pinned build as ffmpeg above; checksum from the published .sha256.
        Tool::Ffprobe => macos_arm64().then(|| macos_ffprobe(tool)),
        // Official release binary (universal2), pinned with the SHA2-256SUMS
        // value published alongside the release.
        Tool::Ytdlp if windows_x64() => Some(windows_ytdlp(tool)),
        Tool::Ytdlp => macos().then(|| macos_ytdlp(tool)),
        // ggerganov/whisper.cpp model repo on Hugging Face: public, ungated;
        // checksum from the repo's git-lfs pointer. Platform-independent.
        Tool::WhisperModel => Some(ToolSpec {
            tool,
            url: format!(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/{WHISPER_BASE_REVISION}/ggml-base.bin"
            ),
            sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe".into(),
            archive: Archive::None,
        }),
    }
}

/// Every artifact this platform could fetch, in a stable order — the
/// inventory a user is shown before approving any download, and the
/// answer to "what does this thing pull off the internet?".
#[must_use]
pub fn inventory() -> Vec<ToolSpec> {
    inventory_for(&Tool::ALL)
}

/// Unique download artifacts needed for `tools`, preserving request order.
/// Shared bundles such as `FFmpeg` + `ffprobe` appear exactly once.
#[must_use]
pub fn inventory_for(tools: &[Tool]) -> Vec<ToolSpec> {
    let mut inventory = Vec::new();
    for spec in tools.iter().copied().filter_map(spec_for) {
        let disclosed = inventory
            .iter()
            .any(|existing: &ToolSpec| existing.url == spec.url && existing.sha256 == spec.sha256);
        if !disclosed {
            inventory.push(spec);
        }
    }
    inventory
}

fn windows_ffmpeg_bundle(tool: Tool) -> ToolSpec {
    let members = [Tool::Ffmpeg, Tool::Ffprobe]
        .into_iter()
        .map(|member_tool| ArchiveMember {
            tool: member_tool,
            path: format!("{WINDOWS_FFMPEG_ROOT}/{}", member_tool.file_name()),
        })
        .collect();
    ToolSpec {
        tool,
        url: WINDOWS_FFMPEG_URL.into(),
        sha256: WINDOWS_FFMPEG_SHA256.into(),
        archive: Archive::Zip { members },
    }
}

fn windows_ytdlp(tool: Tool) -> ToolSpec {
    ToolSpec {
        tool,
        url: WINDOWS_YTDLP_URL.into(),
        sha256: WINDOWS_YTDLP_SHA256.into(),
        archive: Archive::None,
    }
}

fn macos_ffmpeg(tool: Tool) -> ToolSpec {
    zipped_tool(
        tool,
        "https://ffmpeg.martin-riedl.de/download/macos/arm64/1783011502_8.1.2/ffmpeg.zip",
        "ef1aa60006c7b77ce170c1608c08d8e4ba1c30c5746f2ac986ded932d0ac2c3c",
    )
}

fn macos_ffprobe(tool: Tool) -> ToolSpec {
    zipped_tool(
        tool,
        "https://ffmpeg.martin-riedl.de/download/macos/arm64/1783011502_8.1.2/ffprobe.zip",
        "c39787f4af7a3932502d2d48db6f6feaaa836b48a73ef78c32cc3285df61dfaf",
    )
}

fn zipped_tool(tool: Tool, url: &str, sha256: &str) -> ToolSpec {
    ToolSpec {
        tool,
        url: url.into(),
        sha256: sha256.into(),
        archive: Archive::Zip {
            members: vec![ArchiveMember {
                tool,
                path: tool.file_name().into(),
            }],
        },
    }
}

fn macos_ytdlp(tool: Tool) -> ToolSpec {
    ToolSpec {
        tool,
        url: "https://github.com/yt-dlp/yt-dlp/releases/download/2026.07.04/yt-dlp_macos".into(),
        sha256: "498bd0dae17855c599d371d68ec5bafc439a9d8640e838be25c765a9792f261b".into(),
        archive: Archive::None,
    }
}

fn macos() -> bool {
    cfg!(target_os = "macos")
}

fn macos_arm64() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

fn windows_x64() -> bool {
    cfg!(all(target_os = "windows", target_arch = "x86_64"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inventory_covers_every_fetchable_tool() {
        let inventory = inventory();
        for spec in Tool::ALL.iter().copied().filter_map(spec_for) {
            assert!(
                inventory
                    .iter()
                    .any(|item| { item.url == spec.url && item.sha256 == spec.sha256 }),
                "the disclosed inventory must not omit fetchable artifacts"
            );
        }
    }

    #[test]
    fn every_available_spec_is_immutable_and_verifiable() {
        for tool in Tool::ALL {
            let Some(spec) = spec_for(tool) else { continue };
            assert!(spec.url.starts_with("https://"), "{tool:?}: https only");
            assert!(
                !spec.url.contains("latest"),
                "{tool:?}: versions must be pinned, not 'latest'"
            );
            assert!(
                !spec.url.contains("/resolve/main/"),
                "{tool:?}: repository revisions must be immutable"
            );
            assert_eq!(spec.sha256.len(), 64, "{tool:?}: full sha256 required");
            assert!(spec.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    #[test]
    fn windows_ffmpeg_and_ffprobe_share_one_pinned_bundle() {
        let ffmpeg = spec_for(Tool::Ffmpeg).unwrap();
        let ffprobe = spec_for(Tool::Ffprobe).unwrap();

        assert_eq!(ffmpeg.url, ffprobe.url);
        assert_eq!(ffmpeg.sha256, ffprobe.sha256);
        let Archive::Zip { members } = ffmpeg.archive else {
            panic!("Windows FFmpeg must use the pinned ZIP bundle");
        };
        assert_eq!(members.len(), 2);
        assert_eq!(
            inventory_for(&[Tool::Ffmpeg, Tool::Ffprobe]).len(),
            1,
            "consent inventory must disclose the shared download once"
        );
        assert_eq!(ffprobe.provided_tools(), vec![Tool::Ffmpeg, Tool::Ffprobe]);
    }
}
