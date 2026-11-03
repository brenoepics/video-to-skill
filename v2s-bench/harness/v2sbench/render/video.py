"""Turn a recipe into a byte-reproducible mp4, and check what came out.

Every knob that could leak the wall clock or the machine into the file is
pinned here: frames go in as raw RGB24 at a constant rate (no timestamp
guessing), ``+bitexact`` on the muxer and both encoders drops version
strings and ``creation_time``, a bitstream filter drops x264's version
SEI, ``-map_metadata -1`` drops what the inputs carried, ``-threads 1``
pins x264's slice layout (which otherwise follows the core count), and
``-sws_flags +bitexact`` pins the RGB-to-YUV conversion.

What is *not* pinned is the ffmpeg build: a different libx264 produces a
different file, so the result records the version string that made it.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from . import narration
from .recipe import Recipe, load_recipe
from .screens import Canvas, Style, draw

# How long a typed command takes to appear: short, but long enough that a
# shot detector sees a gradual change rather than one clean cut.
DEFAULT_TYPING_SECS = 0.6


class RenderError(RuntimeError):
    """ffmpeg, ffprobe or the output file did not do what was asked."""


@dataclass(frozen=True)
class Segment:
    """One run of identical frames: a still, or one keystroke of typing."""

    start_frame: int
    frames: int
    still: int
    chars: int | None = None
    """Characters of the command typed so far; None is the settled screen."""
    change: bool = False
    """Whether this segment starts a gold visual change."""


@dataclass(frozen=True)
class RenderPlan:
    recipe_id: str
    width: int
    height: int
    fps: int
    duration_secs: float
    frame_count: int
    segments: tuple[Segment, ...]
    warnings: tuple[str, ...] = ()

    @property
    def change_frames(self) -> tuple[int, ...]:
        return tuple(s.start_frame for s in self.segments if s.change)


@dataclass(frozen=True)
class RenderResult:
    path: Path
    sha256: str
    duration_secs: float
    fps: int
    frame_count: int
    width: int
    height: int
    narrated: bool
    narration_backend: str | None
    narration_cues: int
    ffmpeg_version: str
    recipe_id: str
    warnings: tuple[str, ...] = field(default=())


def plan(
    recipe: Recipe, *, typing: bool = True, typing_secs: float = DEFAULT_TYPING_SECS
) -> RenderPlan:
    """Work out every frame of the video without drawing any of them.

    Each still's ``t`` is rounded to the nearest frame, so a still never
    drifts more than half a frame from the time the gold label claims.
    """
    fps, total = recipe.fps, recipe.frame_count
    starts = [min(total, round(still.t * fps)) for still in recipe.stills]
    ends = [*starts[1:], total]
    segments: list[Segment] = []
    warnings: list[str] = []
    for index, (still, start, end) in enumerate(zip(recipe.stills, starts, ends, strict=True)):
        span = end - start
        if span <= 0:
            warnings.append(f"still at t={still.t} is shorter than one frame and is dropped")
            continue
        change = index > 0 and not still.continuous
        typed = still.typing.typed if (typing and still.typing) else ""
        stages = min(len(typed), max(1, round(typing_secs * fps)), span - 1) if typed else 0
        if typed and span < round((typing_secs + 1.0) * fps):
            warnings.append(
                f"still at t={still.t} types {len(typed)} characters but holds for only "
                f"{span / fps:.1f}s; a 1 fps sampler may only ever see a half-typed command"
            )
        for stage in range(stages):
            chars = len(typed) * (stage + 1) // stages
            segments.append(Segment(start + stage, 1, index, chars, change and stage == 0))
        segments.append(
            Segment(start + stages, span - stages, index, None, change and stages == 0)
        )
    return RenderPlan(
        recipe.id, recipe.width, recipe.height, fps, recipe.duration_secs, total,
        tuple(segments), tuple(warnings),
    )


def render(
    recipe_path: str | Path,
    out_path: str | Path,
    *,
    tts: str = "auto",
    quiet: bool = False,
    typing: bool = True,
    typing_secs: float = DEFAULT_TYPING_SECS,
    ffmpeg: str = "ffmpeg",
    require_narration: bool = False,
) -> RenderResult:
    """Render ``recipe_path`` to ``out_path`` and return what was produced."""
    recipe = load_recipe(recipe_path)
    schedule = plan(recipe, typing=typing, typing_secs=typing_secs)
    out = Path(out_path)
    out.parent.mkdir(parents=True, exist_ok=True)
    backend = narration.detect_backend(tts)
    notes = list(schedule.warnings)
    with tempfile.TemporaryDirectory(prefix="v2sbench-render-") as tmp:
        work = Path(tmp)
        track = narration.synthesise(recipe.narration, recipe.duration_secs, work,
                                     backend=backend, ffmpeg=ffmpeg)
        if track.overlaps or track.truncated:
            notes.append(f"{track.overlaps} narration cue(s) run into the next and "
                         f"{track.truncated} past the end of the video")
        for note in notes:
            _say(f"v2sbench.render: {recipe.id}: {note}")
        if not track.available:
            _say(f"v2sbench.render: {recipe.id}: NO NARRATION -- rendering a SILENT video. "
                 "Transcript-derived arms are not scoreable on this file.")
            if require_narration:
                raise RenderError(f"{recipe.id}: narration required but no TTS backend is usable")
        elif not quiet:
            _say(f"v2sbench.render: {recipe.id}: narrated by {track.backend}, {track.cues} cues")
        _encode(recipe, schedule, out, track.wav, work, ffmpeg=ffmpeg)
    digest = hashlib.sha256(out.read_bytes()).hexdigest()
    if not quiet:
        _say(f"v2sbench.render: {recipe.id}: {out} sha256={digest[:16]}...")
    return RenderResult(
        path=out, sha256=digest, duration_secs=schedule.frame_count / recipe.fps,
        fps=recipe.fps, frame_count=schedule.frame_count,
        width=recipe.width, height=recipe.height,
        narrated=track.available, narration_cues=track.cues,
        narration_backend=track.backend if track.available else None,
        ffmpeg_version=ffmpeg_version(ffmpeg), recipe_id=recipe.id,
        warnings=tuple(notes),
    )


def frames(recipe: Recipe, schedule: RenderPlan) -> list[tuple[Canvas, int]]:
    """Each segment's canvas with the number of frames it is held for."""
    style = Style.for_height(recipe.height)
    out: list[tuple[Canvas, int]] = []
    for segment in schedule.segments:
        still = recipe.stills[segment.still]
        screen = still.screen if segment.chars is None else still.typing.at(segment.chars)
        out.append((draw(screen, recipe.width, recipe.height, style), segment.frames))
    return out


