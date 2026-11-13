"""A performance take: a riff the author played on Arturia's Acid V in Ableton Live, its notes and
its knob moves read out of the clip, played again on our Acid and compared over time (#565).

Usage: take.py export CLIP          read the clip from Live into out/takes/<clip>/take.json
       take.py render-live CLIP     Acid V playing the clip in Live, as out/takes/<clip>/acid-v-live.wav
       take.py probe-live TRACK     how Acid V in Live takes accents and slides, by its output
       take.py probe                the same, with Acid V hosted here by pedalboard
       take.py synthetic NAME       a take written here, to run the rest without Live
       take.py compare NAME         our song from the take, both sides rendered, and the report

`export`, `render-live` and `probe-live` need Live open with the bridge (README, "Ableton Live's
devices"); everything else runs without it, from what `export` wrote. A take's folder ends up with:

- `take.json`, the clip as Live has it: its notes in beats, Acid V's parameters, and each recorded
  envelope read every 64th of a beat;
- `rows.toml` and `ours/`, our song: the notes on an Acid track, each accent and slide as the
  program writes them, and the knobs' moves in FX lanes (`take_rows.py`), which `vimtracker`
  opens and `vimtracker render` plays;
- `ours.wav`, `acid-v.wav`, Acid V hosted here playing the take's notes and moves again
  (`take_replay.py`), and `acid-v-live.wav` if Live rendered it;
- `report.md` with its plots and `measures.json` (`take_report.py`).

Acid V's knobs are mapped to ours by where they're turned: decision 117 fitted our `Cutoff`,
`Resonance`, `Env mod`, `Decay` and `Accent` to Acid V's so that the same share of each knob's turn
sounds the same, which is how `nuance_specs.py` sets them, so Acid V's 0.5 is our `80`. A knob
with no envelope stays where Acid V has it; one that moved is followed. Acid V's other parameters
aren't ours, and a take where they're away from where the specs keep them is flagged.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

import numpy as np

import take_rows

HERE = Path(__file__).resolve().parent
REPOSITORY = HERE.parent.parent
TAKES = HERE / "out" / "takes"
BINARY = REPOSITORY / "target" / "release" / "vimtracker"
SONG_BUILDER = REPOSITORY / "target" / "release" / "examples" / "take_song"
RATE = 48_000
# How often `export` reads an envelope, in beats: a 64th, 7.8 ms at 120 BPM.
ENVELOPE_STEP = 1 / 64
# Acid V's knobs by the names Live and pedalboard give them, and ours they're fitted to
# (decision 117): the same share of the turn on both.
KNOBS = {
    "Cutoff": "acid.cutoff",
    "Resonance": "acid.resonance",
    "Envelope Mod": "acid.envmod",
    "Decay": "acid.decay",
    "Accent": "acid.accent",
}
# Acid V's own controls that move its knobs from its panel's front, which aren't the knobs
# themselves: pedalboard found them independent of them, so a take that moves one can't be
# followed and is flagged.
MACROS = ("Brightness", "Timbre", "Time", "Movement")
# Acid V's parameters away from where the Acid specs keep them make its sound one ours doesn't
# have, by the normalized value Live gives: the value and what it means.
CLEAN = {
    "Distortion Dry/Wet": (0.0, "its distortion off"),
    "Sub Osc Volume": (0.0, "its sub oscillator at its bottom, -70 dB"),
    "Noise": (0.0, "its hiss at its bottom, -100 dB"),
}
# The velocity Acid V accents from and the overlap it slides from, as found by `take.py probe`
# on 2026-11-12 and by decision 117's specs, until a probe of the author's own says otherwise.
ACCENT_FROM = 100
SLIDE_FROM_SECONDS = 0.0


class Refused(SystemExit):
    """A take that can't be compared, with why."""


def slug(name: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "-", name).strip("-") or "take"


def folder_of(name: str) -> Path:
    return TAKES / slug(name)


@dataclass
class Note:
    """A note of the take, its times in beats from the pass's start."""

    pitch: int
    start: float
    duration: float
    velocity: float
    accent: bool = False
    slide: bool = False

    @property
    def end(self) -> float:
        return self.start + self.duration


