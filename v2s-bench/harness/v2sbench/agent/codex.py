"""The OpenAI Codex CLI in non-interactive mode (`codex exec`).

Written so that an arm means the same thing here as it does under Claude
Code. Two details decide that, and both are easy to get wrong.

**`AGENTS.md` is always concatenated into the prompt.** Codex builds an
instruction chain on every run, from `$CODEX_HOME` down through the
project root to the working directory, and joins the files it finds. That
is *not* Claude Code's skill discovery, where a skill is advertised by its
description and the agent chooses whether to read it. So inlining a skill
into `AGENTS.md` produces the **forced** condition (A7), not the ambient
one (A1). Mapping "skill -> AGENTS.md" and calling it A1 would compare
ambient-Claude against forced-Codex, and the difference would read as a
model effect. The two arms are therefore built differently here:

* **A1 (ambient)** writes the skill to `skills/<name>/SKILL.md` in the
  workspace and puts only a one-line pointer -- its name and description --
  into `AGENTS.md`, mirroring how Claude Code advertises a skill. The agent
  still has to decide to open it.
* **A7 (forced)** inlines the whole skill, so it is unavoidably in context.

**Token usage splits cached input out.** `turn.completed` reports
`usage.input_tokens` alongside `usage.cached_input_tokens`, and summing
only the first understates input badly -- and unevenly across arms, since a
skill arm carries a longer prompt and caches more of it. The same mistake
cost four orders of magnitude on the Claude adapter before it was caught.
`reasoning_output_tokens` counts as output, because it is billed and
because a model that reasons more is not thereby cheaper.

Also note the default sandbox is **read-only**, so a trial that does not
set `--sandbox workspace-write` cannot change anything and fails for a
reason that has nothing to do with the skill.

Verified against the documented interface, **not against the binary** --
Codex is not installed in the environment this was written in. Run
`v2sbench preflight` and a single-task trial before a real run.
"""

from __future__ import annotations

import json
import shutil
import tempfile
import time
from pathlib import Path

from .base import AgentError, AgentRun

#: Flags that must never be used, with the reason.
FORBIDDEN_FLAGS = {
    "--sandbox danger-full-access": (
        "removes the isolation the trial depends on; the task container is "
        "already the boundary"
    ),
    "--full-auto": "no longer supported; use --sandbox workspace-write",
}

#: Where an ambient skill is written inside the workspace.
SKILL_PARENT = "skills"


class CodexAgent:
    name = "codex"

    def __init__(
        self,
        *,
        model: str,
        binary: str = "codex",
        sandbox: str = "workspace-write",
        extra_args: tuple[str, ...] = (),
    ) -> None:
        if not model:
            raise AgentError("the codex adapter needs an explicit --model")
        if sandbox == "danger-full-access":
            raise AgentError(
                "refusing --sandbox danger-full-access: "
                + FORBIDDEN_FLAGS["--sandbox danger-full-access"]
            )
        self.model = model
        self.binary = binary
        self.sandbox = sandbox
        self.extra_args = extra_args
        self._version: str | None = None

    def version(self) -> str | None:
        return self._version

    def probe(self, sandbox) -> str:
        """Record the version from inside the environment it will run in."""
        res = sandbox.exec([self.binary, "--version"], timeout=60)
        if not res.ok:
            raise AgentError(
                f"cannot run {self.binary!r} in the environment: "
                f"{(res.stderr or res.stdout).strip()[:300]}"
            )
        self._version = res.stdout.strip()
        return self._version

    def inject_skill(self, sandbox, workdir: str, skill_dir: Path, *, forced: bool) -> str:
        """Offer the skill the way this agent actually takes one.

        `AGENTS.md` is concatenated into the prompt on every run, so what
        goes in it is unavoidably read. That makes it the right home for
        the forced arm and the wrong home for the ambient one.

        Ambient: the skill lands in the workspace and `AGENTS.md` carries
        only its name and description, mirroring how Claude Code
        advertises a skill -- the agent still chooses whether to open it.
        Forced: the whole procedure is inlined, so it cannot be missed.
        """
        name = skill_dir.name
        target = f"{workdir}/{SKILL_PARENT}/{name}"
        sandbox.exec(["mkdir", "-p", f"{workdir}/{SKILL_PARENT}"])
        sandbox.put(skill_dir, target)

        skill_md = skill_dir / "SKILL.md"
        if forced:
            body = (
                "# Procedure for this task\n\n"
                + (skill_md.read_text() if skill_md.exists() else "")
            )
        else:
            description = _frontmatter_description(skill_md) or f"Procedure for {name}."
            body = (
                "# Available skills\n\n"
                f"- **{name}** - {description}\n"
                f"  The full procedure is at `{SKILL_PARENT}/{name}/SKILL.md`. "
                "Read it if it is relevant to the task.\n"
            )

        staged = Path(tempfile.mkdtemp(prefix="v2sbench-agents-")) / "AGENTS.md"
        staged.write_text(body)
        sandbox.put(staged, f"{workdir}/AGENTS.md")
        shutil.rmtree(staged.parent, ignore_errors=True)
        return target

    def argv(self, prompt: str) -> list[str]:
        """Build the invocation. Separate from ``run`` so tests can assert it."""
        argv = [
            self.binary, "exec",
            "--model", self.model,
            # Default is read-only: without this the agent cannot act at all.
            "--sandbox", self.sandbox,
            # JSONL on stdout, one event per line.
            "--json",
            # Do not inherit whatever config the host happens to have; the
            # only intended difference between arms is the material.
            "--ignore-user-config",
        ]
        argv += list(self.extra_args)
        argv.append(prompt)
        return argv

    def run(
        self,
        *,
        prompt: str,
        sandbox,
        workdir: str,
        material_dir: str | None = None,
        material_text: str | None = None,
        forced_invocation: str | None = None,
        max_turns: int = 40,
        wall_secs: int = 600,
        seed: int = 0,
        env: dict[str, str] | None = None,
    ) -> AgentRun:
        full_prompt = prompt
        if material_text:
            full_prompt += (
                "\n\n--- Reference material (a transcript of a tutorial video "
                "about this task) ---\n" + material_text
            )

        started = time.monotonic()
        res = sandbox.exec(
            self.argv(full_prompt),
            timeout=wall_secs,
            cwd=workdir,
            env={"CODEX_API_KEY": (env or {}).get("CODEX_API_KEY", ""), **(env or {})},
        )
        run = parse_jsonl(res.stdout)
        run.wall_secs = time.monotonic() - started
        run.timed_out = res.timed_out
        run.exit_code = res.exit_code
        run.stdout_tail = res.stdout[-4000:]
        run.stderr_tail = res.stderr[-4000:]
        if material_dir:
            run.material_read = _material_read(res.stdout, material_dir)
        if res.timed_out:
            return run
        if res.exit_code != 0 and not run.commands:
            raise AgentError(
                f"{self.binary} exited {res.exit_code} with no tool calls: "
                f"{(res.stderr or res.stdout)[-600:]}"
            )
        return run


