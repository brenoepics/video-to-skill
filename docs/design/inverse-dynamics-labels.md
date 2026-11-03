# Design note: optional inverse-dynamics action labels (#38)

Status: proposal, no code. Decides whether and how to run VideoAgentTrek's
Video2Action models to label GUI actions (click, drag, scroll, type,
keypress) in screen-recording segments. Written 2026-11-02 from the paper
(arXiv 2510.19488, CC BY 4.0), the repository (xlang-ai/VideoAgentTrek, MIT)
and the Hugging Face model cards.

## What the models are

| Artifact | What | Base | Licence | Size |
|---|---|---|---|---|
| `xlangai/VideoAgentTrek-IDM-s1-7B` | stage 1, action event detection: ordered `(type, start, end)` spans from a clip | Qwen2.5-VL-7B-Instruct, full-parameter SFT | Apache-2.0 | 16.58 GB bf16 safetensors (5 shards, 8.3 B params) |
| `xlangai/VideoAgentTrek-IDM-s2-7B` | stage 2, action content: type + parameters (click coordinates, typed text) from frames inside a span | Qwen2.5-VL-7B-Instruct, full-parameter SFT | Apache-2.0 | 16.58 GB bf16 safetensors |
| `xlangai/VideoAgentTrek-ScreenFilter` | cursor detector for the "screen recording" gate | YOLOv8x (Ultralytics) | **AGPL-3.0** | `best.pt` |

The issue's "two ~8 GB downloads" is off by two: the published weights are
bf16, 16.6 GB each, 33 GB for both. Nobody publishes a GGUF or other
quantized build of the fine-tunes; the base model has an official GGUF
(`ggml-org/Qwen2.5-VL-7B-Instruct-GGUF`), which is of no use here. Both
model cards are empty (no usage, no input spec); the input conventions
below come from the paper.

How they are fed: stage 1 downsamples to 4 fps and cuts the video into
non-overlapping 10 s clips (temporal patch of 2 frames); stage 2 samples
frames inside each detected span at `f = min(30, max(4, ⌊20/Δt⌋))`, so a
brief click gets 15–30 frames and a long typing span 4–20.

Reported quality (paper, held-out logs for stage 1; a 500-sample blinded
manual review for stage 2):

| Action | stage 1 F1 | stage 2 accuracy |
|---|---|---|
| click | 0.82 | 0.71 |
| scroll | 0.86 | 0.74 |
| type | 0.75 | 0.67 |
| drag | 0.62 | 0.37 |
| keypress | 0.14 | 0.36 |
| all | 0.78 | 0.66 |

Keypress and drag are unusable; clicks, scrolls and typing are useful as
evidence, not as truth.

## The ScreenFilter licence question

ScreenFilter is a YOLOv8x fine-tune; YOLOv8 and models derived from it are
AGPL-3.0 unless an Ultralytics enterprise licence is bought, and the weights
card says so. Shipping or downloading it into a tool whose output is a
skill package would put an AGPL component in the pipeline. **Not needed**:
PR #54 (issue #31) gives every segment a `cursor_presence` from a classical
arrow tracker and that is the same gate the paper applies (a pointer in
≥ 80 % of frames for ≥ 6 s; ScreenFilter's own F1 is 89.6 %, the tracker's
is unmeasured on real recordings but it needs no download and no licence).
Decision: never fetch ScreenFilter; gate on `cursor_presence`.

The two IDM checkpoints are Apache-2.0 on an Apache-2.0 base (Qwen2.5-VL),
compatible with this repository and with `deny.toml` as it stands.

## Is there a Rust path for Qwen2.5-VL inference with pinned weights?

Three candidates were checked.

1. **llama.cpp through `llama-cpp-2`** (crates.io 0.1.160, 2026-10-15,
   MIT/Apache-2.0; bindgen over a vendored llama.cpp built with cmake —
   the same shape as the `whisper-rs` → whisper.cpp dependency the
   extractor already carries, so Metal/CUDA/MSVC build knowledge and the
   `cargo deny` posture transfer). libmtmd supports Qwen2.5-VL (text GGUF +
   `mmproj` projector GGUF) and the crate exposes it as an `mtmd` module with
   an example. **Catch:** the fine-tunes exist only as safetensors, so we
   would have to convert them ourselves (`convert_hf_to_gguf.py`, plus
   `--mmproj`) and host the GGUFs (GitHub release assets, attested like the
   binaries; ~5 GB at Q4_K_M + ~1.3 GB projector per model). That is a
   supply-chain step the registry does not have today: the pinned sha256
   would be of *our* conversion, with a reproducibility note (converter
   commit, quant type) rather than an upstream file.
