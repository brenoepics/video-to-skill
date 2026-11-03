# Hermetic video: screencasts rendered from recipes

The hermetic tier of this benchmark ships tutorial videos as **data**. A
recipe (`videos/recipes/*.json`, validated by
[`schemas/recipe.schema.json`](../schemas/recipe.schema.json)) describes a
screencast; `v2sbench.render` turns it into an mp4 at run time. No video
bytes are committed, so there is no licensing question, nothing to
re-download, and nothing a model could have memorised from the open web.

Read this together with [preregistration §7.1](preregistration.md), which
states the trade-off this tier makes. **[Limitations](#limitations-what-a-rendered-screencast-is-not)
at the end of this page is the part that matters most**: results on
rendered video *overstate* what the extraction pipeline achieves on real
screencasts, and nothing here should be quoted without that caveat.

## What the renderer produces

| | |
|---|---|
| Container | mp4, `yuv420p`, H.264 (libx264, CRF 20, `preset medium`) |
| Frame rate | constant, from the recipe (`fps`), exact frame count = `round(duration_secs * fps)` |
| Size | 1280x720 or 1920x1080 (the schema's enum) |
| Audio | one mono AAC track at **16 kHz** — whisper's native rate — or no audio track at all |
| Reproducibility | byte-identical across runs on one machine with one ffmpeg and one TTS build |

## The pipeline

```
recipe.json
  │  recipe.py    load → JSON Schema → renderer invariants → normalise
  ▼
Recipe(stills=[Still(t, screen, step, continuous, typing)], narration=[Cue(t, text)])
  │  video.plan()        stills → frames (constant rate, typing expanded per keystroke)
  ▼                                        narration.synthesise()
RenderPlan(segments)                         cues → say / espeak-ng → 16 kHz mono
  │  screens.draw()      segment → RGB24 raster          → summed into one WAV
  ▼                                                           │
raw RGB24 on ffmpeg's stdin ───────────────────────────────────┴──→ out.mp4
```

Nothing is written to disk except the narration WAV and the mp4: frames go
straight down a pipe, so a 20 s 720p render costs about five seconds and no
temporary gigabytes.

## API

```python
from v2sbench.render import render, plan, verify, load_recipe

result = render("videos/recipes/term-git-init.json", "out/term-git-init.mp4")
result.sha256              # '59de08de…'   what was produced
result.narrated            # True
result.narration_backend   # 'espeak-ng' | 'say' | None
result.ffmpeg_version      # 'ffmpeg version 6.1.1-3ubuntu5 …'
result.warnings            # authoring problems worth fixing (see below)

verify(result.path, load_recipe(recipe), expect_audio=result.narrated)
```

`render(recipe_path, out_path, *, tts="auto", quiet=False, typing=True,
typing_secs=0.6, ffmpeg="ffmpeg", require_narration=False)`.

`plan(recipe, *, typing=True, typing_secs=0.6)` returns the same
frame-by-frame schedule the renderer follows without invoking ffmpeg, so
timing can be asserted in a unit test; `frames(recipe, plan)` turns that
schedule into canvases, and `save_png(canvas, path)` writes one out to
look at. `verify(out_path, recipe, *, expect_audio=None)` re-reads the
finished file with ffprobe and raises unless the size, frame rate, frame
count, duration (within one frame) and audio presence are what the recipe
asked for.

## Determinism

Two renders of one recipe, on one machine, with one ffmpeg and one TTS
build, produce **byte-identical** files. Measured on the test fixture
(`harness/tests/fixtures/recipe_sample.json`, 1280x720, 12 fps, 20 s,
ffmpeg 6.1.1-3ubuntu5, espeak-ng 1.51):

```
59de08de2064793b4f03054c617f3c74ebfd12381883ebf17c4dde62a63336a3  hermetic-1.mp4
59de08de2064793b4f03054c617f3c74ebfd12381883ebf17c4dde62a63336a3  hermetic-2.mp4
```

What makes that true:

| Source of drift | What pins it |
|---|---|
| Glyph rasterisation | a 5x7 bitmap font bundled **as data** in `screens.py`, upscaled by an integer factor. No Pillow, no FreeType, no system font: every lit pixel follows from integer arithmetic |
| Frame timing | raw RGB24 frames at a constant rate on stdin, `-fps_mode cfr`, exact frame count from the recipe |
| Colour conversion | `-sws_flags +bitexact+accurate_rnd` |
| Encoder metadata | `-fflags +bitexact` *as an output option* (it reaches the muxer only there), `-flags:v/-flags:a +bitexact`, `-map_metadata -1`, `-map_chapters -1`. The file carries no `creation_time` and no `Lavf<version>` tag |
| x264's version SEI | `-bsf:v filter_units=remove_types=6`. `+bitexact` does **not** suppress the SEI in which x264 writes `core 164 r3108 …` plus its whole option list; dropping that NAL leaves the decoded pixels bit-identical (checked with `framemd5`) |
| Thread-dependent slicing | `-threads 1`. x264 otherwise picks its slice layout from the core count |
| Audio mixing | cues are decoded to raw 16 kHz mono PCM and summed into a silent buffer at integer sample offsets, in Python. `amix` was avoided because it rescales by input count, so adding a cue would change every other cue's loudness |
| Wall clock | nothing in the pipeline reads it. Temporary directory names and the output filename do not reach the file (two renders through different temp dirs hash the same) |

### What the guarantee does **not** cover

* **A different ffmpeg or libx264 build produces a different file.** This is
  not fixable from here, so `RenderResult.ffmpeg_version` records the build
  that made the file; a run record that pins it makes a mismatch visible
  rather than mysterious.
* **A different TTS build produces different audio**, and therefore a
  different file. espeak-ng 1.51 is byte-deterministic for a given text,
  voice, speed and pitch (verified), but espeak-ng 1.52 need not match it,
  and macOS `say` matches neither — its voices are bundled with the OS.
  Preregistration §7.1 says "byte-identical on every machine with the same
  ffmpeg"; strictly it is *the same ffmpeg **and** the same TTS build*. If
  cross-machine byte-identity is ever needed as a gate, pin both, or gate
  on the video stream alone (the drawing is machine-independent).
* **The PNG preview path** uses Pillow when installed and the standard
  library otherwise. Previews are for human eyes; they never touch the mp4.

## Narration

Backends are tried in order: macOS `say`, then `espeak-ng`, then none.
Each cue is synthesised separately, decoded to 16 kHz mono, and placed at
its `t`; the gaps are silence. Cue text is passed on **stdin**, never as an
argument, so a cue beginning with `-` cannot become a command-line flag.

If no backend is installed the renderer emits a video with **no audio
track**, sets `narration_backend=None`, and prints to stderr — even under
`quiet=True`:

```
v2sbench.render: <id>: NO NARRATION -- rendering a SILENT video.
Transcript-derived arms are not scoreable on this file.
```

Pass `require_narration=True` to turn that into a hard failure. A silent
render must never be scored in the transcript-only or raw-transcript arms:
those arms would be measuring an empty context and would look like a
crushing win for the full-video arm, which is exactly the kind of artefact
this benchmark exists to avoid.

`result.warnings` also reports cues that run into the next cue or past the
end of the video. Overlapping cues still render (they are summed and
clamped), but they are an authoring mistake: whisper will transcribe the
overlap as one slurred stretch.

## Typing animation

A still whose `terminal` block has a `type` field shows the command being
typed over `typing_secs` (0.6 s by default), one segment per frame, rather
than appearing all at once. Instant appearance would make shot detection
trivially easy in a way real screencasts are not; the gradual change gives
the detector something closer to a real frame-to-frame delta. It is
deterministic — the k-th frame shows `len(cmd) * (k+1) // stages`
characters — and switchable with `typing=False`.

**This interacts with 1 fps sampling.** The extractor samples one frame per
second, so a typed command must stay fully on screen across at least one
whole-second boundary or OCR may only ever see a half-typed command. The
planner warns when a typed still is shorter than `typing_secs + 1 s`:

```
still at t=12.0 types 38 characters but holds for only 1.0s;
a 1 fps sampler may only ever see a half-typed command
```

Give every typed still at least two seconds.

## Writing a recipe

The schema is the contract and is not extended here. On top of it the
renderer enforces what it needs and the schema cannot express — these are
the same rules `Recipe::validate` applies in the parent project's Rust
gold-set renderer, so a recipe that renders here renders there:

* the first still is at `t=0`, and still times strictly increase;
* every still starts before `duration_secs`;
* each still declares **exactly one** of `terminal` / `dialog` / `slide` /
  `desktop`;
* a `step` label marks one contiguous run of stills — a label cannot come
  back after another step;
* narration cues strictly increase, carry text, and start before the end.

Semantics worth knowing:

* **The terminal is a state machine, not a picture.** A `terminal` block is
  a set of edits applied to the terminal as it stands, in field order
  `lines`, `title`, `prompt`, `type`, `output`. `type` replaces the bare
  prompt with `prompt + command`; `output` appends lines and a fresh
  prompt; `lines` replaces the whole screen; the screen scrolls at 18 rows.
  A slide in the middle of a session does not reset the terminal.
* **`continuous` means "not a change".** Every still after the first that
  is *not* marked `continuous` is a gold visual change, matched by the
  boundary metric at ±2 s (two 1 fps ticks). Use `continuous` for scrolling
  or streaming output inside one state — that is the convention the parent
  project's eval uses, and keeping to it is what lets a recipe move between
  the two systems.
* **Narration must be vague where the pixels are exact** (preregistration
  §7.2). Say "initialise the repository in a new directory called demo
  repo" while the screen shows `git init demo-repo`.

### Portability to the parent project's gold set

The two formats are shape-compatible but not identical, and the renderer
deliberately does not paper over the gaps:

| | v2s-bench schema | Rust gold set |
|---|---|---|
| slide heading | `slide.heading` | `slide.title`, plus `slide.footer` |
| dialog focus | `dialog.focus` (also where the cursor goes) | `dialog.status`, no focus field |
| terminal | identical | identical |

A recipe using `slide.heading` or `dialog.focus` needs those two fields
renamed before `vts-extract eval` will read it, and vice versa. Everything
else — `stills`, `t`, `step`, `continuous`, `terminal`, `narration` —
carries across unchanged.

## Looking at a frame

```python
from v2sbench.render import load_recipe, plan, frames, save_png
recipe = load_recipe("videos/recipes/term-git-init.json")
save_png(frames(recipe, plan(recipe))[2][0], "/tmp/frame.png")
```

or pull one out of the finished file:

```bash
ffmpeg -ss 7.5 -i out.mp4 -frames:v 1 /tmp/frame.png
```

## Limitations: what a rendered screencast is **not**

Every number produced on this tier carries these caveats. They are not
hedges; they are the reason the tier exists to *bound the mechanism, not
measure the product*.

* **Flat colours.** Three or four exact RGB values per screen, no
  gradients, no transparency, no sub-pixel anti-aliasing. Text/background
  contrast is far above anything a real theme gives you.
* **A bitmap font.** 5x7 glyphs scaled by an integer factor: no hinting, no
  ligatures, no kerning, every glyph on an exact pixel grid. Real
  screencasts use anti-aliased variable-width-ish terminal fonts rendered
  by FreeType or Core Text. OCR finds this font far easier than a real one.
  Four glyphs are additionally redrawn *so that OCR cannot confuse them* —
  `-` is narrowed so `--flag` reads as two dashes, `l` gains a foot so
  `1`/`l`/`I` are three shapes, `g` gains a tail so `cargo` is not read as
  `car9o`. Real fonts do not come with that courtesy.
* **No compression artifacts worth the name.** CRF 20 on flat synthetic
  frames is nearly lossless. Real screencasts arrive re-encoded by a
  platform at a fraction of the bitrate, with ringing around text, chroma
  smear on coloured prompts and macroblocking after a cut.
* **No cursor jitter, no scroll inertia, no human timing.** The pointer
  teleports to the focused control and sits there, perfectly still. Typing
  is linear. Nobody mis-types, backspaces, pauses to read, or drags a
  window. Shot detection has an easier job here than it will ever have on a
  real recording.
* **No real window chrome.** A rectangle, a title bar and three dots. No
  shadows, no rounded corners, no translucency, no desktop behind the
  window, no notifications sliding in, no second monitor.
* **Synthetic narration.** One espeak-ng or `say` voice, constant pace,
  no accent, no room tone, no filler words, no breath, no background music,
  no crosstalk. ASR word error rate on this audio is far below what it is
  on a real tutorial — and the parent project's own benchmark notes that
  spelled flags (`-y`, `-m`) rarely survive transcription even here.
* **No camera, no browser, no editor, no video player** — four screen kinds
  and nothing else. Whole categories of real tutorial are unrepresentable.

So: strong extraction scores on this tier mean the pipeline works when the
input is clean. They do **not** predict performance on a real screencast,
and the two tiers are never averaged.
