"""Local backend: a throwaway directory, no isolation.

For harness development only. It exists so the whole pipeline can be
exercised on a machine with no container runtime, which is exactly the
situation this harness was first written in.

It is deliberately awkward to use: ``create()`` refuses it without an
explicit opt-in, and every record it produces carries
``sandbox_backend="local"`` so a reader can see that the trial had no
network isolation and no guaranteed-clean filesystem. A figure built from
local trials is not a reproducible figure.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

from .base import ExecResult, SandboxError


class LocalSandbox:
    backend = "local"

    def __init__(self, *, workdir: str | None = None, keep: bool = False) -> None:
        self._tmp: str | None = None
        self._explicit = workdir
        self.workdir = workdir or ""
        self.support_dir = ""
        self.bin_dir = ""
        self.keep = keep
        self._started = False

    def start(self) -> None:
        if self._explicit:
            Path(self._explicit).mkdir(parents=True, exist_ok=True)
            self.workdir = self._explicit
        else:
            self._tmp = tempfile.mkdtemp(prefix="v2sbench-local-")
            self.workdir = str(Path(self._tmp) / "work")
        self.support_dir = str(Path(self.workdir).parent / "support")
        self.bin_dir = str(Path(self.workdir).parent / "bin")
        for directory in (self.workdir, self.support_dir, self.bin_dir):
            Path(directory).mkdir(parents=True, exist_ok=True)
        self._started = True

    def stop(self) -> None:
        if self._tmp and not self.keep:
            shutil.rmtree(self._tmp, ignore_errors=True)
        self._started = False

    def __enter__(self) -> LocalSandbox:
        self.start()
        return self

    def __exit__(self, *exc: object) -> None:
        self.stop()

    def exec(
        self,
        argv: list[str],
        *,
        timeout: float | None = None,
        cwd: str | None = None,
        env: dict[str, str] | None = None,
    ) -> ExecResult:
        if not self._started:
            raise SandboxError("sandbox not started")
        merged = dict(os.environ)
        merged.update(env or {})
        # Tools are installed into this sandbox's own bin rather than the
        # host's, so the host is never modified by a trial.
        merged["PATH"] = f"{self.bin_dir}:{merged.get('PATH', '')}"
        started = time.monotonic()
        try:
            proc = subprocess.run(
                argv,
                capture_output=True,
                text=True,
                timeout=timeout,
                cwd=cwd or self.workdir,
                env=merged,
            )
        except subprocess.TimeoutExpired as exc:
            return ExecResult(
                argv=argv,
                exit_code=124,
                stdout=_text(exc.stdout),
                stderr=_text(exc.stderr),
                duration_secs=time.monotonic() - started,
                timed_out=True,
            )
        except FileNotFoundError as exc:
            raise SandboxError(f"cannot execute {argv[0]!r}: {exc}") from exc
        return ExecResult(
            argv=argv,
            exit_code=proc.returncode,
            stdout=proc.stdout,
            stderr=proc.stderr,
            duration_secs=time.monotonic() - started,
        )

    def put(self, src: Path, dest: str) -> None:
        target = self._resolve(dest)
        target.parent.mkdir(parents=True, exist_ok=True)
        if src.is_dir():
            shutil.copytree(src, target, dirs_exist_ok=True)
        else:
            shutil.copy2(src, target)

    def get(self, src: str, dest: Path) -> None:
        source = self._resolve(src)
        dest.parent.mkdir(parents=True, exist_ok=True)
        if source.is_dir():
            shutil.copytree(source, dest, dirs_exist_ok=True)
        else:
            shutil.copy2(source, dest)

    def image_digest(self) -> str | None:
        return None

    def _resolve(self, path: str) -> Path:
        candidate = Path(path)
        return candidate if candidate.is_absolute() else Path(self.workdir) / candidate


def _text(raw: object) -> str:
    if raw is None:
        return ""
    if isinstance(raw, bytes):
        return raw.decode("utf-8", "replace")
    return str(raw)
