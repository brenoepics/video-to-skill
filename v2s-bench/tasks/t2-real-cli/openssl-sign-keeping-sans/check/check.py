#!/usr/bin/env python3
"""Checker for openssl-sign-keeping-sans.

`subject-alternative-names-carried` is the assertion that carries the task.
Signing the request without asking for its extensions produces a
certificate with the right subject, the right public key and a signature
that `openssl verify` accepts, and no extensions whatsoever -- so a checker
that only verified the chain would pass the wrong procedure.

`public-key-is-the-requested-one` and `issued-not-self-signed` cover the
other two plausible shortcuts: generating a fresh self-signed certificate
for the host, which does carry the names and is signed by nobody who
matters.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

WANT_DNS = {"api.internal", "api.svc.internal"}
CA_SUBJECT_CN = "Harbour Works Root CA"


def openssl(*args: str) -> tuple[int, str]:
    try:
        done = subprocess.run(["openssl", *args], capture_output=True, text=True, timeout=120)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return 127, str(exc)
    return done.returncode, (done.stdout or "") + (done.stderr or "")


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

    cert = str(work / "api.crt")
    ca = str(work / "ca.crt")
    csr = str(work / "api.csr")
    have_cert = (work / "api.crt").is_file()

    code, verify_out = openssl("verify", "-CAfile", ca, cert) if have_cert else (1, "api.crt missing")
    add(
        "certificate-verifies-against-the-ca",
        code == 0,
        "openssl verify accepts api.crt against ca.crt"
        if code == 0
        else "verification failed: %s" % verify_out.strip()[:200],
    )

    code_san, san_text = openssl("x509", "-in", cert, "-noout", "-ext", "subjectAltName") if have_cert else (1, "")
    found = set(re.findall(r"DNS:([A-Za-z0-9.*-]+)", san_text))
    add(
        "subject-alternative-names-carried",
        code_san == 0 and WANT_DNS <= found,
        "subject alternative names present: %s" % sorted(found)
        if WANT_DNS <= found
        else "subject alternative names on the certificate: %s" % (sorted(found) or "none"),
    )

    code_u, usage_text = (
        openssl("x509", "-in", cert, "-noout", "-ext", "keyUsage,extendedKeyUsage")
        if have_cert
        else (1, "")
    )
    lowered = usage_text.lower()
    usage_ok = (
        code_u == 0
        and "digital signature" in lowered
        and "key encipherment" in lowered
        and ("tls web server authentication" in lowered or "serverauth" in lowered)
    )
    add(
        "key-usage-and-purpose-carried",
        usage_ok,
        "key usage and extended key usage came through"
        if usage_ok
        else "usage extensions read: %r" % (usage_text.strip()[:160] or "none",),
    )

    code_ck, cert_key = openssl("x509", "-in", cert, "-noout", "-pubkey") if have_cert else (1, "")
    code_rk, req_key = openssl("req", "-in", csr, "-noout", "-pubkey")
    same_key = code_ck == 0 and code_rk == 0 and cert_key.strip() == req_key.strip() and bool(cert_key.strip())
    add(
        "public-key-is-the-requested-one",
        same_key,
        "the certificate certifies the public key from api.csr"
        if same_key
        else "the certificate's public key is not the one in api.csr",
    )

    code_i, issuer = openssl("x509", "-in", cert, "-noout", "-issuer") if have_cert else (1, "")
    code_s, subject = openssl("x509", "-in", cert, "-noout", "-subject") if have_cert else (1, "")
    issuer_line = issuer.strip()
    subject_line = subject.strip()
    issued_ok = (
        code_i == 0 and code_s == 0
        and CA_SUBJECT_CN in issuer_line
        and issuer_line[len("issuer="):] != subject_line[len("subject="):]
    )
    add(
        "issued-not-self-signed",
        issued_ok,
        "issued by the CA, not by itself"
        if issued_ok
        else "%s / %s" % (issuer_line[:120], subject_line[:120]),
    )

    lower_bound = openssl("x509", "-in", cert, "-noout", "-checkend", "31363200")[0] if have_cert else 1
    upper_bound = openssl("x509", "-in", cert, "-noout", "-checkend", "31708800")[0] if have_cert else 0
    window_ok = lower_bound == 0 and upper_bound != 0
    add(
        "validity-is-about-a-year",
        window_ok,
        "still valid in 363 days and expired by 367"
        if window_ok
        else "validity window is wrong (valid at 363 days: %s, at 367 days: %s)"
        % (lower_bound == 0, upper_bound == 0),
    )

    code_ca, ca_text = openssl("x509", "-in", ca, "-noout", "-subject", "-issuer")
    code_req, req_check = openssl("req", "-in", csr, "-noout", "-verify")
    intact = (
        code_ca == 0
        and ca_text.count(CA_SUBJECT_CN) == 2
        and code_req == 0
        and (work / "ca.key").is_file()
    )
    add(
        "ca-and-request-untouched",
        intact,
        "the CA is still the self-signed root it was and api.csr still verifies"
        if intact
        else "ca.crt/api.csr changed: %r / %r" % (ca_text.strip()[:120], req_check.strip()[:120]),
    )

    add(
        "extensions-taken-over-at-signing-time",
        re.search(r"(?mi)^\s*openssl\b[^\n]*(-copy_extensions|-extfile)", log_text) is not None,
        "the signing step in the command log asks for the request's extensions",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
