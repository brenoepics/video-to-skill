"""The Claude Code CLI in print mode.

Several details here are load-bearing and were verified against the
installed binary rather than taken from memory, because getting any of
them wrong produces a confident, wrong number:

**Never pass ``--bare``.** It is the obvious choice for a hermetic run --
it skips hooks, plugin sync and CLAUDE.md discovery -- but its own help
text says *"Skills still resolve via /skill-name"*. Under ``--bare`` a
skill is present but cannot be discovered from its description, so the
ambient arm (A1) would be silently reduced to the baseline and the
benchmark would report a null result caused entirely by a harness flag.
Hermeticity comes from the container instead.

**Always pass ``--permission-mode`` explicitly.** A ``-p`` run that does
not set one starts in ``manual``, where anything needing approval is
denied. Leave it unset and you are measuring the permission system, not
the agent.

**Read token usage from ``modelUsage``, not ``usage``.** ``usage``
excludes subagent tokens, so it would systematically undercount exactly
the arms whose skills cause the agent to fork work -- biasing the cost
comparison in the treatment's favour.

**``--max-turns`` is accepted but absent from ``--help``** on the version
tested (2.1.317). It is passed defensively: if a future version rejects
it, the adapter says so rather than silently running an unbounded trial.
"""

from __future__ import annotations

import json
import re
import shlex
import time

from .base import AgentError, AgentRun

#: Flags that must never be used, with the reason, asserted at construction.
FORBIDDEN_FLAGS = {
    "--bare": "strips ambient skill discovery; see this module's docstring",
    "--fallback-model": "a silent model switch mid-run makes the record's model id a lie",
    "--resume": "a trial must start from a clean session",
    "--continue": "a trial must start from a clean session",
    "--dangerously-skip-permissions": "would let a trial escape the sandbox policy",
}

_BASH_CMD = re.compile(r'"command"\s*:\s*"((?:[^"\\]|\\.)*)"')


class ClaudeCodeAgent:
    name = "claude-code"

    def __init__(
        self,
        *,
        model: str,
        binary: str = "claude",
        permission_mode: str = "acceptEdits",
        allowed_tools: tuple[str, ...] = ("Bash", "Read", "Write", "Edit", "Glob", "Grep"),
        extra_args: tuple[str, ...] = (),
        in_sandbox: bool = True,
    ) -> None:
        if model.lower() in {"opus", "sonnet", "haiku", "fable"}:
            raise AgentError(
                f"refusing the model alias {model!r}: an alias resolves to a different "
                "model over time, which makes every figure recorded with it "
                "uninterpretable later. Pass a full model id."
            )
        for flag in extra_args:
            if flag in FORBIDDEN_FLAGS:
                raise AgentError(f"refusing {flag}: {FORBIDDEN_FLAGS[flag]}")
        self.model = model
        self.binary = binary
        self.permission_mode = permission_mode
        self.allowed_tools = allowed_tools
        self.extra_args = extra_args
        self.in_sandbox = in_sandbox
        self._version: str | None = None

    # -- introspection -----------------------------------------------------

    def version(self) -> str | None:
        return self._version

    def probe(self, sandbox) -> str:
        """Record the agent version from inside the environment it will run in.

        The version on the host is not necessarily the version in the
        container, and the one that ran the trial is the one worth recording.
        """
        res = sandbox.exec([self.binary, "--version"], timeout=60)
        if not res.ok:
            raise AgentError(
                f"cannot run {self.binary!r} in the environment: {res.stderr.strip() or res.stdout.strip()}"
            )
        self._version = res.stdout.strip()
        return self._version

    # -- the trial ---------------------------------------------------------

    def argv(self, prompt: str, *, max_turns: int) -> list[str]:
        """Build the invocation. Separate from ``run`` so tests can assert it."""
        argv = [
            self.binary,
            "-p", prompt,
            "--model", self.model,
            # Explicit, because the default in print mode denies anything
            # that would prompt.
            "--permission-mode", self.permission_mode,
            # Nobody is available to answer a prompt in a batch run; make
            # that the declared policy rather than an accident.
            "--permission-prompts", "none",
            "--allowedTools", *self.allowed_tools,
            # stream-json gives per-turn structure, tool calls and usage.
            "--output-format", "stream-json",
            "--verbose",
            "--max-turns", str(max_turns),
        ]
        argv += list(self.extra_args)
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
        full_prompt = _compose_prompt(prompt, material_text, forced_invocation)
        argv = self.argv(full_prompt, max_turns=max_turns)
        started = time.monotonic()
        res = sandbox.exec(
            argv,
            timeout=wall_secs,
            cwd=workdir,
            env={"CLAUDE_CODE_ENTRYPOINT": "v2sbench", **(env or {})},
        )
        wall = time.monotonic() - started
        run = parse_stream_json(res.stdout)
        run.wall_secs = wall
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


