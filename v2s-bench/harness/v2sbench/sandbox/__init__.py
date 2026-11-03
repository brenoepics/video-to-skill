"""Execution backends for trials.

Every arm of a comparison must run in an identical environment, so the
sandbox is chosen per run, never per arm. Backends:

``docker``
    The default and the only one valid for a published figure. Offline by
    default, reset between trials, pinned by image digest.
``local``
    A throwaway directory on the host with no isolation. For harness
    development only; it requires an explicit opt-in flag and is recorded
    on every trial so a reader can see it was used.
``bubblewrap``
    Reserved. The parent project already uses bubblewrap for its own
    verification sandbox; wiring it here is future work and the factory
    refuses it rather than pretending.
"""

from __future__ import annotations

from .base import ExecResult, Sandbox, SandboxError
from .docker import effective_policy
from .factory import available_backends, create

__all__ = [
    "ExecResult",
    "Sandbox",
    "SandboxError",
    "available_backends",
    "create",
    "effective_policy",
]
