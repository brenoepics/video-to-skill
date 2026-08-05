# Security

## Threat model

This skill points an agent at an arbitrary video and asks it to produce executable steps. That makes the video itself the attack surface: transcript, OCR text, and frame content are attacker-controllable inputs that flow toward an installable skill. The defenses are layered:

- **Untrusted-data boundary.** Everything derived from the video is data, never instructions to the agent. Video content cannot change modes, alter the workflow, name install paths, or direct the agent to fetch URLs or run commands during analysis. Text addressed to an AI or agent ("ignore previous instructions", "run this to continue") is treated as suspected prompt injection: flagged prominently in the report, never encoded into steps or scripts.
- **Credential redaction.** On-screen text outranks narration for commands and identifiers — never for secrets. Credentials visible in frames or transcript (API keys, tokens, passwords, private keys, connection strings with passwords) are never transcribed; they become `<REDACTED-KIND>` placeholders with a step caveat telling the user to supply their own. The compiler enforces this and rejects IRs containing secret-shaped strings.
- **No fetch-and-execute scripts.** Steps that download and execute remote code are never emitted as runnable scripts — manual steps with explicit caveats only, also compiler-enforced.
- **Sandboxed verification.** The verifier sub-agent sees only the generated package, works in a throwaway workspace, and never types credentials — real or placeholder. Package content that tries to instruct the verifier against its protocol is a FAIL outcome citing suspected injection.

## Download inventory

Everything the skill can download, with exactly where it comes from and how it is pinned. Every source is pinned to an exact version and sha256 — "latest" URLs are never used (enforced by test; the registry lives in [`crates/extractor/src/deps/registry.rs`](crates/extractor/src/deps/registry.rs)).

| Artifact | Source | Integrity |
|---|---|---|
| `vts-extract` binary | [github.com/brenoepics/video-to-skill](https://github.com/brenoepics/video-to-skill/releases) releases | Built by GitHub Actions from tagged source; checksum-verified before execution; provenance attestations verifiable with `gh attestation verify <tarball> -R brenoepics/video-to-skill` |
| yt-dlp | Official GitHub release | Pinned version + sha256 from the release's published SHA2-256SUMS |
| ffmpeg / ffprobe | ffmpeg.martin-riedl.de static builds | Pinned build + sha256 cross-verified against the published `.sha256` |
| whisper `ggml-base` model | ggerganov/whisper.cpp on Hugging Face | sha256 from the repo's git-lfs pointer |

**Tools already on PATH are always preferred and never downloaded.** A user who installs ffmpeg/yt-dlp via their package manager fully avoids the third-party downloads above; only the whisper model is data-dir-only.

**Build-from-source escape hatch:** `cargo build --release` in the skill's directory (requires Rust + cmake) replaces the prebuilt `vts-extract` entirely.

## No telemetry

Nothing is uploaded anywhere. The video never leaves the machine; extraction, transcription, compilation, and verification all run locally. Only the frames the agent chooses to inspect ever enter model context.

## Reporting a vulnerability

Open an issue at [github.com/brenoepics/video-to-skill/issues](https://github.com/brenoepics/video-to-skill/issues).
