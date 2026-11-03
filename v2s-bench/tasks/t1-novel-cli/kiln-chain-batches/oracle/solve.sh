#!/bin/sh
# Oracle for kiln-chain-batches: fire once, carry the printed batch id
# into the lane profile as its prior, prime again, fire again.
set -eu
cd "${1:-.}"
kiln prime
first=$(kiln fire | sed -n 's/^batch: //p')
printf 'prior-batch: %s\n' "$first" >> kiln.profile
kiln prime
kiln fire
kiln peek
