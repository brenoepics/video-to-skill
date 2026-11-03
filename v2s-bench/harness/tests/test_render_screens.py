"""Pixels are the product here, so these tests look at pixels.

Two things are being defended. First determinism: the same screen must
draw to the same bytes, every time, with no dependence on a font file or
a drawing library. Second legibility: the extractor OCRs these frames,
so a glyph pair that collapses into one shape (``1``/``l``, ``0``/``O``,
``g``/``9``, ``-``/``--``) is a functional bug, not a cosmetic one.
"""

from __future__ import annotations

from itertools import pairwise

import pytest

from v2sbench.render.recipe import Screen
from v2sbench.render.screens import Canvas, Style, draw

W, H = 1280, 720
TERMINAL = Screen(
    "terminal",
    "demo - bash",
    ("$ git init demo-repo", "Initialized empty Git repository", "$ "),
)
DIALOG = Screen(
    "dialog", "Repository Settings", ("Sign commits: on",), ("Cancel", "Save"), "Save"
)
SLIDE = Screen("slide", "Start a repository", ("Create it", "Stage it", "Commit it"))
DESKTOP = Screen("desktop", status="v2s-bench - idle")
ALL = (TERMINAL, DIALOG, SLIDE, DESKTOP)


def glyph_raster(char: str, scale: int = 3) -> bytes:
    """One character on a small canvas, for comparing glyph shapes."""
    canvas = Canvas(8 * scale, 9 * scale, (0, 0, 0))
    canvas.text(0, 0, scale, (255, 255, 255), char)
    return bytes(canvas.buf)


def colour_count(canvas: Canvas, colour: tuple[int, int, int]) -> int:
    return sum(
        canvas.pixel(x, y) == colour
        for y in range(0, canvas.height, 2)
        for x in range(0, canvas.width, 2)
    )


@pytest.mark.parametrize("screen", ALL, ids=lambda s: s.kind)
def test_every_screen_kind_draws_at_the_requested_size(screen: Screen) -> None:
    canvas = draw(screen, W, H)
    assert (canvas.width, canvas.height) == (W, H)
    assert len(canvas.buf) == W * H * 3


@pytest.mark.parametrize("screen", ALL, ids=lambda s: s.kind)
def test_drawing_is_deterministic(screen: Screen) -> None:
    assert bytes(draw(screen, W, H).buf) == bytes(draw(screen, W, H).buf)
    assert draw(screen, W, H).to_png() == draw(screen, W, H).to_png()


def test_an_unknown_screen_kind_is_an_error_not_a_blank_frame() -> None:
    with pytest.raises(ValueError, match="unknown screen kind"):
        draw(Screen("hologram"), W, H)


@pytest.mark.parametrize(("left", "right"), [("1", "l"), ("1", "I"), ("l", "I"), ("0", "O"),
                                             ("g", "9"), ("-", "_"), ("|", "l")])
def test_confusable_characters_draw_as_different_shapes(left: str, right: str) -> None:
    assert glyph_raster(left) != glyph_raster(right)


def test_a_double_dash_shows_two_dashes_and_not_one_bar() -> None:
    scale = 3
    canvas = Canvas(20 * scale, 9 * scale, (0, 0, 0))
    canvas.text(0, 0, scale, (255, 255, 255), "--")
    lit = [x for x in range(canvas.width) if canvas.pixel(x, 3 * scale) == (255, 255, 255)]
    gaps = [b - a for a, b in pairwise(lit) if b - a > 1]
    assert len(gaps) == 1, "the two dashes must be separated by blank columns"
    assert gaps[0] >= 3 * scale, "and the gap must be wide enough to see"


def test_glyphs_are_tall_enough_at_720p_to_be_read() -> None:
    style = Style.for_height(H)
    assert style.scale == 3
    assert 7 * style.scale >= 21  # cap height in pixels
    assert (W // style.cell) >= 68  # columns, close to a real 720p terminal


def test_the_terminal_shows_a_tinted_prompt_a_command_and_a_cursor() -> None:
    style = Style.for_height(H)
    canvas = draw(TERMINAL, W, H, style)
    assert colour_count(canvas, style.accent) > 0, "the prompt is drawn in the accent colour"
    assert colour_count(canvas, style.term_fg) > 0, "the command is drawn in the foreground"
    assert colour_count(canvas, style.chrome) > 0, "there is a title bar"
    assert colour_count(canvas, style.term_dim) > 0, "the block cursor is drawn"


def test_the_terminal_changes_when_the_command_changes() -> None:
    other = Screen("terminal", TERMINAL.title, ("$ git status", *TERMINAL.lines[1:]))
    assert bytes(draw(TERMINAL, W, H).buf) != bytes(draw(other, W, H).buf)


def test_an_over_long_line_stays_inside_the_window() -> None:
    style = Style.for_height(H)
    wide = Screen("terminal", "demo - bash", ("x" * 400,))
    canvas = draw(wide, W, H)
    assert canvas.pixel(W - 2, H // 2) == style.desk_bg, "nothing is drawn past the window edge"


def test_the_dialog_marks_its_focus_and_puts_a_cursor_on_it() -> None:
    style = Style.for_height(H)
    canvas = draw(DIALOG, W, H, style)
    assert colour_count(canvas, style.ring) > 0, "the focused control has a focus ring"
    assert colour_count(canvas, style.focus) > 0, "the focused button is filled"
    pointer = [
        (x, y)
        for y in range(H // 2, H)
        for x in range(W // 2, W)
        if canvas.pixel(x, y) == style.slide_fg
    ]
    assert pointer, "a mouse pointer is drawn below and right of centre"


def test_a_dialog_without_focus_still_draws_a_cursor() -> None:
    style = Style.for_height(H)
    canvas = draw(Screen("dialog", "Plain", ("a row",), ("Ok",)), W, H, style)
    assert colour_count(canvas, style.slide_fg) > 0


def test_the_slide_marks_one_bullet_per_line() -> None:
    style = Style.for_height(H)
    canvas = draw(SLIDE, W, H, style)
    left = 6 * style.cell
    marked = {y for y in range(H) if canvas.pixel(left + 1, y) == style.accent}
    runs = sum(1 for y in sorted(marked) if y - 1 not in marked)
    assert runs == len(SLIDE.lines)


def test_the_desktop_is_plain_above_its_status_bar() -> None:
    style = Style.for_height(H)
    canvas = draw(DESKTOP, W, H, style)
    assert {canvas.pixel(x, H // 2) for x in range(0, W, 7)} == {style.desk_bg}
    assert colour_count(canvas, style.chrome_fg) > 0, "the status line is drawn"


def test_the_png_export_is_a_real_png() -> None:
    png = draw(DESKTOP, W, H).to_png()
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    assert png[12:16] == b"IHDR" and png.rstrip().endswith(b"IEND\xaeB`\x82")


def test_style_scales_with_the_frame_height() -> None:
    assert Style.for_height(720).scale == 3
    assert Style.for_height(1080).scale == 4
    assert Style.for_height(1080).line > Style.for_height(720).line
