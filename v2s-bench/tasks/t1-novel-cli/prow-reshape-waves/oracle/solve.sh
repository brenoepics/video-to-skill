#!/bin/sh
# Oracle for prow-reshape-waves: move web, retire legacy-sync, then
# render the reshaped plan with the waves locked together.
set -eu
cd "${1:-.}"
prow plan --wave 2 --unit web
prow drop --unit legacy-sync
prow status
prow sheet --out rollout/sheet.txt --interlock
