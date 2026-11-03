"""The sandbox contract."""

from __future__ import annotations

import shlex
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol, runtime_checkable


class SandboxError(RuntimeError):
    """The environment failed, as opposed to the agent failing.

    The distinction matters: a trial that raises this is recorded with
    ``status="error"`` and excluded from the endpoint, never counted as a
    task failure. Conflating the two biases every arm that happens to run
    on a flakier environment.
    """


@dataclass(frozen=True)
class ExecResult:
    argv: list[str]
    exit_code: int
    stdout: str
    stderr: str
    duration_secs: float
    timed_out: bool = False

    @property
    def ok(self) -> bool:
        return self.exit_code == 0 and not self.timed_out

    def command(self) -> str:
        return " ".join(shlex.quote(a) for a in self.argv)


@runtime_checkable
class Sandbox(Protocol):
    """A disposable place to run one trial.

    Implementations must be usable as a context manager and must leave
    nothing behind on the host after ``stop()``.
    """

    backend: str
    workdir: str
    support_dir: str
    """A directory the agent never sees, for the checker and the oracle.

    Kept off the protocol's workdir on purpose: if the checker were visible
    while the agent worked, the agent could read the assertions and satisfy
    them directly, and the benchmark would be measuring its own grader."""

    def start(self) -> None: ...

    def stop(self) -> None: ...

    def exec(
        self,
        argv: list[str],
        *,
        timeout: float | None = None,
        cwd: str | None = None,
        env: dict[str, str] | None = None,
    ) -> ExecResult: ...

    def put(self, src: Path, dest: str) -> None: ...

    def get(self, src: str, dest: Path) -> None: ...

    def image_digest(self) -> str | None: ...

    def __enter__(self) -> Sandbox: ...

    def __exit__(self, *exc: object) -> None: ...
