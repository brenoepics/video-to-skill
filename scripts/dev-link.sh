#!/bin/sh
# Test the skill against the local build: one command that ends in a chat.
#
#   scripts/dev-link.sh [--data-dir DIR] [--force] [--no-build] [--no-chat]
#   scripts/dev-link.sh --unlink
#
# Builds vts-extract, links this checkout into ~/.claude/skills/video-to-skill
# so Claude Code loads *this* SKILL.md, points VTS_DATA_DIR at a dev data dir,
# and launches `claude` in the current directory. Then, in the chat:
#
#   /video-to-skill <video>
#
# The skill's own `vts-extract check --fix` bootstraps ffmpeg, yt-dlp and the
# models into the dev data dir on first use, after showing you the list.
#
# Why a separate data dir: SKILL.md step 1 looks in <data-dir>/bin for a
# cached release binary before it looks at target/release, and both carry the
# pinned version, so with the default data dir the release wins and the local
# build is never exercised. Pointing VTS_DATA_DIR at a dir where no release was
# ever fetched (and where this script links the build as that first candidate)
# makes the dev build the only vts-extract the skill can find.
#
#   --data-dir DIR  dev data dir (default: $VTS_DATA_DIR if set, else ~/.vts-dev)
#   --force         move a real ~/.claude/skills/video-to-skill directory aside
#   --no-build      reuse target/release/vts-extract instead of building
#   --no-chat       build + link, then print the export line instead of
#                   launching claude (CI, scripts, the test)
#   --unlink        point the skill back at the npx skills install (or remove
#                   the link when there is none); the data dir is left alone
set -eu

say() { printf '%s\n' "$@"; }
die() { printf 'dev-link: %s\n' "$@" >&2; exit 1; }

usage() {
  sed -n '2,/^set -eu/{/^set -eu/d;s/^# \{0,1\}//p;}' "$0"
}

data_dir="${VTS_DATA_DIR:-$HOME/.vts-dev}"
force=0
build=1
chat=1
unlink=0
while [ $# -gt 0 ]; do
  case "$1" in
    --data-dir) [ $# -ge 2 ] || die "--data-dir needs a directory"; data_dir="$2"; shift ;;
    --data-dir=*) data_dir="${1#--data-dir=}" ;;
    --force) force=1 ;;
    --no-build) build=0 ;;
    --no-chat) chat=0 ;;
    --unlink) unlink=1 ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
  shift
done

checkout="$(cd "$(dirname "$0")/.." && pwd -P)"
[ -f "$checkout/SKILL.md" ] || die "no SKILL.md beside $checkout/scripts; run me from a checkout"
skills_dir="$HOME/.claude/skills"
link="$skills_dir/video-to-skill"
npx_install="$HOME/.agents/skills/video-to-skill"

# --unlink: give ~/.claude/skills/video-to-skill back to `npx skills add`.
if [ "$unlink" = 1 ]; then
  if [ -L "$link" ]; then
    rm "$link"
    say "removed $link"
  elif [ -e "$link" ]; then
    die "$link is a real directory, not a dev link; nothing to restore"
  fi
  if [ -d "$npx_install" ]; then
    # The same relative link `npx skills add` creates.
    ln -s ../../.agents/skills/video-to-skill "$link"
    say "restored $link -> $npx_install"
  else
    say "no npx skills install at $npx_install; to reinstall it:" \
        "  npx skills@latest add brenoepics/video-to-skill"
  fi
  exit 0
fi

# A chat needs claude; say so before spending minutes on a build.
if [ "$chat" = 1 ] && ! command -v claude >/dev/null 2>&1; then
  die "claude is not on PATH; install Claude Code (https://claude.com/claude-code)," \
      "  or pass --no-chat to build and link without launching it"
fi

