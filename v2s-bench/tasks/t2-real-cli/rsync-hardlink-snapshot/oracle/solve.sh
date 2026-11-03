#!/bin/sh
# Oracle for rsync-hardlink-snapshot: rsync is given yesterday's snapshot to
# compare against, by a path relative to the destination it is writing, so
# the files it finds unchanged become second names for the stored copies.
set -eu
cd "${1:-.}"
rsync -a --delete --link-dest=../2026-11-01 source/ snapshots/2026-11-02/
ls -li snapshots/2026-11-01 snapshots/2026-11-02