def _encode(
    recipe: Recipe, schedule: RenderPlan, out: Path, wav: Path | None, work: Path, *, ffmpeg: str
) -> None:
    argv = [ffmpeg, "-hide_banner", "-nostdin", "-v", "error", "-y", "-fflags", "+bitexact",
            "-f", "rawvideo", "-pix_fmt", "rgb24",
            "-s", f"{recipe.width}x{recipe.height}", "-r", str(recipe.fps), "-i", "pipe:0"]
    if wav is not None:
        argv += ["-i", str(wav), "-map", "0:v", "-map", "1:a",
                 "-c:a", "aac", "-b:a", "64k", "-ar", str(narration.SAMPLE_RATE),
                 "-ac", "1", "-flags:a", "+bitexact", "-shortest"]
    else:
        argv += ["-map", "0:v"]
    argv += ["-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
             "-g", str(recipe.fps * 2), "-threads", "1", "-flags:v", "+bitexact",
             "-sws_flags", "+bitexact+accurate_rnd", "-fps_mode", "cfr", "-r", str(recipe.fps),
             "-video_track_timescale", str(recipe.fps * 1000),
             # NAL type 6 is the SEI into which x264 writes its version and
             # option list, which +bitexact does not suppress; dropping it
             # leaves the decoded pixels untouched (checked with framemd5).
             "-bsf:v", "filter_units=remove_types=6",
             # Only as an *output* option does this reach the muxer, which is
             # what strips the "Lavf<version>" container tag.
             "-fflags", "+bitexact",
             "-map_metadata", "-1", "-map_chapters", "-1", str(out)]
    log = work / "ffmpeg.log"
    with log.open("wb") as errors:
        proc = subprocess.Popen(argv, stdin=subprocess.PIPE, stderr=errors)
        assert proc.stdin is not None
        try:
            for canvas, count in frames(recipe, schedule):
                proc.stdin.write(bytes(canvas.buf) * count)
        except BrokenPipeError:  # ffmpeg died; its log says why
            pass
        finally:
            proc.stdin.close()
        code = proc.wait()
    if code != 0 or not out.is_file():
        raise RenderError(f"ffmpeg exited {code} rendering {recipe.id}: {log.read_text()[:400]}")


