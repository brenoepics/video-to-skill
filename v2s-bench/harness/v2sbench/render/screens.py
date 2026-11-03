"""Draw a resolved screen into an RGB raster, with no third-party code.

Why no Pillow or FreeType here: FreeType rasterisation depends on the
library version, the hinting mode and the font file, so the same recipe
would render different pixels on two machines and the preregistration's
"byte-identical output on every machine with the same ffmpeg" claim would
be false. The font is instead *bundled as data* -- the parent project's
5x7 bitmap font (``crates/extractor/src/eval/synth/font.rs``), upscaled
by an integer factor -- so every lit pixel follows from arithmetic alone.

Four glyphs are redrawn for OCR's sake. ``-`` is three columns wide, not
five, so ``--flag`` reads as two dashes rather than one bar; lowercase
``l`` loses its flag and gains a foot, so ``1``, ``l`` and ``I`` are
three shapes; and ``g`` gets a left tail while ``9`` gets a straight
stem, because in the original font ``cargo`` renders as ``car9o``.
``0`` keeps the slashed-zero diagonal that tells it from ``O``.
"""

from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass

from .recipe import Screen

GLYPH_W, GLYPH_H = 5, 7
_FONT_HEX = (
    "00000000000000040404040400040a0a0a000000000a0a1f0a1f0a0a040f140e051e04181902040813030c1214"
    "0815120d0c04080000000002040808080402080402020204080004150e1504000004041f040400000000000c04"
    "080000000e00000000000000000c0c000102040810000e11131519110e040c040404040e0e11010204081f1f02"
    "040201110e02060a121f02021f101e0101110e0608101e11110e1f0102040808080e11110e11110e0e11110f01"
    "0101000c0c000c0c00000c0c000c04080204081008040200001f001f0000080402010204080e1101020400040e"
    "11010d15150e0e11111f1111111e11111e11111e0e11101010110e1c12111111121c1f10101e10101f1f10101e"
    "1010100e11101711110f1111111f1111110e04040404040e0702020202120c111214181412111010101010101f"
    "111b1515111111111119151311110e11111111110e1e11111e1010100e11111115120d1e11111e1412110f1010"
    "0e01011e1f0404040404041111111111110e11111111110a041111111515150a11110a040a11111111110a0404"
    "041f01020408101f0e08080808080e001008040201000e02020202020e040a11000000000000000000001f0804"
    "020000000000000e010f110f1010161911111e00000e1010110e01010d1311110f00000e111f100e0609081c08"
    "080800000f110f011e1010161911111104000c0404040e0200060202120c101012141814120404040404040600"
    "001a151511110000161911111100000e1111110e00001e111e101000000d130f01010000161910101000000e10"
    "0e011e08081c080809060000111111130d00001111110a040000111115150a0000110a040a11000011110f010e"
    "00001f0204081f02040408040402040404040404040804040204040800000815020000"
)
_GLYPHS: dict[str, tuple[int, ...]] = {
    chr(32 + i): tuple(int(_FONT_HEX[i * 14 + j * 2 : i * 14 + j * 2 + 2], 16) for j in range(7))
    for i in range(95)
}
_UNKNOWN = (0x1F,) * GLYPH_H  # a filled box, like the Rust renderer

Rgb = tuple[int, int, int]


