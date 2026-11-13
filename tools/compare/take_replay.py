"""Acid V playing a take again, hosted here by pedalboard: the take's MIDI as Live sends it, and its
knobs moved along their envelopes, a buffer of 512 frames at a time, as a host moves a plugin's
automation (#565). And the probe that finds how Acid V takes Live's MIDI, by its output: from
which velocity a note is accented, and from how much overlap the next note slides.

The replay starts Acid V from its `Default` preset with its sequencer off, as the Acid specs do,
then sets every parameter the take gave as Live had it, by the share of its range Live gives, which
is the plugin's own normalized value, `raw_value` here. So with no Live, from `take.json` alone,
this is what Acid V would play, and with Live's own render beside it, the two can be checked
against each other.
"""

from __future__ import annotations

import json
import sys

import numpy as np
from mido import Message
from scipy.io import wavfile

import measure
import plugin_state

PLUGIN = "/Library/Audio/Plug-Ins/VST3/Acid V.vst3"
BUFFER = 512
RATE = 48_000
TAIL_SECONDS = 0.5


def load():
    import pedalboard

    plugin = pedalboard.load_plugin(PLUGIN)
    # Acid V takes a state only once it's been given it twice (render_reference.py).
    for _ in range(2):
        plugin_state.start_from(plugin, PLUGIN, "Default", {"Sequencer_OnOff": 0})
    return plugin


def by_name(plugin) -> dict:
    """The plugin's parameters by the names Live shows, which are the plugin's own."""
    return {parameter.name: parameter for parameter in plugin.parameters.values()}


# Where the Acid specs keep Acid V's parameters that aren't knobs, as normalized values: its
# master at 0 dB, its oscillator as it loads, and its distortion, sub oscillator and hiss off.
CLEAN = {
    "Master Volume": 1.0,
    "Osc Pitch": 0.5,
    "Osc Waveform": 0.0,
    "Distortion Dry/Wet": 0.0,
    "Sub Osc Volume": 0.0,
    "Noise": 0.0,
}


def clean_parameters(knobs: dict) -> list[dict]:
    """Parameters as Live describes them, for a take made here: the clean ones, then the knobs."""
    out = []
    for name, value in {**CLEAN, **knobs}.items():
        out.append({"name": name, "value": float(value), "min": 0.0, "max": 1.0, "shown": f"{value:.3f}",
                    "from": "0", "to": "1", "quantized": False})
    return out


def messages(take, later: float = 0.0) -> list[tuple[float, int, Message]]:
    """The take's notes as Live sends them, in seconds: each note's own start and end, so notes
    that overlap overlap here too, with a note's end before another's start at the same time."""
    events = []
    for note in take.notes:
        velocity = max(1, min(127, round(note.velocity)))
        start, end = take.seconds(note.start) + later, take.seconds(note.end) + later
        events.append((start, 1, Message("note_on", note=note.pitch, velocity=velocity)))
        events.append((end, 0, Message("note_off", note=note.pitch, velocity=0)))
    events.sort(key=lambda event: (event[0], event[1]))
    return events


def play(plugin, events, seconds: float, automation=None) -> np.ndarray:
    """Play timed MIDI for `seconds`, a buffer at a time, first calling `automation(seconds)` to
    set the knobs at each buffer's start. Stereo samples (2, frames)."""
    frames = int(round(seconds * RATE))
    # Each message's frame, so one falling on a buffer's first frame is in that buffer at 0 and
    # not lost a hair before it, as a time in seconds can be.
    timed = [(int(round(time * RATE)), message) for time, _, message in events]
    out = []
    at = 0
    while at < frames:
        if automation is not None:
            automation(at / RATE)
        chunk = [message.copy(time=(frame - at) / RATE) for frame, message in timed if at <= frame < at + BUFFER]
        out.append(plugin(chunk, duration=BUFFER / RATE, sample_rate=RATE, num_channels=2, buffer_size=BUFFER, reset=at == 0))
        at += BUFFER
    return np.concatenate(out, axis=1)[:, :frames].astype(np.float32)