def _compose_prompt(
    prompt: str, material_text: str | None, forced_invocation: str | None
) -> str:
    """Assemble the prompt.

    ``material_text`` is used by the raw-transcript arm, which necessarily
    enters through the prompt rather than through skill discovery. That is a
    real channel difference between A1 and A6 and it is documented in the
    methodology: read H6 alongside H8, which bounds the discovery effect
    separately.
    """
    parts: list[str] = []
    if forced_invocation:
        parts.append(forced_invocation)
    parts.append(prompt)
    if material_text:
        parts.append(
            "\n\n--- Reference material (a transcript of a tutorial video about this "
            "task) ---\n" + material_text
        )
    return "\n\n".join(parts)


def parse_stream_json(stdout: str) -> AgentRun:
    """Pull the trial's metrics out of a stream-json transcript.

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
        if not line or not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        etype = event.get("type")
        if etype == "assistant":
            turns += 1
        for cmd in _tool_commands(event):
            run.commands.append(cmd)
        usage = _model_usage(event)
        if usage:
            saw_usage = True
            tokens_in += usage[0]
            tokens_out += usage[1]
        if etype == "result":
            run.usd_cost = _coerce_float(
                event.get("total_cost_usd", event.get("cost_usd"))
            )
            if event.get("num_turns") is not None:
                turns = max(turns, int(event["num_turns"]))
            run.raw["result"] = {
                k: event.get(k)
                for k in ("subtype", "is_error", "duration_ms", "num_turns", "session_id")
            }
    run.turns = turns or None
    if saw_usage:
        run.tokens_in, run.tokens_out = tokens_in, tokens_out
    run.action_modality = "shell" if run.commands else "unknown"
    return run


def _tool_commands(event: dict) -> list[str]:
    """Extract Bash commands from any tool_use block in the event."""
    out: list[str] = []
    message = event.get("message") or {}
    content = message.get("content") if isinstance(message, dict) else None
    if not isinstance(content, list):
        return out
    for block in content:
        if not isinstance(block, dict) or block.get("type") != "tool_use":
            continue
        payload = block.get("input") or {}
        if isinstance(payload, dict) and isinstance(payload.get("command"), str):
            out.append(payload["command"])
    return out


def _model_usage(event: dict) -> tuple[int, int] | None:
    """Read ``modelUsage``, never ``usage``.

    ``usage`` omits subagent tokens, which would undercount precisely the
    arms whose skill leads the agent to delegate.
    """
    for key in ("modelUsage", "model_usage"):
        block = event.get(key)
        if isinstance(block, dict) and block:
            tin = tout = 0
            for per_model in block.values():
                if not isinstance(per_model, dict):
                    continue
                tin += int(per_model.get("inputTokens", per_model.get("input_tokens", 0)) or 0)
                tout += int(
                    per_model.get("outputTokens", per_model.get("output_tokens", 0)) or 0
                )
            return tin, tout
    return None


def _material_read(stdout: str, material_dir: str) -> bool:
    needle = material_dir.rstrip("/").split("/")[-1]
    return needle in stdout or "SKILL.md" in stdout


def _coerce_float(value: object) -> float | None:
    try:
        return float(value)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return None


def quote(argv: list[str]) -> str:
    return " ".join(shlex.quote(a) for a in argv)