class Take:
    """A take as `export` wrote it, with what it means for our side worked out."""

    def __init__(self, data: dict, folder: Path | None = None, midi: dict | None = None):
        self.data = data
        self.folder = folder
        self.name = data.get("clip", "take")
        self.tempo = float(data["tempo"])
        self.length = float(data["end"]) - float(data["start"])
        self.step = float(data["step"])
        self.parameters = {p["name"]: p for p in data["parameters"]}
        self.envelopes = {name: np.asarray(values, dtype=np.float64) for name, values in data.get("envelopes", {}).items()}
        midi = midi or {}
        self.accent_from = int(midi.get("accent from", ACCENT_FROM))
        self.slide_from = float(midi.get("slide from seconds", SLIDE_FROM_SECONDS))
        self.notes, self.warnings = self.classify([n for n in data["notes"] if not n.get("mute")])

    @classmethod
    def read(cls, folder: Path) -> "Take":
        path = folder / "take.json"
        if not path.exists():
            raise Refused(f"there's no take at {path}; `take.py export CLIP` reads one from Live")
        return cls(json.loads(path.read_text()), folder, read_midi_findings())

    def seconds(self, beats: float) -> float:
        return beats * 60.0 / self.tempo

    @property
    def duration(self) -> float:
        return self.seconds(self.length)

    def classify(self, given: list[dict]) -> tuple[list[Note], list[str]]:
        """Each note's accent, from its velocity, and its slide, from overlapping the next, as
        Acid V takes them from Live's MIDI."""
        warnings = []
        notes = [Note(int(n["pitch"]), float(n["start"]), float(n["duration"]), float(n["velocity"])) for n in given]
        early = [note for note in notes if note.start < 0]
        if early:
            warnings.append(f"{len(early)} notes start before the clip's start marker; left out")
            notes = [note for note in notes if note.start >= 0]
        notes.sort(key=lambda note: (note.start, note.pitch))
        for index, note in enumerate(notes):
            note.accent = round(note.velocity) >= self.accent_from
            following = notes[index + 1] if index + 1 < len(notes) else None
            if following is None:
                continue
            if abs(following.start - note.start) < 1e-9:
                warnings.append(f"two notes start together at beat {note.start:g}: a 303 plays one, the higher here")
            overlap = self.seconds(note.end - following.start)
            note.slide = overlap > self.slide_from + 1e-9
        # Of notes starting together, the last, the highest, is the one heard.
        kept = [n for i, n in enumerate(notes) if i + 1 >= len(notes) or abs(notes[i + 1].start - n.start) >= 1e-9]
        return kept, warnings

    def static(self, name: str) -> float | None:
        """A parameter's value, normalized, where the take starts: its envelope's first if it
        has one, or what the device shows."""
        if name in self.envelopes:
            return float(self.envelopes[name][0])
        parameter = self.parameters.get(name)
        return None if parameter is None else normalized(parameter)

    def value_at(self, name: str, beat: float) -> float | None:
        """A parameter's normalized value at a time, in beats from the pass's start, between the
        points its envelope was read at."""
        envelope = self.envelopes.get(name)
        if envelope is None:
            return self.static(name)
        place = beat / self.step
        return float(np.interp(place, np.arange(envelope.size), envelope))

    def moved(self) -> list[str]:
        """The parameters whose envelope moves at all."""
        return [name for name, values in self.envelopes.items() if values.size and np.ptp(values) > 1e-6]

    def knobs_at(self, beat: float) -> dict:
        """Acid V's five knobs at a time, as shares of their turn."""
        return {name: self.value_at(name, beat) for name in KNOBS if self.static(name) is not None}

    def flags(self) -> list[str]:
        """What in the take ours can't follow, for the report."""
        out = list(self.warnings)
        for name in self.moved():
            if name in MACROS:
                out.append(f"`{name}` moved, a control on Acid V's front panel over its knobs, which ours hasn't: move the knobs themselves")
            elif name not in KNOBS:
                out.append(f"`{name}` moved, which isn't one of the knobs ours follows")
        for name, (wanted, meaning) in CLEAN.items():
            value = self.static(name)
            if value is not None and (abs(value - wanted) > 1e-3 or name in self.moved()):
                shown = self.parameters.get(name, {}).get("shown", f"{value:.3f}")
                out.append(f"`{name}` is at {shown}, where the Acid specs keep {meaning}")
        return out


