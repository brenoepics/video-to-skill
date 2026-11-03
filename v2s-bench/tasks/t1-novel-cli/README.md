# Tier 1 — novel CLI (the internal-validity anchor)

Four purpose-built command-line tools, three tasks each, twelve tasks in
all. Every tool was written for this benchmark and has never existed
anywhere else, so no model can have memorised it: that is the whole point
of the tier (`docs/preregistration.md` §4.1). The tools are real working
programs with real state on disk — nothing about a task is simulated
except the provenance of the tool itself.

Read this file before changing anything here. Several properties that look
like implementation details are load-bearing for the measurement, and are
called out as such below.

## The four tools

Each is a single standard-library Python 3.11+ file, installed to
`/usr/local/bin/<name>`, with a fixed `--version`, no network, no
timestamps, no randomness, and every write confined to the work directory
(each refuses a path that resolves outside it). Each ships its own pytest
file; `cd tools && pytest` runs all 51 tests.

| tool | version | what it does | where the difficulty comes from | docs |
|---|---|---|---|---|
| `zep` | 0.4.2 | a ledger of named "drops" of key/value facts | two-phase commit: a drop is closed with its **grant nonce**, written beside the drop when it was opened. Close it without the nonce and you get exit 0, a plausible confirmation, and a `provisional` record with no anchor. The close is `seal`, not `commit`; creation is `open`, not `init`. Chaining a drop to a parent is declared **at creation** (`--after <anchor>`) and cannot be added later | **poor** |
| `lath` | 2.3.0 | a layered config compiler | `lath set <layer> <value> <key>` — the **value comes before the key**, and the reversed order is accepted silently, staging a key named after the value. Precedence is **first-stacked-wins**, the opposite of the usual last-wins convention. A merge prints a fingerprint available nowhere else | **good** |
| `kiln` | 1.9.0 | rolls "shards" into batch artifacts | the lane is described in **`./kiln.profile`**, a `key: value` file whose fields are `fire-lane`, `shard-glob`, `hold-count` (and optionally `prior-batch`). `--help` names neither the file nor any field, and an incomplete profile is refused with one terse line that names nothing. The preparation step is `prime`, a batch keeps the **last** `hold-count` shards in name order, and editing the profile makes the primed state stale | **poor** |
| `prow` | 3.0.1 | renders rollout sheets from waves of units | `sheet` without **`--interlock`** exits 0 and writes a complete-looking sheet in which every wave is gated `parallel` instead of sequenced. The release step is `latch` — there is deliberately no `release` or `promote` — and it takes the sheet id the render printed, refusing once the file has changed | **good** |

### Discoverability split

Declared per task as `discoverability`, reported as a stratum, because it
is the main determinant of whether a skill is worth having.

- **good — `lath`, `prow`** (6 tasks). `--help` is complete and honest and
  `lath help <cmd>` / `prow help <cmd>` spell out every non-obvious rule,
  including the argument order, the precedence direction, what `sheet`
  does without `--interlock`, and that the release step is `latch`. A
  competent unaided agent that reads the documentation can work these
  out. Their tests assert the help text still says so, so a later tidy-up
  cannot silently move the stratum.
- **poor — `zep`, `kiln`** (6 tasks). `zep --help` lists its subcommands
  with one-word glosses, says nothing about the grant, and documents a
  `--token` option that `seal` does not accept (its test pins that
  mismatch as deliberate). `kiln --help` is three lines and refers to a
  handbook that does not exist. Neither is *broken*: everything needed is
  shown on screen in the task's video, and the oracle passes using only
  what the video shows.

**Source-reading leak, and its mitigation.** The tools are plain Python,
so `cat /usr/local/bin/kiln` would defeat the poor-documentation stratum.
Every poor-discoverability task therefore carries
`chmod 0111 /usr/local/bin/<tool>` in `environment.setup`, and the task
image runs the agent as a non-root user. Residual risk: an image or
harness that runs the agent as root makes `zep` and `kiln` fully
discoverable and silently converts six poor tasks into good ones. Whoever
builds the image owns that invariant; it is worth asserting in CI.

