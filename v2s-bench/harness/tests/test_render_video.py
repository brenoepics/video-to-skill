"""End-to-end: the plan, the mp4, and the promise that it is reproducible.

The determinism test is the important one. If it ever fails, the whole
hermetic tier stops being hermetic, and the benchmark would be comparing
runs over subtly different inputs without knowing it.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
from pathlib import Path
from typing import Any

import pytest

from v2sbench.render import (
    RenderError,
    detect_backend,
    frames,
    load_recipe,
    plan,
    render,
    save_png,
    verify,
)
from v2sbench.render.screens import draw

FIXTURE = Path(__file__).parent / "fixtures" / "recipe_sample.json"
needs_ffmpeg = pytest.mark.skipif(shutil.which("ffmpeg") is None, reason="ffmpeg is not installed")


@pytest.fixture(scope="module")
def tiny(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """A 3 s recipe: every code path the fixture exercises, a seventh of
    the frames. The schema fixes the frame size to 1280x720 or 1920x1080,
    so the saving has to come out of the duration."""
    data: dict[str, Any] = {
        "schema_version": 1,
        "id": "tiny-demo",
        "title": "Tiny demo",
        "width": 1280,
        "height": 720,
        "fps": 10,
        "duration_secs": 3.0,
        "stills": [
            {"t": 0.0, "slide": {"heading": "Tiny", "bullets": ["one"]}},
            {"t": 1.0, "step": "01-init", "terminal": {"type": "git init"}},
            {"t": 2.0, "step": "01-init", "terminal": {"output": ["Initialized"]}},
        ],
        "narration": [{"t": 0.2, "text": "A very short demo."}],
    }
    path = tmp_path_factory.mktemp("recipes") / "tiny.json"
    path.write_text(json.dumps(data), encoding="utf-8")
    return path


def test_the_plan_puts_every_still_on_its_own_frame() -> None:
    recipe = load_recipe(FIXTURE)
    schedule = plan(recipe)
    assert schedule.frame_count == recipe.frame_count
    assert sum(s.frames for s in schedule.segments) == recipe.frame_count
    assert schedule.change_frames == tuple(
        round(t * recipe.fps) for t in recipe.change_times()
    )
    starts = [s.start_frame for s in schedule.segments]
    assert starts == sorted(starts) and starts[0] == 0


def test_typing_spreads_a_command_over_several_frames_and_can_be_switched_off() -> None:
    recipe = load_recipe(FIXTURE)
    animated = plan(recipe, typing=True)
    instant = plan(recipe, typing=False)
    assert len(animated.segments) > len(instant.segments)
    assert len(instant.segments) == len(recipe.stills)
    typed = [s for s in animated.segments if s.chars is not None]
    assert typed, "the fixture types commands"
    assert all(s.frames == 1 for s in typed), "one frame per keystroke batch"
    first = next(s for s in typed)
    assert 0 < first.chars < len(recipe.stills[first.still].typing.typed)


def test_the_plan_warns_when_a_typed_still_is_too_short_to_be_sampled() -> None:
    recipe = load_recipe(FIXTURE)
    schedule = plan(recipe, typing_secs=4.0)
    assert any("half-typed" in w for w in schedule.warnings)


def test_frames_cover_the_whole_video_and_match_the_screens() -> None:
    recipe = load_recipe(FIXTURE)
    schedule = plan(recipe)
    produced = frames(recipe, schedule)
    assert sum(count for _, count in produced) == recipe.frame_count
    first_canvas = produced[0][0]
    assert bytes(first_canvas.buf) == bytes(
        draw(recipe.stills[0].screen, recipe.width, recipe.height).buf
    )


@needs_ffmpeg
def test_two_renders_of_one_recipe_are_byte_identical(tiny: Path, tmp_path: Path) -> None:
    first = render(tiny, tmp_path / "a.mp4", quiet=True)
    second = render(tiny, tmp_path / "b.mp4", quiet=True)
    assert first.sha256 == second.sha256
    assert (tmp_path / "a.mp4").read_bytes() == (tmp_path / "b.mp4").read_bytes()
    assert first.sha256 == hashlib.sha256((tmp_path / "a.mp4").read_bytes()).hexdigest()


@needs_ffmpeg
def test_the_rendered_file_carries_no_creation_time_or_encoder_version(
    tiny: Path, tmp_path: Path
) -> None:
    out = render(tiny, tmp_path / "meta.mp4", quiet=True).path
    probed = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format_tags:stream_tags", "-of", "json",
         str(out)],
        capture_output=True, text=True, check=True,
    ).stdout
    assert "creation_time" not in probed
    assert "Lavf" not in probed, "the libavformat version must not be in the container"
    assert b"x264 - core" not in out.read_bytes(), "x264's version SEI must be stripped"


@needs_ffmpeg
def test_the_fixture_renders_to_something_ffprobe_agrees_with(tmp_path: Path) -> None:
    recipe = load_recipe(FIXTURE)
    result = render(FIXTURE, tmp_path / "fixture.mp4", quiet=True)
    facts = verify(result.path, recipe, expect_audio=result.narrated)
    assert facts["width"] == recipe.width and facts["height"] == recipe.height
    assert facts["fps"] == f"{recipe.fps}/1"
    assert facts["frames"] == recipe.frame_count == result.frame_count
    assert abs(facts["duration_secs"] - recipe.duration_secs) <= 1 / recipe.fps
    assert result.ffmpeg_version.startswith("ffmpeg version")
    assert result.recipe_id == recipe.id


@needs_ffmpeg
def test_verify_complains_when_the_file_is_not_what_was_expected(
    tiny: Path, tmp_path: Path
) -> None:
    result = render(tiny, tmp_path / "v.mp4", quiet=True)
    recipe = load_recipe(tiny)
    with pytest.raises(RenderError, match="audio"):
        verify(result.path, recipe, expect_audio=not result.narrated)
    with pytest.raises(RenderError, match=r"no video stream|does not match|ffprobe failed"):
        verify(tmp_path / "missing.mp4", recipe)


@needs_ffmpeg
def test_a_silent_render_is_marked_silent_rather_than_pretending(
    tiny: Path, tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    result = render(tiny, tmp_path / "silent.mp4", tts="none", quiet=True)
    assert result.narrated is False and result.narration_backend is None
    assert "NO NARRATION" in capsys.readouterr().err
    facts = verify(result.path, load_recipe(tiny), expect_audio=False)
    assert facts["has_audio"] is False


@needs_ffmpeg
def test_narration_can_be_demanded_and_the_render_refuses_without_it(
    tiny: Path, tmp_path: Path
) -> None:
    with pytest.raises(RenderError, match="narration required"):
        render(tiny, tmp_path / "x.mp4", tts="none", require_narration=True, quiet=True)


@needs_ffmpeg
@pytest.mark.skipif(detect_backend() is None, reason="no TTS backend on this machine")
def test_narration_is_mixed_in_at_whispers_native_rate(tiny: Path, tmp_path: Path) -> None:
    result = render(tiny, tmp_path / "spoken.mp4", quiet=True)
    assert result.narrated and result.narration_cues == 1
    facts = verify(result.path, load_recipe(tiny), expect_audio=True)
    assert facts["audio_codec"] == "aac" and facts["audio_rate"] == 16_000


def test_an_unknown_tts_backend_is_refused_rather_than_ignored() -> None:
    from v2sbench.render import NarrationError

    with pytest.raises(NarrationError, match="unknown tts backend"):
        detect_backend("robot-voice")


def test_a_frame_can_be_written_out_for_inspection(tmp_path: Path) -> None:
    recipe = load_recipe(FIXTURE)
    canvas = frames(recipe, plan(recipe))[0][0]
    writer = save_png(canvas, tmp_path / "frame.png")
    assert writer in {"pillow", "stdlib"}
    assert (tmp_path / "frame.png").read_bytes().startswith(b"\x89PNG\r\n\x1a\n")
