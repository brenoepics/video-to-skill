# Analysis method

Loaded on demand from SKILL.md. This is the detail layer: genre routing,
inspection budgets, and re-watch heuristics.

## Genre routing

Classify from the first keyframes + speech style, then apply the row:

| Genre | Tell-tale signs | Inspection strategy | Step extraction focus |
|---|---|---|---|
| Screencast / CLI | terminal text, editors, monospace | OCR-read every distinct screen state; commands must be verbatim (never credentials — redact those) | exact commands, flags, file paths, outputs as success signals |
| GUI walkthrough | cursor, menus, dialogs; `cursor_presence` ≥ 0.8 | keyframes at each screen change; read the segment's `cursor` crops (before/after around the pointer) before re-watching around clicks | intent-level steps: "open X pane, enable Y" — never coordinates |
| Slides / talk | static slides, low motion, speech-dense | one keyframe per slide; transcript carries most content | concepts, claims, definitions; slide text confirms terminology |
| Physical task | hands, objects, camera motion | densest inspection: clip at 2fps around each action described | goal + technique + cautions; flag that execution can't be verified |
| Mixed | any combination | route per segment, not per video | follow the dominant genre of each segment |

Each segment in `timeline.json` carries `cursor_presence`: the share of
its sampled seconds with a mouse pointer on screen, found by a classical
arrow tracker. At 0.8 or more the segment is a screen recording with a
pointer (GUI walkthrough, or a CLI session driven with the mouse); near
0 it is slides, a camera, or a terminal driven from the keyboard. Trust
it for routing before guessing from the keyframe; `null` means the
bundle predates the tracker.

## Budgets

Context is the scarce resource. Images cost ~1-2k tokens each.

- **≤ 15 min video**: initial pass = every segment keyframe, plus each
  long segment's interior keyframes (`interior_keyframes` in the
  timeline, one per ~15s — slow screencasts are pre-subdivided, so the
  deduped set stays small). Re-watch allowance: ~1 extra image per
  segment, spent only where triggered (below).
- **15-60 min**: rank segments by (a) speech mentioning concrete
  actions, (b) motion density, (c) on-screen-text likelihood (genre).
  Inspect top ~20 segments' keyframes first; expand only where the
  report needs evidence.
- **> 60 min**: work in chapters. Summarize the transcript per chapter
  first, pick the 3-5 segments per chapter that carry the procedure,
  inspect only those. Consider sub-agents per chapter, each returning
  step candidates with evidence.

## Re-watch triggers (spend budget only on these)

Each segment's keyframe (frames/shotNNN.jpg) is already the settled
result of the change that started the segment — the first second after
it in which the screen held still — so a command's output or an opened
dialog is normally in the keyframe itself. Scrolling never starts a
segment: content that only moved stays in the running segment, so a
long read-through is one segment whose interior keyframes carry the
coverage. Before re-watching a long segment, check those interior
keyframes (frames/shotNNN-kMM.jpg) — the periodic coverage often
already shows the state change that used to require hand-exporting
frames.

1. Speech describes an action ("now click...", "then run...") but
   neither the segment keyframe nor any interior keyframe shows its
   result.
2. Motion-density spike relative to the video's median — something
   happened between keyframes.
3. On-screen text is present but unreadable at keyframe resolution —
   read the segment's `changed_region` crop first
   (frames/shotNNN-changed.jpg, referenced with its pixel rectangle in
   `timeline.json`): it is the part of the screen that changed since
   the previous segment, at native resolution, so a new terminal line
   or dialog arrives at full pixel size in a small image. Fall back to
   the native still (frames/shotNNN-native.jpg) when the segment has no
   crop (the opening screen, or a change of the whole screen) or the
   text you need lies outside the changed part. Re-watch only after
   both.
4. A step's success signal (output, dialog, result state) needs visual
   confirmation.

In a segment with a pointer, the `cursor` field holds two native crops
around where the pointer was just before the change that started the
segment: `before` (frames/shotNNN-cursor-before.jpg, the moment before —
what was about to be clicked) and `after` (frames/shotNNN-cursor-after.jpg,
the settled result at the same spot), with the pointer's pixel position
and the crops' rectangles. Read them before re-watching around a click:
they show the control and its effect at full pixel size.

Prefer `frame-at` (one image) over `clip` (many). Use `clip` at 2fps
only across a described action's duration, never the whole segment.

## Confidence

- **high**: claim confirmed by both speech and pixels, or by exact
  on-screen text.
- **medium**: single-source (speech-only or visual-only) but coherent
  with surrounding evidence.
- **low**: inferred, partially occluded, or contradicted once. Say why.

## Transcript caution

Whisper mishears technical terms (e.g. "trunks" → "punks", tool names,
flags). Any token that will be *executed or typed* must come from a
frame, not the transcript. When only the transcript has it, mark the
step low-confidence and say the exact string is unverified. When
`timeline.json` says `transcript_source: creator-captions`, the words are
the creator's own and mishearing is not a risk; the rule stands anyway,
because captions paraphrase and abbreviate what was typed.

## Platform priors

A URL download records what the platform already knew, under
`platform` in `manifest.json` and at the top of `timeline.json`:

- **Chapters** (`chapters`: start, end, title) are the creator's own
  section boundaries. Treat them as step candidates to confirm against
  the frames, and as the unit of work for long videos.
- **Skips** (`skips`: start, end, category) are SponsorBlock ranges —
  sponsor, intro, outro, selfpromo. The media is uncut, so they share
  the timeline with everything else. A segment more than half inside
  such a range carries `skip: <category>`: read nothing from it, derive
  no step from it, spend no re-watch budget there. A segment that only
  overlaps a range keeps its keyframe; check the range bounds before
  citing a timestamp near them.
- **Captions** (`captions` in the manifest) name the creator track kept
  in the bundle. `transcript.json` is built from it when present, with
  whisper supplying word timing only; `source` in the transcript and
  `transcript_source` in the timeline say which happened.
