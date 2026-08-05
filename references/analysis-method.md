# Analysis method

Loaded on demand from SKILL.md. This is the detail layer: genre routing,
inspection budgets, and re-watch heuristics.

## Genre routing

Classify from the first keyframes + speech style, then apply the row:

| Genre | Tell-tale signs | Inspection strategy | Step extraction focus |
|---|---|---|---|
| Screencast / CLI | terminal text, editors, monospace | OCR-read every distinct screen state; commands must be verbatim | exact commands, flags, file paths, outputs as success signals |
| GUI walkthrough | cursor, menus, dialogs | keyframes at each screen change; re-watch around clicks (motion spikes) | intent-level steps: "open X pane, enable Y" — never coordinates |
| Slides / talk | static slides, low motion, speech-dense | one keyframe per slide; transcript carries most content | concepts, claims, definitions; slide text confirms terminology |
| Physical task | hands, objects, camera motion | densest inspection: clip at 2fps around each action described | goal + technique + cautions; flag that execution can't be verified |
| Mixed | any combination | route per segment, not per video | follow the dominant genre of each segment |

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

Before re-watching a long segment, check its interior keyframes
(frames/shotNNN-kMM.jpg) — the periodic coverage often already shows
the state change that used to require hand-exporting frames.

1. Speech describes an action ("now click...", "then run...") but
   neither the segment keyframe nor any interior keyframe shows its
   result.
2. Motion-density spike relative to the video's median — something
   happened between keyframes.
3. On-screen text is present but unreadable at keyframe resolution —
   fetch the native still (frames/shotNNN-native.jpg) before re-watching.
4. A step's success signal (output, dialog, result state) needs visual
   confirmation.

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
step low-confidence and say the exact string is unverified.
