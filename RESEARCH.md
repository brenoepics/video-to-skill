# video-to-skill — Research Survey & Architecture Plan

*Compiled 2026-08-05 from a four-track literature and market review (procedure extraction from instructional video · agents learning skills from demonstrations · long-video multimodal understanding · competitive landscape).*

---

## 1. The validated gap

The field splits into four camps, and **no one occupies the seam between them**:

| Camp | Representative work | Input | Output | Why it falls short |
|---|---|---|---|---|
| Video-understanding products | Scribe, Tango, Loom AI, Guidde, Vidocu, NotebookLM, video2doc | Any video (sometimes) | **Human-readable** docs/SOPs/notes | Mostly transcript-only; frames used as illustrations, not grounding; nothing an agent can act on |
| Demonstration-capture tools | Anthropic Cowork Recorder skill, browser-use/workflow-use, Anchor Replicate, SkillForge | **Live instrumented capture** of your own session (click/keystroke telemetry) | Agent-executable workflow/skill | Cannot ingest an existing video file; misses the entire back-catalog of tutorial video |
| Academic video mining | VPT, Watch & Learn, VideoAgentTrek, TongUI, Video2GUI | Web video at scale | **Training data for model weights** | Knowledge is baked into a model, not a portable, inspectable, per-task artifact |
| Academic skill induction | Voyager, Agent Workflow Memory, ICAL, ASI, SkillWeaver | The agent's **own trajectories** | Skill libraries / workflows | Cold-start problem: the agent must already succeed once before a skill exists |

