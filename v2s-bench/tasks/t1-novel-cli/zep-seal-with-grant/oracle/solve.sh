#!/bin/sh
# Oracle for zep-seal-with-grant: the real procedure, driven through the
# real tool. Nothing here writes zep's state files directly.
set -eu
cd "${1:-.}"
nonce=$(zep grant rc-7 | sed -n 's/^grant: //p')
zep seal rc-7 --grant "$nonce"
zep show rc-7
zep ledger
