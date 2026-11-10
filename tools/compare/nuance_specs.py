"""Write the `nuance-*` specs: the Acid against two references at once, Arturia's Acid V and
AudioRealism's ABL3, for M27's nuance suite (#347, decision 125).

Usage: nuance_specs.py

The specs are written into `specs/`, and kept there, so this is only run again to change them.
`run.py --suite nuance` runs them all and writes an index of our gaps from the consensus.

- `nuance-knob-*`: decision 117's single knobs, `acid-cutoff` and the rest, read from their specs
  with ABL3 added beside Acid V.
- `nuance-accents`: accents in a row, `AAAA`, `A.A.` and `AA..`, at Res 0, 50% and 90%.
- `nuance-slides`: slides up and down by a semitone, a fifth and an octave.
- `nuance-legato`: two notes from 60 ms apart to overlapping, and slid, against each other.
- `nuance-gate`: one note held from a sixty-fourth to a quarter note.
- `nuance-extremes`: every knob at its bottom or top, with every other's, 32 cases.

ABL3 is set up as a stock 303 (README, "The plugins"): its hardware mods' trims at their stock
50%, no drive, distortion or extra high-pass, its noise and flutter off, and in its Note mode,
where MIDI notes play it rather than choose its patterns.
"""

from __future__ import annotations

import itertools
import textwrap
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPECS = HERE / "specs"

ACID_V = "Acid V"
ABL3 = "ABL3"
# Acid V as decision 117's specs set it: its Default preset with the sequencer off, its
# distortion, sub oscillator and hiss off, and an octave up, as it plays MIDI 36 at 32.7 Hz.
ACID_V_SECTION = {
    "name": ACID_V,
    "plugin": "/Library/Audio/Plug-Ins/VST3/Acid V.vst3",
    "buffer_size": 512,
    "accent_from": 100,
    "preset": "Default",
    "state": {"Sequencer_OnOff": 0},
}
ACID_V_FIXED = {
    "master_volume_db": 0.0,
    "osc_pitch_st": 12.0,
    "osc_waveform": 0.0,
    "distortion_dry_wet": 0.0,
    "sub_osc_volume_db": -70.0,
    "noise_db": -100.0,
}
# ABL3 in its Note mode (`MidiMode = 1`; Pattern mode, 0, plays a pattern for each key), with the
# extra attack click its manual says is there to help it cut through, and its output clipping,
# off. It's mono, stays silent after a reset until it's played a middle C, and makes a sound of
# its own for its first 90 ms, so it renders after a second's pre-roll that starts with one.
ABL3_SECTION = {
    "name": ABL3,
    "plugin": "/Library/Audio/Plug-Ins/VST3/ABL3x.vst3",
    "buffer_size": 512,
    "channels": 1,
    "accent_from": 101,
    "preroll": 1.0,
    "wake_note": 60,
    "state": {"MidiMode": 1, "VCAClicks": 0, "OutputClipping": 0},
}
ABL3_FIXED = {
    "waveform": "303 Saw",
    "volume": 0.75,
    "highpass": 0.0,
    "drive": 0.0,
    "distort": 0.0,
    "reso_trim": 0.5,
    "gate_trim": 0.5,
    "vcf_trim": 0.5,
    "bass_boost": 0.5,
    "treble_boost": 0.5,
    "detune": 0.5,
    "pregain": 0.0,
    "vco_base": 0.5,
    "noise_level": 0.0,
    "vco_shift": 0.0,
}
# Acid V's knobs by ABL3's names.
ABL3_NAMES = {"cutoff": "cutoff", "resonance": "resonance", "envelope_mod": "envmod", "decay": "decay", "accent": "accent"}

# Our Acid with a sawtooth and no drive, as decision 117's specs have it.
OURS = {"acid.wave": "Saw", "acid.drive": 0x00}

# A sixteenth at 120 BPM, in seconds, and an accent's and a plain note's velocity on every side.
SIXTEENTH = 0.125
GATE = 0.0625
PLAIN = 99
ACCENT = 112


