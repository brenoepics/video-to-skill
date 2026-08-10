# Security

## Threat model

This skill points an agent at an arbitrary video and asks it to produce executable steps. That makes the video itself the attack surface: transcript, OCR text, and frame content are attacker-controllable inputs that flow toward an installable skill. The defenses are layered:

- **Untrusted-data boundary.** Everything derived from the video is data, never instructions to the agent. Video content cannot change modes, alter the workflow, name install paths, or direct the agent to fetch URLs or run commands during analysis. Text addressed to an AI or agent ("ignore previous instructions", "run this to continue") is treated as suspected prompt injection: flagged prominently in the report, never encoded into steps or scripts.
- **Credential redaction, enforced by the compiler.** On-screen text outranks narration for commands and identifiers — never for secrets. This is the *first* evidence rule and it scopes the verbatim rule, not the other way round. Credentials visible in frames or transcript become `<REDACTED-KIND>` placeholders with a caveat telling the user to supply their own. The compiler refuses to build a package whose IR carries secret-shaped text in **any** field — title, description, overview, step goals, actions, success criteria, caveats, scripts, evidence quotes, variants, or on-screen artifacts. Refusal, not sanitisation: a redaction the author did not make is not silently applied.
- **Kernel-enforced execution sandbox.** Verification runs commands transcribed from an untrusted video, so it runs them under an OS sandbox — macOS seatbelt (`sandbox-exec`) or Linux bubblewrap — with **no network**, **writes confined to a throwaway workspace**, and the machine's **credential stores unreadable** (`~/.ssh`, `~/.aws`, keychains, `~/.claude`, `.netrc`, and more). Where no mechanism exists the run is **refused**, never downgraded to an unconfined shell; the affected steps are badged `unverifiable`. Read the exact policy yourself with `vts-extract sandbox --workspace <dir> --print-policy`.
- **No fetch-and-execute scripts.** Steps that download and execute remote code are never emitted as runnable scripts — manual steps with explicit caveats only, also compiler-enforced.
- **Consent before anything leaves or lands.** No download and no installation happens without the user seeing it first. `vts-extract check --fix` prints the full inventory (URL + sha256 per artifact) and downloads nothing; `vts-extract install` prints the complete plan and writes nothing. Both require an explicit `--yes` to proceed.

## Response to the skills.sh marketplace audits

Three auditors scanned this skill on 2026-08-07 (commit `1b827ca`). Every finding is reproduced below with what was done about it. Fixes landed in **v0.2.4**.

| Auditor — finding | Severity | Status | What changed |
| --- | --- | --- | --- |
| **Snyk W007** — insecure credential handling: "commands verbatim from pixels" would force the model to reproduce on-screen secrets | HIGH | **Fixed** | Redaction is now evidence rule 1 and explicitly outranks the verbatim rule. The compiler's scan was extended from 5 fields to **every** video-derived field in the IR, and from 5 secret classes to 13 (AWS, GitHub, Slack, OpenAI-style, Stripe, Google, GitLab, npm, SendGrid, JWT, bearer headers, private-key blocks, credentials embedded in URLs) plus quoted and bare credential assignments. |
| **Snyk W012** — unverifiable external dependency: pinned executables and model weights fetched at runtime and executed locally | MEDIUM | **Mitigated** | Downloads are consent-gated and fully disclosed: `check --fix` prints every URL and sha256 and fetches nothing until `--yes`. `check --print-downloads` lists the complete inventory on demand. Integrity never depended on trusting the host — see "On download origins" below. |
| **Gen Agent Trust Hub** — REMOTE_CODE_EXECUTION: verification executes shell commands extracted from user-provided videos | HIGH | **Fixed** | Execution is now kernel-confined (seatbelt / bubblewrap): no network, writes confined to a throwaway workspace, credential stores unreadable. Refused outright where no sandbox exists. Previously this was a convention in a protocol document; it is now a boundary with [tests that assert the kernel enforces it](crates/extractor/tests/sandbox.rs). |
| **Gen Agent Trust Hub** — COMMAND_EXECUTION: downloads and runs a prebuilt binary from the author's GitHub releases | HIGH | **Accurate, bounded** | True and unavoidable for a local extractor. Bounded by: version-pinned URL, sha256 verified before the binary is ever executed, GitHub build provenance attestations (`gh attestation verify`), reproducible from tagged source, and a `cargo build --release` escape hatch that skips the download entirely. |
| **Gen Agent Trust Hub** — EXTERNAL_DOWNLOADS: ffmpeg/ffprobe from a third-party personal domain rather than an official repository | HIGH | **Accurate, mitigated** | Fair, and see the honest note below: no ffmpeg.org-endorsed static macOS **arm64** build exists. Mitigated by sha256 pinning, full disclosure, consent, and the fact that a PATH ffmpeg is always preferred and never downloaded. |
| **Gen Agent Trust Hub** — EXTERNAL_DOWNLOADS: whisper model weights from Hugging Face | HIGH | **Accurate, bounded** | Public, ungated, version-pinned, sha256-verified from the repo's git-lfs pointer. It is data, not code — loaded by whisper, never executed. |
| **Socket** — broader footprint than a documentation skill: supply-chain trust in a personal-repo binary, transitive installation of a generated skill, untrusted content becoming executable agent instructions. "No clear credential harvesting or overt exfiltration is shown." | MEDIUM | **Mitigated** | The transitive-install concern is the sharpest one here, and it was correct: v0.2.3 installed into every detected agent as its default behaviour. As of v0.2.4 `install` prints the plan and writes nothing without `--yes`. The other two are the sandbox and disclosure items above. |

