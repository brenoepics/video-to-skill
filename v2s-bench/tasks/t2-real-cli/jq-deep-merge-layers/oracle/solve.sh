#!/bin/sh
# Oracle for jq-deep-merge-layers: the real fold, driven through jq.
# Nothing here writes the expected object literally.
set -eu
cd "${1:-.}"
jq -n 'reduce inputs as $layer ({}; . * $layer)' base.json site.json local.json > effective.json
jq -S . effective.json
