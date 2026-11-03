#!/bin/sh
# Oracle for rsync-mirror-protect-excluded: the contents of the source are
# mirrored (note the trailing slash), the deletion pass is on, and the two
# names that must not cross are hidden from both sides -- which is also what
# keeps the receiver-only directory from being deleted.
set -eu
cd "${1:-.}"
rsync -a --delete --exclude=/tmp/ --exclude=/logs/ site/ public/
find public -type f | sort
