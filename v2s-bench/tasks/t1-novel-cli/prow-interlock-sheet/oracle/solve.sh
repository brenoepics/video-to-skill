#!/bin/sh
# Oracle for prow-interlock-sheet: render the plan with the waves locked
# together.
set -eu
cd "${1:-.}"
prow sheet --out rollout/sheet.txt --interlock
cat rollout/sheet.txt
