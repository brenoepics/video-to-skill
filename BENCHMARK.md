# Benchmarks

`vts-extract bench` runs a fixed three-entry suite. The inputs are generated
on the fly and never change, so numbers stay comparable across machines and
commits.

## What is measured

| Entry | What it does | What it reports |
|---|---|---|
| `asr-45s` | Transcribes ~45 s of macOS `say`-generated speech muxed into an mp4 — exercises demux + whisper | Wall time and realtime factor (`media_secs / wall_secs`) |
| `extract-120s` | Full extract pipeline — ingest + asr + frames + timeline — on a 120 s 1280x720@30 ffmpeg `testsrc` video with a sine audio track | End-to-end wall time, realtime factor, and per-stage wall times (ingest, transcribe, frames, timeline) |
| `frames-batch-5` | Batch frame-at of 5 timestamps on that bundle | Wall time |

Every run also records the machine: chip (macOS: `sysctl -n
machdep.cpu.brand_string`; Linux: first `model name` from `/proc/cpuinfo`;
otherwise `unknown`), logical cores, OS, and the `vts-extract` version.

## Reproduce

```bash
# via the skills installer (prebuilt extractor)
npx skills@latest add brenoepics/video-to-skill

# or from a clone
cargo build --release --locked

# then
vts-extract check --fix   # bootstraps ffmpeg, yt-dlp, whisper weights — checksum-pinned
vts-extract bench         # prints the results table
```

`vts-extract bench --write-docs <repo-root>` additionally splices the
generated markdown into this file and the README summary — that is what CI
runs.

## Caveats

- The automated results below come from GitHub-hosted macOS runners, where
  whisper runs **CPU-only — no Metal**. Local Apple Silicon is substantially
  faster than anything in the table.
- Local reference (M5, Metal): 12 minutes of 4K video transcribed in 9.9 s.
  That is a one-off local measurement kept as an anecdote — it is **not**
  part of the automated suite.
- The inputs are synthetic (`say` speech, `testsrc` video) so the suite is
  hermetic and repeatable. Real-world videos — music, crosstalk, compression
  artifacts, 4K frames — will land on different absolute numbers; the suite
  is for tracking relative change.
- Hosted runners are shared hardware; expect some run-to-run jitter.

## Results

<!-- bench-results:start -->
_No automated results recorded yet — CI fills this in._
<!-- bench-results:end -->
