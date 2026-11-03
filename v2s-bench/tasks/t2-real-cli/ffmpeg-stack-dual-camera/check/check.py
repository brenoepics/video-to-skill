#!/usr/bin/env python3
"""Checker for ffmpeg-stack-dual-camera.

Media is judged on decoded content, never on file size. The picture is
checked by decoding one frame to raw RGB and reading a pixel out of each
half; the sound is checked by decoding to 16-bit mono PCM and counting
sign changes, which separates the two sine tones by a factor of four.

Two assertions carry the task. `one-track-and-it-is-cam-right` catches the
plausible wrong procedure of letting ffmpeg pick the audio (or mapping the
first input's), which yields a file that plays perfectly and carries the
wrong microphone. `ends-with-the-shorter-input` catches omitting the
instruction to stop early, which yields a file one second longer with the
last second silent.

argv[1] work directory, argv[2] executed-command log.
"""

from __future__ import annotations

import json
import re
import struct
import subprocess
import sys
from pathlib import Path


def run(argv: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(argv, capture_output=True, timeout=180)


def probe(path: Path, entries: str, extra: list[str] | None = None) -> list[str]:
    argv = ["ffprobe", "-v", "error", *(extra or []), "-show_entries", entries,
            "-of", "default=noprint_wrappers=1:nokey=1", str(path)]
    result = run(argv)
    if result.returncode != 0:
        return []
    return [line for line in result.stdout.decode("utf-8", "replace").split() if line]


def streams(path: Path, kind: str) -> list[dict]:
    result = run(["ffprobe", "-v", "error", "-select_streams", kind,
                  "-show_streams", "-of", "json", str(path)])
    if result.returncode != 0:
        return []
    try:
        return json.loads(result.stdout.decode("utf-8", "replace")).get("streams", [])
    except ValueError:
        return []


def duration(path: Path) -> float | None:
    values = probe(path, "format=duration")
    try:
        return float(values[0])
    except (IndexError, ValueError):
        return None


def frame_rgb(path: Path, at: float, width: int, height: int) -> bytes | None:
    result = run(["ffmpeg", "-nostdin", "-v", "error", "-ss", "%.3f" % at, "-i", str(path),
                  "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
    if result.returncode != 0 or len(result.stdout) < width * height * 3:
        return None
    return result.stdout


def pixel(buf: bytes, width: int, x: int, y: int) -> tuple[int, int, int]:
    offset = (y * width + x) * 3
    return buf[offset], buf[offset + 1], buf[offset + 2]


def tone_hz(path: Path, stream: str = "0:a:0") -> float | None:
    """Dominant frequency of a mono mixdown, by zero-crossing rate."""
    result = run(["ffmpeg", "-nostdin", "-v", "error", "-i", str(path), "-map", stream,
                  "-f", "s16le", "-ac", "1", "-ar", "48000", "-"])
    if result.returncode != 0 or len(result.stdout) < 48000 * 2:
        return None
    raw = result.stdout
    count = len(raw) // 2
    samples = struct.unpack("<%dh" % count, raw[: count * 2])
    crossings = sum(1 for i in range(1, count) if (samples[i - 1] < 0) != (samples[i] < 0))
    seconds = count / 48000.0
    return crossings / seconds / 2.0


def is_red(rgb: tuple[int, int, int]) -> bool:
    r, g, b = rgb
    return r > 120 and r > 2 * g and r > 2 * b


def is_green(rgb: tuple[int, int, int]) -> bool:
    r, g, b = rgb
    return g > 100 and g > 2 * r and g > 2 * b


def main() -> int:
    work = Path(sys.argv[1])
    log_text = ""
    if len(sys.argv) > 2:
        try:
            log_text = Path(sys.argv[2]).read_text(encoding="utf-8", errors="replace")
        except OSError:
            log_text = ""

    results: list[dict] = []

    def add(ident: str, passed: bool, detail: str, required: bool = True) -> None:
        results.append({"id": ident, "passed": bool(passed), "required": required, "detail": detail})

    review = work / "review.mp4"
    left = work / "cam-left.mp4"
    right = work / "cam-right.mp4"

    video = streams(review, "v") if review.is_file() else []
    audio = streams(review, "a") if review.is_file() else []
    geometry_ok = (
        len(video) == 1
        and video[0].get("width") == 640
        and video[0].get("height") == 240
        and video[0].get("r_frame_rate") == "25/1"
    )
    add(
        "review-geometry",
        geometry_ok,
        "one 640x240 video stream at 25 fps"
        if geometry_ok
        else "review.mp4 missing or wrong geometry: %r"
        % ([(s.get("width"), s.get("height"), s.get("r_frame_rate")) for s in video],),
    )

    sides_detail = "review.mp4 has no decodable frame"
    sides_ok = False
    if geometry_ok:
        buf = frame_rgb(review, 1.0, 640, 240)
        if buf:
            left_px = pixel(buf, 640, 80, 120)
            right_px = pixel(buf, 640, 560, 120)
            sides_ok = is_red(left_px) and is_green(right_px)
            sides_detail = "left half %r, right half %r" % (left_px, right_px)
    add("cameras-on-the-right-sides", sides_ok, sides_detail)

    tone = tone_hz(review) if audio else None
    track_ok = len(audio) == 1 and tone is not None and 1000.0 < tone < 1450.0
    add(
        "one-track-and-it-is-cam-right",
        track_ok,
        "exactly one sound track, dominant tone %.0f Hz" % tone
        if track_ok and tone
        else "audio streams=%d dominant tone=%s Hz" % (len(audio), None if tone is None else round(tone)),
    )

    produced = duration(review)
    shorter = min(x for x in (duration(left), duration(right)) if x) if left.is_file() and right.is_file() else None
    length_ok = produced is not None and shorter is not None and abs(produced - shorter) <= 0.2
    add(
        "ends-with-the-shorter-input",
        length_ok,
        "runs %.3fs against the shorter input's %.3fs" % (produced or -1, shorter or -1),
    )

    src_ok = True
    notes = []
    for path, want_dur, want_tone in ((left, 4.0, 300.0), (right, 3.0, 1200.0)):
        dur = duration(path)
        vid = streams(path, "v")
        tone_here = tone_hz(path)
        good = (
            dur is not None
            and abs(dur - want_dur) <= 0.2
            and len(vid) == 1
            and vid[0].get("width") == 320
            and tone_here is not None
            and abs(tone_here - want_tone) <= 0.25 * want_tone
        )
        src_ok = src_ok and good
        notes.append("%s dur=%s tone=%s" % (path.name, None if dur is None else round(dur, 2),
                                            None if tone_here is None else round(tone_here)))
    add("sources-untouched", src_ok, "; ".join(notes))

    add(
        "stacked-through-a-filter-graph",
        re.search(r"(?mi)^\s*ffmpeg\b[^\n]*-filter_complex[^\n]*stack", log_text) is not None,
        "a stacking filter graph appears in the command log",
        required=False,
    )

    print(json.dumps({"assertions": results}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
