"""A take laid out as a phrase of our Acid track's steps, written as the program's step text, for
`take_song` to build the song from (#565).

**Notes** go in rows on a sixteenth's grid. A note off the grid is nudged there, in 24ths of a step
either way up to 11, as a row's nudge plays it. A note holds for its own length: a row's gate
lasts as many pulses as it needs, up to 8, and its length is its share of the last, so a row may
take several steps of the grid; the next row starts after it. One held over more than eight steps
is held by a full gate and ended by an `OFF`. A slide is `HLD` on the gate, as the program writes
one, its pulses reaching the next note's row; an accent is a velocity of `7F`, and a plain note
keeps its own under `70`, which our Acid plays all alike (decision 121).

**Knobs** that moved go in FX lanes, one step of each lane to a step of the grid, whatever the rows
do, so a long note doesn't take away its steps' moves. Each knob's value is a glide, `g`, to the
next of its own in its column, with its fine part, so the knob moves on a line between them, and
its last is a hold, `h`, so it stays. Up to three knobs that move take a column each, and a value
every step; four to six share them, each with a value every other step. Values a line through
their neighbours would give anyway are left out, so the lanes show where the knob turns.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field

# A step of the grid, a sixteenth, in beats.
STEP_BEATS = 0.25
NUDGES = 24
MOST_NUDGE = 11
MOST_PULSES = 8
# The program's notes: C-3 is MIDI 60, and the lowest is 12, `c_1`.
NAMES = ["C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-"]
NAMES_BELOW_ZERO = ["c_", "c+", "d_", "d+", "e_", "f_", "f+", "g_", "g+", "a_", "a+", "b_"]
LOWEST_NOTE, HIGHEST_NOTE = 12, 127
ACCENTED = 0x7F
PLAIN_MOST = 0x6F
FX_COLUMNS = 3
# How far, on the 00 to FF scale, a knob's value may stray from the line through its neighbours
# before it's kept: a sixteenth of one of its steps.
KEEP_WITHIN = 1 / 16


@dataclass
class Layout:
    steps: list[str]
    rows: int
    lanes: int
    settings: dict
    columns: list[list[str]] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    transpose: int = 0


def note_name(midi: int) -> str:
    octave = midi // 12 - 2
    if octave < 0:
        return f"{NAMES_BELOW_ZERO[midi % 12]}{-octave}"
    return f"{NAMES[midi % 12]}{octave}"


def nudge_text(nudge: int) -> str:
    return "" if nudge == 0 else f" {'+' if nudge > 0 else '-'}{abs(nudge):X}"


def transpose_of(take) -> int:
    """How far ours plays a note from the one Live sends: Acid V sounds an octave under the MIDI
    note, so `nuance_specs.py` raises its oscillator 12 semitones, and a take played with it
    elsewhere moves ours with it."""
    pitch = take.parameters.get("Osc Pitch")
    semitones = 0
    if pitch is not None:
        low, high = float(pitch["min"]), float(pitch["max"])
        # Live gives a plugin's parameters from 0 to 1; Acid V's Osc Pitch spans -12 to +12.
        share = (float(pitch["value"]) - low) / (high - low) if high > low else 0.5
        semitones = round(-12 + 24 * share)
    return semitones - 12


def our_velocity(note) -> int:
    return ACCENTED if note.accent else min(max(int(round(note.velocity)), 1), PLAIN_MOST)


def layout(take) -> Layout:
    warnings = []
    total = math.ceil(take.length / STEP_BEATS - 1e-9)
    transpose = transpose_of(take)
    notes = []
    for note in take.notes:
        if note.start >= take.length:
            continue
        pitch = note.pitch + transpose
        if not LOWEST_NOTE <= pitch <= HIGHEST_NOTE:
            warnings.append(f"MIDI {note.pitch} at beat {note.start:g} is out of our range once moved {transpose:+d}; left out")
            continue
        notes.append((note, pitch))
    # Each note's row, one after another, and its nudge from there.
    placed = []
    last = -1
    for note, pitch in notes:
        place = note.start / STEP_BEATS
        row = max(round(place), last + 1)
        # A note can't be nudged before the song starts.
        nudge = max(round((place - row) * NUDGES), 0 if row == 0 else -MOST_NUDGE - 1)
        if abs(nudge) > MOST_NUDGE:
            warnings.append(f"the note at beat {note.start:g} moved {(row - place) * STEP_BEATS:+.3f} beats to a row of its own")
            nudge = max(-MOST_NUDGE, min(MOST_NUDGE, nudge))
        if row >= total:
            warnings.append(f"the note at beat {note.start:g} is past the take's end; left out")
            continue
        placed.append((note, pitch, row, nudge))
        last = row

    texts = []  # the note part of each row and its pulses

    def rest_until(at: int, now: int) -> int:
        while now < at:
            texts.append(("--- --", 1))
            now += 1
        return now

    now = 0
    for index, (note, pitch, row, nudge) in enumerate(placed):
        now = rest_until(row, now)
        following = placed[index + 1][2] if index + 1 < len(placed) else total
        room = following - row
        name = f"{note_name(pitch)} {our_velocity(note):02X}{nudge_text(nudge)}"
        if note.slide and index + 1 < len(placed):
            if room <= MOST_PULSES:
                texts.append((f"{name} HLD{room}", room))
                now = row + room
                continue
            warnings.append(f"the slide from beat {note.start:g} reaches {room} steps on, past a gate's 8; played without it")
        starts = row + nudge / NUDGES
        held = note.end / STEP_BEATS - starts
        pulses = max(1, math.ceil(held - 1e-9))
        off = None
        if pulses > room:
            pulses, share = room, 1.0
        elif pulses > MOST_PULSES:
            pulses, share, off = MOST_PULSES, 1.0, note.end / STEP_BEATS
        else:
            share = held - (pulses - 1)
        length = max(1, min(0xFF, round(share * 0xFF)))
        texts.append((f"{name} SGL{pulses} {length:02X}", pulses))
        now = row + pulses
        if off is not None:
            at = round(off)
            if now <= at < following:
                now = rest_until(at, now)
                texts.append((f"OFF --{nudge_text(max(-MOST_NUDGE, min(MOST_NUDGE, round((off - at) * NUDGES))))}", 1))
                now += 1
    rest_until(total, now)

    columns, settings = lanes(take, total, warnings)
    steps = []
    for at in range(total):
        notes_part = texts[at][0] if at < len(texts) else "--- --"
        fx = " ".join(column[at] for column in columns) if columns else ""
        steps.append(f"{notes_part} {fx}".rstrip())
    return Layout(steps, len(texts), len(columns), settings, columns, warnings, transpose)


def scaled(share: float) -> tuple[int, int]:
    """A share of a knob's turn as our value on the 00 to FF scale, coarse and fine."""
    value = max(0.0, min(255.0, share * 255.0))
    coarse = min(int(value), 255)
    fine = 0 if coarse == 255 else min(int(round((value - coarse) * 256)), 255)
    return coarse, fine


