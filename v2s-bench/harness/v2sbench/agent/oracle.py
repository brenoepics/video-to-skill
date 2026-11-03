"""Runs a task's own oracle solution instead of an agent.

Used only by the oracle gate. It is not an arm and its records are marked
mock: the oracle knows the answer, so its success rate is a statement about
the task, not about any model.
"""

from __future__ import annotations

import time
from pathlib import Path

from .base import AgentError, AgentRun

class OracleAgent:
    name = "oracle"
    is_mock = True

    def __init__(self, task) -> None:
        self.task = task

    def version(self) -> str | None:
        return "oracle"

    def run(self, *, prompt, sandbox, workdir, max_turns=40, wall_secs=600, **_) -> AgentRun:
        directory = self.task.directory / "oracle"
        if not directory.is_dir():
            raise AgentError(
                f"task {self.task.id} declares an oracle but {directory} is missing"
            )
        started = time.monotonic()
        mount = f"{sandbox.support_dir}/oracle"
        sandbox.exec(["mkdir", "-p", mount])
        sandbox.put(directory, mount)
        entry = self._entrypoint(sandbox, directory, mount)
        result = sandbox.exec(
            ["sh", "-lc", f"chmod +x {entry} 2>/dev/null; {entry}"],
            timeout=wall_secs, cwd=workdir,
        )
        run = AgentRun(
            commands=[f"<oracle> {Path(entry).name}"],
            turns=1, tokens_in=0, tokens_out=0, usd_cost=0.0,
            wall_secs=time.monotonic() - started,
            exit_code=result.exit_code, timed_out=result.timed_out,
            material_read=None, action_modality=self._modality(),
        )
        run.stdout_tail = result.stdout[-2000:]
        run.stderr_tail = result.stderr[-2000:]
        return run

    def _entrypoint(self, sandbox, directory: Path, mount: str) -> str:
        name = getattr(self.task, "oracle_entrypoint", None) or "solve.sh"
        for candidate in (f"{mount}/{name}", f"{mount}/{directory.name}/{name}"):
            probe = sandbox.exec(["sh", "-lc", f"test -f {candidate} && echo yes || true"])
            if "yes" in probe.stdout:
                return candidate
        raise AgentError(f"task {self.task.id}: oracle entrypoint {name!r} not found")

    def _modality(self) -> str:
        declared = getattr(self.task, "oracle_modality", None)
        return declared or "shell"
