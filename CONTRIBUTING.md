# Contributing

## The gate

Every change must pass all three, locally and in CI (same commands, no exceptions):

```
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Project policies (enforced, not aspirational)

- **TDD, always.** Every behavior change starts with a failing test (red), then the minimal implementation (green). Follow the `/tdd` skill: test at pre-agreed seams through public interfaces, one vertical slice at a time, no horizontal test-first batches. Refactoring happens at review time, not inside the loop.
- **300-line file limit.** No `.rs` file may exceed 300 lines — enforced by the `guardrails` test suite, which fails `cargo test` and names the offenders. Split modules by responsibility instead of growing files.
- **No panics in library code.** `unwrap()`/`expect()` are denied by clippy in non-test code (see `clippy.toml` for the test exemption). Propagate errors with `anyhow::Result` in binaries and typed errors at library seams.
- **Pedantic clippy is on** workspace-wide. Targeted `#[allow]`s are fine when justified with a comment on the line; blanket allows are not.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org), enforced by
the `commit-msg` hook and CI: `type(scope): subject` with type one of
feat/fix/docs/style/refactor/perf/test/build/ci/chore/revert, subject
≤ 50 chars, lowercase start, no trailing period. Enable the hook once:

```
git config core.hooksPath .githooks
```

## Testing the skill while developing

One command builds the extractor, points `~/.claude/skills/video-to-skill`
at this checkout, and opens Claude Code here with the dev data dir set:

```
scripts/dev-link.sh
```

Then, in the chat:

```
/video-to-skill <video>
```

Edit, re-run, repeat. The skill's own `vts-extract check --fix` bootstraps
ffmpeg, yt-dlp and the models into the dev data dir on first use, after
showing you the list. `scripts/dev-link.sh --unlink` hands the skill back
to the `npx skills` install.

### Why the dev data dir

SKILL.md step 1 looks for `<data-dir>/bin/vts-extract-<version>` *before*
`target/release/vts-extract`, and a release you once fetched carries the
same pinned version as your build, so with the default data dir the
cached release wins and your changes never run. The script sets
`VTS_DATA_DIR` to `~/.vts-dev` (`--data-dir DIR` moves it), where no
release was ever fetched, and links the build there as that first
candidate, so both candidates resolve to it. It also refuses to link
while SKILL.md pins a version other than the one `vts-extract --version`
reports, and names the pin to update.

### Without a chat

`--no-chat` builds and links, then prints the export line instead of
launching claude — for CI, scripts, and the test (`--no-build` reuses the
last build; `--force` moves a real `~/.claude/skills/video-to-skill`
directory aside). With that export in place, headless Claude Code drives
the same skill from a script. A `-p` run denies, instead of prompting
for, every shell command no allow rule covers, and the skill drives
`vts-extract` through Bash — so pre-approve the build at the path the
skill resolves it from, the dev data dir's `bin/` link:

```
export VTS_DATA_DIR=~/.vts-dev
ANTHROPIC_API_KEY=... claude -p "/video-to-skill <video>" --permission-mode acceptEdits \
  --allowedTools "Bash($VTS_DATA_DIR/bin/vts-extract-<version> *)" --output-format json
```

(`--output-format stream-json` lists any remaining denials under
`permission_denials`.)

Extractor-only changes need no Claude at all — the script leaves the
build at the path SKILL.md documents:

```
target/release/vts-extract extract <video> --out .vts/<slug>/bundle   # the bundle SKILL.md step 2 reads
target/release/vts-extract eval --baseline crates/extractor/eval/baseline.json   # quality gate, BENCHMARK.md
target/release/vts-extract bench                                      # speed suite, BENCHMARK.md
```

`cargo test` runs the script itself against a throwaway HOME with a
recording `claude` on PATH (`tests/dev_link.rs`); Windows skips it.

## Layout

- `crates/extractor` — the `vts-extract` binary (thin `main.rs` over `lib.rs`).
- `scripts/dev-link.sh` — the dev loop above; `.github/scripts/` holds CI helpers.
