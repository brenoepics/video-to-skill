#!/bin/sh
# Verdict on the revision that is currently checked out.
#   0   -- ratio.sh reduces 12:18 correctly
#   1   -- it does not
#   125 -- this revision cannot be judged: ratio.sh does not even parse
sh -n ./ratio.sh 2>/dev/null || exit 125
out=$(sh ./ratio.sh 12 18 2>/dev/null)
[ "$out" = "2:3" ]
