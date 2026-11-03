#!/bin/sh
# Oracle for git-rescue-dangling-commit: audit the object store for objects
# nothing points at, then put a branch on the one that is wanted. No commit
# is created and nothing is rewritten.
set -eu
cd "${1:-.}"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
lost=""
for sha in $(git fsck --no-progress 2>/dev/null | sed -n 's/^dangling commit //p'); do
  if [ "$(git log -1 --format=%s "$sha")" = "add the retry ceiling" ]; then
    lost="$sha"
  fi
done
test -n "$lost"
git branch rescue "$lost"
git log -1 --format='%H %cI %s' rescue
