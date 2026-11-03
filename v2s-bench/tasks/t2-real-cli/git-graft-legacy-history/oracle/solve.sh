#!/bin/sh
# Oracle for git-graft-legacy-history: git is told to substitute a commit
# that records the extra parent, so the walk continues into the older
# history while every existing object stays exactly as it is.
set -eu
cd "${1:-.}"
root=$(git rev-list --max-parents=0 main)
git replace --graft "$root" legacy
git log --oneline main
git replace -l