### On download origins

Every artifact is pinned to an exact version **and** an exact sha256, verified before the file is executed or loaded; a test enforces that no URL contains "latest". This is the part worth being precise about: **a compromised origin cannot change the bytes you get without failing the checksum.** Origin trust governs *availability* and the *initial* pin, not integrity at fetch time.

That said, the auditors are right that origin still matters, and here the ecosystem has a genuine gap: **ffmpeg.org endorses no static macOS arm64 build.** Its download page links evermeet.cx for macOS (x86_64, which would need Rosetta) and BtbN for Linux and Windows only. Every arm64 static build on offer is somebody's personal build server. Rather than pretend otherwise:

- Tools already on PATH are **always** preferred and never downloaded — `brew install ffmpeg` avoids the third-party fetch completely;
- what would be fetched is disclosed before it is fetched, and requires consent;
- the pin is exact and cross-verified against the publisher's own `.sha256`.

### What was not "fixed", and why

Two findings describe what the tool *is*, and no amount of engineering removes them:

1. **It downloads and runs a local extractor.** A tool that transcribes video locally needs ffmpeg, yt-dlp, and whisper weights. The alternative is uploading the user's video to a server, which is strictly worse for privacy and is the thing this project exists to avoid.
2. **It turns untrusted video content into executable agent instructions.** That is the product. The answer is not to stop doing it but to make every stage of it bounded: untrusted-data boundary during analysis, compiler refusal for secrets and fetch-exec, kernel sandbox for execution, consent for installation.

A reader who is not comfortable with those two facts should not install this skill, and that is a legitimate conclusion to reach.

## Download inventory

Everything the skill can download, with exactly where it comes from and how it is pinned. Run `vts-extract check --print-downloads` to print this same list from the binary you actually have. The registry lives in [`crates/extractor/src/deps/registry.rs`](crates/extractor/src/deps/registry.rs).

| Artifact | Source | Integrity |
|---|---|---|
| `vts-extract` binary | [github.com/brenoepics/video-to-skill](https://github.com/brenoepics/video-to-skill/releases) releases | Built by GitHub Actions from tagged source; checksum-verified before execution; provenance attestations verifiable with `gh attestation verify <tarball> -R brenoepics/video-to-skill` |
| yt-dlp | Official GitHub release | Pinned version + sha256 from the release's published SHA2-256SUMS |
| ffmpeg / ffprobe | ffmpeg.martin-riedl.de static builds | Pinned build + sha256 cross-verified against the published `.sha256`; see "On download origins" above |
| whisper `ggml-base` model | ggerganov/whisper.cpp on Hugging Face | sha256 from the repo's git-lfs pointer |

**Tools already on PATH are always preferred and never downloaded.** A user who installs ffmpeg/yt-dlp via their package manager fully avoids the third-party downloads above; only the whisper model is data-dir-only.

**Build-from-source escape hatch:** `cargo build --release` in the skill's directory (requires Rust + cmake) replaces the prebuilt `vts-extract` entirely.

## No telemetry

Nothing is uploaded anywhere. The video never leaves the machine; extraction, transcription, compilation, and verification all run locally. Only the frames the agent chooses to inspect ever enter model context.

## Reporting a vulnerability

Open an issue at [github.com/brenoepics/video-to-skill/issues](https://github.com/brenoepics/video-to-skill/issues).
