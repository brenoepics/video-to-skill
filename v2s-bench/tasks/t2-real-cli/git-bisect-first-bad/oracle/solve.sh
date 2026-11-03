#!/bin/sh
# Oracle for git-bisect-first-bad: git drives the search from the verdict
# script, is given the broken end first, and is told to let go afterwards.
# The hash is read out of git's own record, never written by hand.
set -eu
cd "${1:-.}"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
git bisect start main main~8 >/dev/null
git bisect run ./check-ratio.sh > bisect-run.out 2>&1 || true
sed -n 's/^\([0-9a-f]\{40\}\) is the first bad commit$/\1/p' bisect-run.out > first-bad.txt
test -s first-bad.txt
git bisect reset >/dev/null
rm -f bisect-run.out
cat first-bad.txt
git log --oneline -1
