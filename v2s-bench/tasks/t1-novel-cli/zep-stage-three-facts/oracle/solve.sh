#!/bin/sh
# Oracle for zep-stage-three-facts: open, stage every fact, then close
# with the grant. Every write goes through zep.
set -eu
cd "${1:-.}"
zep open hotfix-3
while read -r key value; do
  [ -n "$key" ] || continue
  zep put hotfix-3 "$key" "$value"
done < handover.txt
zep seal hotfix-3 --grant "$(zep grant hotfix-3 | sed -n 's/^grant: //p')"
zep show hotfix-3
