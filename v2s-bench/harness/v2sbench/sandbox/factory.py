"""Backend selection.

The sandbox is a property of the *run*, never of the arm: if two arms ran
in different environments the comparison between them is void, so the
factory is called once per run and the chosen backend is recorded on every
trial.
"""

from __future__ import annotations

import shutil
import subprocess

from .base import SandboxError

BACKENDS = ("docker", "local", "bubblewrap")


def available_backends() -> dict[str, str]:
    """Report which backends this machine can actually run, and why not."""
    out: dict[str, str] = {}
    if shutil.which("docker") is None:
        out["docker"] = "unavailable: docker not on PATH"
    else:
        probe = subprocess.run(["docker", "info"], capture_output=True, text=True)
        out["docker"] = (
            "available" if probe.returncode == 0
            else "unavailable: CLI present, no daemon"
        )
    out["local"] = "available (requires --allow-unsandboxed; not valid for a published figure)"
    out["bubblewrap"] = (
        "available but not implemented" if shutil.which("bwrap")
        else "unavailable: bwrap not on PATH (and not implemented)"
    )
    return out


def create(
    backend: str,
    *,
    image: str | None = None,
    workdir: str = "/work",
    network: str = "none",
    display: bool = False,
    allow_unsandboxed: bool = False,
    keep: bool = False,
):
    """Build a sandbox, refusing the unsafe-by-default choices."""
    if backend == "docker":
        if not image:
            raise SandboxError("the docker backend needs an image")
        from .docker import DockerSandbox

        return DockerSandbox(image, workdir=workdir, network=network, display=display)

    if backend == "local":
        if not allow_unsandboxed:
            raise SandboxError(
                "the local backend runs model-authored commands directly on this machine "
                "with no isolation and no network restriction. Pass --allow-unsandboxed to "
                "accept that. Trials recorded this way are marked local and are not valid "
                "for a published figure."
            )
        from .local import LocalSandbox

        return LocalSandbox(keep=keep)

    if backend == "bubblewrap":
        raise SandboxError(
            "the bubblewrap backend is not implemented. The parent project uses bubblewrap "
            "for its own verification sandbox; wiring it here is future work. Use docker."
        )

    raise SandboxError(f"unknown backend {backend!r}; known: {BACKENDS}")
