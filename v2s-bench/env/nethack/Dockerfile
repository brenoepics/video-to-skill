# The NetHack rung's runtime. **NEVER BUILT.** The container this was
# written in has the docker CLI and no daemon, so this file is a design
# with a verified pip step, not a tested image. See env/nethack/README.md.
#
# What IS verified: `pip install --require-hashes -r requirements.txt` into
# a clean CPython 3.13 environment on linux/amd64 -- the same interpreter
# and platform as the base image -- after which NetHack runs headless and
# seeds reproducibly (env/nethack/probe_determinism.py, output in the
# README). A cp313 manylinux wheel exists for nle 1.3.0, so nothing is
# compiled here and no cmake/bison/flex toolchain is needed.
#
# LICENCE: nle ships a compiled NetHack 3.6 under the NetHack General
# Public Licence. Nothing is vendored into this repository. If an image
# built from this file is published, the NGPL's source-availability
# obligation applies: point at github.com/facebookresearch/nle and NetHack
# upstream.
FROM python:3.13-slim

COPY requirements.txt /opt/v2s-game/requirements.txt
RUN pip install --no-cache-dir --require-hashes -r /opt/v2s-game/requirements.txt

# The BALROG progress metric (dungeon level x experience level -> empirical
# human win probability) is the graded score to reuse rather than reinvent.
# BALROG is MIT. It is NOT on PyPI: `pip install balrog` installs an
# unrelated access-control library, so it must be vendored from the
# repository at a pinned commit. Left commented out until the rung has an
# oracle and a measured milestone; a pinned commit written down now would
# be a pin nobody has tested.
# RUN git clone --depth 1 https://github.com/balrog-ai/BALROG /opt/balrog \
#     && git -C /opt/balrog checkout <pinned-commit>

ENV PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
# NetHack wants a writable home for its config and bones files.
ENV HOME=/work NETHACKOPTIONS=""
WORKDIR /work
