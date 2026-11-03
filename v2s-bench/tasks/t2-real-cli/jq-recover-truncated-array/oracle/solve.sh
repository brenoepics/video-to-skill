#!/bin/sh
# Oracle for jq-recover-truncated-array: jq's streaming parser, which emits
# what it has already parsed before it hits the damage. jq exits 5 on the
# truncation after writing the intact records, so the exit code is tolerated
# and the result is checked instead.
set -u
cd "${1:-.}"
jq -cn --stream 'fromstream(1|truncate_stream(inputs))' events.json > recovered.jsonl 2>/dev/null
test -s recovered.jsonl || { echo "nothing recovered" >&2; exit 1; }
wc -l < recovered.jsonl