def knobs(cutoff: float, resonance: float, envmod: float, decay: float, accent: float) -> tuple[dict, dict, dict]:
    """Our knobs and both references', from shares of each knob's turn."""
    ours = {
        "acid.cutoff": round(cutoff * 255),
        "acid.resonance": round(resonance * 255),
        "acid.envmod": round(envmod * 255),
        "acid.decay": round(decay * 255),
        "acid.accent": round(accent * 255),
    }
    acid_v = {"cutoff": cutoff, "resonance": resonance, "envelope_mod": envmod, "decay": decay, "accent": accent}
    return ours, acid_v, abl3_of(acid_v)


def abl3_of(acid_v: dict) -> dict:
    """ABL3's knobs for Acid V's, to the hundredth, its knobs' own step."""
    return {ABL3_NAMES[k]: round(float(v), 2) for k, v in acid_v.items() if k in ABL3_NAMES}


# Writing TOML.


def key(name: str) -> str:
    return name if all(c.isalnum() or c in "_-." for c in name) else f'"{name}"'


def value(v, hexadecimal: bool = False) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, str):
        return f'"{v}"'
    if isinstance(v, int):
        return f"0x{v:02X}" if hexadecimal else str(v)
    if isinstance(v, float):
        return repr(round(v, 6))
    if isinstance(v, dict):
        return inline(v, hexadecimal)
    if isinstance(v, list):
        return "[" + ", ".join(value(x, hexadecimal) for x in v) + "]"
    raise TypeError(v)


def inline(table: dict, hexadecimal: bool = False) -> str:
    return "{ " + ", ".join(f"{key(k)} = {value(v, hexadecimal)}" for k, v in table.items()) + " }"


def lines(table: dict, hexadecimal: bool = False) -> str:
    return "".join(f"{key(k)} = {value(v, hexadecimal)}\n" for k, v in table.items())


def note(time: float, length: float, number: int, accent: bool | None = None, slide: bool = False) -> dict:
    out = {"time": round(time, 6), "length": round(length, 6), "note": number}
    if accent is not None:
        out["velocity"] = ACCENT if accent else PLAIN
        out["accent"] = accent
    if slide:
        out["slide"] = True
    return out


def references(acid_v: dict, abl3: dict) -> str:
    text = ""
    for section, fixed, own in ((ACID_V_SECTION, ACID_V_FIXED, acid_v), (ABL3_SECTION, ABL3_FIXED, abl3)):
        body = {k: v for k, v in section.items()}
        text += "\n[[references]]\n" + lines(body)
        text += "\n[references.parameters]\n" + lines(fixed | own)
    return text


def case(name: str, ours: dict | None = None, acid_v: dict | None = None, abl3: dict | None = None, notes: list | None = None) -> str:
    text = f'\n[[cases]]\nname = "{name}"\n'
    if ours:
        text += f"ours = {inline(ours, hexadecimal=True)}\n"
    if acid_v or abl3:
        theirs = {}
        if acid_v:
            theirs[ACID_V] = acid_v
        if abl3:
            theirs[ABL3] = abl3
        text += f"references = {inline(theirs)}\n"
    if notes is not None:
        text += "notes = [\n" + "".join(f"  {inline(n)},\n" for n in notes) + "]\n"
    return text


def report(fields: dict, about: str) -> str:
    return "\n[report]\n" + lines(fields) + f'about = """\n{about}\n"""\n'


def write(name: str, comment: str, body: str) -> None:
    path = SPECS / f"{name}.toml"
    usage = f"#\n#   uv run run.py specs/{name}.toml --report out/{name}.html\n"
    text = "".join(f"# {line}".rstrip() + "\n" for line in comment.strip().splitlines()) + usage + "\n" + body
    path.write_text(text)
    # It must read back as TOML.
    tomllib.loads(text)
    print(f"wrote {path.relative_to(HERE)}")


