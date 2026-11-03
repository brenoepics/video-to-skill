#!/usr/bin/env python3
"""Checker for jq-recover-truncated-array.

The complete records are recovered here independently, by decoding the
array body one value at a time and stopping where the bytes run out, so
nothing about the fixture is hard-coded. Two assertions carry the task:
`nothing-invented` catches the plausible wrong procedure of completing the
severed record by hand (it would appear with the sequence number the file
stops on), and `damaged-file-left-damaged` catches repairing events.json in
place -- after that the file parses, which it must not.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


def complete_elements(text: str) -> tuple[list[object], bool]:
    """Decode as many top-level array elements as the bytes allow.

    Returns the elements and whether the array was closed properly. This is
    the same contract jq's streaming parser honours: everything parsed
    before the damage is kept, nothing after it is guessed.
    """
    decoder = json.JSONDecoder()
    index = text.find("[")
    if index < 0:
        return [], False
    index += 1
    out: list[object] = []
    while True:
        while index < len(text) and text[index] in " \t\r\n,":
            index += 1
        if index >= len(text):
            return out, False
        if text[index] == "]":
            return out, True
        try:
            value, index = decoder.raw_decode(text, index)
        except ValueError:
            return out, False
        out.append(value)


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

    source = work / "events.json"
    try:
        raw = source.read_text(encoding="utf-8", errors="replace")
    except OSError:
        raw = ""
    expected, closed = complete_elements(raw)

    still_broken = False
    if raw:
        try:
            json.loads(raw)
        except ValueError:
            still_broken = True
    add(
        "damaged-file-left-damaged",
        bool(raw) and still_broken and not closed and len(expected) > 0,
        "events.json is still the truncated original (%d intact records, array unclosed)"
        % len(expected)
        if still_broken and not closed
        else "events.json is missing, or has been repaired or rewritten in place",
    )

    out_path = work / "recovered.jsonl"
    try:
        produced_raw = out_path.read_text(encoding="utf-8")
    except OSError:
        produced_raw = ""

    lines = produced_raw.split("\n")
    trailing_ok = produced_raw.endswith("\n") and lines[-1] == ""
    body = lines[:-1] if trailing_ok else lines
    parsed: list[object] = []
    shape_problem = ""
    if not produced_raw:
        shape_problem = "recovered.jsonl is missing or empty"
    elif not trailing_ok:
        shape_problem = "recovered.jsonl does not end in exactly one newline"
    else:
        for number, line in enumerate(body, 1):
            if not line.strip():
                shape_problem = "line %d is blank" % number
                break
            if line != line.strip():
                shape_problem = "line %d is padded with whitespace" % number
                break
            try:
                value = json.loads(line)
            except ValueError as exc:
                shape_problem = "line %d does not parse: %s" % (number, exc)
                break
            if not isinstance(value, dict):
                shape_problem = "line %d is not a JSON object" % number
                break
            if json.dumps(value, separators=(",", ":"), sort_keys=False) != line:
                # compact was asked for; re-serialising with the same key
                # order must give the line back byte for byte.
                shape_problem = "line %d is not compact JSON" % number
                break
            parsed.append(value)
    add(
        "recovered-is-jsonl",
        not shape_problem,
        "%d compact records, one per line" % len(parsed) if not shape_problem else shape_problem,
    )

    add(
        "all-complete-records-present",
        parsed == expected and len(expected) > 0,
        "recovered.jsonl holds exactly the %d complete records, in order" % len(expected)
        if parsed == expected
        else "expected %d records, got %d%s"
        % (
            len(expected),
            len(parsed),
            ""
            if len(parsed) != len(expected)
            else "; first difference at index %d"
            % next((i for i, (a, b) in enumerate(zip(parsed, expected)) if a != b), -1),
        ),
    )

    # The severed record's own sequence number appears nowhere in a faithful
    # recovery: it is the one the bytes run out inside.
    severed_seq = None
    tail = raw[raw.rfind("}") + 1 :] if "}" in raw else raw
    match = re.search(r'"seq"\s*:\s*(\d+)', tail)
    if match:
        severed_seq = int(match.group(1))
    invented = [r for r in parsed if isinstance(r, dict) and r.get("seq") == severed_seq]
    if severed_seq is None:
        detail = "cannot tell which record was severed: events.json no longer ends mid-record"
    elif invented:
        detail = "a record for the severed seq=%r was filled in: %r" % (severed_seq, invented[:1])
    else:
        detail = "the severed record (seq=%r) was not reconstructed" % (severed_seq,)
    add("nothing-invented", severed_seq is not None and not invented, detail)

    add(
        "recovered-with-the-streaming-parser",
        re.search(r"(?mi)^\s*jq\b[^\n]*--stream", log_text) is not None,
        "a streaming jq invocation appears in the command log",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