2. **mistral.rs** (`mistralrs` on crates.io since 0.7, 0.8.x now;
   Apache-2.0/MIT). `model_metadata.rs` lists Qwen2.5-VL with text, vision
   and *video* modalities, loads safetensors directly from a local path and
   quantizes in place (ISQ, e.g. Q4K) at load, with Metal and CUDA
   features. No conversion step: the registry would pin the upstream shards
   (5 files, 16.6 GB per model) and the loader would quantize on first use.
   Cost: a much larger dependency tree (candle, tokenizers, its own kernels),
   a longer compile, and a crate that moves fast; the one-line ISQ load
   still spends 33 GB of disk for weights that run at 5 GB.
3. **candle** alone: no first-party Qwen2.5-VL example; the only
   implementation found is a third-party crate (`aha`) that was not
   verified. Not viable.

Either 1 or 2 is a real path. 1 reuses what the project already does for
whisper and keeps the runtime small; 2 keeps the downloads upstream-pinned.
If this is built, start with 1 and publish the converted GGUFs as attested
release assets.

## Memory and time per video (estimates, to be measured before any build)

Memory: bf16 weights alone are 16.6 GB, so a bf16 run needs ~20 GB of
unified/GPU memory with activations and a 3k-token clip; at Q4_K_M the
language model is ~4.7 GB plus a ~1.3 GB fp16 projector, ~7–8 GB peak.
The two stages run one after the other and only one model is resident, so
the peak is one model, but the second stage pays another model load.

Time, from the paper's sampling and Qwen2.5-VL's token accounting (one
visual token per 28×28 patch, two frames per temporal patch; at a 448×252
frame budget that is 144 tokens per frame pair):

- stage 1: 10 s clip = 40 frames = 20 pairs ≈ 2,900 visual tokens; a
  10-minute video is 60 clips ≈ 175k prefill tokens;
- stage 2: the paper's corpus averages ~39 steps per video; ~15 frames per
  step ≈ 2,200 tokens, ≈ 85k prefill tokens, plus short decodes.

At the 400–800 prompt tokens/s a Q4 7B VLM reaches on Apple Silicon Metal
(the vision tower adds to that), a 10-minute GUI video costs roughly
5–10 minutes of inference, i.e. about 1–2× realtime, where the extractor
today runs at ~30× realtime on a CI VM and far faster locally. On a CPU-only
runner (the CI VMs keep whisper on the CPU for the paravirtual-Metal reason
in BENCHMARK.md) it would be 10–30× slower still: this can never run in the
test matrix or the bench, only behind an `--ignored` real-weights test like
whisper's.

## Feature-flag plan (if the user decides to proceed)

- Cargo feature `actions`, off by default, pulling the runtime; release
  builds without it stay as they are. CI compiles the feature once
  (`cargo check --features actions`) without weights.
- Weights: two registry entries in `src/deps/registry.rs` (owned by the
  deps session; added through a PR there), each a list of files with
  sha256 and a commit-pinned URL, fetched only by
  `vts-extract check --fix --yes --actions`, never implicitly.
- Runtime gate: `vts-extract extract --actions` runs the models only on
  segments with `cursor_presence ≥ 0.8` (from #54), stage 1 per 10 s clip of
  those segments, stage 2 per detected span. Everything else is skipped and
  the timeline says so in `notes`.
- Output: `timeline.json` segments gain
  `actions: [{type, start_secs, end_secs, confidence, params}]` with
  `params` one of `{x, y}` (native pixels), `{text}`, `{direction, amount}`;
  keypress and drag are dropped (F1 0.14 / 0.62, accuracy 0.36 / 0.37).
  `references/analysis-method.md` tells the agent they are evidence for
  what happened, never coordinates to put in a skill (steps stay
  intent-level, as today).
- Tests: unit tests on the span/parameter parsing against canned model
  output; one `--ignored` end-to-end test on the GUI walkthrough fixture
  from #54 with real weights.

## Recommendation

Do not build this now. The cheap evidence already landed or in review —
settled keyframes (#42), native crops of what changed (#46) and before/after
crops around the pointer with a presence gate (#54) — gives the reading
agent what Video2Action would label, at a cost of a second decode rather
than two 7 B models and 33 GB of weights, and the agent is better at
reading a before/after crop than stage 2 is at naming the action (0.66).
Revisit when the gold-set eval (#34) shows GUI step recall is the limiting
factor; then build path 1 (llama.cpp, self-converted attested GGUFs), on
demand per segment rather than at extract time, and measure the numbers
above before pinning anything.

## Sources

- Paper: https://arxiv.org/abs/2510.19488 (HTML: https://arxiv.org/html/2510.19488v1)
- Repository: https://github.com/xlang-ai/VideoAgentTrek
- Weights: https://huggingface.co/xlangai/VideoAgentTrek-IDM-s1-7B , https://huggingface.co/xlangai/VideoAgentTrek-IDM-s2-7B , https://huggingface.co/xlangai/VideoAgentTrek-ScreenFilter
- llama.cpp multimodal: https://github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md ; Rust bindings: https://github.com/utilityai/llama-cpp-rs
- mistral.rs: https://github.com/EricLBuehler/mistral.rs (model table in `mistralrs-core/src/model_metadata.rs`)
