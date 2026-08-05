# Procedure IR (schema v1)

The contract between your analysis and the compiler. Write it as
`procedure.json` in the bundle, then run:

```
vts-extract compile --bundle <bundle> --ir <bundle>/procedure.json --out <dir>
```

The compiler validates and refuses: non-kebab-case names/ids, empty or
oversized descriptions, steps without evidence, unknown confidence
values, frame references that don't exist in the bundle. Fix the IR and
re-run — compilation is pure and repeatable.

## Shape

```json
{
  "schema_version": 1,
  "skill_name": "vim-basics",            // kebab-case; becomes the install dir
  "title": "Vim Basics",
  "description": "Use when ...",          // frontmatter: what + when to trigger, ≤1024 chars
  "overview": "markdown ...",             // SKILL.md body intro
  "genre": "screencast",
  "source": { "original": "<url-or-path>", "title": "...", "duration_secs": 712.0 },
  "steps": [
    {
      "id": "01-quit-vim",               // kebab-case, ordered, unique
      "goal": "Quit Vim, discarding changes if needed",
      "actions": "Type `:q` ...",         // markdown; commands VERBATIM FROM PIXELS
      "success_criteria": "Back at the shell prompt.",
      "confidence": "high",               // high | medium | low
      "caveats": null,                    // optional string
      "evidence": [                       // ≥1 required per step
        { "timestamp_secs": 54.0, "frame": "frames/shot009.jpg", "quote": "quit, discard changes" }
      ],
      "scripts": [                        // optional runnable extractions
        { "name": "quit-vim.sh", "contents": "#!/bin/sh\n..." }
      ]
    }
  ],
  "artifacts": [                          // on-screen text worth indexing
    { "text": "vscodevim.vim", "timestamp_secs": 200.0, "frame": "frames/shot020.jpg" }
  ],
  "gaps": ["22 of 27 keyframes not inspected"]
}
```

## Authoring rules

- Steps mirror your analysis report: same evidence, same confidence.
  Anything you marked low-confidence there stays low-confidence here —
  the compiler renders the warning prominently.
- `frame` paths are bundle-relative; cite the exact frame that backs
  the claim (the compiler copies it into the package).
- Put a `scripts` entry on any step whose commands can run standalone;
  prefer one small script per step over one big one.
- A non-procedural video gets **no IR** — report that generate mode
  does not apply instead of inventing steps (the compiler rejects
  zero-step IRs for the same reason).
- `description` decides when the generated skill triggers: name the
  tool, the tasks, and typical user phrasings.
