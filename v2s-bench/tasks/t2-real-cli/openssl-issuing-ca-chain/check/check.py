#!/usr/bin/env python3
"""Checker for openssl-issuing-ca-chain.

`leaf-signed-by-the-issuing-ca` is the assertion that catches the plausible
wrong result: a host certificate signed straight from the root verifies
perfectly against the root, looks like a finished job, and leaves the
intermediate doing nothing. `issuing-cert-is-a-ca-for-one-level` catches the
other one, an intermediate that was signed but never told it was a CA, which
is where the chain is refused. `leaf-is-not-a-ca` catches handing the
intermediate's extension section to the host as well.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT_CN = "Harbour Works Root CA"
ISSUING_CN = "Harbour Works Issuing CA"
LEAF_CN = "db.internal"


def openssl(*args: str) -> tuple[int, str]:
    try:
        done = subprocess.run(["openssl", *args], capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return 127, str(exc)
    return done.returncode, (done.stdout or "") + (done.stderr or "")


def field(path: str, what: str) -> str:
    code, text = openssl("x509", "-in", path, "-noout", what)
    return text.strip() if code == 0 else ""


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

    root = str(work / "root.crt")
    issuing = str(work / "issuing.crt")
    leaf = str(work / "db.crt")
    have_issuing = (work / "issuing.crt").is_file()
    have_leaf = (work / "db.crt").is_file()

    issuing_issuer = field(issuing, "-issuer") if have_issuing else ""
    issuing_subject = field(issuing, "-subject") if have_issuing else ""
    code_v, verify_issuing = openssl("verify", "-CAfile", root, issuing) if have_issuing else (1, "issuing.crt missing")
    signed_ok = code_v == 0 and ROOT_CN in issuing_issuer and ISSUING_CN in issuing_subject
    add(
        "issuing-cert-signed-by-the-root",
        signed_ok,
        "issuing.crt carries the required subject and verifies against the root"
        if signed_ok
        else "issuer=%r subject=%r verify=%r"
        % (issuing_issuer[:90], issuing_subject[:90], verify_issuing.strip()[:120]),
    )

    code_bc, bc = openssl("x509", "-in", issuing, "-noout", "-ext", "basicConstraints") if have_issuing else (1, "")
    bc_text = bc.replace(" ", "").lower()
    ca_ok = code_bc == 0 and "ca:true" in bc_text and "pathlen:0" in bc_text
    add(
        "issuing-cert-is-a-ca-for-one-level",
        ca_ok,
        "basic constraints say CA with a depth of zero beneath it"
        if ca_ok
        else "basic constraints read %r" % (bc.strip()[:140] or "none",),
    )

    leaf_issuer = field(leaf, "-issuer") if have_leaf else ""
    leaf_subject = field(leaf, "-subject") if have_leaf else ""
    issued_by_intermediate = (
        bool(leaf_issuer)
        and bool(issuing_subject)
        and leaf_issuer[len("issuer="):] == issuing_subject[len("subject="):]
        and LEAF_CN in leaf_subject
    )
    add(
        "leaf-signed-by-the-issuing-ca",
        issued_by_intermediate,
        "db.crt's issuer is the issuing CA's subject"
        if issued_by_intermediate
        else "db.crt issuer=%r (the issuing CA's subject is %r)"
        % (leaf_issuer[:90], issuing_subject[:90]),
    )

    code_chain, chain_out = (
        openssl("verify", "-CAfile", root, "-untrusted", issuing, leaf)
        if have_leaf and have_issuing
        else (1, "a certificate is missing")
    )
    add(
        "chain-verifies-with-only-the-root-trusted",
        code_chain == 0,
        "db.crt verifies with root.crt trusted and issuing.crt supplied"
        if code_chain == 0
        else chain_out.strip()[:200],
    )

    code_lbc, leaf_bc = openssl("x509", "-in", leaf, "-noout", "-ext", "basicConstraints") if have_leaf else (1, "")
    leaf_bc_text = leaf_bc.replace(" ", "").lower()
    leaf_not_ca = have_leaf and "ca:true" not in leaf_bc_text
    add(
        "leaf-is-not-a-ca",
        leaf_not_ca,
        "db.crt is not a certificate authority"
        if leaf_not_ca
        else "db.crt basic constraints read %r" % (leaf_bc.strip()[:140],),
    )

    def same_key(cert: str, key: Path) -> bool:
        code_c, cert_key = openssl("x509", "-in", cert, "-noout", "-pubkey")
        code_k, priv_key = openssl("pkey", "-in", str(key), "-pubout")
        return code_c == 0 and code_k == 0 and bool(cert_key.strip()) and cert_key.strip() == priv_key.strip()

    keys_ok = (
        have_issuing and have_leaf
        and same_key(issuing, work / "issuing.key")
        and same_key(leaf, work / "db.key")
    )
    add(
        "certificates-use-the-given-keys",
        keys_ok,
        "each certificate certifies the key it was supposed to"
        if keys_ok
        else "a certificate does not match the private key it was meant for",
    )

    root_subject = field(root, "-subject")
    root_issuer = field(root, "-issuer")
    root_ok = (
        ROOT_CN in root_subject
        and root_subject[len("subject="):] == root_issuer[len("issuer="):]
        and (work / "root.key").is_file()
        and openssl("verify", "-CAfile", root, root)[0] == 0
    )
    add(
        "root-untouched",
        root_ok,
        "root.crt is still the self-signed root it was"
        if root_ok
        else "root.crt subject=%r issuer=%r" % (root_subject[:90], root_issuer[:90]),
    )

    add(
        "constraints-supplied-at-signing-time",
        re.search(r"(?mi)^\s*openssl\b[^\n]*(-extfile|-extensions)", log_text) is not None,
        "an extension section was handed to a signing step in the command log",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
