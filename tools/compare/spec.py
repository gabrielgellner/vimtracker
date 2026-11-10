"""Reading a comparison spec, as `vimtracker probe` reads it, and making what both sides play.

A spec is TOML (decision 110). Our side reads `[ours]`, the reference side `[reference]`, and both
read `rate`, `duration`, and `notes` or `input`. The input signals and the notes' timing here must
match `crates/vt-tui/src/dsp_probe.rs`: `run.py` checks the signals against the probe's own.
"""

from __future__ import annotations

import math
import tomllib
from dataclasses import dataclass
from pathlib import Path

import numpy as np

DEFAULT_RATE = 48_000
# The program's default velocity, and the velocity from which a note is accented.
DEFAULT_VELOCITY = 0x64
ACCENT_VELOCITY = 0x70
# How long after the next note starts a sliding note's MIDI note-off goes, so the two overlap.
SLIDE_OVERLAP_SECONDS = 0.001


@dataclass
class Spec:
    path: Path
    data: dict

    @property
    def folder(self) -> Path:
        return self.path.parent

    @property
    def rate(self) -> int:
        return int(self.data.get("rate", DEFAULT_RATE))

    @property
    def cases(self) -> list[str]:
        return [case["name"] for case in self.data.get("cases", [])]

    def reference(self, case: str | None) -> dict:
        """The reference's section, with a case's parameters over its own."""
        reference = dict(self.data.get("reference") or {})
        if not reference.get("plugin"):
            raise SystemExit(f"{self.path}: [reference] needs a `plugin` path")
        parameters = dict(reference.get("parameters", {}))
        parameters.update(self.case(case).get("reference", {}))
        reference["parameters"] = parameters
        return reference

    def case(self, name: str | None) -> dict:
        cases = self.data.get("cases", [])
        if name is None:
            if cases:
                raise SystemExit(f"{self.path} has cases, so choose one: {', '.join(self.cases)}")
            return {}
        for case in cases:
            if case["name"] == name:
                return case
        raise SystemExit(f"{self.path} has no case called {name}; it has {', '.join(self.cases)}")


def read(path: Path) -> Spec:
    with open(path, "rb") as file:
        return Spec(path=path, data=tomllib.load(file))


def frames_in(seconds: float, rate: int) -> int:
    """Frames in so many seconds, rounded to the nearest, as the probe rounds them."""
    return max(0, math.floor(seconds * rate + 0.5))


def gain_of(db: float) -> float:
    return 10.0 ** (db / 20.0)


def _sine(frequency: float, frames: np.ndarray, rate: int) -> np.ndarray:
    return np.sin(2.0 * np.pi * frequency * frames / rate)


def signal(spec: Spec) -> np.ndarray:
    """An effect's input as stereo samples, shape (2, frames), made as the probe makes it."""
    given = spec.data["input"]
    rate = spec.rate
    kind = given["signal"]
    if kind == "file":
        from pedalboard.io import AudioFile

        with AudioFile(str(spec.folder / given["path"])).resampled_to(rate) as file:
            audio = file.read(file.frames)
        if audio.shape[0] == 1:
            audio = np.vstack([audio, audio])
        return audio[:2].astype(np.float32)
    if kind == "sine":
        frames = np.arange(frames_in(given["seconds"], rate))
        mono = gain_of(given["level"]) * _sine(given["frequency"], frames, rate)
    elif kind == "sweep":
        start, stop, seconds = given["from"], given["to"], given["seconds"]
        ratio = math.log(stop / start)
        t = np.arange(frames_in(seconds, rate)) / rate
        phase = 2.0 * np.pi * start * seconds / ratio * (np.exp(t / seconds * ratio) - 1.0)
        mono = gain_of(given["level"]) * np.sin(phase)
    elif kind == "steps":
        step = given.get("step", 1.0)
        count = math.floor((given["to"] - given["from"]) / step + 1e-9) + 1
        held = frames_in(given["hold"], rate)
        frames = np.arange(count * held)
        levels = given["from"] + step * (frames // held)
        mono = 10.0 ** (levels / 20.0) * _sine(given["frequency"], frames, rate)
    elif kind == "burst":
        starts = frames_in(given["before"], rate)
        ends = starts + frames_in(given["on"], rate)
        total = ends + frames_in(given["after"], rate)
        frames = np.arange(total)
        levels = np.where((frames >= starts) & (frames < ends), given["level"], given["rest"])
        mono = 10.0 ** (levels / 20.0) * _sine(given["frequency"], frames, rate)
    elif kind == "silence":
        mono = np.zeros(frames_in(given["seconds"], rate))
    else:
        raise SystemExit(f"{spec.path}: no input signal called {kind}")
    mono = mono.astype(np.float32)
    return np.vstack([mono, mono])


def velocity(note: dict) -> int:
    """A note's velocity with its accent, as the probe plays it."""
    value = int(note.get("velocity", DEFAULT_VELOCITY))
    accent = note.get("accent")
    if accent is True:
        return max(value, ACCENT_VELOCITY)
    if accent is False:
        return min(value, ACCENT_VELOCITY - 1)
    return value


def midi(spec: Spec) -> list:
    """The notes as MIDI messages at times in seconds, for pedalboard. A sliding note is held until
    just after the next starts, as the probe holds it into the next."""
    from mido import Message

    notes = sorted(spec.data["notes"], key=lambda note: note["time"])
    events = []  # (seconds, order, message): note-offs before note-ons at the same time
    for index, note in enumerate(notes):
        start, end = note["time"], note["time"] + note["length"]
        events.append((start, 1, Message("note_on", note=note["note"], velocity=velocity(note))))
        following = notes[index + 1] if index + 1 < len(notes) else None
        if note.get("slide") and following is not None and following["time"] >= end:
            if following["note"] == note["note"]:
                continue  # the next note carries this key on
            end = following["time"] + SLIDE_OVERLAP_SECONDS
        events.append((end, 0, Message("note_off", note=note["note"], velocity=0)))
    events.sort(key=lambda event: (event[0], event[1]))
    messages = []
    for seconds, _, message in events:
        messages.append(message.copy(time=seconds))
    return messages


def duration_frames(spec: Spec, input_frames: int | None = None) -> int:
    duration = spec.data.get("duration")
    if duration is None:
        if input_frames is None:
            raise SystemExit(f"{spec.path}: a voice's spec needs a `duration`")
        return input_frames
    return frames_in(duration, spec.rate)