# 1. Build, and make sure target/release/vts-extract beside SKILL.md is the
#    result (SKILL.md step 1, candidate 2) even when cargo builds elsewhere.
candidate="$checkout/target/release/vts-extract"
if [ "$build" = 1 ]; then
  command -v cargo >/dev/null 2>&1 || die "cargo is not on PATH; install Rust or pass --no-build"
  (cd "$checkout" && cargo build --release --locked)
  target_dir="$(cd "$checkout" && cargo metadata --format-version 1 --no-deps \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
  [ -n "$target_dir" ] || die "could not read cargo's target directory"
  built="$target_dir/release/vts-extract"
  [ -x "$built" ] || die "build finished but $built is missing"
  if ! { [ -e "$candidate" ] && [ "$built" -ef "$candidate" ]; }; then
    mkdir -p "$checkout/target/release"
    rm -f "$candidate"
    ln -s "$built" "$candidate"
    say "cargo builds into $target_dir; linked target/release/vts-extract -> $built"
  fi
fi
[ -x "$candidate" ] || die "no $candidate; run without --no-build"

# 2. The pin in SKILL.md must be the version the build reports, or every
#    resolver that follows SKILL.md ignores the build and fetches a release.
built_version="$("$candidate" --version | sed 's/^vts-extract //')"
[ -n "$built_version" ] || die "$candidate --version printed nothing"
pin="$(sed -n 's/^## 1\. Locate the extractor (requires vts-extract \([0-9][^)]*\)).*/\1/p' "$checkout/SKILL.md")"
[ -n "$pin" ] || die "SKILL.md has no '## 1. Locate the extractor (requires vts-extract X.Y.Z)' heading"
if [ "$pin" != "$built_version" ]; then
  mentions="$(grep -c "$pin" "$checkout/SKILL.md" || true)"
  die "SKILL.md pins vts-extract $pin but target/release/vts-extract reports $built_version" \
      "  update the pin: replace every '$pin' in SKILL.md ($mentions mentions) with '$built_version'," \
      "  starting with '## 1. Locate the extractor (requires vts-extract $built_version)'" \
      "  (or bump Cargo.toml's workspace version to $pin and rebuild if the build is the stale one)"
fi
say "SKILL.md pins vts-extract $pin — matches the build"

# 3. ~/.claude/skills/video-to-skill -> this checkout.
mkdir -p "$skills_dir"
if [ -L "$link" ]; then
  current="$(cd "$link" 2>/dev/null && pwd -P || true)"
  if [ "$current" = "$checkout" ]; then
    say "already linked: $link -> $checkout"
  else
    previous="$(readlink "$link" || echo "$current")"
    rm "$link"
    ln -s "$checkout" "$link"
    say "replaced $link (was -> $previous) -> $checkout"
  fi
elif [ -e "$link" ]; then
  [ "$force" = 1 ] || die "$link is a real directory, not a symlink" \
      "  re-run with --force to move it aside as $link.bak-<timestamp>"
  backup="$link.bak-$(date +%Y%m%d%H%M%S)"
  mv "$link" "$backup"
  ln -s "$checkout" "$link"
  say "moved $link aside to $backup" "linked $link -> $checkout"
else
  ln -s "$checkout" "$link"
  say "linked $link -> $checkout"
fi

# 4. The dev data dir, with the build as the first thing SKILL.md step 1
#    finds there. The skill's own `check --fix` fills in the tools.
case "$data_dir" in /*) ;; *) data_dir="$PWD/$data_dir" ;; esac
mkdir -p "$data_dir"
cached="$data_dir/bin/vts-extract-$built_version"
if [ -L "$cached" ] || [ ! -e "$cached" ]; then
  mkdir -p "$data_dir/bin"
  rm -f "$cached"
  ln -s "$candidate" "$cached"
  say "linked $cached -> $candidate (SKILL.md step 1, candidate 1)"
else
  say "warning: $cached is a cached release binary; SKILL.md step 1 picks it" \
      "  over the dev build. Use another --data-dir, or remove it, to test the build."
fi

# 5. Chat here with the dev data dir set, or say how to.
if [ "$chat" = 1 ]; then
  say "" "launching claude in $PWD with VTS_DATA_DIR=$data_dir — in the chat:" \
      "  /video-to-skill <video>"
  export VTS_DATA_DIR="$data_dir"
  exec claude
fi
say "" "Next, in the shell that launches claude:" \
    "  export VTS_DATA_DIR=$data_dir" \
    "then, in the chat:" \
    "  /video-to-skill <video>"
command -v claude >/dev/null 2>&1 \
  || say "" "note: claude is not on PATH here; run the export in a shell where it is."
