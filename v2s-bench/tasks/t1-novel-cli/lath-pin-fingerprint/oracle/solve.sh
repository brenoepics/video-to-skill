#!/bin/sh
# Oracle for lath-pin-fingerprint: merge, carry the printed fingerprint
# into a new layer as a value, stack it, merge again.
set -eu
cd "${1:-.}"
mark=$(lath flatten --out build/app.json | sed -n 's/^fingerprint: //p')
lath set release "$mark" pinned_from
lath stack release
lath flatten --out dist/app.json
