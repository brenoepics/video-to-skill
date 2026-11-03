#!/bin/sh
# Oracle for kiln-author-profile: write the profile kiln looks for, then
# let kiln build its own state from it.
set -eu
cd "${1:-.}"
cat > kiln.profile <<'PROFILE'
fire-lane: amber
shard-glob: shards/*.txt
hold-count: 3
PROFILE
kiln prime
kiln peek
