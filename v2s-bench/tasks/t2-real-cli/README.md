# Tier 2 — real pinned command-line tools

> **Grader-visible documentation.** This file names the assertion that each
> task's checker turns on, and the wrong procedures each one is built to
> catch. Anyone authoring arm material (A1–A5, including the hand-written
> A4 skill) must not read it: that is the same structural blinding
> `docs/preregistration.md` §3.2 applies to `check/` and `oracle/`. It is
> here for reviewers and for whoever has to maintain the suite.

12 tasks over 6 real tools. Tier 1 removes contamination by inventing its
tools; this tier gives that up deliberately in exchange for external
validity, and buys back what it can with declared knowledge gaps, published
probes and version pins. Every task here is a thing a real tutorial video
would plausibly teach, on a tool a real engineer has installed.

## 1. What makes a task admissible here

A tier-2 task earns its place only if a competent model can plausibly get it
wrong. `git init`, `tar -xzf` and `:q!` are saturated: the baseline scores
~100% and no skill can move it, so such a task measures saturation, not
utility (§4.1). Each task below therefore turns on one of:

- **a non-obvious flag or operator** whose absence changes the result
  (`jq`'s recursive merge, `--link-dest`, `-copy_extensions`);
- **a required-but-surprising precondition or two-phase operation**
  (`ATTACH` before a cross-database read; request-then-sign with the CA
  constraints supplied at signing time; the verdict script's third exit
  status);
- **a flag whose omission silently produces a different valid-looking
  result** — the highest-value kind, because it is exactly where
  trial-and-error fails. Six of the twelve are of this kind and are
  declared `feedback_bandwidth: silent`;
- **a resolution rule the documentation does not state** (`--link-dest` is
  resolved against the destination, not the working directory).

## 2. Realised strata

`discoverability` is decided on what the *pinned image* actually offers.
Ubuntu's official images exclude `/usr/share/man`, so there are no manual
pages: the question is what `--help`, `-h`, `.help` and the tool's own
output give you. That is why `jq` scores `poor` (its `--help` lists
`--stream` in one line and refers you to a manual page that is not
installed) while `openssl` scores `good` (`openssl x509 -help` prints
`-copy_extensions`, `-extfile` and `-extensions` outright).

`feedback_bandwidth` is decided on the **modal** wrong attempt, not the most
convenient one, and the trailing comment in each `task.yaml` records the
reasoning and any mixed cases.

### 2×2, discoverability × (silent vs. not)

|              | silent | uninformative + informative |
|---|---|---|
| **poor**     | 3 (jq, rsync ×2)                 | 2 (jq, sqlite3) |
| **good**     | 3 (ffmpeg, git, openssl)         | 4 (git ×2, openssl ×2) |

### Three-way feedback bandwidth

| value | n | tasks |
|---|---|---|
| `silent` | 6 | jq-deep-merge-layers, ffmpeg-stack-dual-camera, git-graft-legacy-history, rsync-hardlink-snapshot, rsync-mirror-protect-excluded, openssl-sign-keeping-sans |
| `uninformative` | 1 | openssl-decrypt-kdf-params |
| `informative` | 5 | jq-recover-truncated-array, sqlite-attach-merge-ledgers, git-bisect-first-bad, git-rescue-dangling-commit, openssl-issuing-ca-chain |

### Full cells

| discoverability × feedback | n | tools |
|---|---|---|
| poor × silent | 3 | jq, rsync, rsync |
| poor × uninformative | **0** | — |
| poor × informative | 2 | jq, sqlite3 |
| good × silent | 3 | ffmpeg, git, openssl |
| good × uninformative | 1 | openssl |
| good × informative | 3 | git, git, openssl |

`discoverability: good` + `feedback_bandwidth: informative` — the cell tier 1
cannot populate at all — holds 3 tasks from 2 tools. No cell with more than
one task is supplied by a single tool.

**`poor` × `uninformative` is empty, and that is a finding rather than an
oversight.** A contentless refusal is a property of immature or
badly-layered error reporting, and the mature CLIs whose documentation is
*also* poor (`jq`, `rsync`, `make`) do not produce one: `jq` names the line
and the defect, `rsync` names the path it could not find. The one genuine
`uninformative` case in the whole tier comes from OpenSSL, whose provider
error stacks print a C source location and nothing actionable — and OpenSSL
documents its own flags well, so it lands in `good`. Filling `poor` ×
`uninformative` honestly would need a tool chosen for being bad at both,
which is a different kind of task from the rest of this tier. Flagged for
the design rather than papered over.

### Tools

| tool | pinned version | tasks |
|---|---|---|
| jq | `1.7.1-3ubuntu0.24.04.2` | 2 |
| ffmpeg | `7:6.1.1-3ubuntu5` | 1 |
| sqlite3 | `3.45.1-1ubuntu2.8` | 1 |
| git | `1:2.43.0-1ubuntu7.4` | 3 |
| rsync | `3.2.7-1ubuntu1.5` | 2 |
| openssl | `3.0.13-0ubuntu3.16` | 3 |

## 3. Environment

`environment.image: ubuntu:24.04` for every task — a real, public, pinned
tag, not a name that has to be built before the suite can run. Each task's
`setup` then does three things, in order:

1. installs the tool it needs **at an exact apt version**, guarded by
   `command -v` so it is a no-op on an image that already carries it;
2. asserts that exact version with `dpkg-query`, so a drifted image fails
   loudly at setup instead of quietly changing what the task measures;
3. builds the fixture.

No binary fixtures are committed. The media files, SQLite databases, git
repositories and X.509 material are all produced by the pinned tools during
setup, which also means the fixture is version-pinned along with the tool.

Running with `network: none` therefore requires the packages to be baked in.
The reference image for this tier is `ubuntu:24.04` plus exactly the apt
lines in the `setup` blocks and `python3` for the checkers; with them baked
in, step 1 is a no-op and step 2 is the gate. **A plain `ubuntu:24.04` with
`network: none` will fail setup on step 1, loudly.** That is the intended
failure: it is better than silently running a task against whatever version
the archive happens to serve.

Git fixtures set `GIT_CONFIG_GLOBAL=/dev/null`, `commit.gpgsign=false` and
explicit author/committer identities and dates, so commit identities are
reproducible. Two checkers rely on that (see `git-rescue-dangling-commit`).

## 4. The twelve tasks

Each entry gives the gap, why the strata are what they are, the assertion
that catches the plausible wrong procedure, and the headroom argument —
why the baseline should not simply score this.

### jq-deep-merge-layers · jq · poor · silent
**Gap.** `+` and `add` over JSON objects overwrite a key's value wholesale;
`*` recurses into it. Folding several files needs `reduce inputs` under
`-n`, or `--slurp` and an explicit chain.
**Why poor.** `jq --help` documents neither operator and points at a manual
page the image does not have.
**Why silent.** `jq -s add` exits 0 and prints well-formed configuration.
**Catches.** `nested-siblings-survived` — seven keys that live beside an
overridden key and vanish under a one-level merge. A checker that only
asked "is the overridden port right?" would pass the wrong procedure.
**Headroom.** The model very likely knows `*` exists. What it has to do
here is choose it over `add` *and* fold three files in order; and because
nothing complains, there is no signal to correct a wrong first choice.
Expect a middling probe score and a baseline well under it — a
knowing–doing gap rather than a knowledge gap.
**Escape hatch.** A hand-written deep merge in `python3` satisfies the
checker. See §6.

### jq-recover-truncated-array · jq · poor · informative
**Gap.** jq's ordinary parser reads the whole value before emitting
anything, so a truncated array yields nothing at all; the streaming parser
plus `fromstream(1|truncate_stream(inputs))` keeps everything parsed before
the damage.
**Why poor.** `--stream` gets one line in `--help`; `fromstream` and
`truncate_stream` appear only in the absent manual page.
**Why informative.** `jq: parse error: Unfinished string at EOF at line 61`
names the file, the line and the defect, so a persistent agent can converge
— including by the legitimate alternative of repairing a *copy*.
**Catches.** `damaged-file-left-damaged` (repairing `events.json` in place
makes it parse, which it must not) and `nothing-invented` (the severed
record's own sequence number must appear nowhere).
**Headroom.** The composition is the hard part; `--stream` alone does not
produce whole records. The expected plaintext is never hard-coded: the
checker re-decodes the array one element at a time and stops where the
bytes run out.

### ffmpeg-stack-dual-camera · ffmpeg · good · silent
**Gap.** Labelling a filter-graph output turns off ffmpeg's automatic stream
selection: the label must be mapped, and from then on *no* stream is chosen
for you, so the wanted audio must be mapped out of the second input
explicitly, and the output told to end with the stream that ends first.
**Why good.** `ffmpeg -h full` documents `-filter_complex`, the `[linklabel]`
form of `-map`, and `-shortest`; `ffprobe` enumerates the streams.
**Why silent.** The unconnected-output refusal names its own fix, but it is
a gate, not the discriminator: once the graph is wired, the wrong audio
track and the extra second of video both produce a file that plays.
**Catches.** `one-track-and-it-is-cam-right` (dominant tone measured by
zero-crossing rate on decoded PCM — 1200 Hz against 300 Hz) and
`ends-with-the-shorter-input`. Media is judged on decoded content and
ffprobe-reported properties; file size is never used.
**Headroom.** `hstack` and `-shortest` are common knowledge. The part that
is not is that a labelled output suppresses audio auto-selection, so an
agent that writes the obvious command gets a silent, playable, wrong file.

### sqlite-attach-merge-ledgers · sqlite3 · poor · informative
**Gap.** One connection sees one file unless a second is attached under a
name *in the same session*; then the primary-key collisions have to be
resolved in the direction the task wants, and the resolution that first
comes to mind replaces the row already present.
**Why poor.** `.help` covers dot commands; nothing on the image documents
SQL.
**Why informative.** `no such table: inc.entries` and `UNIQUE constraint
failed: entries.id` both name exactly what is wrong.
**Catches.** `existing-rows-survived` — `INSERT OR REPLACE` gives the right
row count and the right ids and silently overwrites the ledger's own
figures with the incoming restatement.
**Headroom.** `ATTACH` is well known; which conflict clause keeps the
*existing* row is the coin-flip, and it is unreported.

### git-graft-legacy-history · git · good · silent
**Gap.** `git replace --graft` makes a commit appear to have parents it does
not record, joining an import onto its prior history without touching a
single object.
**Why good.** `git replace -h` prints `--graft <commit> [<parent>...]`.
**Why silent.** `git rebase --onto legacy --root main` produces a log
identical to the one asked for and replaces every commit identity from the
join upwards, with nothing to indicate it.
**Catches.** `no-commit-was-rewritten` — read with `--no-replace-objects`,
main must still record three commits over one parentless root and legacy
two. No commit hash is hard-coded; the check is structural.
**Headroom.** This is the clearest "model can state it, model does not reach
for it" candidate in the tier. `.git/info/grafts` is accepted as an
equivalent end state.

### git-bisect-first-bad · git · good · informative
**Gap.** `git bisect start` wants the *broken* end first; the verdict script
has a third exit status meaning "cannot judge", and a search that reads it
as a failure lands on the unjudgeable commit; and the search leaves the
repository on a detached revision until it is told to let go.
**Why good.** `git bisect -h` lists `start`, `run` and `reset`, and the
verdict script in the repository documents its own three exit statuses.
**Why informative.** `You need to start by "git bisect start"`, `Bad rev
input`, and a `git status` that says which bisect is in progress.
**Catches.** `answer-is-the-first-bad-commit` — the history is laid out so
the unjudgeable commit is the first revision the search probes, so wrapping
the verdict script to collapse its third status converges on the wrong
commit while looking entirely reasonable. The checker determines the answer
independently, by extracting each revision into a scratch directory and
running the repository's own verdict script. `no-operation-in-progress`
catches the other common end state, a correct answer left mid-bisect.
**Headroom.** `git bisect run` is widely known, so the probe will score
high; the three-way exit contract and the argument order are where the
trials should separate. Flagged as the strongest saturation risk in the
tier (§6).

### git-rescue-dangling-commit · git · good · informative
**Gap.** The commit's branch was deleted *and* it was never checked out, so
it is in no reflog: the usual recovery route turns up nothing and the object
store has to be audited for objects nothing points at.
**Why good.** `git fsck -h` and `git reflog -h` are both on the image.
**Why informative.** Any wrong object name is refused precisely, and
`git fsck` names the dangling commit outright.
**Catches.** `rescue-is-the-original-commit` — reproducing the work as a
fresh commit gives the same subject, the same parent and the same file, and
a different identity. The checker compares the recorded committer and author
instants against the fixture's, which a commit made during the trial cannot
have, and requires the parent to be main's tip (read from the repository,
not hard-coded).
**Headroom.** `git reflog` is the reflex and it is useless here, which is
the point: the fixture is built with `git commit-tree` so no reflog entry
exists. Expect the probe to score high on "recover a lost commit" and the
trials to separate on *this* variant of it.

### rsync-hardlink-snapshot · rsync · poor · silent
**Gap.** `--link-dest` makes unchanged files second names for the stored
copy in a previous snapshot — and its path is resolved relative to the
*destination*, not the working directory.
**Why poor.** `--help` has one line for the flag and nothing about the
resolution rule, which is the part that bites.
**Why silent.** The procedure most reach for is a plain copy: correct in
every visible respect, silently a second copy of every byte. (The narrower
mistake of mis-resolving the path does draw a warning; the comment in
`task.yaml` says so.)
**Catches.** `unchanged-files-are-one-stored-file` compares inode numbers,
devices and link counts; `dropped-file-absent` catches hard-linking
yesterday's tree wholesale and copying over it, which leaves the retired
file behind. Apparent size is never used.
**Headroom.** The flag is guessable; the relative-path rule is not, and
getting it wrong produces a complete, correct-looking snapshot. The fixture
makes the changed file differ in both size and mtime so rsync's quick check
is decisive and the task does not accidentally turn on `--checksum`.

### rsync-mirror-protect-excluded · rsync · poor · silent
**Gap.** Deleting what the source no longer has and protecting what the
source never had are the *same* setting: an exclusion hides a name from both
ends, so an excluded directory that exists only on the receiver survives the
deletion pass. `--delete-excluded`, which sounds stricter, destroys exactly
that directory. Separately, the source's trailing slash decides between
mirroring contents and nesting the directory.
**Why poor.** `--help` documents `--delete` and `--exclude` and says nothing
about their interaction; that is in the manual page's FILTER RULES, absent
from the image.
**Why silent.** Every variant exits 0 and prints nothing.
**Catches.** `server-logs-survived` (byte-for-byte), plus
`scratch-not-published`, `retired-page-removed` and
`no-nested-copy-of-the-source` — four independent silent wrong results.
**Headroom.** Trailing-slash semantics are well known and not the gap. The
gap is the protection side-effect, which is a fact about rsync's filter
model rather than a flag to remember.

### openssl-sign-keeping-sans · openssl · good · silent
**Gap.** A certificate request's extensions are *requests*: signing drops
them unless the signing step is told to take them over, or given them again
from a configuration section.
**Why good.** `openssl x509 -help` prints `-copy_extensions`, `-extfile` and
`-extensions`.
**Why silent.** The wrong command prints the usual confirmation, and the
certificate it produces has the right subject, the right public key, a valid
CA signature and no extensions at all. `openssl verify` says `OK`.
**Catches.** `subject-alternative-names-carried` and
`key-usage-and-purpose-carried`; `public-key-is-the-requested-one` and
`issued-not-self-signed` catch the other shortcut, noticing the names are
missing and self-signing instead.
**Headroom.** The single most-shipped X.509 mistake there is. The model may
well know `-copy_extensions` when asked; whether it reaches for it unprompted,
with `openssl verify` telling it everything is fine, is the measurement.

### openssl-issuing-ca-chain · openssl · good · informative
**Gap.** Signing does not confer certificate-authority status: basic
constraints, with the depth allowed beneath, have to be handed to the
signing step. Then the host certificate must be signed by the new
intermediate rather than by the root everyone already trusts.
**Why good.** `openssl verify -help` prints `-untrusted` with its purpose,
and `openssl x509 -help` prints `-extfile`/`-extensions`.
**Why informative.** `error 79 at 1 depth lookup: invalid CA certificate`
names the defect and the depth; `unable to get local issuer certificate`
names the missing link.
**Catches.** `leaf-signed-by-the-issuing-ca` — a host certificate signed
straight from the root verifies perfectly *even with the intermediate
supplied*, because OpenSSL finds the shorter chain, so the chain assertion
alone would pass it. `leaf-is-not-a-ca` catches reusing the intermediate's
extension section for the host.
**Headroom.** Two-phase, two levels, and the one place the whole thing fails
loudly. Expect the smallest skill effect here, which is why the task is in
the suite: it is the `informative` end of the moderator.

### openssl-decrypt-kdf-params · openssl · good · uninformative
**Gap.** Nothing about the key derivation is recorded in the file, so every
choice has to be restated to open it — including the digest, whose default
is not what was used.
**Why good.** `openssl enc -help` lists `-pbkdf2`, `-iter` and `-md`, and
the fixture's handover note states the values in prose.
**Why uninformative.** Every wrong combination fails identically: `bad
decrypt` and a provider error naming a C source file and a line number. It
also writes a block of garbage out before it notices, so a file exists at
roughly the right size. (The oldest mistake of all — omitting the stretching
entirely — does draw a deprecation hint, and it names the wrong parameter to
fix. The comment in `task.yaml` says so.)
**Catches.** `note-recovered-exactly` — the checker opens the archive itself
into a scratch file and requires the agent's output to equal it byte for
byte, so neither the garbage file nor a tampered digest passes.
**Headroom.** The information is all on the box; the failure mode gives no
purchase at all, which is precisely the regime the feedback-bandwidth
hypothesis predicts a skill should help most.

## 5. Videos

One hermetic recipe per task under `videos/recipes/`, 1280×720 at 10 fps,
40–62 s, every one a terminal session.

**All output on screen was captured from a real run.** The recipes were
generated by a script that executes each command shown and pastes its actual
stdout and stderr, trimmed to the window from the tail — as a real terminal
scrolls. Invented output would teach something false and silently corrupt
the benchmark, so none of it is invented. Terminal lines are kept inside the
69-column grid the renderer draws at 720p; long commands are shown as shell
continuations, which is what a tutorial does anyway.

**Every video teaches its procedure on a different subject from its task.**
Other file names, other values, and where there is a choice, the other
variant of the same mechanism: the `ffmpeg` video stacks *vertically* and
keeps the *first* input's audio; the `openssl` chain video uses a depth of
one; the `rsync` snapshot video works on weekly mail snapshots. Nothing on
screen can be copied across as an answer — only the procedure transfers.
Each task's `notes` field records what was changed.

**Narration is vague exactly where the pixels are exact.** The cues say that
a step matters, or that two spellings of a path differ, or that a check has
three answers rather than two — and never which flag, which operator or
which value. "Hand git both ends of the range. The order they go in catches
people out, so read it off the screen rather than guessing." "There is one
more thing to restate, and it is the one whose default is not what was used.
It is on screen." The syntax exists only in the `type` lines. This is the
property `docs/preregistration.md` §7.2 turns on: if narration alone
sufficed, A2 and A6 would match A1 and the benchmark's most important
comparison would be zero by construction.

Several videos show the wrong procedure first and then the right one,
because that is how a real tutorial handles a silent failure — and because
the refusal or the missing extension is itself the evidence. The wrong
command is always on screen as a real command with its real output.

`placebo_source` pairs tasks within a tool family (the two jq tasks, the two
rsync tasks, the three git tasks, the three openssl tasks), so A5 gets a
genuine extraction of a genuine procedure that does not solve the task.
`ffmpeg-stack-dual-camera` and `sqlite-attach-merge-ledgers` are the only
task for their tool in this tier, so they have **no `placebo_source`**: a
same-family placebo does not exist for them yet, and pointing them at
another tool's video would make A5 a topical-irrelevance control rather
than the length-and-pipeline-matched one §3.3 specifies. Either add a second
task for those two tools or report them as unplaceboed.

## 6. Compromises, honestly

- **Escape hatches.** Tier 1's tools sign their own outputs, so a
  hand-written end state can be rejected. Real tools do not. For
  `jq-deep-merge-layers` and `jq-recover-truncated-array`, a scripted
  equivalent in `python3` satisfies the checker; the end state is what is
  graded. The gap is then "knowing the semantics" rather than "knowing jq",
  which is a weaker but still real measurement. For the other ten the tool
  is effectively unavoidable: X.509 signatures, AES ciphertext, git object
  identities, inode sharing and decoded media content cannot be produced by
  hand. `key_actions` records which route was taken either way.
- **Saturation risk, in order.** `git-bisect-first-bad` is the most likely
  to be saturated — `git bisect run` is famous. Its separation rests on the
  argument order, the third exit status and the reset, and if the probe
  comes back at 0.9+ with its `informative` bandwidth it is the one task in
  this tier the pre-declared exclusion rule (§5.3) will remove.
  `git-rescue-dangling-commit` is second: `git reflog` scores high on a
  probe and is useless on this fixture, so I expect a large
  knowing–doing gap rather than saturation, but it could go either way.
  `rsync-mirror-protect-excluded` is third if the model treats it as a
  trailing-slash question, which it partly is.
- **Strict probe patterns.** The `probe_answer_pattern` regexes were tested
  against 17 correct and 36 plausible-wrong answers; they accept all of the
  first and reject all of the second. They are deliberately strict (the
  `git-graft` pattern rejects `.git/info/grafts`, which the *task* accepts),
  so `probe_known` is a **lower bound** on what the model knows, and
  headroom is correspondingly an upper bound. This biases the suite towards
  claiming headroom and is declared here rather than discovered later.
- **One mixed stratum each.** `ffmpeg-stack-dual-camera` and
  `rsync-hardlink-snapshot` are declared `silent` although one of their
  failure routes does produce a message; `openssl-decrypt-kdf-params` is
  declared `uninformative` although the most primitive mistake draws a
  hint. Each `task.yaml` comment states which route the label is about.
- **`poor` × `uninformative` is empty** — §2.
- **Fixture determinism.** RSA key generation is random, so the openssl
  fixtures differ between trials; no checker depends on a key or a
  certificate fingerprint. libx264 output is deterministic for a pinned
  ffmpeg, and no checker depends on its bytes either — only on decoded
  content and ffprobe properties.
- **`openssl-decrypt-kdf-params` holds a fixture constant** (the sealed
  note's own text, via the passphrase check) and `sqlite-attach-merge-ledgers`
  holds the fixture's rows. Both are listed in `leakage_forbidden`.
- **Not validated in the reference image.** Everything below was validated
  on Ubuntu 24.04 at exactly the pinned package versions, outside a
  container, through a local stand-in for the harness trial loop. The
  Docker path and `v2sbench validate --oracle --trivial` under the Docker
  backend have not been exercised.

## 7. How this was validated

For every one of the twelve, in a fresh temporary directory: `files/` was
installed, `environment.setup` was run command by command exactly as
`trial.py` runs it, then either `oracle/solve.sh` or a plausible wrong
procedure was run with the work directory as cwd, then `check/check.py` was
run with `argv[1]` the work directory and `argv[2]` a command log.

- 12/12 oracles pass every required assertion.
- 12/12 checkers reject a do-nothing agent (the trivial-agent gate, §6.2).
- 19 wrong procedures, at least one per task, all rejected:

| task | wrong procedure |
|---|---|
| jq-deep-merge-layers | `jq -s 'add' base.json site.json local.json` |
| jq-recover-truncated-array | trim the file back and close the array, then `jq -c '.[]'` |
| jq-recover-truncated-array | stream correctly, then complete the severed record by hand |
| ffmpeg-stack-dual-camera | unlabelled graph output, audio left to ffmpeg, no `-shortest` |
| sqlite-attach-merge-ledgers | `ATTACH` + `INSERT OR REPLACE` |
| git-graft-legacy-history | `git rebase --onto legacy --root main` |
| git-bisect-first-bad | `git bisect run sh -c './check-ratio.sh; test $? -eq 0'` |
| git-bisect-first-bad | hand-driven search, repository left mid-bisect |
| git-rescue-dangling-commit | read the file out of the object store and commit it again |
| rsync-hardlink-snapshot | `rsync -a --delete source/ snapshots/2026-11-02/` |
| rsync-hardlink-snapshot | `--link-dest=snapshots/2026-11-01` (relative to cwd) |
| rsync-mirror-protect-excluded | `rsync -a --delete site/ public/` |
| rsync-mirror-protect-excluded | `--delete --delete-excluded --exclude=/tmp/ --exclude=/logs/` |
| rsync-mirror-protect-excluded | correct flags, source without its trailing slash |
| openssl-sign-keeping-sans | `openssl x509 -req ... -out api.crt` with no extension handling |
| openssl-sign-keeping-sans | `openssl req -new -x509` self-signed, names intact, issuer wrong |
| openssl-issuing-ca-chain | intermediate signed with no basic constraints |
| openssl-issuing-ca-chain | host signed straight from the root |
| openssl-decrypt-kdf-params | every parameter restated except the digest |

The wrong-procedure scripts are not committed: they must not be reachable
from a task directory, since `files/` is copied into the agent's working
directory. The table above is the reproduction recipe.
