"""Docker backend: the only one valid for a published figure.

Trials run in a container started with no network, a fresh writable
workdir, and a resolved image digest recorded on the record. Each trial
gets its own container, so state cannot leak from one arm into the next --
the most dangerous kind of leak in a paired design, because it would look
like a treatment effect.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import tarfile
import tempfile
import time
import uuid
from pathlib import Path

from .base import ExecResult, SandboxError


class DockerSandbox:
    backend = "docker"

    def __init__(
        self,
        image: str,
        *,
        workdir: str = "/work",
        network: str = "none",
        display: bool = False,
        cpus: str | None = None,
        memory: str | None = None,
        docker_bin: str = "docker",
    ) -> None:
        self.image = image
        self.workdir = workdir
        self.network = network
        self.display = display
        self.cpus = cpus
        self.memory = memory
        self.docker = docker_bin
        self.name = f"v2sbench-{uuid.uuid4().hex[:12]}"
        self.support_dir = "/opt/v2sbench-support"
        self.bin_dir = "/usr/local/bin"
        self._started = False
        self._digest: str | None = None

    # -- lifecycle ---------------------------------------------------------

    def start(self) -> None:
        self._require_daemon()
        argv = [
            self.docker, "run", "--rm", "--detach",
            "--name", self.name,
            "--network", self.network,
            "--workdir", self.workdir,
            # The agent is untrusted with respect to the host: a benchmark
            # that runs model-authored commands is running arbitrary code.
            "--cap-drop", "ALL",
            "--security-opt", "no-new-privileges",
            "--pids-limit", "512",
        ]
        if self.cpus:
            argv += ["--cpus", self.cpus]
        if self.memory:
            argv += ["--memory", self.memory]
        if self.display:
            # Shared memory for X; the image is responsible for starting Xvfb.
            argv += ["--shm-size", "512m", "--env", "DISPLAY=:99"]
        argv += [self.image, "sleep", "infinity"]

        proc = subprocess.run(argv, capture_output=True, text=True)
        if proc.returncode != 0:
            raise SandboxError(f"docker run failed: {proc.stderr.strip()}")
        self._started = True
        self.exec(["mkdir", "-p", self.workdir, self.support_dir])

    def stop(self) -> None:
        if not self._started:
            return
        subprocess.run(
            [self.docker, "rm", "--force", self.name],
            capture_output=True, text=True, check=False,
        )
        self._started = False

    def __enter__(self) -> DockerSandbox:
        self.start()
        return self

    def __exit__(self, *exc: object) -> None:
        self.stop()

    # -- operations --------------------------------------------------------

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
        cmd = [self.docker, "exec", "--workdir", cwd or self.workdir]
        for key, value in (env or {}).items():
            cmd += ["--env", f"{key}={value}"]
        cmd += [self.name, *argv]
        started = time.monotonic()
        try:
            proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        except subprocess.TimeoutExpired as exc:
            return ExecResult(
                argv=argv,
                exit_code=124,
                stdout=_text(exc.stdout),
                stderr=_text(exc.stderr),
                duration_secs=time.monotonic() - started,
                timed_out=True,
            )
        return ExecResult(
            argv=argv,
            exit_code=proc.returncode,
            stdout=proc.stdout,
            stderr=proc.stderr,
            duration_secs=time.monotonic() - started,
        )

    def put(self, src: Path, dest: str) -> None:
        if not self._started:
            raise SandboxError("sandbox not started")
        proc = subprocess.run(
            [self.docker, "cp", str(src), f"{self.name}:{dest}"],
            capture_output=True, text=True,
        )
        if proc.returncode != 0:
            raise SandboxError(f"docker cp into container failed: {proc.stderr.strip()}")

    def get(self, src: str, dest: Path) -> None:
        if not self._started:
            raise SandboxError("sandbox not started")
        dest.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "out.tar"
            proc = subprocess.run(
                [self.docker, "cp", f"{self.name}:{src}", str(archive)],
                capture_output=True, text=True,
            )
            if proc.returncode != 0:
                raise SandboxError(f"docker cp out of container failed: {proc.stderr.strip()}")
            if tarfile.is_tarfile(archive):
                with tarfile.open(archive) as tar:
                    tar.extractall(dest, filter="data")
            else:
                shutil.copy2(archive, dest)

    def image_digest(self) -> str | None:
        """The pinned identity of the environment, recorded per trial.

        A result attributed to ``ubuntu:24.04`` is not reproducible; one
        attributed to a digest is.
        """
        if self._digest is not None:
            return self._digest
        proc = subprocess.run(
            [self.docker, "image", "inspect", self.image, "--format", "{{json .RepoDigests}}"],
            capture_output=True, text=True,
        )
        if proc.returncode != 0:
            return None
        try:
            digests = json.loads(proc.stdout.strip() or "[]")
        except json.JSONDecodeError:
            return None
        self._digest = digests[0] if digests else None
        return self._digest

    # -- helpers -----------------------------------------------------------

    def _require_daemon(self) -> None:
        if shutil.which(self.docker) is None:
            raise SandboxError(
                "docker is not on PATH. Use --sandbox local --allow-unsandboxed for "
                "harness development, but note that local runs are not valid for a "
                "published figure."
            )
        proc = subprocess.run([self.docker, "info"], capture_output=True, text=True)
        if proc.returncode != 0:
            raise SandboxError(
                "the docker CLI is present but no daemon is reachable "
                f"({proc.stderr.strip().splitlines()[-1] if proc.stderr.strip() else 'no detail'})"
            )


def _text(raw: object) -> str:
    if raw is None:
        return ""
    if isinstance(raw, bytes):
        return raw.decode("utf-8", "replace")
    return str(raw)