ABOUT_REFERENCES = (
    "Every case renders on three sides: our Acid, Arturia's Acid V on its Default preset with its \\\n"
    "sequencer off, and AudioRealism's ABL3 set up as a stock 303, in its Note mode. Where Acid V \\\n"
    "and ABL3 agree, that's the target; where they disagree, both are shown. Acid V accents a note \\\n"
    "from velocity 100, ABL3 from 101 and ours from 0x70, so a plain note plays at 99 and an \\\n"
    "accent at 0x70 on every side."
)


# The specs.


def knob_specs() -> None:
    """Decision 117's single knobs, with ABL3 beside Acid V."""
    for stem in ("cutoff", "cutoff-peak", "resonance", "envmod", "decay", "accent", "accent-apart"):
        source = SPECS / f"acid-{stem}.toml"
        data = tomllib.loads(source.read_text())
        shared = dict(data["reference"]["parameters"])
        acid_v = {k: v for k, v in shared.items() if k not in ACID_V_FIXED}
        body = f"rate = {data.get('rate', 48000)}\nduration = {data['duration']}\n"
        for n in data["notes"]:
            body += "\n[[notes]]\n" + lines(n)
        body += "\n[ours]\n" + lines({"machine": data["ours"]["machine"]})
        body += "\n[ours.settings]\n" + lines(flatten(data["ours"]["settings"]), hexadecimal=True)
        body += references(acid_v, abl3_of(acid_v))
        layout = dict(data["report"])
        about = layout.pop("about").strip()
        knob = layout["reference_knob"]
        layout["reference_knob"] = {ACID_V: knob, ABL3: ABL3_NAMES[knob]}
        both = "against Acid V's and ABL3's" if "against Acid V's" in layout["title"] else "against Acid V and ABL3"
        layout["title"] = layout["title"].replace("against Acid V's", both).replace("against Acid V ", both + " ")
        wrapped = "\n\n".join(textwrap.fill(part, 96) for part in about.split("\n\n"))
        body += report(layout, wrapped + "\n\n" + ABOUT_REFERENCES)
        for c in data["cases"]:
            theirs = c["reference"]
            body += case(c["name"], flatten(c["ours"]), dict(theirs), abl3_of(theirs))
        write(
            f"nuance-knob-{stem}",
            f"Decision 117's `acid-{stem}` against both references, Arturia's Acid V and AudioRealism's\n"
            "ABL3, for the nuance suite (#347). Written by nuance_specs.py.",
            body,
        )


def flatten(table: dict, prefix: str = "") -> dict:
    out = {}
    for k, v in table.items():
        k = f"{prefix}.{k}" if prefix else k
        if isinstance(v, dict):
            out.update(flatten(v, k))
        else:
            out[k] = v
    return out


def header(duration: float, ours: dict, acid_v: dict, abl3: dict) -> str:
    body = f"rate = 48000\nduration = {duration}\n"
    body += '\n[ours]\nmachine = "acid"\n'
    body += "\n[ours.settings]\n" + lines(OURS | ours, hexadecimal=True)
    return body + references(acid_v, abl3)