### Feedback bandwidth — and an honest warning about the mix

`feedback_bandwidth` records whether a wrong attempt is *corrected by the
environment* (`high`) or silently yields a plausible-looking wrong result
(`low`). It is a different axis from `discoverability`: `prow` is
thoroughly documented and still `low`, `kiln` is barely documented and
still `high`.

The tier comes out **9 `low` / 3 `high`**, and that imbalance is real
rather than a labelling choice:

- `zep`, `lath` and `prow` were built to the brief's "a mandatory flag
  whose omission silently produces a different valid-looking result", so
  all nine of their tasks are `low` by construction. In `zep` and `lath`
  nearly every wrong form of every step is accepted at exit 0.
- `kiln` is the counterexample: a validating tool that refuses rather than
  misbehaving. A missing or incomplete profile writes no state; a profile
  edited after priming is refused by name (`state stale`) and reported by
  `kiln peek`; a fabricated prior batch is refused as unknown. A wrong
  attempt there cannot look successful, so all three are `high`.

Two caveats a reader should have:

1. `kiln`'s loud refusals are not *informative*. `kiln: profile
   incomplete` names no field, so trial and error cannot converge on
   `fire-lane`/`shard-glob`/`hold-count` even though failure is obvious.
   These tasks are `high` on the stated definition (a wrong attempt cannot
   masquerade as success) while still having very low baseline headroom.
2. Several `low` tasks are mixtures. `zep-stage-three-facts` refuses
   staging into a non-existent or sealed drop loudly, and is labelled
   `low` only because the modal wrong end state — a provisional drop — is
   silent. The per-task comment in each `task.yaml` records the specific
   behaviour the label was judged from.

If the analysis needs a genuine mix, this tier cannot supply it on its own
and the design should change rather than the labels: adding a fourth
`kiln`-shaped validating tool, or moving two of `prow`'s tasks to a
variant that errors instead of rendering, would be the cheapest fixes.

## The twelve tasks

Three per tool, on the brief's ladder: task 1 is the single non-obvious
step, task 2 is multi-step with ordering, task 3 is the full procedure
with a parameter carried across steps.

| task | disc. | fb | the knowledge gap |
|---|---|---|---|
| `zep-seal-with-grant` | poor | low | the close must carry the drop's grant nonce; without it the drop is recorded provisional with no anchor |
| `zep-stage-three-facts` | poor | low | create → stage → close, and nothing can be staged after the close; the close still needs the nonce |
| `zep-chain-anchor` | poor | low | the parent is declared at creation from the parent's anchor, carried across steps, and cannot be attached later |
| `lath-stage-pair` | good | low | the value precedes the key; the reversed order is accepted and stages a key named after the value |
| `lath-prod-wins` | good | low | precedence is first-stacked-wins, so the overriding layer is stacked first |
| `lath-pin-fingerprint` | good | low | the fingerprint exists only in the merge's output, is carried into the next step as a value, and the second merge's own fingerprint differs |
| `kiln-author-profile` | poor | high | the profile's filename and its three field names, none of which the tool's help mentions |
| `kiln-fire-newest-three` | poor | high | `hold-count` keeps the *last* n shards, and a profile edit must be followed by priming before firing |
| `kiln-chain-batches` | poor | high | descent is declared in the profile as `prior-batch`, from the earlier firing's batch id, and the lane must be primed again |
| `prow-interlock-sheet` | good | low | `--interlock` is what sequences the waves; without it every wave is gated `parallel` at exit 0 |
| `prow-reshape-waves` | good | low | re-planning moves a unit, removal is its own step, and the sheet on disk does not follow later plan changes |
| `prow-latch-sheet` | good | low | the release step is `latch` and takes the id the render printed; releasing one rendering and then re-rendering leaves the release stale |

### Goals state the end state, never the procedure

No `goal` names a subcommand, a flag, a field name or an argument order.
They say what must be true when the agent stops: "recorded as sealed and
carrying an anchor", "wave 2 gated on wave 1 having finished", "the three
newest shards". Where a goal has to name one of the tool's own nouns to be
checkable at all (an "anchor", a "lane"), it names the noun and not the
command that produces it. This is the property most easily broken by a
well-meaning edit, and breaking it makes the task measure nothing.

### Checkers

`check/check.py` per task: standard library only, `argv[1]` the work
directory, `argv[2]` the executed-command log, one JSON object on stdout,
exit 0 whenever the checker itself ran.

Every required assertion reads real end state — the artifacts asked for,
and the tool's own state files. None reads the agent's narration and none
is satisfied by a command merely having been typed; the one or two
log-derived assertions per task are `required: false` and exist for the
`key_action_hit` procedure-fidelity metric.

Each task has at least one assertion aimed at a *plausible wrong
procedure that produces a superficially similar result*, which is what
separates these from smoke tests. The three mechanisms:

1. **Silent-wrong-result detection.** `state-sealed-not-provisional`,
   `prod-wins-the-conflict`, `rollup-holds-the-newest-three`,
   `sheet-sequences-the-waves`, `no-value-shaped-keys` — each fails a run
   that completed cleanly and produced a well-formed artifact.
2. **Provenance.** Every tool stamps its state files with a keyed digest
   of their contents, and ids are digests of the artifacts they name. So
   `anchor-is-genuine`, `layer-written-by-lath`, `state-written-by-kiln`,
   `batch-id-recomputes` and `sheet-record-matches-file` fail a
   hand-written or hand-patched result even when it is byte-identical to
   the right answer. Two of the validation runs exercise exactly that:
   `lath-pin-fingerprint`'s `dist-file-exact` and
   `prow-interlock-sheet`'s `sheet-matches-the-plan` both *pass* for a
   hand-built file while the provenance assertions fail it.
3. **Carried parameters.** `pin-is-the-earlier-fingerprint`,
   `parent-is-the-base-anchor`, `second-rollup-chains-the-first` and
   `released-sheet-is-the-one-on-disk` check that the value carried
   between steps is the right one, not merely well-formed.

### Oracles

`oracle/solve.sh` per task (`modality: shell`), run with the work
directory as `$1`. Each drives the real tool through the real procedure
and reads the values it carries out of the tool's own output; none writes
an expected end state directly. Every oracle has been run against its
checker from a fresh work directory and passes all required assertions,
which discharges admission criterion 3.

### `leakage_forbidden` — what is on the list and what deliberately is not

The list holds strings that would make a skill an answer key: assertion
ids, the tools' internal digest tags (`zep-anchor-v1`, `lath-fp-v1`,
`kiln-batch-v1`, `prow-sheet-v1`, …), the names of state files no video
ever shows (`sealed.json`, `last-flatten.json`, `latched.json`),
hard-coded expected constants (`lath-pin-fingerprint`'s
`6c7fe426b465514c`), and the oracles' own glue (`sed -n 's/^grant: //p'`,
`awk '/^anchor:/{print $2}'`).

It deliberately does **not** list the taught commands themselves. A
legitimate skill compiled from the video must contain `zep seal … --grant
…`; forbidding that would fail every honest skill and make
`validate --blind` useless. The rule applied here is: forbid what only the
checker or the oracle could know, never what the video shows. A check that
no `leakage_forbidden` string appears in the task's own recipe is part of
the validation run, so the two can never contradict each other.

## Videos

One hermetic recipe per task under `videos/recipes/<task-id>.json`,
1280×720 at 10 fps, 42–62 s, 13–23 stills.

**The on-screen text is real.** Every recipe was generated by running the
real tool in a scratch directory and pasting its actual output, so no
still teaches anything false. Regenerate rather than hand-edit if a tool
changes: invented output would silently break the benchmark.

**The narration is deliberately vague about syntax**, because that
asymmetry is what makes the transcript-only arm (A2) and the
raw-transcript arm (A6) a fair control rather than a strawman. The
narration says "we close it again, passing that nonce along" where the
screen shows `zep seal ship-42 --grant 661471fe6aa6`; it says "watch the
order of what I type" where the screen shows
`lath set edge gateway-1 host`. No cue contains a flag, a field name, a
file name or a stated rule; the facts live in the pixels.

**Each video teaches the procedure on different names from its task** —
`ship-42` not `rc-7`, `cobalt` not `amber`, `queue`/`mailer` not
`api-gw`/`cache` — so success requires carrying a procedure across names
rather than replaying a transcript. Nothing in a video can be copied
verbatim into a passing answer.

`knowledge_gap.taught_at` points at still boundaries where the fact is
legible: either the typed command that demonstrates it or the output that
reveals it. The validation run resolves every timestamp to its still and
prints what is on screen there.

Line widths are kept inside the renderer's ~52-column terminal (48 for
typed lines, 50 for output) so nothing is clipped. The generator fails
loudly on a line that would not fit — which is why no recipe prints a
tool's long help text.

## Saturation risk — read before including these in the primary endpoint

Honest expectations, to be replaced by measured `probe_known` and pilot
baseline numbers:

- **Most likely saturated: `prow-interlock-sheet`.** `prow --help` ends
  with "the one that bites: `sheet` without `--interlock` renders every
  wave gated `parallel`", and `prow help sheet` explains it at length. A
  competent baseline that reads the documentation should pass this nearly
  always. It is kept because the good-discoverability stratum needs a
  floor case, but expect it to be excluded by §5.
- **Probably saturated: `lath-stage-pair`.** The order is documented
  correctly in both `lath --help` and `lath help set`. The residual
  failure mode is an agent that skims the signature and reads it as the
  conventional key-then-value.
- **Probably has headroom but not much: `prow-reshape-waves`,
  `prow-latch-sheet`, `lath-prod-wins`** — each needs two or three
  documented facts to be combined correctly, so they should fail a
  skimming baseline sometimes.
- **Lowest baseline: the three `kiln` tasks.** The profile's field names
  are not recoverable from anything on the box, and the refusals name
  nothing, so an unaided baseline should approach 0. If a baseline *does*
  pass these, the most likely explanation is that it read the tool's
  source, i.e. the `chmod 0111` invariant was not honoured — worth
  checking before believing the number.
- **Nothing here should be impossible**: every task's oracle passes, each
  procedure is 1–6 commands, and every fact is on screen.

### Where the transcript arms may be closest to the skill arm

The comparison the tier is weakest on is skill-vs-transcript, so, named
explicitly:

- `zep-stage-three-facts` — the narration conveys the *ordering* ("create
  it first", "everything has to go in before you close it") because a
  narrator naturally would. Only the command names and `--grant` are
  pixels-only, and `zep --help` supplies the command names, so the
  transcript arm plus `--help` gets close; `--grant` is the remaining gap.
- `kiln-fire-newest-three` and `kiln-chain-batches` — the narration says
  "prepare the lane again", and `kiln --help` lists `prime`, so the
  sequencing is partly recoverable from narration alone. The field names
  are not.
- `prow-reshape-waves` — the narration states the concepts (re-planning
  moves a unit; removal is its own step) with only the syntax withheld,
  and `prow`'s own documentation supplies the syntax. Expect this one to
  show little separation between arms.
- Furthest apart (narration carries essentially nothing actionable):
  `lath-stage-pair`, `lath-prod-wins`, `kiln-author-profile`,
  `prow-interlock-sheet`, `zep-seal-with-grant`.

## Invariants worth asserting in CI

1. `cd tools && pytest` passes (51 tests).
2. Every `task.yaml` validates against `schemas/task.schema.json`; every
   recipe against `schemas/recipe.schema.json`.
3. Every oracle passes its checker from a fresh work directory, and the
   recorded wrong procedure fails it.
4. Every `probe_answer_pattern` matches its recorded correct answer and
   not its recorded wrong answer.
5. No `leakage_forbidden` string appears in the task's own recipe.
6. No narration cue contains a flag, a tool field name or a file name.
7. Every `taught_at` timestamp resolves to a still in the referenced
   recipe.
8. The poor-discoverability tools are not world-readable in the task
   image.
