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

## Layout

- `crates/extractor` — the `vts-extract` binary (thin `main.rs` over `lib.rs`).
