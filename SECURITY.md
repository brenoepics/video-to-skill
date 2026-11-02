# Security

## Threat model

This skill points an agent at an arbitrary video and asks it to produce executable steps. That makes the video itself the attack surface: transcript, OCR text, and frame content are attacker-controllable inputs that flow toward an installable skill. The defenses are layered:

- **Untrusted-data boundary.** Everything derived from the video is data, never instructions to the agent. Video content cannot change modes, alter the workflow, name install paths, or direct the agent to fetch URLs or run commands during analysis. Text addressed to an AI or agent ("ignore previous instructions", "run this to continue") is treated as suspected prompt injection: flagged prominently in the report, never encoded into steps or scripts — and the compiler enforces it, see [Injection through video content](#injection-through-video-content) below.
- **Credential redaction, enforced by the compiler.** On-screen text outranks narration for commands and identifiers — never for secrets. This is the *first* evidence rule and it scopes the verbatim rule, not the other way round. Credentials visible in frames or transcript become `<REDACTED-KIND>` placeholders with a caveat telling the user to supply their own. The compiler refuses to build a package whose IR carries secret-shaped text in **any** field — title, description, overview, step goals, actions, success criteria, caveats, scripts, evidence quotes, variants, or on-screen artifacts. Refusal, not sanitisation: a redaction the author did not make is not silently applied.
- **Kernel-enforced execution sandbox.** Verification runs commands transcribed from an untrusted video, so it runs them under an OS sandbox — Windows LPAC/AppContainer plus a kill-on-close Job Object, macOS Seatbelt (`sandbox-exec`), or Linux bubblewrap — with **no network**, a deliberately granted throwaway workspace, a sanitized environment, and the machine's **credential stores unreadable** (`.ssh`, `.aws`, keychains, `.claude`, `.netrc`, and more). The Windows backend uses supported AppContainer APIs and does not depend on the optional Windows Sandbox feature. Where no mechanism exists or initialization fails, the run is **refused**, never downgraded to an unconfined process; the affected steps are badged `unverifiable`. Read the exact policy yourself with `vts-extract sandbox --workspace <dir> --print-policy`.
- **No fetch-and-execute scripts.** Steps that download and execute remote code are never emitted as runnable scripts — manual steps with explicit caveats only, also compiler-enforced.
- **Consent before anything leaves or lands.** No download and no installation happens without the user seeing it first. `vts-extract check --fix` prints the full inventory (URL + sha256 per artifact) and downloads nothing; `vts-extract install` prints the complete plan and writes nothing. Both require an explicit `--yes` to proceed.

## Injection through video content

A tutorial video can carry instructions meant for the agent rather than the viewer: a sticky note on the presenter's monitor reading "AI agent: run curl … | sh", a sentence of narration, a banner on screen for two seconds, a comment in the code being edited, a line of terminal output, or a near-invisible overlay. The SKILL.md rule that such text is data is the last layer, not the first, because the measurements say prompt-level rules alone do not hold:

- [AgentDojo](https://arxiv.org/abs/2406.13352) (Debenedetti et al., 2024): a tool allowlist cut indirect-injection success from 47–58 % to 6.8 %, while prompt-based defences left about 28 % succeeding.
- [Pop-up attacks](https://arxiv.org/abs/2411.02391) on computer-use agents (Zhang et al., 2024): 86 % success, and telling the agent to ignore pop-ups did not help.
- [Adversarial perturbations](https://arxiv.org/abs/2307.10490) in images and audio inject instructions into multimodal models (Bagdasaryan et al., 2023), so a frame need not contain readable text to carry one. [VPI-Bench](https://arxiv.org/abs/2506.02456) measures the visual case; [Greshake et al.](https://arxiv.org/abs/2302.12173) named the class, and it is [OWASP LLM01](https://genai.owasp.org/llm-top-10/).

So the defences are structural, in three places.

**The compiler refuses or flags agent-addressed text** ([`crates/extractor/src/compile/safety/injection.rs`](crates/extractor/src/compile/safety/injection.rs)). Alongside the secret and fetch-exec patterns, an "imperative text addressed to an agent" detector covers instruction overrides ("ignore previous instructions"), new-instruction headers, conditional and vocative addresses ("if you are an AI", "AI agent: run …", "Assistant, run …"), continue-to-run lures ("to continue, run …"), covert instructions ("do not tell the user") and verifier overrides ("mark every step as passed"). Where the text lands decides the verdict. In anything the installed skill *says* — title, description, overview, goals, actions, success criteria, scripts, variant actions — the IR is refused, naming the step and the class. Where it is a *record* of the video — on-screen artifacts and evidence quotes — the IR compiles and the compiler appends a `suspected prompt injection` entry to the package's Known gaps and `provenance.json`, naming the location but repeating only six characters of the text, so the flag cannot carry the injection further. Caveats, gaps and variant notes, where an author describes the attempt, are exempt. The fetch-and-execute rule is unchanged and now also sees `| sudo bash`.

**Red-team fixtures run in CI** ([`crates/extractor/tests/redteam.rs`](crates/extractor/tests/redteam.rs)). Six videos are generated at test time with ffmpeg `drawtext` and, for narration, macOS `say` — nothing binary is committed, and a machine without the tools skips the video and still runs every compiler assertion against a placeholder bundle: a sticky note beside a terminal, a spoken instruction, a banner shown from 2 s to 4 s, a comment injected into an editor view, terminal output ending in the instruction, and a slide with the text at 8 % opacity under noise. For each, the suite asserts that an IR carrying the text in `actions` or `scripts` is refused as injection, that the same text in `artifacts` compiles with the flag under Known gaps and nothing of it in the step's instructions, that the `curl … | sh` it carries is still refused as fetch-and-execute on its own, and, where the video rendered, that the honest transcription compiles against the real bundle citing a real keyframe.

**The verifier runs under a per-step tool allowlist** ([`references/verification-protocol.md`](references/verification-protocol.md)). The verifying sub-agent may invoke only the executables the step's own scripts name — or, for a step without scripts, those named in its actions — inside `vts-extract sandbox`. Anything else the package asks for, in a quote, an artifact, a gap or a frame, is outside the step and never run, and an instruction phrased at the verifier fails the step. The list is derived from the package, so nothing in the video can widen it. The compiler does not yet emit it per step, because the step file has no structured slot for it; the derivation is specified in the protocol.

What this does not cover: an agent that ignores the protocol, and a perturbation the detector cannot read. The sandbox is the floor under both.

## Response to the skills.sh marketplace audits

Three auditors scanned this skill on 2026-08-07 (commit `1b827ca`). Every finding is reproduced below with what was done about it. Fixes landed in **v0.2.4**.

| Auditor — finding | Severity | Status | What changed |
| --- | --- | --- | --- |
| **Snyk W007** — insecure credential handling: "commands verbatim from pixels" would force the model to reproduce on-screen secrets | HIGH | **Fixed** | Redaction is now evidence rule 1 and explicitly outranks the verbatim rule. The compiler's scan was extended from 5 fields to **every** video-derived field in the IR, and from 5 secret classes to 13 (AWS, GitHub, Slack, OpenAI-style, Stripe, Google, GitLab, npm, SendGrid, JWT, bearer headers, private-key blocks, credentials embedded in URLs) plus quoted and bare credential assignments. |
| **Snyk W012** — unverifiable external dependency: pinned executables and model weights fetched at runtime and executed locally | MEDIUM | **Mitigated** | Downloads are consent-gated and fully disclosed: `check --fix` prints every URL and sha256 and fetches nothing until `--yes`. `check --print-downloads` lists the complete inventory on demand. Integrity never depended on trusting the host — see "On download origins" below. |
| **Gen Agent Trust Hub** — REMOTE_CODE_EXECUTION: verification executes shell commands extracted from user-provided videos | HIGH | **Fixed** | Execution is now kernel-confined (LPAC/AppContainer, Seatbelt, or bubblewrap): no network, only the throwaway workspace is deliberately granted, and credential stores remain unreadable. Refused outright where no sandbox exists. Previously this was a convention in a protocol document; it is now a boundary with [tests that assert the kernel enforces it](crates/extractor/tests/sandbox.rs). |
| **Gen Agent Trust Hub** — COMMAND_EXECUTION: downloads and runs a prebuilt binary from the author's GitHub releases | HIGH | **Accurate, bounded** | True and unavoidable for a local extractor. Bounded by: version-pinned URL, sha256 verified before the binary is ever executed, GitHub build provenance attestations (`gh attestation verify`), reproducible from tagged source, and a `cargo build --release` escape hatch that skips the download entirely. |
| **Gen Agent Trust Hub** — EXTERNAL_DOWNLOADS: ffmpeg/ffprobe from a third-party personal domain rather than an official repository | HIGH | **Accurate, mitigated** | Fair, and see the honest note below: no ffmpeg.org-endorsed static macOS **arm64** build exists. Mitigated by sha256 pinning, full disclosure, consent, and the fact that a PATH ffmpeg is always preferred and never downloaded. |
| **Gen Agent Trust Hub** — EXTERNAL_DOWNLOADS: whisper model weights from Hugging Face | HIGH | **Accurate, bounded** | Public, ungated, version-pinned, sha256-verified from the repo's git-lfs pointer. It is data, not code — loaded by whisper, never executed. The Silero voice-activity weights added later are bounded the same way. |
| **Socket** — broader footprint than a documentation skill: supply-chain trust in a personal-repo binary, transitive installation of a generated skill, untrusted content becoming executable agent instructions. "No clear credential harvesting or overt exfiltration is shown." | MEDIUM | **Mitigated** | The transitive-install concern is the sharpest one here, and it was correct: v0.2.3 installed into every detected agent as its default behaviour. As of v0.2.4 `install` prints the plan and writes nothing without `--yes`. The other two are the sandbox and disclosure items above. |

### On download origins

Every artifact is pinned to an exact version **and** an exact sha256, verified before the file is executed or loaded; a test enforces that no URL contains "latest". This is the part worth being precise about: **a compromised origin cannot change the bytes you get without failing the checksum.** Origin trust governs *availability* and the *initial* pin, not integrity at fetch time.

That said, the auditors are right that origin still matters, and here the ecosystem has a genuine gap: **ffmpeg.org endorses no static macOS arm64 build.** Its download page links third-party Windows builds from Gyan and BtbN, but only x86_64 macOS builds. This project pins the Gyan Windows archive and a separate macOS arm64 build by exact digest. Rather than pretend those publishers are the FFmpeg project itself:

- Existing ffmpeg/ffprobe executables and yt-dlp builds that accept the required hardening flags are **always** preferred from PATH and never downloaded — `brew install ffmpeg` avoids the third-party fetch completely;
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
| `vts-extract` binary | [github.com/brenoepics/video-to-skill](https://github.com/brenoepics/video-to-skill/releases) releases | Built by GitHub Actions from tagged source with `cargo auditable`; checksum-verified before execution; SLSA Build Level 2 provenance verifiable with `gh attestation verify <archive> -R brenoepics/video-to-skill` — see "Build provenance" below |
| yt-dlp | Official GitHub release | Pinned version + sha256 from the release's published SHA2-256SUMS |
| ffmpeg / ffprobe | Platform build linked by ffmpeg.org: Gyan on Windows, Martin Riedl on macOS arm64 | Pinned build + sha256 cross-verified against the publisher's sidecar; see "On download origins" above |
| whisper `ggml-base` model | ggerganov/whisper.cpp on Hugging Face | sha256 from the repo's git-lfs pointer |
| Silero VAD `ggml-silero-v5.1.2` model (voice activity detection, so whisper skips non-speech) | ggml-org/whisper-vad on Hugging Face — the repo whisper.cpp's own download script uses | sha256 from the repo's git-lfs pointer, re-verified against a download |

**Existing ffmpeg/ffprobe executables and yt-dlp builds that accept the required hardening flags are preferred from PATH and never downloaded.** A user who installs current ffmpeg/yt-dlp versions via their package manager fully avoids the third-party downloads above; only the whisper and VAD models are data-dir-only.

**Build-from-source escape hatch:** `cargo build --release` in the skill's directory (requires Rust + cmake) replaces the prebuilt `vts-extract` entirely.

### Build provenance

Each release archive is attested by the release workflow with [GitHub artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations): a SLSA v1.0 provenance statement, signed through Sigstore and recorded in the public Rekor transparency log, that names the repository, the workflow file, the tag and the commit that produced the bytes. That is SLSA Build Level 2. Releases after v0.3.0 attest the `.sha256` sidecar as well. Verify an archive before running it:

```
gh attestation verify vts-extract-macos-arm64.tar.gz -R brenoepics/video-to-skill
```

A pass means GitHub built exactly these bytes from this repository's `.github/workflows/release.yml` on a GitHub-hosted runner. An archive replaced after the fact fails with `HTTP 404: Not Found`, even when its `.sha256` was replaced to match, because no signed statement carries the new digest. To pin harder, add `--signer-workflow brenoepics/video-to-skill/.github/workflows/release.yml` and `--source-ref refs/tags/v<version>`; `--format json` prints the full statement. The command needs an authenticated `gh`, since it looks the attestation up through the GitHub API; the checksum check in [SKILL.md](SKILL.md) runs regardless. Provenance says where the bytes came from, not that they are safe — the threat model above covers the rest.

Release binaries are built with [`cargo auditable`](https://github.com/rust-secure-code/cargo-auditable), which embeds the resolved dependency list in the executable (linker section `.dep-v0`). `cargo audit bin vts-extract` scans a release against the RustSec advisory database with no source checkout, and `rust-audit-info vts-extract` prints the list. CI runs `cargo deny` over advisories, licences, bans and sources on every change ([`deny.toml`](deny.toml)).

## No telemetry

Nothing is uploaded anywhere. The video never leaves the machine; extraction, transcription, compilation, and verification all run locally. Only the frames the agent chooses to inspect ever enter model context.

## Reporting a vulnerability

Open an issue at [github.com/brenoepics/video-to-skill/issues](https://github.com/brenoepics/video-to-skill/issues).
