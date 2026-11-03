#!/bin/sh
# Oracle for kiln-fire-newest-three: correct the hold count in the lane
# profile, prime the lane again so the state matches it, then fire once.
set -eu
cd "${1:-.}"
sed 's/^hold-count: .*/hold-count: 3/' kiln.profile > kiln.profile.next
mv kiln.profile.next kiln.profile
kiln prime
kiln fire
kiln peek
