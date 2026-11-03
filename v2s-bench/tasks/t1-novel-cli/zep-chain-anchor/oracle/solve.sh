#!/bin/sh
# Oracle for zep-chain-anchor: read the parent's anchor out of zep, open
# the child declaring that parent, stage the fact, close with the grant.
set -eu
cd "${1:-.}"
parent=$(zep show base-19 | awk '/^anchor:/{print $2}')
zep open patch-20 --after "$parent"
read -r key value < patch.txt
zep put patch-20 "$key" "$value"
zep seal patch-20 --grant "$(zep grant patch-20 | sed -n 's/^grant: //p')"
zep show patch-20
