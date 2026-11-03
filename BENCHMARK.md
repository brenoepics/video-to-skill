# Benchmarks

`vts-extract bench` runs a fixed three-entry suite. The inputs are generated
on the fly and never change, so numbers stay comparable across machines and
commits.

## What is measured

| Entry | What it does | What it reports |
|---|---|---|
| `asr-45s` | Transcribes ~45 s of macOS `say`-generated speech muxed into an mp4 — exercises demux + voice activity detection + whisper | Wall time and realtime factor (`media_secs / wall_secs`) |
| `extract-120s` | Full extract pipeline — ingest + asr + frames + timeline — on a 120 s 1280x720@30 ffmpeg `testsrc` video with a sine audio track (no speech: see the VAD caveat below) | End-to-end wall time, realtime factor, and per-stage wall times (ingest, transcribe, frames, timeline) |
| `frames-batch-5` | Batch frame-at of 5 timestamps on that bundle | Wall time |

Every run also records the machine: chip (macOS: `sysctl -n
machdep.cpu.brand_string`; Linux: first `model name` from `/proc/cpuinfo`;
otherwise `unknown`), logical cores, OS, whether whisper ran on the GPU or
the CPU, and the `vts-extract` version.

## Reproduce it yourself

```bash
# via the skills installer (prebuilt extractor)
npx skills@latest add brenoepics/video-to-skill

# or from a clone
cargo build --release --locked

# then
vts-extract check --fix --yes   # bootstraps ffmpeg, yt-dlp, whisper + VAD weights — checksum-pinned
vts-extract bench         # prints the results table
```

`vts-extract bench --write-docs <repo-root>` additionally splices the
generated markdown into this file and the README summary — that is what CI
runs.

## Caveats

- The automated results below come from GitHub-hosted macOS runners, which
  are virtual machines. A VM only sees a paravirtual GPU whose Metal lacks
  simdgroup reductions and matrix multiply, and ggml's fallback kernels
  there run tens of times slower than the CPU — so `vts-extract` detects
  the hypervisor (`sysctl kern.hv_vmm_present`, or an Apple "(Virtual)"
  brand string) and keeps whisper on the **CPU**. The machine line records
  which device a run used. Local Apple Silicon with real Metal is
  substantially faster than anything in the table.
- Local reference (M5, Metal): 12 minutes of 4K video transcribed in 9.9 s.
  That is a one-off local measurement kept as an anecdote — it is **not**
  part of the automated suite.
- The inputs are synthetic (`say` speech, `testsrc` video) so the suite is
  hermetic and repeatable. Real-world videos — music, crosstalk, compression
  artifacts, 4K frames — will land on different absolute numbers; the suite
  is for tracking relative change.
- Hosted runners are shared hardware; expect some run-to-run jitter.
- **Voice activity detection changed what `transcribe` measures.** Since the
  Silero VAD model joined the toolchain, whisper only decodes the stretches
  the detector marks as speech. The `extract-120s` fixture's audio is a pure
  440 Hz sine, which VAD classifies as *no speech*, so its `transcribe`
  stage now covers demux + detection only and is **not comparable** with
  rows generated before VAD (where whisper spent the whole stage decoding
  the tone). `asr-45s` is continuous speech and stays comparable: VAD only
  trims the pauses `say` leaves between sentences.

## Quality evaluation

`vts-extract eval` measures extraction *quality*, which the speed suite
above cannot see. It generates a **synthetic gold set** of short tutorial
videos (terminal sessions with typed commands and streamed output, a
GUI dialog sequence, a wizard, a slide deck, scrolls and hard cuts,
narrated by macOS `say`), runs each through the real pipeline, and
scores the result against hand-written labels. Nothing binary is
committed: every video is rendered deterministically at eval time from
`crates/extractor/eval/<id>/recipe.json`, and the labels live beside it
in `procedure.json` (a valid Procedure IR, see
`references/ir-format.md`). No licensed external corpus of CLI tutorial
videos could be pinned, so the set is synthetic; the papers in issue
#34 (VideoGUI, GUI-World, MONDAY, Terminal-Bench 2.0) are calibration
references only.

```bash
vts-extract check --fix --yes            # ffmpeg + whisper + VAD weights, checksum-pinned
vts-extract eval --out eval-results.json --summary eval-summary.md \
  --baseline crates/extractor/eval/baseline.json
```

### Metrics

The extractor emits shots, states and a transcript, never steps, so the
gated metrics score what it alone determines; the agent-dependent ones
are scored only when `--packages <dir>` points at compiled packages
(one `<gold id>/` directory each, with `provenance.json` and optionally
`verification.json`), and are otherwise reported as **not scored** with
the reason.

