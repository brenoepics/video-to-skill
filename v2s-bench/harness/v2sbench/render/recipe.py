"""Load, validate and normalise a hermetic screencast recipe.

Why this is separate from the drawing code: the recipe is the *contract*
(``schemas/recipe.schema.json``), and a still's ``terminal`` block is not
a picture -- it is a set of edits applied to the terminal as it currently
stands, exactly as in the parent project's Rust gold-set renderer
(``crates/extractor/src/eval/synth/terminal.rs``). Resolving that state
machine into concrete, fully specified screens *before* a pixel is drawn
keeps ``screens.py`` pure (screen in, raster out) and lets tests assert
what is on screen without rendering anything.

The JSON Schema is the outer gate; this module also enforces what the
*renderer* needs and the schema cannot express (a still at t=0, strictly
increasing times, exactly one screen per still, contiguous step runs) --
the same rules ``Recipe::validate`` applies on the Rust side.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

TERMINAL_ROWS = 18  # rows the terminal shows; older lines scroll off the top
DEFAULT_PROMPT = "$ "
SCREEN_KEYS = ("terminal", "dialog", "slide", "desktop")
SCHEMA_PATH = Path(__file__).resolve().parents[3] / "schemas" / "recipe.schema.json"


class RecipeError(ValueError):
    """A recipe is unusable; the message names the recipe and the time."""


@dataclass(frozen=True)
class Screen:
    """One resolved screen, ready to draw.

    One dataclass, not four subclasses: the kinds share a title and a
    list of lines, so one shape keeps the drawing dispatch small.
    ``lines`` is terminal lines, dialog rows or slide bullets by ``kind``.
    """

    kind: str
    title: str = ""
    lines: tuple[str, ...] = ()
    buttons: tuple[str, ...] = ()
    focus: str | None = None
    status: str | None = None


@dataclass(frozen=True)
class Typing:
    """What is needed to draw a command mid-keystroke.

    ``lines`` is the terminal *without* the typed line, so ``at(n)`` is
    a pure function of the recipe and a character count -- which is what
    makes the typing animation reproducible.
    """

    title: str
    lines: tuple[str, ...]
    prompt: str
    typed: str

    def at(self, chars: int) -> Screen:
        shown = (*self.lines, self.prompt + self.typed[:chars])
        return Screen("terminal", self.title, _window(shown))


@dataclass(frozen=True)
class Still:
    t: float
    screen: Screen
    step: str | None = None
    continuous: bool = False
    typing: Typing | None = None


@dataclass(frozen=True)
class Cue:
    t: float
    text: str


@dataclass(frozen=True)
class Recipe:
    schema_version: int
    id: str
    title: str
    width: int
    height: int
    fps: int
    duration_secs: float
    stills: tuple[Still, ...]
    narration: tuple[Cue, ...] = ()
    notes: str | None = None
    source: Path | None = field(default=None, compare=False)

    @property
    def frame_count(self) -> int:
        """Frames at constant frame rate; the rendered file's exact length."""
        return max(1, round(self.duration_secs * self.fps))

    def still_durations(self) -> list[float]:
        """Each still's on-screen seconds, the last running to the end."""
        ends = [*(s.t for s in self.stills[1:]), self.duration_secs]
        return [end - s.t for s, end in zip(self.stills, ends, strict=True)]

    def change_times(self) -> list[float]:
        """Gold visual-change times: every non-``continuous`` still but the first.

        Matches ``Recipe::change_times`` on the Rust side, scored at 1 fps
        sampling within +/-2 s; ``continuous`` means scrolling or
        streaming inside one state, so it is *not* a change.
        """
        return [s.t for s in self.stills[1:] if not s.continuous]

    def steps(self) -> list[tuple[str, float, float]]:
        """``(step, start, end)`` per contiguous run of stills sharing a label."""
        spans: list[tuple[str, float, float]] = []
        for still, secs in zip(self.stills, self.still_durations(), strict=True):
            if still.step is None:
                continue
            if spans and spans[-1][0] == still.step and spans[-1][2] == still.t:
                spans[-1] = (still.step, spans[-1][1], still.t + secs)
            else:
                spans.append((still.step, still.t, still.t + secs))
        return spans


