# video-to-skill

[![Downloads](https://img.shields.io/github/downloads/brenoepics/video-to-skill/total)](https://github.com/brenoepics/video-to-skill/releases)
[![Release](https://img.shields.io/github/v/release/brenoepics/video-to-skill)](https://github.com/brenoepics/video-to-skill/releases/latest)
[![CI](https://github.com/brenoepics/video-to-skill/actions/workflows/ci.yml/badge.svg)](https://github.com/brenoepics/video-to-skill/actions/workflows/ci.yml)

Turn any video into an installable agent skill — with proof.

```
/video-to-skill https://youtube.com/watch?v=...   → analyzes, compiles, verifies, installs
/vim-basics                                       → your agent now knows what the video taught
```

## Install

```bash
npx skills@latest add brenoepics/video-to-skill
```

That's the whole setup. First use bootstraps everything (extractor
binary, ffmpeg, yt-dlp, whisper) — checksum-pinned, no Homebrew, no
Python, no accounts.

## Why it's different

- **Pixels over transcript.** Commands are read off the screen, never
  paraphrased from narration — whisper hears "some", the frame says
  `SUM(B3:B10)`.
- **Verified by execution.** A sandboxed agent runs the generated steps
  against their success criteria before the skill earns its ✅ badge.
- **Skills that grow.** Feed it a second video of the same task:
  agreements raise confidence, conflicts become cited variants.

Every step in a generated skill carries `[t=MM:SS]` provenance back to
the video. Everything runs locally — the video never leaves your machine.

Works best on screencasts, CLI/GUI tutorials, and talks; fast physical
demos lose motion detail. Unverifiable claims ship flagged, not hidden.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) — TDD, conventional commits, and
a 300-line file limit, all enforced.