def render(take, path) -> np.ndarray:
    """Acid V playing the take, as `path`, as long as one pass of the clip."""
    plugin = load()
    known = by_name(plugin)
    for name, parameter in take.parameters.items():
        if name in known and name not in take.envelopes:
            known[name].raw_value = take.static(name)
    moving = [name for name in take.envelopes if name in known]

    def automation(seconds: float) -> None:
        beat = seconds * take.tempo / 60.0
        for name in moving:
            known[name].raw_value = take.value_at(name, min(beat, take.length))

    out = play(plugin, messages(take), take.duration + TAIL_SECONDS, automation)
    out = out[:, : int(round(take.duration * RATE))]
    wavfile.write(path, RATE, np.ascontiguousarray(out.T))
    return out


# The probe: what Acid V makes of a velocity, and of notes that overlap.

# Velocities, one note each from rest; a note's pitch as Live sends it.
VELOCITIES = [1, 30, 64, 90, 96, 97, 98, 99, 100, 101, 102, 105, 110, 112, 120, 127]
PROBE_PITCH, SLID_TO = 48, 55
NOTE_SECONDS, APART_SECONDS = 0.15, 0.6
# How far the first of two notes runs past the second's start, in ms; under 0 is a gap.
OVERLAPS_MS = [-20, -5, -1, 0, 1, 2, 5, 10, 30]
PAIR_SECONDS = 0.9
FIRST_SECONDS = 0.25
SECOND_SECONDS = 0.25
LEAD_SECONDS = 0.25
# Acid V's knobs for the probe, as shares: Accent full, so an accent is plain to hear.
PROBE_KNOBS = {"Cutoff": 0.35, "Resonance": 0.5, "Envelope Mod": 0.6, "Decay": 0.4, "Accent": 1.0}
# An accent is this much louder or brighter than the plain notes.
ACCENT_DB, ACCENT_OCTAVES = 1.5, 0.25
# A note is sliding into if its pitch, early on, is this far under where it settles, in cents.
SLIDING_CENTS = 150.0


def probe_notes() -> tuple[list[dict], float]:
    """The probe's notes, in seconds, and how long it plays."""
    notes = []
    at = LEAD_SECONDS
    for velocity in VELOCITIES:
        notes.append({"pitch": PROBE_PITCH, "start": at, "duration": NOTE_SECONDS, "velocity": velocity})
        at += APART_SECONDS
    for overlap in OVERLAPS_MS:
        notes.append({"pitch": PROBE_PITCH, "start": at, "duration": FIRST_SECONDS + overlap / 1000, "velocity": 99})
        notes.append({"pitch": SLID_TO, "start": at + FIRST_SECONDS, "duration": SECOND_SECONDS, "velocity": 99})
        at += PAIR_SECONDS
    return notes, at


def probe_findings(samples: np.ndarray, rate: int) -> dict:
    """Read the probe's render: each velocity's level and brightness over the plain notes', and
    each overlap's pitch early in the second note against where it settles."""
    notes, _ = probe_notes()
    single = notes[: len(VELOCITIES)]
    marks = measure.note_marks(samples, rate, [note["start"] for note in single])
    plain = [mark for note, mark in zip(single, marks) if note["velocity"] <= 90]
    level = float(np.median([mark["RMS dBFS"] for mark in plain]))
    bright = float(np.median([np.log2(mark["bright Hz"]) for mark in plain]))
    velocities = []
    for note, mark in zip(single, marks):
        louder = mark["RMS dBFS"] - level
        brighter = float(np.log2(mark["bright Hz"]) - bright)
        velocities.append({"velocity": note["velocity"], "louder dB": round(louder, 2), "brighter octaves": round(brighter, 3),
                           "accented": bool(louder > ACCENT_DB or brighter > ACCENT_OCTAVES)})
    accent_from = None
    for row in reversed(velocities):
        if not row["accented"]:
            break
        accent_from = row["velocity"]
    overlaps = []
    pairs = notes[len(VELOCITIES):]
    for index, overlap in enumerate(OVERLAPS_MS):
        second = pairs[2 * index + 1]["start"]
        times, found = measure.pitch_track(samples, rate, second - 0.02, second + SECOND_SECONDS - 0.02, 25.0, 500.0)
        early = found[(times >= second + 0.005) & (times <= second + 0.025)]
        settled = found[(times >= second + 0.15) & (times <= second + 0.22)]
        cents = float(1200 * np.log2(np.nanmedian(early) / np.nanmedian(settled))) if early.size and settled.size else float("nan")
        overlaps.append({"overlap ms": overlap, "early cents": round(cents, 1), "slides": bool(cents < -SLIDING_CENTS)})
    slide_from = max((row["overlap ms"] for row in overlaps if not row["slides"]), default=min(OVERLAPS_MS))
    return {
        "velocities": velocities,
        "accent from": accent_from,
        "overlaps": overlaps,
        "slide from seconds": slide_from / 1000.0,
    }


