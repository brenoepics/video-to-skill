#!/usr/bin/env python3
"""Checker for openssl-decrypt-kdf-params.

The expected plaintext is not written here: the checker opens vault.enc
itself, into a scratch file, and requires vault.txt to equal that byte for
byte. That both decides the task and proves the archive was not altered.

`note-recovered-exactly` is the assertion that carries the task. A wrong
key-derivation parameter does not merely fail -- openssl writes out a
block of garbage before it notices, so vault.txt exists, is roughly the
right size, and is wrong. A checker that tested only for the file's
presence, or its length, would pass that.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HANDOVER_SEAL = ("-aes-256-cbc", "-pbkdf2", "-iter", "240000", "-md", "sha512")


def main() -> int:
    work = Path(sys.argv[1])
    log_text = ""
    if len(sys.argv) > 2:
        try:
            log_text = Path(sys.argv[2]).read_text(encoding="utf-8", errors="replace")
        except OSError:
            log_text = ""

    results: list[dict] = []

    def add(ident: str, passed: bool, detail: str, required: bool = True) -> None:
        results.append({"id": ident, "passed": bool(passed), "required": required, "detail": detail})

    scratch = Path(tempfile.mkdtemp(prefix="unseal-"))
    reference: bytes | None = None
    opened = False
    try:
        out = scratch / "reference"
        done = subprocess.run(
            ["openssl", "enc", "-d", *HANDOVER_SEAL,
             "-in", str(work / "vault.enc"), "-out", str(out),
             "-pass", "file:%s" % (work / "passphrase.txt")],
            capture_output=True, timeout=180,
        )
        if done.returncode == 0 and out.is_file():
            reference = out.read_bytes()
            opened = bool(reference)
    except (OSError, subprocess.TimeoutExpired):
        opened = False
    finally:
        for leftover in scratch.glob("*"):
            leftover.unlink(missing_ok=True)
        scratch.rmdir()

    add(
        "archive-still-opens-unchanged",
        opened,
        "vault.enc still opens with the parameters the handover note records"
        if opened
        else "vault.enc no longer opens as sealed: it has been altered or replaced",
    )

    try:
        produced = (work / "vault.txt").read_bytes()
    except OSError:
        produced = b""

    exact = bool(reference) and produced == reference
    add(
        "note-recovered-exactly",
        exact,
        "vault.txt is the sealed note, byte for byte (%d bytes)" % len(produced)
        if exact
        else "vault.txt is %d bytes and does not match the sealed note (first bytes %r)"
        % (len(produced), produced[:24]),
    )

    try:
        recorded = (work / "vault.sha256").read_text(encoding="utf-8").split()[0].strip().lower()
    except (OSError, IndexError):
        recorded = ""
    digest = hashlib.sha256(produced).hexdigest() if produced else ""
    add(
        "digest-matches-the-recovered-file",
        bool(recorded) and digest == recorded,
        "vault.txt hashes to the digest shipped with the archive"
        if digest == recorded and recorded
        else "recorded digest %r, recovered file hashes to %r" % (recorded[:16], digest[:16]),
    )

    handover = work / "HANDOVER.txt"
    inputs_ok = (
        handover.is_file()
        and "SHA-512" in handover.read_text(encoding="utf-8", errors="replace")
        and (work / "passphrase.txt").read_bytes() == b"courier-handover-2026-11\n"
        and bool(recorded)
        and not (work / ".note.plain").exists()
    )
    add(
        "inputs-untouched",
        inputs_ok,
        "the archive, the passphrase, the digest and the handover note are as supplied"
        if inputs_ok
        else "an input was modified or removed",
    )

    add(
        "opened-with-the-stated-digest",
        re.search(r"(?mi)^\s*openssl\s+enc\b[^\n]*-md\s+sha512", log_text) is not None,
        "the decryption in the command log restates the digest",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