def load_recipe(path: str | Path) -> Recipe:
    """Read, schema-check and normalise the recipe at ``path``."""
    source = Path(path)
    try:
        data = json.loads(source.read_text(encoding="utf-8"))
    except OSError as exc:
        raise RecipeError(f"cannot read recipe {source}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise RecipeError(f"recipe {source} is not valid JSON: {exc}") from exc
    if not isinstance(data, dict):
        raise RecipeError(f"recipe {source} must be a JSON object, not {type(data).__name__}")
    validate_against_schema(data, source=source)
    return build_recipe(data, source=source)


def validate_against_schema(data: dict[str, Any], *, source: Path | None = None) -> bool:
    """Check ``data`` against the JSON Schema; True when it was checked.

    ``jsonschema`` need not be present for a bare ``render`` install, so a
    missing library or schema downgrades to :func:`build_recipe`'s checks.
    """
    try:
        import jsonschema
    except ImportError:
        return False
    if not SCHEMA_PATH.is_file():
        return False
    try:
        jsonschema.validate(data, json.loads(SCHEMA_PATH.read_text(encoding="utf-8")))
    except jsonschema.ValidationError as exc:
        at = "/".join(str(p) for p in exc.absolute_path) or "(root)"
        raise RecipeError(
            f"recipe {source or '?'} fails the schema at {at}: {exc.message}"
        ) from exc
    return True


_REQUIRED = ("schema_version", "id", "title", "width", "height", "fps", "duration_secs")


def build_recipe(data: dict[str, Any], *, source: Path | None = None) -> Recipe:
    """Normalise a mapping into a :class:`Recipe`, enforcing renderer rules."""
    rid = str(data.get("id", "?"))
    missing = [k for k in _REQUIRED if k not in data]
    if missing:
        raise RecipeError(f"recipe {rid}: missing required field(s) {', '.join(missing)}")
    if data["schema_version"] != 1:
        raise RecipeError(f"recipe {rid}: schema_version {data['schema_version']!r}; we speak 1")
    width, height, fps = int(data["width"]), int(data["height"]), int(data["fps"])
    duration = float(data["duration_secs"])
    if min(width, height, fps) <= 0 or duration <= 0:
        raise RecipeError(f"recipe {rid}: width, height, fps and duration_secs must be positive")
    if width % 2 or height % 2:
        raise RecipeError(f"recipe {rid}: {width}x{height} has an odd side; H.264 cannot encode it")
    if len(data.get("stills") or []) < 2:
        raise RecipeError(f"recipe {rid}: needs at least two stills (the schema's minItems)")
    return Recipe(
        1, rid, str(data["title"]), width, height, fps, duration,
        _build_stills(rid, data["stills"], duration, f"{rid} - bash"),
        _build_narration(rid, data.get("narration") or [], duration),
        data.get("notes"), source,
    )


def _build_stills(
    rid: str, raw: list[dict[str, Any]], duration: float, title: str
) -> tuple[Still, ...]:
    terminal = _TerminalState(title)
    stills: list[Still] = []
    previous: str | None = None
    seen: set[str] = set()
    for index, item in enumerate(raw):
        t = float(item["t"])
        if (index == 0 and t != 0.0) or (index and t <= stills[-1].t):
            raise RecipeError(f"recipe {rid}: still times must start at 0 and increase (at t={t})")
        if t >= duration:
            raise RecipeError(f"recipe {rid}: still at t={t} starts at or after the end")
        present = [k for k in SCREEN_KEYS if k in item]
        if len(present) != 1:
            raise RecipeError(f"recipe {rid}: still at t={t} declares {present or 'no'} "
                              "screen; exactly one of terminal/dialog/slide/desktop is required")
        step = item.get("step")
        if step is not None and step != previous and step in seen:
            raise RecipeError(f"recipe {rid}: step '{step}' labels two separate runs of stills")
        if step is not None:
            seen.add(step)
        previous = step
        screen, typing = _resolve(present[0], item[present[0]], terminal)
        stills.append(Still(t, screen, step, bool(item.get("continuous", False)), typing))
    return tuple(stills)


def _resolve(kind: str, spec: dict[str, Any], term: _TerminalState) -> tuple[Screen, Typing | None]:
    strs = lambda key: tuple(str(v) for v in spec.get(key, ()))  # noqa: E731
    if kind == "terminal":
        return term.apply(spec)
    if kind == "dialog":
        title = str(spec.get("title", ""))
        return Screen(kind, title, strs("rows"), strs("buttons"), spec.get("focus")), None
    if kind == "slide":
        return Screen(kind, str(spec.get("heading", "")), strs("bullets")), None
    return Screen(kind, status=spec.get("status")), None


def _build_narration(rid: str, raw: list[dict[str, Any]], duration: float) -> tuple[Cue, ...]:
    cues: list[Cue] = []
    for item in raw:
        t, text = float(item["t"]), str(item["text"])
        if cues and t <= cues[-1].t:
            raise RecipeError(f"recipe {rid}: narration cues must increase (at t={t})")
        if t >= duration or not text.strip():
            raise RecipeError(f"recipe {rid}: cue at t={t} needs text, and must start before end")
        cues.append(Cue(t, text))
    return tuple(cues)


def _window(lines: tuple[str, ...]) -> tuple[str, ...]:
    return lines[-TERMINAL_ROWS:] if len(lines) > TERMINAL_ROWS else lines


class _TerminalState:
    """The terminal a recipe edits still by still.

    Edits apply in the field order ``lines``, ``title``, ``prompt``,
    ``type``, ``output`` -- the Rust renderer's order, so one recipe
    yields the same screen in both systems.
    """

    def __init__(self, title: str) -> None:
        self.title = title
        self.prompt = DEFAULT_PROMPT
        self.lines: list[str] = [DEFAULT_PROMPT]

    def apply(self, op: dict[str, Any]) -> tuple[Screen, Typing | None]:
        if "lines" in op:
            self.lines = [str(line) for line in op["lines"]]
        if "title" in op:
            self.title = str(op["title"])
        if "prompt" in op:
            prompt = str(op["prompt"])
            if self.lines and self.lines[-1] == self.prompt:
                self.lines[-1] = prompt
            self.prompt = prompt
        typing: Typing | None = None
        if "type" in op:
            before = tuple(self._bare())
            typing = Typing(self.title, before, self.prompt, str(op["type"]))
            self.lines = [*before, self.prompt + str(op["type"])]
        if "output" in op:
            self.lines = [*self._bare(), *(str(o) for o in op["output"]), self.prompt]
        return Screen("terminal", self.title, _window(tuple(self.lines))), typing

    def _bare(self) -> list[str]:
        """The lines with a trailing bare prompt removed; typing replaces it."""
        return self.lines[:-1] if self.lines and self.lines[-1] == self.prompt else list(self.lines)
