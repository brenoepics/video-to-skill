<!--
The task prompt preamble, identical for every arm.

This file is committed so the baseline can be inspected and argued with.
A weak baseline is the standard way to manufacture a benchmark win: if the
reference arm is told nothing and given no encouragement to look around,
"the skill helped" can mean no more than "the other arm was not trying".
So this prompt is written to be the strongest fair baseline we can state
without naming any procedure -- it pushes toward reading documentation,
verifying effects, and not guessing, which is exactly the behaviour that
would make a skill redundant.

Arms differ ONLY in the context material supplied alongside this text. If
you change this file, you change every arm at once, which is the point:
no arm may receive prompt help another does not.
-->

You are working inside a throwaway container. Your job is to reach the end
state described under **Task** below, by actually doing it — not by
describing how it could be done.

Work methodically:

- **Look before acting.** Inspect what is on the system: run a tool's
  `--help`, read its manual page, list the working directory, read any
  README or configuration files you find. Tools here may be unfamiliar, and
  their documentation may be incomplete or out of date.
- **Verify rather than assume.** After a step that should change something,
  check that it changed in the way you intended. A command that exits
  successfully has not necessarily done what you wanted.
- **Prefer evidence to recall.** If you are unsure of a flag, a subcommand
  or an argument order, find out on the system instead of guessing from
  memory.
- **Do not fabricate completion.** If you cannot reach the end state, say
  what blocked you. A truthful partial result is more useful than a claim
  that something succeeded when it did not.
- **Stay inside the working directory.** Do not modify anything outside it,
  and do not attempt network access.

When you believe the end state has been reached, stop.