def verify(
    out_path: str | Path,
    recipe: Recipe,
    *,
    expect_audio: bool | None = None,
    ffprobe: str = "ffprobe",
) -> dict[str, Any]:
    """Probe the rendered file and insist it matches the recipe: a dropped
    frame, a variable frame rate or a silently missing audio track would
    each quietly change what the benchmark measures."""
    out = Path(out_path)
    probed = _ffprobe(out, ffprobe)
    video = next((s for s in probed["streams"] if s["codec_type"] == "video"), None)
    audio = next((s for s in probed["streams"] if s["codec_type"] == "audio"), None)
    if video is None:
        raise RenderError(f"{out} has no video stream")
    facts: dict[str, Any] = {
        "width": int(video["width"]), "height": int(video["height"]),
        "fps": video["r_frame_rate"], "frames": int(video.get("nb_frames", 0)),
        "duration_secs": float(probed["format"]["duration"]), "has_audio": audio is not None,
        "audio_codec": audio["codec_name"] if audio else None,
        "audio_rate": int(audio["sample_rate"]) if audio else None,
    }
    frame, expected = 1.0 / recipe.fps, recipe.frame_count / recipe.fps
    problems: list[str] = []
    size = (facts["width"], facts["height"])
    if size != (recipe.width, recipe.height):
        problems.append(f"size {size} != ({recipe.width}, {recipe.height})")
    if facts["fps"] != f"{recipe.fps}/1":
        problems.append(f"frame rate {facts['fps']} != {recipe.fps}/1")
    if facts["frames"] != recipe.frame_count:
        problems.append(f"{facts['frames']} frames != {recipe.frame_count}")
    if abs(facts["duration_secs"] - expected) > frame:
        problems.append(f"duration {facts['duration_secs']:.3f}s != {expected:.3f}s (+/-1 frame)")
    if expect_audio is not None and facts["has_audio"] != expect_audio:
        problems.append(f"audio present={facts['has_audio']}, expected {expect_audio}")
    if problems:
        raise RenderError(f"{out} does not match recipe {recipe.id}: " + "; ".join(problems))
    return facts


def save_png(canvas: Canvas, path: str | Path) -> str:
    """Write one frame out for inspection; returns the writer it used.

    Pillow is used when installed (the ``render`` extra), the standard
    library otherwise. Neither writer ever touches the video.
    """
    try:
        from PIL import Image
    except ImportError:
        Path(path).write_bytes(canvas.to_png())
        return "stdlib"
    Image.frombytes("RGB", (canvas.width, canvas.height), bytes(canvas.buf)).save(path)
    return "pillow"


def ffmpeg_version(ffmpeg: str = "ffmpeg") -> str:
    """The first line of ``ffmpeg -version``: the one thing we cannot pin."""
    done = subprocess.run([ffmpeg, "-version"], capture_output=True, check=False, text=True)
    if done.returncode != 0:
        raise RenderError(f"{ffmpeg} is not runnable: {done.stderr[:200]}")
    return done.stdout.splitlines()[0].strip()


def _ffprobe(path: Path, ffprobe: str) -> dict[str, Any]:
    done = subprocess.run(
        [ffprobe, "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)],
        capture_output=True, check=False, text=True,
    )
    if done.returncode != 0:
        raise RenderError(f"ffprobe failed on {path}: {done.stderr[:200]}")
    return json.loads(done.stdout)


def _say(message: str) -> None:
    """To stderr even when quiet: a silent video has to be loud."""
    print(message, file=sys.stderr, flush=True)