def normalized(parameter: dict) -> float:
    low, high = float(parameter["min"]), float(parameter["max"])
    return (float(parameter["value"]) - low) / (high - low) if high > low else 0.0


def read_midi_findings() -> dict:
    """What a probe found of how Acid V takes Live's MIDI: Live's own if it's been run there."""
    for name in ("acid-v-midi-live.json", "acid-v-midi.json"):
        path = TAKES / name
        if path.exists():
            return json.loads(path.read_text())
    return {}


# Our side.


def build_song(take: Take, folder: Path) -> Path:
    """Write our song for the take into `folder/ours`, through the program's own model."""
    layout = take_rows.layout(take)
    rows = folder / "rows.toml"
    rows.write_text(take_rows.toml_of(layout, take))
    project = folder / "ours"
    if not SONG_BUILDER.exists():
        raise Refused(f"there's no {SONG_BUILDER}; run `cargo build --release -p vt-tui --example take_song`")
    subprocess.run([str(SONG_BUILDER), str(rows), str(project)], check=True)
    take.warnings.extend(layout.warnings)
    return project


def render_ours(take: Take, project: Path, out: Path, steps: int) -> None:
    if not BINARY.exists():
        raise Refused(f"there's no {BINARY}; run `cargo build --release`")
    subprocess.run([str(BINARY), "render", str(project), str(out), "--rows", str(steps), "--rate", str(RATE)], check=True, stdout=subprocess.DEVNULL)


# The commands.


def export(clip: str) -> Path:
    import live_bridge

    answer = live_bridge.request("take", clip=clip, step=ENVELOPE_STEP, timeout=60.0)
    answer.pop("ok", None)
    folder = folder_of(clip)
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "take.json").write_text(json.dumps(answer, indent=1))
    take = Take(answer, folder)
    print(f"{folder / 'take.json'}: {len(take.notes)} notes over {take.length:g} beats at {take.tempo:g} BPM, "
          f"{take.duration:.1f} s, on {answer['device']}")
    moved = take.moved()
    print("moved: " + (", ".join(moved) if moved else "no knob"))
    for flag in take.flags():
        print(f"note: {flag}")
    return folder


def render_live(clip: str, tail: float = 0.5) -> Path:
    """Acid V playing the clip in Live, silently, lined up with the take by its first note."""
    import live_bridge
    from scipy.io import wavfile

    folder = folder_of(clip)
    take = Take.read(folder)
    seconds = take.duration + tail
    print(f"Live: playing {clip} for {seconds:.1f} s, silent", file=sys.stderr, flush=True)
    answer = live_bridge.request("render take", clip=clip, seconds=seconds, timeout=seconds + 60.0)
    received = live_bridge.read_recording(answer["recorded"], RATE)
    lag = onset_lag(received, take)
    frames = int(round(take.duration * RATE))
    begin = max(lag, 0)
    out = received[:, begin : begin + frames]
    if out.shape[1] < frames:
        out = np.pad(out, ((0, 0), (0, frames - out.shape[1])))
    path = folder / "acid-v-live.wav"
    wavfile.write(path, RATE, np.ascontiguousarray(out.T, dtype=np.float32))
    (folder / "acid-v-live.json").write_text(json.dumps({**answer, "lag frames": lag}, indent=1))
    print(f"wrote {path}, lined up {lag} frames ({lag / RATE * 1000:.1f} ms) in")
    return path


def onset_lag(received: np.ndarray, take: Take, below_db: float = 50.0) -> int:
    """How many frames into a recording the take's start is: where it first comes within
    `below_db` of its loudest, less the first note's time. Acid V's notes start at once, within a
    millisecond, so this lines a recording up with the take to about that."""
    if not take.notes:
        raise Refused("the take has no notes to line Live's recording up by")
    level = np.abs(received).max(axis=0)
    loudest = float(level.max(initial=0.0))
    if loudest <= 0:
        raise Refused("Live recorded silence: is the track's instrument Acid V, and does the clip play it?")
    first = int(np.argmax(level >= loudest * 10 ** (-below_db / 20)))
    return first - int(round(take.seconds(take.notes[0].start) * RATE))


