#!/bin/sh
# Oracle for g2048-two-boards: a scripted competent policy, played move by
# move through the frozen `v2s-game` interface. It is not a replay of a
# recorded win -- it reads each observation and decides from it, so it
# would play an unseen board too -- and it never touches .v2s-game
# directly.
set -eu
cd "${1:-.}"
exec python3 "$(dirname "$0")/play.py"
