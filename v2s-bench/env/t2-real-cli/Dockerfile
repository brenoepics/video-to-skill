# Tier 2 — the six real CLIs, pinned and baked in.
#
# Build:
#   docker build -t v2sbench/t2-real-cli:0.1 -f env/t2-real-cli/Dockerfile .
#
# Why this image exists at all: the tier-2 tasks declare `network: none`
# and their setup steps install tools with apt. Those two cannot both be
# true at run time, so every tier-2 trial would have failed at setup on a
# plain `ubuntu:24.04`. The packages are installed here instead.
#
# The tasks' setup lines are guarded by `command -v`, so they are no-ops
# against this image and still work on a machine where someone has
# installed the tools another way. They also assert the exact version with
# `dpkg-query`, so a drifted image fails loudly at setup rather than
# quietly measuring a different tool than the one the task was written
# against — which is the whole point of pinning.
#
# As in tier 1, the agent runs unprivileged and nothing needs the network
# at run time.

FROM ubuntu:24.04

# Exact versions, matching what the tasks assert. If a rebuild cannot
# resolve one of these, that is the pin working: the archive has moved and
# the tasks must be re-validated against the new version, not silently run
# against it.
ARG JQ_VERSION=1.7.1-3ubuntu0.24.04.2
ARG FFMPEG_VERSION=7:6.1.1-3ubuntu5
ARG GIT_VERSION=1:2.43.0-1ubuntu7.4
ARG OPENSSL_VERSION=3.0.13-0ubuntu3.16
ARG SQLITE_VERSION=3.45.1-1ubuntu2.8
ARG RSYNC_VERSION=3.2.7-1ubuntu1.5

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
      "jq=${JQ_VERSION}" \
      "ffmpeg=${FFMPEG_VERSION}" \
      "git=${GIT_VERSION}" \
      "openssl=${OPENSSL_VERSION}" \
      "sqlite3=${SQLITE_VERSION}" \
      "rsync=${RSYNC_VERSION}" \
      ca-certificates python3; \
    rm -rf /var/lib/apt/lists/*

# Prove every pin landed, at build time, so a trial never discovers it.
RUN set -eux; \
    for p in jq ffmpeg git openssl sqlite3 rsync; do \
      dpkg-query -W -f='${Package} ${Version}\n' "$p"; \
    done

# git refuses to commit without an identity, and several tasks build their
# fixture by committing. Set here so no task has to, and so every task
# builds the same fixture.
ENV GIT_AUTHOR_NAME=v2sbench \
    GIT_AUTHOR_EMAIL=v2sbench@invalid \
    GIT_COMMITTER_NAME=v2sbench \
    GIT_COMMITTER_EMAIL=v2sbench@invalid \
    GIT_CONFIG_GLOBAL=/home/agent/.gitconfig

RUN set -eux; \
    useradd --create-home --uid 10001 --shell /bin/bash agent; \
    mkdir -p /work /opt/v2sbench-support; \
    chown agent:agent /work /opt/v2sbench-support; \
    printf '[init]\n\tdefaultBranch = main\n[user]\n\tname = v2sbench\n\temail = v2sbench@invalid\n' \
      > /home/agent/.gitconfig; \
    chown agent:agent /home/agent/.gitconfig

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

RUN set -eux; test "$(id -u)" != "0"

CMD ["sleep", "infinity"]
