#!/bin/sh
# Oracle for crafter-first-pickaxe: a scripted competent policy, played
# through the frozen `v2s-game` interface. It never imports crafter and
# never writes the journal directly.
set -eu
cd "${1:-.}"
exec python3 "$(dirname "$0")/play.py"
