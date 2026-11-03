#!/bin/sh
# Oracle for lath-stage-pair: value before key, twice.
set -eu
cd "${1:-.}"
lath set base 8080 port
lath set base eu-west region
lath show base
