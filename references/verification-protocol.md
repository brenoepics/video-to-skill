# Verification protocol (verify-by-execution)

A generated skill earns its badge only when a fresh agent actually
followed it. Protocol:

## Isolation

- Spawn a **sub-agent whose entire context is the generated package** —
  it must never see the video, bundle, timeline, or your analysis. Its
  success measures the artifact, not leaked knowledge.
- It works in a **throwaway temp workspace**; all file operations stay
  inside it, and it is deleted afterwards.

## Safety policy (classify before executing)

Never execute steps that: modify the system outside the workspace
(installs, `defaults write`, config edits), touch paid services or
credentials, or are destructive. Mark them `skipped` with the reason.
Steps that need hardware/UI a sandbox lacks are `unverifiable`.
Network use by a step is allowed only if the step's purpose is network
access, and is noted in the outcome detail.

## Execution

- For each remaining step, perform the **actions** and check the
  **success criteria** as written. The criteria are the contract — a
  repair may fix actions, caveats, or evidence, but must never weaken a
  success criterion to make it pass.
- Record one outcome per step: `pass` | `fail` | `skipped` |
  `unverifiable`, each with a one-line detail.

## Repair loop (bounded: max 2 repairs)

On `fail`: diagnose the divergence, fix the Procedure IR (not the
rendered files), recompile with `vts-extract compile`, and re-verify
the failed steps. Every repair must cite the divergence that motivated
it in the step's caveats or actions. After 2 repairs, stop and report
honestly.

## Record

Write the report (schema v1) and stamp it:

```json
{
  "schema_version": 1,
  "verified": true,            // only if every EXECUTED step passed
  "attempts": 1,               // 1 + number of repairs
  "summary": "one line",
  "steps": [ { "id": "01-...", "outcome": "pass", "detail": "..." } ]
}
```

```
vts-extract verify --skill <package-dir> --report <report.json>
```

The tool validates step ids against the package and stamps a
✅ Verified / ⚠ NOT verified badge into the generated SKILL.md.
A skill whose executable steps are all `skipped`/`unverifiable` is
badged verified=false with a summary explaining it is
unverifiable-by-execution, not failed.