def compare(name: str) -> Path:
    import take_replay
    import take_report

    folder = folder_of(name)
    take = Take.read(folder)
    if not take.notes:
        raise Refused(f"{name} has no notes")
    project = build_song(take, folder)
    steps = math.ceil(take.length / take_rows.STEP_BEATS - 1e-9)
    render_ours(take, project, folder / "ours.wav", steps)
    take_replay.render(take, folder / "acid-v.wav")
    page = take_report.write(take, folder)
    print(f"wrote {page}")
    return page


def synthetic(name: str) -> Path:
    """A take made here, as `export` would write it, for trying the rest without Live: an acid
    line of our own over four bars at 126 BPM, with accents, slides and a gap, while Cutoff sweeps
    up and back, Env Mod rises, Resonance steps up, and Decay and Accent stay put. It isn't any
    known 303 pattern (decision 143)."""
    import take_replay

    folder = folder_of(name)
    folder.mkdir(parents=True, exist_ok=True)
    tempo, bars = 126.0, 4
    length = 4.0 * bars
    # (sixteenth, MIDI note as Live sends it, sixteenths held, velocity): Acid V sounds an
    # octave under the note it's sent.
    line = [
        (0, 48, 0.5, 112), (2, 48, 0.5, 90), (3, 60, 0.5, 90), (4, 51, 1.3, 90), (5, 53, 0.5, 120),
        (6, 48, 0.5, 90), (8, 55, 2.2, 90), (10, 58, 0.5, 127), (11, 48, 0.5, 90), (12, 60, 1.2, 90),
        (13, 51, 0.5, 90), (14, 48, 0.4, 100),
    ]
    notes = []
    for bar in range(bars):
        for sixteenth, pitch, held, velocity in line:
            if bar == 2 and sixteenth in (6, 8):
                continue  # a gap, for the filter to settle
            # A little played timing: some notes a few milliseconds off the grid.
            late = ((sixteenth * 7 + bar * 3) % 5 - 2) * 0.006
            notes.append({"pitch": pitch + (5 if bar == 3 and pitch < 50 else 0), "start": max(0.0, bar * 4.0 + sixteenth / 4.0 + late),
                          "duration": held / 4.0, "velocity": velocity, "mute": False})
    points = int(length / ENVELOPE_STEP) + 1
    beats = np.arange(points) * ENVELOPE_STEP
    along = beats / length
    envelopes = {
        "Cutoff": list(0.25 + 0.45 * np.sin(np.pi * along) ** 2),
        "Envelope Mod": list(0.3 + 0.55 * along),
        "Resonance": list(np.where(beats < 8.0, 0.55, 0.85)),
    }
    static = {"Cutoff": 0.25, "Resonance": 0.55, "Envelope Mod": 0.3, "Decay": 0.35, "Accent": 0.7}
    data = {
        "clip": name,
        "track": "Acid V",
        "device": "Acid V",
        "device class": "Acid V",
        "live": None,
        "synthetic": True,
        "tempo": tempo,
        "signature": [4, 4],
        "start": 0.0,
        "end": length,
        "looping": True,
        "step": ENVELOPE_STEP,
        "notes": notes,
        "parameters": take_replay.clean_parameters(static),
        "envelopes": envelopes,
    }
    (folder / "take.json").write_text(json.dumps(data, indent=1))
    print(f"wrote {folder / 'take.json'}: {len(notes)} notes over {length:g} beats at {tempo:g} BPM")
    return folder


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("export", help="read a clip's notes and envelopes from Live").add_argument("clip")
    commands.add_parser("render-live", help="record Acid V playing the clip in Live, silently").add_argument("clip")
    commands.add_parser("probe-live", help="find how Acid V in Live takes accents and slides").add_argument("track")
    commands.add_parser("probe", help="the same, with Acid V hosted here")
    commands.add_parser("synthetic", help="write a take made here, to try the rest without Live").add_argument("name")
    commands.add_parser("compare", help="build our song, render both sides and write the report").add_argument("name")
    args = parser.parse_args()
    if args.command == "export":
        export(args.clip)
    elif args.command == "render-live":
        render_live(args.clip)
    elif args.command in ("probe", "probe-live"):
        import take_replay

        take_replay.probe(live_track=getattr(args, "track", None))
    elif args.command == "synthetic":
        synthetic(args.name)
    elif args.command == "compare":
        compare(args.name)


if __name__ == "__main__":
    main()
