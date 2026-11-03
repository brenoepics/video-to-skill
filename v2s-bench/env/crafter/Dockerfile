# The Crafter rung's runtime: a pinned, hash-verified Crafter and nothing else.
#
#   docker build -t v2sbench/t4-game-crafter:0.1 env/crafter
#
# NOT BUILT OR TESTED AS AN IMAGE in the environment this was written in,
# which has the docker CLI but no daemon. What *is* verified is the step
# that can fail: `pip install --require-hashes -r requirements.txt` was run
# into a clean CPython 3.13 virtual environment on linux/amd64 -- the same
# interpreter version and platform as the base image -- and the resulting
# install runs the frozen interface, the oracle and the checker. See
# docs/game-tier.md for exactly what is and is not verified.
#
# The tag is deliberately not a digest: the harness resolves the image to a
# digest at run time and records it on every trial
# (schemas/task.schema.json, environment.image), so pinning a digest here
# as well would be a second source of truth that can disagree with the one
# that ends up in the results.
FROM python:3.13-slim

# Crafter is MIT and ships its own assets, so nothing is vendored into this
# repository and nothing is fetched from a rights holder at run time. Every
# distribution is pinned by version and sha256. Building Crafter's sdist
# pulls setuptools through pip's build isolation, unpinned: that is the one
# unpinned input, it affects packaging rather than game rules, and the game
# rules are hashed separately in PACKAGE.sha256.json and verified by the
# checker before it scores anything.
COPY requirements.txt /opt/v2s-game/requirements.txt
RUN pip install --no-cache-dir --require-hashes -r /opt/v2s-game/requirements.txt

# No bytecode caches. A .pyc whose header matches its source would be
# loaded in preference to the source, which would sidestep the checker's
# hash check on Crafter's rule-bearing files. Removing the caches and
# refusing to write new ones raises the cost of that bypass; it does not
# eliminate it, and tasks/t4-game/README.md says so.
ENV PYTHONDONTWRITEBYTECODE=1
RUN find /usr/local/lib -name '__pycache__' -type d -prune -exec rm -rf {} + || true

# The interface itself is NOT baked in. It travels with the task, in
# tasks/t4-game/<id>/files/, and the task's `setup` installs it: one
# install path, mirrored byte for byte from env/crafter/ and pinned by
# env/freeze.json, so the image and the task can never disagree about
# which protocol version is being played.
#
# PROTOCOL.md is deliberately NOT copied in either. It explains why the
# interface is shaped as it is, and in doing so it states the task's
# knowledge gap in plain words -- a table costs two wood, crafting needs
# you standing beside it, stone needs the pickaxe first. Inside the
# container that is an answer key handed to every arm, including the
# baseline. The agent gets `v2s-game help`, which says that requirements
# exist and refuses to say what they are.

# Headless by construction: Crafter renders with numpy and Pillow, with no
# OpenGL, no X11 and no GPU. There is no DISPLAY here and nothing wants one.
ENV PYTHONUNBUFFERED=1
WORKDIR /work
