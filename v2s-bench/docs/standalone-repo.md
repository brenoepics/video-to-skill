# Splitting v2s-bench into its own repository

`v2s-bench/` is a self-contained subtree: nothing inside it imports from
`crates/`, and the only references outward are documentation links and the
`vts-extract` binary it shells out to, which it finds on `PATH`. So it can
become its own repository at any point, with its history intact.

It lives here for now for one practical reason: the session that built it
could not create a GitHub repository, and a subtree keeps the option open
without forcing the choice. There is also a real argument for keeping it
here — the benchmark and the tool it grades move together, and a
pre-registration that lives beside the code it constrains is harder to
quietly desynchronise.

## Split with history

From a clean checkout of the default branch:

```bash
git subtree split --prefix=v2s-bench -b v2s-bench-only
```

That writes a branch whose root is `v2s-bench/`, carrying only the commits
that touched it. Then, with an empty repository already created on GitHub:

```bash
git push git@github.com:brenoepics/v2s-bench.git v2s-bench-only:main
```

Verify before deleting anything here:

```bash
git log --oneline v2s-bench-only | head
```

```bash
git show --stat v2s-bench-only:README.md | head -5
```

## What needs changing afterwards

| Item | Change |
|---|---|
| `README.md`, `PAPER.md` | relative links to `../LICENSE` and to the parent project become absolute URLs |
| `.github/workflows/bench-ci.yml`, `bench-run.yml` | copy from the parent repo, drop the `v2s-bench/**` path filters and the `working-directory` prefixes |
| `LICENSE` | add a copy (it currently inherits the parent's) |
| `docs/preregistration.md` | the freeze commit refers to a hash in *this* repository; record both, and note the move as a dated amendment |
| Published records | `provenance.suite_commit` on existing records points at parent-repo hashes. Do not rewrite them — the whole point of an immutable record is that it is not edited. Note the migration in the amendment instead. |
| Site deployment | if it moves to `bench.v2s.dev`, update `site.config.mjs` and `public/CNAME` |

## Keeping it in sync afterwards

If the benchmark moves out but the tool still needs to consume it:

```bash
git remote add bench git@github.com:brenoepics/v2s-bench.git
```

```bash
git subtree pull --prefix=v2s-bench bench main --squash
```

Prefer a plain dependency over a re-added subtree. A benchmark vendored
back into the repository it grades invites exactly the coupling the split
was meant to remove.