def probe(live_track: str | None = None) -> dict:
    """Run the probe, here or through Live on a track's instrument, and keep what it found."""
    import take

    notes, seconds = probe_notes()
    if live_track is None:
        plugin = load()
        known = by_name(plugin)
        for name, value in {**CLEAN, **PROBE_KNOBS}.items():
            known[name].raw_value = value
        events = []
        for note in notes:
            events.append((note["start"], 1, Message("note_on", note=note["pitch"], velocity=note["velocity"])))
            events.append((note["start"] + note["duration"], 0, Message("note_off", note=note["pitch"], velocity=0)))
        events.sort(key=lambda event: (event[0], event[1]))
        samples = play(plugin, events, seconds)
        where = "Acid V hosted here, its Default preset, with " + ", ".join(f"{k} {v:g}" for k, v in PROBE_KNOBS.items())
        name = "acid-v-midi.json"
    else:
        import live_bridge

        tempo = float(live_bridge.request("ping")["tempo"])
        beats = [{**note, "start": note["start"] * tempo / 60.0, "duration": note["duration"] * tempo / 60.0} for note in notes]
        print(f"Live: playing the probe on {live_track} for {seconds:.1f} s, silent", file=sys.stderr, flush=True)
        answer = live_bridge.request("probe take", track=live_track, notes=beats, length=seconds * tempo / 60.0,
                                     seconds=seconds + 0.5, timeout=seconds + 60.0)
        received = live_bridge.read_recording(answer["recorded"], RATE)
        level = np.abs(received).max(axis=0)
        first = int(np.argmax(level >= level.max() * 10 ** (-50 / 20)))
        lag = first - int(round(notes[0]["start"] * RATE))
        samples = received[:, max(lag, 0):]
        where = f"Acid V in Live on {live_track}, as it was set"
        name = "acid-v-midi-live.json"
    found = probe_findings(samples, RATE)
    found["where"] = where
    take.TAKES.mkdir(parents=True, exist_ok=True)
    (take.TAKES / name).write_text(json.dumps(found, indent=1))
    wavfile.write(take.TAKES / name.replace(".json", ".wav"), RATE, np.ascontiguousarray(samples.T, dtype=np.float32))
    print(f"{where}:")
    for row in found["velocities"]:
        print(f"  velocity {row['velocity']:3d}: {row['louder dB']:+6.2f} dB, {row['brighter octaves']:+.2f} octaves"
              f"{'  accented' if row['accented'] else ''}")
    print(f"  accented from velocity {found['accent from']}")
    for row in found["overlaps"]:
        print(f"  overlap {row['overlap ms']:+4d} ms: {row['early cents']:+7.1f} cents early on{'  slides' if row['slides'] else ''}")
    print(f"  slides once notes overlap by more than {found['slide from seconds'] * 1000:g} ms")
    print(f"wrote {take.TAKES / name}")
    return found
