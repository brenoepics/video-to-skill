"""Render a hermetic screencast: a JSON recipe in, a reproducible mp4 out.

The benchmark ships tutorial videos as *data*. Committing video bytes
would raise a licensing question this project cannot answer and would
make the corpus unauditable, so the hermetic tier describes each tutorial
in ``videos/recipes/*.json`` and renders it at run time. Two renders of
one recipe on one machine with one ffmpeg produce byte-identical files;
``docs/hermetic-video.md`` states exactly how far that guarantee reaches
and -- just as important for the benchmark's credibility -- what a
rendered screencast does *not* reproduce about a real one.

    from v2sbench.render import render, verify, load_recipe
    result = render("videos/recipes/term-git-init.json", "out/term-git-init.mp4")
    verify(result.path, load_recipe("videos/recipes/term-git-init.json"),
           expect_audio=result.narrated)

``plan(recipe)`` returns the same frame-by-frame schedule the renderer
follows, without touching ffmpeg, so timing can be asserted cheaply.
"""

from __future__ import annotations

from .narration import NarrationError, NarrationTrack, detect_backend
from .recipe import Cue, Recipe, RecipeError, Screen, Still, Typing, load_recipe
from .screens import Canvas, Style, draw
from .video import (
    DEFAULT_TYPING_SECS,
    RenderError,
    RenderPlan,
    RenderResult,
    Segment,
    ffmpeg_version,
    frames,
    plan,
    render,
    save_png,
    verify,
)

__all__ = [
    "DEFAULT_TYPING_SECS",
    "Canvas",
    "Cue",
    "NarrationError",
    "NarrationTrack",
    "Recipe",
    "RecipeError",
    "RenderError",
    "RenderPlan",
    "RenderResult",
    "Screen",
    "Segment",
    "Still",
    "Style",
    "Typing",
    "detect_backend",
    "draw",
    "ffmpeg_version",
    "frames",
    "load_recipe",
    "plan",
    "render",
    "save_png",
    "verify",
]
