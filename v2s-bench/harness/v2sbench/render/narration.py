"""Synthesise the narration track from a recipe's cues.

Why a TTS track at all: two of the benchmark's arms (transcript-only and
raw-transcript) are built from ASR output, so a silent video turns those
arms into empty-context arms without saying so. The renderer therefore
reports which backend spoke -- or that none did -- and the harness is
expected to refuse to score those arms on an unnarrated render.

Why the mixing happens here in Python rather than in an ffmpeg filter
graph: ``amix`` rescales by the number of inputs, so adding a cue would
quietly change the loudness of every other one, and a long ``adelay``
chain is hard to read and varies with the filter implementation. Cues
are instead decoded to raw 16 kHz mono PCM (whisper's native rate, so
nothing resamples it later), summed into a silent buffer at their exact
sample offset and clamped -- arithmetic anyone can check, and identical
on every run.
"""

from __future__ import annotations

import array
import shutil
import subprocess
import wave
from dataclasses import dataclass
from pathlib import Path

from .recipe import Cue

SAMPLE_RATE = 16_000
"""Whisper's native rate: resampling once, here, beats twice, later."""

_INT16_MAX = 32767
_BACKENDS = ("say", "espeak-ng")


class NarrationError(RuntimeError):
    """A TTS backend was asked for, found, and still failed."""


@dataclass(frozen=True)
class NarrationTrack:
    """The mixed narration, or the honest absence of one."""

    backend: str | None
    wav: Path | None
    cues: int = 0
    spoken_secs: float = 0.0
    overlaps: int = 0
    """Cues whose speech runs into the next cue's start; they are summed."""
    truncated: int = 0
    """Cues whose speech ran past the end of the video and was cut."""

    @property
    def available(self) -> bool:
        return self.wav is not None


def detect_backend(prefer: str = "auto") -> str | None:
    """Pick a TTS backend: macOS ``say``, then ``espeak-ng``, then none.

    ``prefer="none"`` forces a silent render (useful in tests); naming a
    backend that is not installed is an error rather than a silent
    downgrade, because a benchmark run that expected narration and got
    silence must fail loudly.
    """
    if prefer == "none":
        return None
    if prefer == "auto":
        return next((name for name in _BACKENDS if shutil.which(name)), None)
    if prefer not in _BACKENDS:
        raise NarrationError(f"unknown tts backend {prefer!r}; expected auto/none/{_BACKENDS}")
    if not shutil.which(prefer):
        raise NarrationError(f"tts backend {prefer!r} was requested but is not on PATH")
    return prefer


def synthesise(
    cues: tuple[Cue, ...],
    duration_secs: float,
    work: Path,
    *,
    backend: str | None,
    ffmpeg: str = "ffmpeg",
) -> NarrationTrack:
    """Speak every cue and mix them into one mono 16 kHz WAV.

    Returns a track with ``wav=None`` when there is no backend or no cue,
    which is what makes an unnarrated render detectable downstream.
    """
    if backend is None or not cues:
        return NarrationTrack(backend=backend, wav=None, cues=0)
    work.mkdir(parents=True, exist_ok=True)
    total = round(duration_secs * SAMPLE_RATE)
    mixed = array.array("h", bytes(2 * total))
    spoken, overlaps, truncated = 0, 0, 0
    for index, cue in enumerate(cues):
        samples = _speak(cue.text, index, work, backend=backend, ffmpeg=ffmpeg)
        start = round(cue.t * SAMPLE_RATE)
        end = start + len(samples)
        spoken += len(samples)
        if end > total:
            truncated += 1
            samples = samples[: max(0, total - start)]
            end = start + len(samples)
        nxt = cues[index + 1].t if index + 1 < len(cues) else duration_secs
        if end > round(nxt * SAMPLE_RATE):
            overlaps += 1
        for at, value in enumerate(samples, start=start):
            total_at = mixed[at] + value
            mixed[at] = max(-_INT16_MAX, min(_INT16_MAX, total_at))
    out = work / "narration.wav"
    _write_wav(out, mixed)
    return NarrationTrack(
        backend=backend,
        wav=out,
        cues=len(cues),
        spoken_secs=round(spoken / SAMPLE_RATE, 3),
        overlaps=overlaps,
        truncated=truncated,
    )


def _speak(text: str, index: int, work: Path, *, backend: str, ffmpeg: str) -> array.array[int]:
    """One cue as 16 kHz mono samples.

    The text goes in on stdin, never as an argument, so a cue that starts
    with ``-`` cannot turn into a command-line flag.
    """
    raw = work / f"cue{index:03d}.{'aiff' if backend == 'say' else 'wav'}"
    if backend == "say":
        argv = ["say", "-r", "170", "-o", str(raw)]
    else:
        argv = ["espeak-ng", "-v", "en-us", "-s", "165", "-p", "42", "-w", str(raw)]
    done = subprocess.run(argv, input=text.encode(), capture_output=True, check=False)
    if done.returncode != 0 or not raw.is_file():
        raise NarrationError(f"{backend} failed on cue {index}: {done.stderr.decode()[:200]}")
    decoded = subprocess.run(
        [ffmpeg, "-nostdin", "-v", "error", "-fflags", "+bitexact", "-i", str(raw),
         "-ar", str(SAMPLE_RATE), "-ac", "1", "-f", "s16le", "-"],
        capture_output=True,
        check=False,
    )
    if decoded.returncode != 0:
        tail = decoded.stderr.decode()[:200]
        raise NarrationError(f"ffmpeg could not decode cue {index}: {tail}")
    samples: array.array[int] = array.array("h")
    samples.frombytes(decoded.stdout[: len(decoded.stdout) // 2 * 2])
    return samples


def _write_wav(path: Path, samples: array.array[int]) -> None:
    """A plain PCM WAV. ``wave`` writes no timestamps, so this is stable."""
    with wave.open(str(path), "wb") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(SAMPLE_RATE)
        handle.writeframes(samples.tobytes())