def _frontmatter_description(skill_md: Path) -> str | None:
    """Pull `description:` out of the skill's YAML frontmatter."""
    if not skill_md.exists():
        return None
    inside = False
    for line in skill_md.read_text().splitlines():
        if line.strip() == "---":
            if inside:
                break
            inside = True
            continue
        if inside and line.startswith("description:"):
            return line.split(":", 1)[1].strip().strip("\"'")
    return None


#: Item types that represent the agent running something.
_COMMAND_ITEMS = {"command_execution", "local_shell_call", "shell_call"}


def parse_jsonl(stdout: str) -> AgentRun:
    """Pull a trial's metrics out of a `codex exec --json` stream.

    Tolerant by design: a malformed or truncated line loses that line, not
    the trial, because a trial discarded for a parse error is a trial
    silently removed from one arm.
    """
    run = AgentRun()
    turns = 0
    tokens_in = tokens_out = 0
    saw_usage = False

    for line in stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue

        etype = str(event.get("type", ""))
        if etype == "turn.started":
            turns += 1
        elif etype == "turn.completed":
            usage = event.get("usage") or {}
            if usage:
                saw_usage = True
                # Cached input is still input. Omitting it understates the
                # longer-prompt arms more than the short ones.
                tokens_in += int(usage.get("input_tokens", 0) or 0)
                tokens_in += int(usage.get("cached_input_tokens", 0) or 0)
                tokens_out += int(usage.get("output_tokens", 0) or 0)
                tokens_out += int(usage.get("reasoning_output_tokens", 0) or 0)
                run.raw.setdefault("usage", {}).update(usage)
        elif etype == "turn.failed" or etype == "error":
            run.raw.setdefault("errors", []).append(
                str(event.get("error") or event.get("message") or "")[:300]
            )
        elif etype.startswith("item."):
            command = _command_of(event)
            if command:
                run.commands.append(command)

    run.turns = turns or None
    if saw_usage:
        run.tokens_in, run.tokens_out = tokens_in, tokens_out
    run.action_modality = "shell" if run.commands else "unknown"
    # Codex reports no cost; it is derived downstream from tokens and a
    # dated price, rather than invented here.
    run.usd_cost = None
    return run


def _command_of(event: dict) -> str | None:
    item = event.get("item") or event
    if not isinstance(item, dict):
        return None
    if str(item.get("item_type") or item.get("type") or "") not in _COMMAND_ITEMS:
        return None
    command = item.get("command")
    if isinstance(command, list):
        return " ".join(str(part) for part in command)
    return str(command) if command else None


def _material_read(stdout: str, material_dir: str) -> bool:
    needle = material_dir.rstrip("/").split("/")[-1]
    return needle in stdout or "SKILL.md" in stdout