**Whitespace:** *arbitrary pre-existing video (YouTube tutorial, conference talk, vendor training, a colleague's Loom) → spec-compliant, agent-executable, portable skill package (SKILL.md per agentskills.io) — inferred from pixels + narration alone, with no event telemetry.*

Anthropic's own engineering blog explicitly frames agents that "codify their own patterns of behavior into reusable capabilities" as **future work**. The closest academic system (Learning from Online Videos at Inference Time, arXiv:2511.04137) keeps extracted knowledge **ephemeral** — recomputed per task, never persisted. The closest industry system (Anthropic Recorder) requires live capture. The seam is open.

---

## 2. Key papers to build on

### Procedure extraction from instructional video
- **HowTo100M** (ICCV'19, arXiv:1906.03327) — narration-as-supervision paradigm; noisy ASR-clip alignment.
- **StepFormer** (CVPR'23, arXiv:2304.13265) — unsupervised step discovery/localization via learnable step queries + Drop-DTW.
- **VINA / HT-Step** (ICCV'23, arXiv:2306.03802) — grounding wikiHow steps in video with zero step supervision.
- **Paprika** (CVPR'23, arXiv:2303.18230) — Procedural Knowledge Graph fusing wikiHow + HowTo100M.
- **Video-Mined Task Graphs** (NeurIPS'23, arXiv:2307.08763) — probabilistic task graphs mined from how-to videos as priors.
- **Differentiable Task Graph Learning** (NeurIPS'24 spotlight, arXiv:2406.01486) — end-to-end task-graph induction; +16.7% graph F1 on CaptainCook4D.
- **SCHEMA** (ICLR'24, arXiv:2403.01599) — steps as *state changes*; explicit world-state tracking in language space (never visually verified — an open hook).
- **RECIPE** (2026, arXiv:2605.19976) — **key insight**: a video corpus is a cheap *verifier* even when it's a bad *labeler*; RL reward = grounding quality against the corpus.
- **Resource2Skill** (2026, Microsoft, arXiv:2606.29538) — closest academic cousin: distills videos+repos+articles into a hierarchical multimodal "Skill Wiki" of executable skills (+11.9 pts over skill-less agents). Software-authoring domains only.
- **TutoAI** (CHI'24, arXiv:2403.08049) — the rare work framing "video → structured document" as the deliverable, with human-in-the-loop correction.
- Datasets/benchmarks: CrossTask, COIN, NIV (planning), HT-Step (grounding), Ego-Exo4D (keysteps), CaptainCook4D/Assembly101 (task graphs, mistake detection).

### Agents learning skills from demonstrations
- **VPT** (2022, arXiv:2206.11795) — inverse dynamics model pseudo-labels unlabeled video → behavioral cloning at scale.
- **Watch & Learn** (2025, arXiv:2510.04673) — internet videos → 53K executable UI trajectories via inverse dynamics.
- **VideoAgentTrek** (2025, arXiv:2510.19488) — 55K YouTube tutorials → 1.52M parameterized GUI actions (ScreenFilter cursor detection + Video2Action IDM on Qwen2.5-VL). ~70% action recovery; drag/keystroke actions weak.
- **Learning from Online Videos at Inference Time** (2025, arXiv:2511.04137) — retrieve tutorial videos at inference, convert to structured trajectories, inject as in-context guidance. Visual info beats transcript-only. **Ephemeral** — the gap we exploit.
- **Voyager** (2023, arXiv:2305.16291) — skill library of executable code, self-verification.
- **ICAL** (NeurIPS'24, arXiv:2406.14596) — abstracts noisy demos into causal annotations (state changes, subgoals, construals).
- **AWM** (2024, arXiv:2409.07429) — workflow induction from trajectories; +51.1% WebArena.
- **ASI** (2025, arXiv:2504.06821) — skills as Python programs, **verified by re-execution** — the verification pattern to steal.
- **SkillWeaver** (2025, arXiv:2504.07079) — autonomous skill discovery; skills transfer strong→weak agents (+54.3%).

### Long-video understanding stack (production-relevant SOTA)
- **Deep Video Discovery** (NeurIPS'25, arXiv:2505.18079) — the agentic-search reference: multi-granular DB (global summary / clip captions / frames) + tool-calling agent (`search_segments`, `inspect_frames`, `get_transcript`). SOTA on LVBench. **Agents beat monolithic models on hour-plus video.**
- **Video-RAG** (2024, arXiv:2411.13093) — training-free: ASR + OCR + detection as retrievable "auxiliary texts"; large gains over frames-alone.
- **LongVU** (2024, arXiv:2410.17434) — DINOv2-similarity frame dedup (doubles as slide-change detection).
- **AKS** (CVPR'25) / **BOLT** (arXiv:2503.21483) — query-conditioned keyframe selection under token budgets.
- **Qwen3-VL** (2025, arXiv:2511.21631, Apache-2.0) — best open-weight for **timestamp-grounded** video reasoning (native text-timestamp alignment).
- **Gemini 2.5/3 Pro** — best managed "just upload the video" (1M ctx, ~300 tok/s of video, audio+visual, timestamps).
- **InternVideo3** (2026, arXiv:2606.12195) — leading open-weight video foundation model.

### Component picks (2026)
- **ASR**: Parakeet-TDT 0.6B v3 (English, 49× Whisper speed, 6.3% WER) or Whisper large-v3 (multilingual) + WhisperX forced alignment for word-level timestamps.
- **Diarization**: pyannote 3.1.
- **Shot/scene**: TransNetV2 (fades/wipes) + pHash/DINOv2 dedup for slide/screen-state changes (collapses 1fps frames 10–50×).
- **OCR**: PaddleOCR PP-OCRv5 (cheap, per-keyframe) + VLM for layout/code/diagrams.
- **Embeddings**: text embeddings over captions/transcript (cheapest); SigLIP2 or TwelveLabs Marengo 3.0 if visual-semantic moment search is needed.

---

## 3. Gaps nobody has closed (= the novelty budget)

1. **Video → portable skill artifact** — unclaimed by any paper or product.
2. **No video-sourced SKILL.md generation** — Anthropic's skill-creator is text-in only; they've publicly flagged this as future work.
3. **Verification loop for video-derived procedures** — ASI's verify-by-re-execution has never been combined with video ingestion. Sandbox-execute the drafted skill, diff against video-grounded expectations, repair. Also closes the video-to-live-UI drift that breaks replay tools.
4. **Cold-start inversion** — the human's video supplies the successful trajectory the agent couldn't produce itself; solves AWM/SkillWeaver's acknowledged cold-start limitation.
5. **Cross-video consolidation** — N videos of the same task → one canonical skill with variant branches. Essentially unaddressed.
6. **ASR+frames+OCR triple fusion for procedures** — on-screen text (burned-in step titles, terminal commands, UI labels) is often the *most reliable* signal and is absent from the transcript; procedural literature barely uses it.
7. **Task graphs beyond precedence** — preconditions/effects, tools/materials, parameters, optional vs. mandatory, failure/repair branches. SCHEMA's LLM state-descriptions point the way but are never visually verified.
8. **Video corpus as verifier** (RECIPE) — applied only to planning reward so far; could confidence-score every extracted step by whether it grounds in independent evidence.
9. **Provenance** — no tool links each skill step to timestamped video evidence.
10. **Skill lifecycle** — versioning against app updates, strong→weak agent transfer, eval harnesses scoring generated skills by task success. None exist for video-sourced skills.
11. **Benchmark vacuum** — "video → verified structured procedure" has no benchmark; existing planning datasets are single-trajectory and closed-vocabulary. Publishing one is a paper-shaped opportunity.

---

## 4. Proposed architecture

**Thesis:** *Watch → Understand → Compile → Verify → Consolidate.* The skill is a **compiled, verified, provenance-linked artifact**, not a summary.

```
                ┌────────────────────────────────────────────────────────┐
   video file / │  1. INGEST      ffmpeg · yt-dlp · format normalization  │
   URL          │                 audio demux · 1fps frames + native      │
                │                 stills at scene/slide changes           │
                └───────────────┬────────────────────────────────────────┘
                                ▼
                ┌────────────────────────────────────────────────────────┐
                │  2. PERCEPTION (parallel tracks)                        │
                │   audio: ASR (word-level ts) + diarization              │
                │   visual: TransNetV2 shots · DINOv2/pHash dedup ·       │
                │           PaddleOCR per keyframe · VLM segment captions │
                │           (timestamped; describe actions, cite screen   │
                │           text/code/UI — Qwen3-VL or Gemini)            │
                └───────────────┬────────────────────────────────────────┘
                                ▼
                ┌────────────────────────────────────────────────────────┐
                │  3. GROUNDED INDEX (video RAG, DVD-style)               │
                │   global summary · segment captions+transcript+OCR in   │
                │   hybrid BM25+vector store · frame-level pointers       │
                │   → durable, queryable artifact in its own right        │
                └───────────────┬────────────────────────────────────────┘
                                ▼
                ┌────────────────────────────────────────────────────────┐
                │  4. AGENTIC DISTILLER (tool-calling agent, re-watches)  │
                │   tools: search_segments · get_transcript(t0,t1) ·      │
                │          inspect_frames(t0,t1, question)                │
                │   genre router: GUI screencast → action extraction      │
                │                 CLI/code → command/OCR extraction       │
                │                 talk/lecture → concept/reference skill  │
                │                 physical task → checklist skill         │
                │   output: PROCEDURE IR (task graph: steps with goals,   │
                │   preconditions/effects, parameters, decision points,   │
                │   failure branches, per-step video provenance + conf.)  │
                └───────────────┬────────────────────────────────────────┘
                                ▼
                ┌────────────────────────────────────────────────────────┐
                │  5. SKILL COMPILER                                      │
                │   IR → agentskills.io folder: SKILL.md (frontmatter +   │
                │   intent-level instructions) + scripts/ (extracted      │
                │   commands, code) + references/ (key frames, tables,    │
                │   provenance manifest with timestamps)                  │
                └───────────────┬────────────────────────────────────────┘
                                ▼
                ┌────────────────────────────────────────────────────────┐
                │  6. VERIFIER (the moat)                                 │
                │   sandbox-execute the skill (fresh agent, no video      │
                │   access) · diff outcome vs. video-grounded success     │
                │   criteria · repair loop · skill-creator-style trigger  │
                │   evals · ship only skills that pass                    │
                └───────────────┬────────────────────────────────────────┘
                                ▼
                ┌────────────────────────────────────────────────────────┐
                │  7. LIBRARY & LIFECYCLE                                 │
                │   cross-video consolidation (N videos → canonical skill │
                │   + variants, RECIPE-style corpus verification) ·       │
                │   versioning · publish to skill marketplaces            │
                └────────────────────────────────────────────────────────┘
```

### Design principles
- **Intent-level, not coordinate-level.** Steps are goals + success criteria ("open the Settings pane and enable X", not "click (412, 87)") — robust to UI drift, portable across agents (Claude Code, Codex, Gemini CLI via the open spec).
- **Every claim has provenance.** Each step carries `[t=12:34–13:02]` links to frames + transcript spans and a confidence score. Low-confidence steps get flagged for the verifier or the human.
- **The transcript is a witness, not the source of truth.** Narration says *why*; the frames say *what actually happened*; OCR captures what neither says (exact commands, versions, flag names). Fuse all three; on conflict, trust pixels.
- **Progressive disclosure native.** Compile to the SKILL.md loading model: name+description → body → bundled references, so a 2-hour course becomes a skill that costs ~50 tokens until needed.

---

## 5. Phased build plan

**Phase 0 — Skeleton (week 1):** repo scaffold, ffmpeg/yt-dlp ingest for mp4/mkv/webm/mov + YouTube URLs, config for model backends (local vs. API).

**Phase 1 — Perception (weeks 1–3):** ASR+alignment, shot/slide segmentation, keyframe dedup, OCR, VLM segment captioning. Milestone: a *grounded timeline JSON* for any video.

**Phase 2 — Index + distiller (weeks 3–6):** hybrid store, DVD-style tool-calling distiller agent, genre router, Procedure IR schema. Milestone: correct task graphs with provenance on 10 diverse test videos.

**Phase 3 — Compiler (weeks 6–8):** IR → agentskills.io package; extracted scripts; provenance manifest. Milestone: skills load and trigger correctly in Claude Code.

**Phase 4 — Verifier (weeks 8–12):** sandboxed execution (start with CLI/code skills — easiest to verify), repair loop, trigger evals. Milestone: **the flagship demo** — feed a YouTube tutorial for a CLI tool the agent has never used; the generated skill lets a fresh agent complete the task; ablation shows transcript-only fails.

**Phase 5 — Consolidation + lifecycle (months 3+):** multi-video canonical skills, corpus-as-verifier confidence, versioning, marketplace publishing, and optionally a public benchmark ("video → verified procedure") — the research contribution.

### Scope discipline
Start with **screencast + CLI/code tutorials** (verifiable end-to-end, OCR-rich, the strongest demo), then GUI tutorials (intent-level steps, sandbox browser verification), then talks/lectures (reference skills), then physical procedures (checklist skills — valuable but unverifiable by execution).

### Risks
- VLM action-recovery on in-the-wild video is ~70% (VideoAgentTrek) — mitigated by intent-level steps + verifier repair, rather than exact action replay.
- Anthropic Recorder adding file-upload ingestion would collapse part of the gap — move fast on the verifier + consolidation layers they're not architected for.
- Long-video cost — mitigated by aggressive dedup (10–50× frame collapse) and agentic selective re-watching instead of full-video VLM passes.

---

## 6. Product form (v2, decided): an agent skill, not an app

Model: [virgiliojr94/book-to-skill](https://github.com/virgiliojr94/book-to-skill) — a hybrid **agent skill + local extractor**, installed by cloning into the skills directory and invoked as a slash command. video-to-skill adopts the same usage:

```
git clone <repo> ~/.claude/skills/video-to-skill
/video-to-skill ./tutorial.mp4        # or a YouTube URL
/tutorial <question or task>          # generated skill, loaded on demand
```

**Why this collapses earlier problems:**
- **No API keys at all.** The invoking agent (Claude Code / Copilot CLI / Amp) *is* the distiller, the VLM, and the OCR — Claude reads deduped keyframes directly as images via its own multimodal input. The single-key provider-abstraction problem disappears.
- **No GUI needed.** Usage is `/video-to-skill <path-or-url>`; the audience is agent users, and the skill handles everything conversationally.
- **No Hugging Face exposure.** The only local model is whisper.cpp (ungated GGML weights, auto-downloaded on first run).

**Division of labor:**
1. **Local extractor (Rust, single prebuilt binary per platform; Metal on Apple Silicon):** the deterministic, non-LLM work the agent can't do — ffmpeg/yt-dlp ingest, scene/slide detection + pHash keyframe dedup, whisper.cpp ASR with word timestamps, optional Apple Vision OCR on Mac. Emits an `extraction/` bundle: `timeline.json` (aligned transcript + shot/slide boundaries + keyframe manifest), `frames/*.jpg`, OCR text. Fully local — the video never leaves the machine; only what the agent chooses to look at enters context.
2. **The agent (via SKILL.md orchestration):** the intelligence — reads the timeline, *selectively inspects keyframes* (the agentic-distiller / Deep-Video-Discovery pattern, with the extractor providing `frame-at <t>` / `clip <t0> <t1>` helper commands for re-watching), builds the Procedure IR, compiles the output skill package (SKILL.md + steps/ + references + provenance with timestamps), and optionally runs verify-by-execution on CLI-type skills.

**Modes** (mirroring book-to-skill): `analyze` (extraction + report only), `generate` (full pipeline), `update` (fold a new/updated video into an existing skill — the cross-video consolidation entry point).

**Output skill layout** (progressive disclosure, ~book-to-skill token budgets):
- `SKILL.md` — task overview, mental model, step index (~3–4K tokens)
- `steps/01-*.md` — per-step: goal, exact commands/actions, success criteria, failure branches, `[t=MM:SS]` provenance (~1K each, on demand)
- `references/` — key frames (annotated screenshots), extracted code/config files
- `provenance.json` — step ↔ timestamp ↔ frame ↔ transcript-span map + confidence scores
