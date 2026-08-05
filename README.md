# video-to-skill

Turn any video — a YouTube tutorial, a conference talk, a colleague's
screen recording — into an **installable, evidence-grounded agent
skill**. Not a summary: a procedure your agent can act on, where every
step cites the exact timestamp and frame it was learned from, and a
✅ badge means a sandboxed agent actually executed it.

```
you:    /video-to-skill https://youtube.com/watch?v=...
agent:  extracts → inspects frames → compiles vim-basics/ → verifies → installs
you:    /vim-basics        (next session — an evidence-backed skill)
```

## Why this is different

Existing tools split into two camps: video-to-docs products (Loom AI,
Scribe, summarizers) output human prose from transcripts, and
demonstration recorders (workflow capture) need live instrumented
sessions. This project takes **pre-existing video files** and produces
**agent-executable skills** — grounded in pixels, not just the
transcript. On-screen text outranks narration: the extension id
`vscodevim.vim`, the exact `:q!` — things a transcript alone gets wrong
(it heard "QBang"). See [RESEARCH.md](RESEARCH.md) for the full survey.

## Install

Primary (via the [skills CLI](https://github.com/vercel-labs/skills)):

```bash
npx skills@latest add brenoepics/video-to-skill
```

Fallback — clone into your agent's skills directory:

```bash
git clone https://github.com/brenoepics/video-to-skill ~/.claude/skills/video-to-skill
```

First run bootstraps everything else: the `vts-extract` binary (prebuilt
per platform, checksum-verified) and its tools (static ffmpeg, yt-dlp,
whisper weights — all pinned, no Homebrew, no Python, no accounts).

## Usage

- `/video-to-skill <file-or-url>` — analyze: an evidence-grounded report
  of what the video teaches (steps, timestamps, on-screen commands).
- Ask for a skill and it compiles + verifies + installs the package.
- Point it at a second video of the same task to **fold** it in:
  confirmations raise confidence, disagreements become explicit
  variants, and the skill records its multi-source history.

## Privacy

Everything runs locally: download, transcription (whisper.cpp on
Metal), frame analysis. The video never leaves your machine — only the
few frames the agent chooses to inspect enter the model context.

## Honest limitations

- Best on screencasts, CLI tutorials, GUI walkthroughs, and talks.
  Fast physical-task videos lose motion detail (frames are sampled).
- Whisper mishears technical terms; that's why anything executable must
  be read from pixels, and unverified claims ship flagged low-confidence.
- Verification executes only steps that are safe in a sandbox; installs
  and system changes are skipped and labeled, never silently passed.

## Development

Rust workspace (`crates/extractor`), TDD throughout, clippy pedantic,
300-line file limit enforced by the test suite: see
[CONTRIBUTING.md](CONTRIBUTING.md). Tickets and evidence live in
`.scratch/video-to-skill-v1/`.