def accents_spec() -> None:
    """Accents in a row and apart, twice through, after a plain note, at three resonances."""
    patterns = {"AAAA": "AAAAAAAA", "A.A.": "A.A.A.A.", "AA..": "AA..AA.."}
    ours, acid_v, abl3 = knobs(0.25, 0.5, 0.5, 0.5, 0.5)
    for d in (ours, acid_v, abl3):
        for k in [k for k in d if "reso" in k]:
            del d[k]
    start = 0.3
    duration = start + 8 * SIXTEENTH + 0.4
    body = header(duration, ours, acid_v, abl3)
    body += report(
        {"kind": "notes", "title": "Accents in a row and apart, against Acid V and ABL3", "columns": 3, "step": SIXTEENTH},
        "A plain C-2, then a pattern of accents twice through, a sixteenth a step at 120 BPM: \\\n"
        "`AAAA`, four accents in a row; `A.A.`, an accent every other step with a rest between; \\\n"
        "and `AA..`, two accents and two rests. Each pattern plays at Res 0, 50% and 90%, with \\\n"
        "Cutoff a quarter up and Env Mod, Decay and Accent at noon. A 303's accents build on each \\\n"
        "other when they're close, its accent sweep's capacitor charging from one to the next, so \\\n"
        "`AAAA` should brighten over its steps, and `A.A.` less.\n\n" + ABOUT_REFERENCES,
    )
    for name, steps in patterns.items():
        for share, label in ((0.0, "00"), (0.5, "80"), (0.9, "E6")):
            notes = [note(0.05, GATE, 36, accent=False)]
            notes += [note(start + i * SIXTEENTH, GATE, 36, accent=True) for i, step in enumerate(steps) if step == "A"]
            body += case(f"{name}-res{label}", {"acid.resonance": round(share * 255)}, {"resonance": share}, {"resonance": share}, notes)
    write(
        "nuance-accents",
        "Accents in a row, `AAAA`, `A.A.` and `AA..`, at Res 0, 50% and 90%, against Arturia's Acid V\n"
        "and AudioRealism's ABL3, for the nuance suite (#347). Written by nuance_specs.py.",
        body,
    )


def slides_spec() -> None:
    """Slides up and down by a semitone, a fifth and an octave."""
    ours, acid_v, abl3 = knobs(0.5, 0.25, 0.5, 0.5, 0.5)
    body = header(0.8, ours, acid_v, abl3)
    body += report(
        {"kind": "transition", "title": "Slides up and down, against Acid V and ABL3", "columns": 3},
        "Two notes, the first held into the second with its slide on, so each side glides from one \\\n"
        "to the other: up from C-2 and down to it, by a semitone, a fifth and an octave, a quarter \\\n"
        "of a second each. Cutoff, Env Mod, Decay and Accent are at noon and Res at a quarter, so \\\n"
        "the pitch is easy to track. A 303's slide is an RC lag of a fixed time, so it should take \\\n"
        "as long by a semitone as by an octave.\n\n" + ABOUT_REFERENCES,
    )
    for semitones, label in ((1, "semitone"), (7, "fifth"), (12, "octave")):
        for direction in ("up", "down"):
            low, high = 36, 36 + semitones
            first, second = (low, high) if direction == "up" else (high, low)
            notes = [note(0.05, 0.25, first, slide=True), note(0.3, 0.3, second)]
            body += case(f"{direction}-{label}", notes=notes)
    write(
        "nuance-slides",
        "Slides up and down by a semitone, a fifth and an octave, against Arturia's Acid V and\n"
        "AudioRealism's ABL3, for the nuance suite (#347). Written by nuance_specs.py.",
        body,
    )


def legato_spec() -> None:
    """Two notes a whole tone apart, from 60 ms apart to overlapping, and slid."""
    ours, acid_v, abl3 = knobs(0.5, 0.25, 0.5, 0.5, 0.5)
    body = header(0.8, ours, acid_v, abl3)
    body += report(
        {"kind": "transition", "title": "Legato against retriggered notes, against Acid V and ABL3", "columns": 3},
        "Two notes, a C-2 and then a D-2 starting at 0.3 s, with the first ending 60, 20, 5 and \\\n"
        "1 ms before the second, just as it starts, or overlapping it by 1 and 20 ms, and last \\\n"
        "held into it with its slide on. On a 303, and in a MIDI-played one, notes that overlap \\\n"
        "slide and keep the envelopes going, and notes that don't are played afresh: the gate \\\n"
        "closes and opens, so the level dips and the filter's envelope starts again. Where that \\\n"
        "changes over, and how deep the dip is at each gap, is what this measures. Knobs as in \\\n"
        "the slides.\n\n" + ABOUT_REFERENCES,
    )
    for gap in (60, 20, 5, 1, 0, -1, -20):
        name = f"gap-{gap}" if gap >= 0 else f"overlap{gap}"
        notes = [note(0.05, round(0.25 - gap / 1000.0, 6), 36), note(0.3, 0.3, 38)]
        body += case(name, notes=notes)
    body += case("slide", notes=[note(0.05, 0.25, 36, slide=True), note(0.3, 0.3, 38)])
    write(
        "nuance-legato",
        "Legato against retriggered notes, two notes from 60 ms apart to overlapping, against\n"
        "Arturia's Acid V and AudioRealism's ABL3, for the nuance suite (#347). Written by\n"
        "nuance_specs.py.",
        body,
    )


