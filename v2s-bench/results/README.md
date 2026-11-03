# results/

What a published run leaves behind, and how to check it.

```
results/
  runs/<run_id>.jsonl   one immutable record per trial, append-only
  probes.json           per-task cold knowledge probe scores, with the raw answers
  report.json           the bundle the website renders  (generated)
```

`runs/`, `probes.json` and `report.json` are git-ignored by default, so a
local run does not dirty the tree. A **published** run is committed
deliberately, by the workflow that produced it, with `git add -f`.

## The record

One JSON object per trial — one `(task, arm, seed)` — against
[`schemas/run-record.schema.json`](../schemas/run-record.schema.json).
Records are **append-only and never edited**. Re-running a frozen suite
against a new model is a new file, never a correction to an old one.

Each carries the provenance needed to attribute it: exact model id and
sampling parameters, harness version *and content digest*, task-suite
digest, container image digest, the agent version as captured inside the
container, the material's `sha256` and leakage verdict, and the video's
tier and hash. A figure without that block is not published —
harness changes alone have been measured to swing the same model by tens
of points on the same tasks, so "which harness" is not a footnote.

Three conventions worth knowing before reading the numbers:

- **`status: error` is not failure.** A trial whose container or harness
  broke is excluded from the endpoint and its rate is published per arm.
  Counting infrastructure failures as task failures penalises whichever
  arm happened to run on a flakier day.
- **`status: refused`** means the material failed the leakage gate. The
  trial was not scored, on purpose.
- **`is_mock: true`** marks records from the stub agent, the oracle gate or
  the trivial-agent gate. The report builder refuses to aggregate these
  with real records, so they cannot become a published figure by accident.

## Auditing a published figure

```bash
cd ../harness && python3 -m v2sbench.cli verify-results
```

It recomputes the whole bundle from the raw records and prints every
disagreement; silence means the arithmetic checks out. This is the point of
keeping the statistics to the standard library — an audit that first
requires installing a scientific stack is one nobody performs.

To recompute from scratch instead:

```bash
cd ../harness && python3 -m v2sbench.cli report --out /tmp/recomputed.json
```

## Probes

`probes.json` holds each task's `probe_known` — how often the model
answered that task's key fact correctly, cold, with no tools — plus every
raw answer. The answers are kept deliberately: a reader who disagrees with
a `probe_answer_pattern` can see what the model actually said and re-score
it themselves.

Probe scores are per model. A probe taken against one model says nothing
about another.