def lanes(take, total: int, warnings: list[str]) -> tuple[list[list[str]], dict]:
    """The FX lanes for the knobs that moved, and every knob's value where the take starts."""
    from take import KNOBS

    settings = {}
    for name, key in KNOBS.items():
        value = take.static(name)
        if value is not None:
            settings[key] = list(scaled(value))
    moving = [name for name in KNOBS if name in take.moved()]
    if not moving:
        return [], settings
    if len(moving) > 2 * FX_COLUMNS:
        raise ValueError("more knobs than lanes")
    every = math.ceil(len(moving) / FX_COLUMNS)
    if every > 1:
        warnings.append(f"{len(moving)} knobs moved, so each has a value every {every} steps, three FX columns shared between them")
    columns = [["---"] * total for _ in range(min(len(moving), FX_COLUMNS))]
    for index, name in enumerate(moving):
        column, phase = index % FX_COLUMNS, index // FX_COLUMNS
        places = list(range(phase, total, every))
        values = [take.value_at(name, at * STEP_BEATS) * 255.0 for at in places]
        kept = thinned(places, values)
        for number, at in enumerate(kept):
            coarse, fine = scaled(values[places.index(at)] / 255.0)
            mark = "h" if number == len(kept) - 1 else "g"
            digits = f"{coarse:02X}" + (f"{fine:02X}" if fine else "")
            columns[column][at] = f"{KNOBS[name]}:{digits}{mark}"
    return columns, settings


def thinned(places: list[int], values: list[float]) -> list[int]:
    """The places to keep so a line through them stays within `KEEP_WITHIN` of every value:
    the first and last, and each from which a straight run to the next can't be held."""
    if len(places) <= 2:
        return list(places)
    kept = [0]
    anchor = 0
    for end in range(2, len(places)):
        for between in range(anchor + 1, end):
            share = (places[between] - places[anchor]) / (places[end] - places[anchor])
            line = values[anchor] + share * (values[end] - values[anchor])
            if abs(line - values[between]) > KEEP_WITHIN:
                kept.append(end - 1)
                anchor = end - 1
                break
    kept.append(len(places) - 1)
    return [places[i] for i in kept]


def toml_of(layout: Layout, take) -> str:
    """The rows file `take_song` reads."""
    def quoted(text: str) -> str:
        return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'

    lines = [
        f"name = {quoted(take.name)}",
        f"bpm = {take.tempo!r}",
        f"rows = {layout.rows}",
        f"lanes = {layout.lanes}",
        "steps = [",
        *(f"  {quoted(step)}," for step in layout.steps),
        "]",
        "",
        "[settings]",
        *(f"{quoted(key)} = [{coarse}, {fine}]" for key, (coarse, fine) in layout.settings.items()),
        "",
    ]
    return "\n".join(lines)