| metric | definition |
|---|---|
| `boundary_precision` / `recall` / `f1` | Timeline segment starts matched one-to-one to the recipe's visual change times within ±2 s (two 1 fps sampling ticks). Scroll and streaming frames marked `continuous` are not changes. |
| `step_iou_precision` / `recall` / `f1` | Each segment is assigned to the gold step it overlaps most. A step is *recovered* when the union of its segments covers it with IoU ≥ 0.5 (recall). A segment is *clean* when its step was recovered and it stays inside that step ±2 s (precision): over-segmentation inside a step is free, a segment straddling two steps is a false positive. |
| `command_transcript_recall` | Gold commands (the backtick spans of each step's `actions`) found in the transcript within the step's window ±2 s, comparing lowercase alphanumerics with spoken punctuation ("dot", "dash", …) dropped from the transcript side. Needs `say` (macOS) and whisper weights; otherwise not scored. |
| `package_step_precision` / `recall` / `f1` | Package steps matched to gold steps when the step's earliest evidence timestamp falls inside the gold span (one-to-one). Agent-dependent. |
| `command_exact_match` | Gold commands reproduced verbatim in the package IR's `actions`. Agent-dependent. |
| `verification_pass_rate` | `pass` outcomes over all step outcomes in the packages' `verification.json`. Needs the sandboxed verify agent. |

Counts are pooled across the whole set (micro-average) and every
proportion carries its **Wilson 95 % lower bound**; an F1 row's bound is
the harmonic mean of its precision and recall bounds. The JSON artifact
records the raw counts per video so a drop can be traced to one recipe.

### Gating

CI's `Eval` workflow (`.github/workflows/eval.yml`, macOS) runs the eval
on every pull request, uploads `eval-results.json` and the markdown
summary as the `eval-results` artifact, posts the summary to the job
page, and **fails when any metric's lower bound drops more than 0.05
below the committed baseline** `crates/extractor/eval/baseline.json`
(`--max-drop`), or when a metric that the baseline scored comes back not
scored. Gating on a lower bound rather than the point estimate keeps the
set's size honest: a hundred boundaries give ±0.08 of slack, so only a
real regression trips it.

Improvements never fail the gate, but the summary says when a metric
rose more than 0.05 above the baseline. **Refresh the baseline in the
same PR that moves a metric** so the next regression is measured from
the new level:

```bash
vts-extract eval --out crates/extractor/eval/baseline.json
```

and commit the file (its `machine` and `generated_on` fields are
provenance; only the metrics are compared). The CI artifact of a green
run is an equally valid baseline.

### Adding a gold video

1. Create `crates/extractor/eval/<id>/` (kebab-case id).
2. Write `recipe.json`: size, fps, `duration_secs`, a list of `stills`
   (`t`, optional `step` label, optional `continuous`, and one screen:
   `terminal` ops `type` / `output` / `lines` / `title` / `prompt`,
   `dialog`, `slide` or `desktop`) and `narration` cues (`t`, `text`).
   Every still after the first that is not `continuous` is a gold visual
   change; the `step` labels give each step its span.
3. Write `procedure.json`: a Procedure IR whose `skill_name` is the id,
   whose steps are exactly the labelled steps in order, with evidence
   timestamps inside their spans and every verbatim command in
   backticks inside `actions` (so it is also shown on screen and
   narrated — the `eval` test suite checks all three).
4. `cargo test --test eval` validates the set; `vts-extract eval --only
   <id> --keep <dir>` renders it so you can watch the mp4 and read the
   bundle. Then refresh the baseline.

Caveats: the gold videos are synthetic (bitmap font, flat colours,
`say` voice), so absolute numbers do not transfer to real screencasts —
the suite tracks relative change. Spelled flags (`-y`, `-m`) are
narrated literally and rarely survive transcription; that is part of
what the command metric measures.

## Results

<!-- bench-results:start -->
| id | label | wall (s) | realtime | stages |
| --- | --- | --- | --- | --- |
| asr-45s | Transcribe ~45s of generated speech | 19.5 | 2.3x | — |
| extract-120s | Full extract of a 120s 720p video | 5.5 | 21.7x | ingest 0.1s · transcribe 0.7s · frames 4.8s · timeline 0.0s |
| frames-batch-5 | Batch export of 5 frames | 0.2 | — | — |

Machine: Apple M1 (Virtual) · 3 logical cores · macos/aarch64 · whisper on cpu · vts-extract 0.3.0

_Generated by `vts-extract bench` on 2026-11-03._
<!-- bench-results:end -->
