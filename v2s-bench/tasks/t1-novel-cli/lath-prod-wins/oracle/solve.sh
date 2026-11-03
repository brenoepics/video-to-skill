#!/bin/sh
# Oracle for lath-prod-wins: stack the winning layer first, then merge.
set -eu
cd "${1:-.}"
lath stack prod
lath stack base
lath layers
lath flatten --out build/app.json