@dataclass(frozen=True)
class Style:
    """Flat, high-contrast colours chosen for OCR rather than for beauty.

    ``scale`` upscales the 5x7 font: 3 at 720p (15x21 px glyphs, 71
    columns, about what a real 720p terminal shows) and 4 at 1080p.
    """

    scale: int = 3
    term_bg: Rgb = (24, 26, 32)
    term_fg: Rgb = (226, 230, 236)
    term_dim: Rgb = (142, 150, 164)
    accent: Rgb = (122, 212, 128)
    chrome: Rgb = (54, 58, 70)
    chrome_fg: Rgb = (214, 218, 226)
    desk_bg: Rgb = (38, 46, 62)
    panel: Rgb = (238, 240, 245)
    panel_fg: Rgb = (26, 28, 34)
    button: Rgb = (206, 212, 222)
    focus: Rgb = (56, 118, 208)
    ring: Rgb = (250, 190, 64)
    slide_bg: Rgb = (18, 28, 46)
    slide_fg: Rgb = (246, 248, 252)
    edge: Rgb = (14, 15, 18)

    @staticmethod
    def for_height(height: int) -> Style:
        return Style(scale=max(2, height // 240))

    @property
    def cell(self) -> int:
        """Character advance in pixels: five glyph columns plus a gap."""
        return (GLYPH_W + 1) * self.scale

    @property
    def line(self) -> int:
        """Line height in pixels, with breathing room under the glyph."""
        return (GLYPH_H + 2) * self.scale


class Canvas:
    """An RGB24 raster with fills, rectangles and bitmap text."""

    def __init__(self, width: int, height: int, background: Rgb) -> None:
        self.width, self.height = width, height
        self.buf = bytearray(bytes(background) * (width * height))

    def rect(self, x: int, y: int, w: int, h: int, color: Rgb) -> None:
        """Fill a rectangle, clipped to the canvas."""
        x0, y0 = max(0, x), max(0, y)
        x1, y1 = min(self.width, x + w), min(self.height, y + h)
        if x1 <= x0 or y1 <= y0:
            return
        row = bytes(color) * (x1 - x0)
        for py in range(y0, y1):
            at = (py * self.width + x0) * 3
            self.buf[at : at + len(row)] = row

    def text(self, x: int, y: int, scale: int, color: Rgb, text: str) -> None:
        """Draw ``text`` at (x, y); each run of lit pixels is one slice write."""
        for index, char in enumerate(text):
            ox = x + (GLYPH_W + 1) * scale * index
            for row, bits in enumerate(_GLYPHS.get(char, _UNKNOWN)):
                col = 0
                while col < GLYPH_W:
                    if bits & (0x10 >> col):
                        run = 1
                        while col + run < GLYPH_W and bits & (0x10 >> (col + run)):
                            run += 1
                        self.rect(ox + col * scale, y + row * scale, run * scale, scale, color)
                        col += run
                    else:
                        col += 1

    @staticmethod
    def text_width(text: str, scale: int) -> int:
        return len(text) * (GLYPH_W + 1) * scale

    def to_png(self) -> bytes:
        """A PNG, for inspecting a frame by eye. Not used for the video."""
        stride = self.width * 3
        raw = b"".join(
            b"\x00" + bytes(self.buf[y * stride : (y + 1) * stride]) for y in range(self.height)
        )
        head = struct.pack(">2I5B", self.width, self.height, 8, 2, 0, 0, 0)
        out = [b"\x89PNG\r\n\x1a\n"]
        for tag, payload in ((b"IHDR", head), (b"IDAT", zlib.compress(raw, 9)), (b"IEND", b"")):
            out.append(struct.pack(">I", len(payload)) + tag + payload)
            out.append(struct.pack(">I", zlib.crc32(tag + payload)))
        return b"".join(out)

    def pixel(self, x: int, y: int) -> Rgb:
        at = (y * self.width + x) * 3
        return (self.buf[at], self.buf[at + 1], self.buf[at + 2])


def draw(screen: Screen, width: int, height: int, style: Style | None = None) -> Canvas:
    """Render one resolved screen. The only entry point tests need."""
    style = style or Style.for_height(height)
    if screen.kind == "terminal":
        return _terminal(screen, width, height, style)
    if screen.kind == "dialog":
        return _dialog(screen, width, height, style)
    if screen.kind == "slide":
        return _slide(screen, width, height, style)
    if screen.kind == "desktop":
        return _desktop(screen, width, height, style)
    raise ValueError(f"unknown screen kind {screen.kind!r}")


def _window(
    canvas: Canvas, style: Style, title: str, body: Rgb, rect: tuple[int, int, int, int] | None
) -> tuple[int, int, int, int]:
    """Border, title bar with three dots, body; returns the body rect."""
    s = style.scale
    m = 4 * s
    x, y, w, h = rect or (m, m, canvas.width - 2 * m, canvas.height - 2 * m)
    bar = style.line + 2 * s
    canvas.rect(x - s, y - s, w + 2 * s, h + 2 * s, style.edge)
    canvas.rect(x, y, w, bar, style.chrome)
    canvas.rect(x, y + bar, w, h - bar, body)
    for i in range(3):
        canvas.rect(x + (2 + i * 3) * s, y + 4 * s, 2 * s, 2 * s, style.chrome_fg)
    canvas.text(x + 14 * s, y + 2 * s, s, style.chrome_fg, title)
    return x, y + bar, w, h - bar


def _split_prompt(line: str) -> tuple[str, str]:
    """Prompt and remainder. Tinting the prompt is not decoration: it is
    how a reader tells a typed command from program output."""
    at = line.find("$ ")
    if 0 <= at <= 24:
        return line[: at + 2], line[at + 2 :]
    return "", line


def _terminal(screen: Screen, width: int, height: int, style: Style) -> Canvas:
    canvas = Canvas(width, height, style.desk_bg)
    x, y, w, _ = _window(canvas, style, screen.title, style.term_bg, None)
    pad = 2 * style.scale
    columns = max(1, (w - 2 * pad) // style.cell)
    last = len(screen.lines) - 1
    for index, raw in enumerate(screen.lines):
        line = raw[:columns]
        top = y + pad + index * style.line
        prompt, rest = _split_prompt(line)
        canvas.text(x + pad, top, style.scale, style.accent, prompt)
        canvas.text(x + pad + Canvas.text_width(prompt, style.scale), top, style.scale,
                    style.term_fg, rest)
        if index == last:
            canvas.rect(x + pad + Canvas.text_width(line, style.scale), top,
                        GLYPH_W * style.scale, (GLYPH_H + 1) * style.scale, style.term_dim)
    return canvas


def _dialog(screen: Screen, width: int, height: int, style: Style) -> Canvas:
    canvas = _desktop(Screen("desktop", status=screen.title), width, height, style)
    s, pad = style.scale, 4 * style.scale
    rows, step = screen.lines, style.line + 3 * style.scale
    longest = max([screen.title, *rows, "".join(screen.buttons)], key=len)
    dw = min(width - 8 * style.cell, max(width // 2, Canvas.text_width(longest, s) + 18 * s))
    dh = style.line + 2 * s + 2 * pad + len(rows) * step + style.line + 6 * s
    x, y, w, h = _window(canvas, style, screen.title, style.panel,
                         ((width - dw) // 2, (height - dh) // 2, dw, dh))
    focus: tuple[int, int] | None = None
    for index, row in enumerate(rows):
        top = y + pad + index * step
        if row == screen.focus:
            canvas.rect(x + pad - s, top - s, w - 2 * pad + 2 * s, style.line + s, style.ring)
            focus = (x + w - pad - 4 * s, top + s)
        canvas.text(x + pad, top, s, style.panel_fg, row)
    bx, by = x + w - pad, y + h - style.line - 4 * s
    for label in reversed(screen.buttons):
        bw = Canvas.text_width(label, s) + 4 * s
        bx -= bw + 3 * s
        chosen = label == screen.focus
        canvas.rect(bx - s, by - s, bw + 2 * s, style.line + 3 * s,
                    style.ring if chosen else style.edge)
        canvas.rect(bx, by, bw, style.line + s, style.focus if chosen else style.button)
        canvas.text(bx + 2 * s, by + s, s, style.slide_fg if chosen else style.panel_fg, label)
        if chosen:
            focus = (bx + bw - 3 * s, by + style.line // 2)
    _cursor(canvas, style, *(focus or (x + w // 2, y + h // 2)))
    return canvas


def _cursor(canvas: Canvas, style: Style, x: int, y: int) -> None:
    """A pointer at (x, y): light arrow, dark outline. The parent project
    extracts evidence around the cursor, so it must be *findable* --
    hence the hard outline, no anti-aliasing and no jitter."""
    s = style.scale
    for i in range(17):
        canvas.rect(x, y + i * s, (max(1, i * 7 // 10) + 2) * s, s, style.edge)
    for i in range(1, 15):
        canvas.rect(x + s, y + i * s, max(1, i * 7 // 10) * s, s, style.slide_fg)


def _slide(screen: Screen, width: int, height: int, style: Style) -> Canvas:
    canvas = Canvas(width, height, style.slide_bg)
    big = style.scale * 2
    left = 6 * style.cell
    canvas.text(left, 4 * style.line, big, style.slide_fg, screen.title)
    canvas.rect(left, 4 * style.line + (GLYPH_H + 3) * big, Canvas.text_width(screen.title, big),
                style.scale, style.focus)
    top = 4 * style.line + (GLYPH_H + 6) * big
    for index, bullet in enumerate(screen.lines):
        row = top + index * (style.line + style.scale) * 2
        canvas.rect(left, row + 2 * style.scale, 3 * style.scale, 3 * style.scale, style.accent)
        canvas.text(left + 2 * style.cell, row, style.scale + 1, style.slide_fg, bullet)
    return canvas


def _desktop(screen: Screen, width: int, height: int, style: Style) -> Canvas:
    canvas = Canvas(width, height, style.desk_bg)
    bar = style.line + 4 * style.scale
    canvas.rect(0, height - bar, width, bar, style.chrome)
    canvas.text(2 * style.cell, height - bar + (bar - GLYPH_H * style.scale) // 2, style.scale,
                style.chrome_fg, screen.status or "")
    canvas.rect(0, 0, width, style.scale, style.edge)
    return canvas
