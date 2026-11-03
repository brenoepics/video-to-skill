# Tier 1 — the four invented CLIs, baked in.
#
# Build:
#   docker build -t v2sbench/t1-novel-cli:0.1 -f env/t1-novel-cli/Dockerfile .
# (from the v2s-bench root — the build context needs tools/)
#
# Two properties here are load-bearing for the measurement, not hardening
# preferences. Changing either silently changes what the benchmark means.
#
# 1. THE AGENT MUST NOT RUN AS ROOT.
#    Six tier-1 tasks are `discoverability: poor`: the procedure is meant
#    to be unavailable from on-box documentation, so the video is the only
#    source. The tools are plain Python scripts, so `cat $(which kiln)`
#    would hand over the whole procedure. Each poor task's setup runs
#    `chmod 0111` on its tool, leaving it executable but unreadable — and
#    that does nothing against root. An image that runs the agent as root
#    silently converts those six tasks into `discoverability: good` and
#    the stratum stops meaning anything.
#
# 2. NO NETWORK IS NEEDED AT RUN TIME.
#    Tasks declare `network: none`. Everything they need is installed
#    here, at build time, so a trial never reaches for the internet.

FROM python:3.13-slim-bookworm

# coreutils for `install`/`chmod`; nothing else is required — the invented
# tools are standard-library Python by design.
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates; \
    rm -rf /var/lib/apt/lists/*

# The invented CLIs. Copied with mode 0755 and owned by root, so the agent
# user can execute them but a task's `chmod 0111` is what decides
# readability.
# Mode is set HERE, not by a task's setup. Setup runs as the agent user
# against root-owned files, so `chmod` there would simply fail; and baking
# it in is stronger anyway, since the agent owns neither the file nor root
# and so cannot undo it.
#
# 0111 = executable, NOT readable. zep and kiln are the two
# `discoverability: poor` tools: their procedure is meant to be
# unavailable on the box, and they are plain Python, so a readable file
# would hand over the whole thing and void the stratum.
COPY --chmod=0111 tools/zep/zep   /usr/local/bin/zep
COPY --chmod=0111 tools/kiln/kiln /usr/local/bin/kiln
# lath and prow are `discoverability: good` -- their documentation is
# supposed to be usable, so there is nothing to hide.
COPY --chmod=0755 tools/lath/lath /usr/local/bin/lath
COPY --chmod=0755 tools/prow/prow /usr/local/bin/prow

RUN set -eux; \
    for t in zep lath kiln prow; do \
      "/usr/local/bin/$t" --version >/dev/null || { echo "$t is not runnable" >&2; exit 1; }; \
    done

# The unprivileged account every trial runs under. /work is writable by it;
# /usr/local/bin is not, so a trial cannot rewrite a tool to reveal itself.
RUN set -eux; \
    useradd --create-home --uid 10001 --shell /bin/bash agent; \
    mkdir -p /work /opt/v2sbench-support; \
    chown agent:agent /work /opt/v2sbench-support

# ---------------------------------------------------------------------
# The agent under test runs INSIDE this container, so its CLI has to be
# here. Choose with --build-arg AGENT=claude|codex|both|none.
#
# Pin the versions: the agent harness is part of what produced a result,
# and an unpinned CLI makes a figure un-attributable. `claude --version`
# is recorded on every trial regardless.
ARG AGENT=claude
ARG CLAUDE_CODE_VERSION=2.1.317
ARG CODEX_VERSION=latest

USER root
RUN set -eux; \
    if [ "$AGENT" != "none" ]; then \
      apt-get update; \
      apt-get install -y --no-install-recommends nodejs npm; \
      rm -rf /var/lib/apt/lists/*; \
      if [ "$AGENT" = "claude" ] || [ "$AGENT" = "both" ]; then \
        npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}"; \
        claude --version; \
      fi; \
      if [ "$AGENT" = "codex" ] || [ "$AGENT" = "both" ]; then \
        npm install -g "@openai/codex@${CODEX_VERSION}"; \
        codex --version; \
      fi; \
    fi
USER agent

WORKDIR /work

# Fail the build rather than the benchmark if the non-root guarantee breaks.
RUN set -eux; \
    test "$(id -u)" != "0"; \
    cp /usr/local/bin/zep /tmp/probe 2>/dev/null && { echo "zep is readable by the agent; the poor-discoverability stratum is void" >&2; exit 1; } || true; \
    cp /usr/local/bin/kiln /tmp/probe 2>/dev/null && { echo "kiln is readable by the agent; the poor-discoverability stratum is void" >&2; exit 1; } || true; \
    zep --version >/dev/null; \
    kiln --version >/dev/null; \
    rm -f /tmp/probe

CMD ["sleep", "infinity"]