def gate_spec() -> None:
    """One note held for different lengths."""
    ours, acid_v, abl3 = knobs(0.5, 0.25, 0.5, 0.5, 0.5)
    body = header(1.0, ours, acid_v, abl3)
    body += report(
        {"kind": "gate", "title": "The gate's length, against Acid V and ABL3", "columns": 3},
        "One C-2 in each case, held for a 128th note at 120 BPM, about 16 ms, to a quarter, \\\n"
        "500 ms. How far its level falls while it's held, and how quickly it dies once it's let \\\n"
        "go, are the VCA's and the gate's timing. Knobs as in the slides.\n\n" + ABOUT_REFERENCES,
    )
    for name, length in (("128th", 15.625), ("64th", 31.25), ("32nd", 62.5), ("16th", 125.0), ("8th", 250.0), ("quarter", 500.0)):
        body += case(f"held-{name}", notes=[note(0.05, length / 1000.0, 36)])
    write(
        "nuance-gate",
        "One note held from a 128th to a quarter note, against Arturia's Acid V and AudioRealism's\n"
        "ABL3, for the nuance suite (#347). Written by nuance_specs.py.",
        body,
    )


def extremes_spec() -> None:
    """Every knob at its bottom or top, with every other's."""
    ours, acid_v, abl3 = knobs(0.0, 0.0, 0.0, 0.0, 0.0)
    body = header(1.0, ours, acid_v, abl3)
    body += report(
        {"kind": "notes", "title": "Every knob's extremes together, against Acid V and ABL3", "columns": 4, "step": SIXTEENTH},
        "Cutoff, Res, Env Mod, Decay and Accent each at the bottom or the top of its turn, every \\\n"
        "one of the 32 ways, each playing two plain C-2s and two accents a sixteenth apart at \\\n"
        "120 BPM. A case's name gives each knob's end, C for Cutoff, R Res, E Env Mod, D Decay and \\\n"
        "A Accent, 0 at the bottom and F at the top: `C0R0E0D0A0` has them all down. It's where \\\n"
        "knobs that each fit on their own can still part, as they interact.\n\n" + ABOUT_REFERENCES,
    )
    notes = [
        note(0.05, GATE, 36, accent=False),
        note(0.05 + SIXTEENTH, GATE, 36, accent=False),
        note(0.05 + 2 * SIXTEENTH, GATE, 36, accent=True),
        note(0.05 + 3 * SIXTEENTH, GATE, 36, accent=True),
    ]
    for top in itertools.product((False, True), repeat=5):
        shares = [1.0 if t else 0.0 for t in top]
        ours, acid_v, abl3 = knobs(*shares)
        name = "".join(f"{letter}{'F' if t else '0'}" for letter, t in zip("CREDA", top))
        body += case(name, ours, acid_v, abl3, notes)
    write(
        "nuance-extremes",
        "Every Acid knob at its bottom or top, with every other's, 32 cases, against Arturia's Acid V\n"
        "and AudioRealism's ABL3, for the nuance suite (#347). Written by nuance_specs.py.",
        body,
    )


def main() -> None:
    knob_specs()
    accents_spec()
    slides_spec()
    legato_spec()
    gate_spec()
    extremes_spec()


if __name__ == "__main__":
    main()
