# Tier 3 — GUI tasks

**Status: specified, not built.** This document states what the tier will
be and why it is shaped that way, so the scope is on record rather than
implied by an empty directory.

## Why it matters most, and is hardest

v2s is aimed at screencasts. Most tutorial video is somebody driving a
graphical application, and the pipeline's distinguishing claim — commands
read off pixels rather than paraphrased from narration — is sharpest
exactly where there is no transcript to fall back on, because the
information is a menu path, a dialog field, a checkbox.

It is also the tier where measurement is hardest, for three reasons that
determine every choice below.

## No hypervisor, so no OSWorld

The two best-known desktop agent suites both boot a QEMU virtual machine
*inside* their container. Both fall back to software emulation when
`/dev/kvm` is absent rather than failing — so they appear to work and run
unusably slowly. Standard CI runners provide no dependable nested
virtualisation and roughly 14GB of disk, against 30GB images.

So tier 3 is **container-native**: real applications running under Xvfb in
a Docker image, plus browser tasks driven through Playwright. The
`display: true` flag in the task schema and the `--shm-size`/`DISPLAY`
handling already in `sandbox/docker.py` exist for this.

The cost of that decision is honest and worth stating: a container-native
desktop is a narrower sample of "GUI work" than a full desktop VM. A
hypervisor-backed tier is future work, not a thing we are pretending to
have.

## Shell bypass is the main threat to the tier's meaning

Roughly 15% of tasks in published GUI suites are silently solvable from a
terminal, and about a third are partly substitutable. That matters more
here than anywhere else in this benchmark: a generated skill that says
"just run this command" would produce a large lift on a task meant to
exercise the GUI, and the lift would be real but about something else
entirely.

Two mitigations, both already in the harness:

- `environment.gui_only: true` marks tasks that must be solved through the
  interface, and `metrics.action_modality` records how the agent actually
  acted (`shell`, `gui`, `mixed`) by inspecting the command log for
  `xdotool`, `wmctrl`, `scrot`, Playwright and friends.
- Modality is **reported per arm**, not just gated. If the treatment arm
  wins while shifting from `gui` to `shell`, that is visible in the table
  instead of buried in the headline.

The oracle's declared `modality` is the reference: a `gui_only` task whose
oracle is pure shell is not a GUI task, and review should reject it.

## Checking GUI outcomes

Execution-based assertions, as everywhere else — screenshots are evidence
for a human, not an oracle. Workable approaches, in order of preference:

1. **Inspect the document the application wrote.** An `.ods` or `.docx` is
   a zip of XML; assert on parsed values and formatting, never on file
   size or a rendered image.
2. **Query the application.** LibreOffice can be driven and interrogated
   headlessly; a browser app's state can be read through the DOM or its
   own storage.
3. **Accessibility tree.** The parent project has an open issue for
   accessibility-API postconditions (`#37`); the same mechanism serves
   here, and this tier should reuse it rather than invent a parallel one.

A judge over screenshots is permitted **only** for the secondary
`key_action_hit` metric where a regex cannot express the check, and only
under the validation requirements in the pre-registration (§8.5): at least
100 human-labelled comparisons with agreement, κ, the confusion matrix and
a human–human ceiling, published before the judge's output is shown.

## Candidate tasks

Eight, as declared in the pre-registration. Sketches, not commitments:

| Application | Candidate task | Knowledge gap | Feedback bandwidth |
|---|---|---|---|
| LibreOffice Calc | produce a chart with a specific axis and series configuration | the dialog path and which panel holds the setting | low — a wrong chart looks like a chart |
| LibreOffice Writer | configure a style so numbering restarts per section | the ordering of two dialogs | low |
| GIMP | batch-export at a given colour profile and bit depth | the export dialog's hidden advanced pane | low |
| Inkscape | path operation producing a specific node count | which boolean operation, and the order of selection | high — the result is visibly wrong |
| Browser app (Playwright) | complete a multi-step form with a non-obvious validation rule | the rule, which only the video states | high |
| Browser app | configure a setting reachable only through a non-obvious menu | the menu path | low |
| File manager / desktop | a preferences change with a surprising precondition | the precondition | low |
| Terminal emulator config | a profile setting that silently does nothing unless another is set first | the dependency between two settings | low |

Weighted toward `feedback_bandwidth: low`, because that is where the
pre-registration expects an effect and where tiers 1 and 2 are thinnest.

## What has to be true before this tier ships

1. A pinned image with the applications, Xvfb, a window manager and the
   input tools, under the CI disk budget, pinned by digest.
2. A GUI action vocabulary frozen and content-hashed, exactly as the game
   tier requires — harness changes have been measured to swing the same
   model by tens of points, and a GUI action layer is a harness.
3. An oracle per task that drives the real interface and passes.
4. The trivial-agent gate passing: a null agent must score zero.
5. Hermetic recipes whose `dialog` stills actually teach the menu path,
   with narration vague where the pixels are exact.
6. A pilot establishing that these tasks are flakier than tiers 1–2 by a
   measured amount, published with the error rate per arm.

Until all six hold, tier 3 reports as `insufficient-data` rather than
contributing to any figure.
