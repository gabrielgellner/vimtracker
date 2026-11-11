"""Reading a comparison spec, as `vimtracker probe` reads it, and making what both sides play.

A spec is TOML (decision 110). Our side reads `[ours]`, the reference side `[reference]` or the
list `[[references]]`, and both read `rate`, `duration`, and `notes` or `input`, which a case can
change. The input signals and the notes' timing here must
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

    @property
    def has_reference(self) -> bool:
        """Whether there's a reference to render, as a spec can measure ours alone."""
        return bool(self.reference_names)

    @property
    def reference_names(self) -> list[str]:
        """The references' names, in the spec's order: each of `[[references]]`'s `name`, or the
        one `[reference]`'s plugin's, or its Live device's."""
        return [name for name, _ in self._references()]

    def _references(self) -> list[tuple[str, dict]]:
        listed = self.data.get("references")
        if listed:
            out = []
            for section in listed:
                if not section.get("name") or not (section.get("plugin") or section.get("live")):
                    raise SystemExit(
                        f"{self.path}: each of [[references]] needs a `name`, and a `plugin` or a `live` device"
                    )
                out.append((section["name"], section))
            return out
        single = self.data.get("reference") or {}
        if single.get("live"):
            return [(single["live"], single)]
        if not single.get("plugin"):
            return []
        return [(Path(single["plugin"]).stem, single)]

    @property
    def report(self) -> dict:
        """How `report.py` lays out the spec's measurements."""
        return dict(self.data.get("report") or {})

    def settings(self, case: str | None) -> dict:
        """Our settings, by their keys, with a case's over them."""
        settings = flatten(self.data.get("ours", {}).get("settings", {}))
        settings.update(flatten(self.case(case).get("ours", {})))
        return settings

    def input(self, case: str | None) -> dict | None:
        """The input, with a case's fields over it, or the case's alone if it names another
        `signal`, as the probe takes it."""
        given = self.data.get("input")
        changes = self.case(case).get("input") or {}
        if given is None:
            return None
        if "signal" in changes and changes["signal"] != given.get("signal"):
            return dict(changes)
        return {**given, **changes}

    def reference(self, case: str | None, name: str | None = None) -> dict:
        """A reference's section, the first if no name is given, with a case's parameters over its
        own: the case's `reference`, which is for every reference, then its `references.<name>`."""
        known = dict(self._references())
        if not known:
            raise SystemExit(f"{self.path}: [reference] needs a `plugin` path or a `live` device")
        name = name or next(iter(known))
        if name not in known:
            raise SystemExit(f"{self.path} has no reference called {name}; it has {', '.join(known)}")
        reference = dict(known[name])
        reference["name"] = name
        parameters = dict(reference.get("parameters", {}))
        chosen = self.case(case)
        parameters.update(chosen.get("reference", {}))
        parameters.update((chosen.get("references") or {}).get(name, {}))
        reference["parameters"] = parameters
        return reference

    def notes(self, case: str | None) -> list[dict]:
        """The notes a case plays: its own, or else the spec's."""
        own = self.case(case).get("notes")
        return list(own) if own is not None else list(self.data.get("notes", []))

    @property
    def plays_notes(self) -> bool:
        return bool(self.data.get("notes")) or any("notes" in case for case in self.data.get("cases", []))

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


def flatten(table: dict, prefix: str = "") -> dict:
    """Settings with bare dotted keys' tables flattened back into keys, as the probe reads them."""
    out = {}
    for key, value in table.items():
        key = f"{prefix}.{key}" if prefix else key
        if isinstance(value, dict):
            out.update(flatten(value, key))
        else:
            out[key] = value
    return out


def read(path: Path) -> Spec:
    """A spec, or for a stub that names a song, the whole spec written from the song, which
    raises `song_line.Skipped` if the song isn't there."""
    import song_line

    path = song_line.resolve(path)
    with open(path, "rb") as file:
        return Spec(path=path, data=tomllib.load(file))


def frames_in(seconds: float, rate: int) -> int:
    """Frames in so many seconds, rounded to the nearest, as the probe rounds them."""
    return max(0, math.floor(seconds * rate + 0.5))


def gain_of(db: float) -> float:
    return 10.0 ** (db / 20.0)


def _sine(frequency: float, frames: np.ndarray, rate: int) -> np.ndarray:
    return np.sin(2.0 * np.pi * frequency * frames / rate)


def signal(spec: Spec, case: str | None = None) -> np.ndarray:
    """An effect's input as stereo samples, shape (2, frames), made as the probe makes it."""
    given = spec.input(case)
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
    elif kind == "impulse":
        mono = np.zeros(max(1, frames_in(given["seconds"], rate)))
        mono[0] = gain_of(given["level"])
    else:
        raise SystemExit(f"{spec.path}: no input signal called {kind}")
    mono = mono.astype(np.float32)
    return np.vstack([mono, mono])


def velocity(note: dict, accent_from: int = ACCENT_VELOCITY) -> int:
    """A note's velocity with its accent, as the probe plays it, or for a reference that accents
    from `accent_from`: an accent is raised to ours and its, whichever is higher, and a plain note
    kept under both, so `accent` means the same on every side."""
    value = int(note.get("velocity", DEFAULT_VELOCITY))
    accent = note.get("accent")
    if accent is True:
        return max(value, ACCENT_VELOCITY, accent_from)
    if accent is False:
        return min(value, ACCENT_VELOCITY - 1, accent_from - 1)
    return value


def midi(spec: Spec, case: str | None = None, accent_from: int = ACCENT_VELOCITY, later: float = 0.0) -> list:
    """A case's notes as MIDI messages at times in seconds, for pedalboard, all `later` seconds
    on. A sliding note is held until just after the next starts, as the probe holds it into the
    next."""
    from mido import Message

    notes = sorted(spec.notes(case), key=lambda note: note["time"])
    events = []  # (seconds, order, message): note-offs before note-ons at the same time
    for index, note in enumerate(notes):
        start, end = note["time"], note["time"] + note["length"]
        played = velocity(note, accent_from)
        events.append((start, 1, Message("note_on", note=note["note"], velocity=played)))
        following = notes[index + 1] if index + 1 < len(notes) else None
        if note.get("slide") and following is not None and following["time"] >= end:
            if following["note"] == note["note"]:
                continue  # the next note carries this key on
            end = following["time"] + SLIDE_OVERLAP_SECONDS
        events.append((end, 0, Message("note_off", note=note["note"], velocity=0)))
    events.sort(key=lambda event: (event[0], event[1]))
    messages = []
    for seconds, _, message in events:
        messages.append(message.copy(time=seconds + later))
    return messages


def duration_frames(spec: Spec, input_frames: int | None = None) -> int:
    duration = spec.data.get("duration")
    if duration is None:
        if input_frames is None:
            raise SystemExit(f"{spec.path}: a voice's spec needs a `duration`")
        return input_frames
    return frames_in(duration, spec.rate)
