# `env/` — the tier-4 agent↔game interfaces

One directory per game. Each holds the interface the agent talks to, the
document that freezes it, the calibration evidence behind the task's
threshold, and (where one is needed) a Dockerfile for the runtime.

```
env/freeze.py        the content-hash gate:  python3 env/freeze.py --check
env/freeze.json      the pinned sha256s, and the task mirrors
env/2048/            MIT, ours. Stdlib only; no image needed
env/crafter/         Crafter 1.8.3, MIT with assets. Hash-pinned runtime
env/nethack/         STUB. Verified runtime and seeding, no oracle, no task
```

Why these files are hashed at all: ARC-AGI-3 moved one model from 62.7% to
99.9% on the same tasks by changing only the harness. In a game tier the
interface *is* the measurement, so it is pinned exactly like the model id
and the container digest (`docs/preregistration.md` §9.2). A change to an
interface is a new protocol version and a new row in every results table.

`freeze.json` also pins **mirrors**: a task ships its interface through
`tasks/t4-game/<id>/files/`, so each interface exists twice on purpose, and
`--check` compares the copies byte for byte while `--update` re-mirrors
them. The duplication is deliberate — a task has to be able to install its
own interface in any backend — and the gate is what keeps it honest.

Read `tasks/t4-game/README.md` for the design and the measured bands, and
`docs/game-tier.md` for how to run it and what is unverified.
