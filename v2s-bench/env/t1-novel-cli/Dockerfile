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
COPY --chmod=0755 tools/zep/zep   /usr/local/bin/zep
COPY --chmod=0755 tools/lath/lath /usr/local/bin/lath
COPY --chmod=0755 tools/kiln/kiln /usr/local/bin/kiln
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

USER agent
WORKDIR /work

# Fail the build rather than the benchmark if the non-root guarantee breaks.
RUN set -eux; \
    test "$(id -u)" != "0"; \
    cp /usr/local/bin/zep /tmp/probe 2>/dev/null && { echo "tools are writable by the agent" >&2; exit 1; } || true; \
    rm -f /tmp/probe

CMD ["sleep", "infinity"]
