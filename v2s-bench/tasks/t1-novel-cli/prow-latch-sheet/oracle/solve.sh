#!/bin/sh
# Oracle for prow-latch-sheet: render the sequenced sheet, carry the id it
# prints into the release step.
set -eu
cd "${1:-.}"
ident=$(prow sheet --out rollout/sheet.txt --interlock | sed -n 's/^sheet: //p')
prow latch "$ident"
prow status
