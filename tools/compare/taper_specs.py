"""Write the `filter-taper-*` specs: each track filter type's Res knob turned in 33 steps at three
cutoffs, against the circuit of Cytomic's The Drop it's paired with (#333).

Usage: taper_specs.py

The specs are written into `specs/`, and kept there, so this is only run again to change them.
Each case is a quiet sine sweep, as in `filter-clean`, read by `report.py`'s `taper` kind.

The Drop's `lp_res` and `hp_res` run from 0 to 125%, and self-oscillate above 100. Our knob's
first 85%, 00 to D9, is set against The Drop's 0 to 100%, and its last 15% against 100 to 125%,
so a type that self-oscillates should start to near D9, as The Drop's circuits do at 100%.
"""

from __future__ import annotations

from pathlib import Path

HERE = Path(__file__).resolve().parent
SPECS = HERE / "specs"

# Our knob's steps, 00 to F8 by 08 and then FF.
STEPS = list(range(0x00, 0x100, 0x08)) + [0xFF]
# Three cutoffs, as `filter-clean` has: about 200 Hz, 988 Hz and 5 kHz.
CUTS = [0x55, 0x90, 0xCC]
# Where our knob is set against The Drop's 100%, as a share of its turn.
KNEE = 0.85
DROP_KNEE = 100.0
DROP_MOST = 125.0

PLUGIN = "/Library/Audio/Plug-Ins/VST3/The Drop.vst3"


def cut_hz(value: int) -> float:
    return 20.0 * 1000.0 ** (value / 255.0)


def drop_res(value: int) -> float:
    """The Drop's resonance, in %, for our Res on the 00 to FF scale."""
    share = value / 255.0
    if share <= KNEE:
        return round(DROP_KNEE * share / KNEE, 2)
    return round(DROP_KNEE + (DROP_MOST - DROP_KNEE) * (share - KNEE) / (1.0 - KNEE), 2)


HEADER = """rate = 48000
duration = 2.5

[input]
signal = "sweep"
from = 20.0
to = 20000.0
level = -30.0
seconds = 2.0

[ours]
"""

# The Drop as it's set for every case: no drive, nothing modulating, no glide between settings,
# its HD circuits, and its knob's full turn at 125%.
REFERENCE = """
[reference]
plugin = "{plugin}"
buffer_size = 512

[reference.parameters]
lp_drive = 0.0
hp_drive = 0.0
lp_hd = true
hp_hd = true
lp_safe_res = false
hp_safe_res = false
lp_resmax = 125.0
hp_resmax = 125.0
lp_keytrack = false
hp_keytrack = false
mod_active = false
glide = "0.000 mS"
mix = 100.0
pad = 0.0
"""


def report(title: str, about: str, reference_knob: list[str], width: list[str]) -> str:
    knobs = ", ".join(f'"{k}"' for k in reference_knob)
    widths = ", ".join(f'"{w}"' for w in width)
    return f'''
[report]
kind = "taper"
title = "{title}"
figure = ["filter", "mode"]
column = "cut"
knob = "res"
reference_knob = [{knobs}]
width = [{widths}]
about = """
{about}
"""
'''


def case(name: str, ours: dict, reference: dict | None) -> str:
    def value(v):
        if isinstance(v, bool):
            return "true" if v else "false"
        if isinstance(v, str):
            return f'"{v}"'
        if isinstance(v, int):
            return f"0x{v:02X}"
        return repr(v)

    def table(d):
        return "{ " + ", ".join(f"{k} = {value(v)}" for k, v in d.items()) + " }"

    text = f'\n[[cases]]\nname = "{name}"\nours = {table(ours)}\n'
    if reference is not None:
        text += f"reference = {table(reference)}\n"
    return text


def drop(section: str, circuit: str, four_pole: bool, cut: int, res: int, routing: str | None = None) -> dict:
    """The Drop's settings for one of its sections, or both, at our Cut and Res."""
    sections = ["lp", "hp"] if section == "both" else [section]
    out = {
        "lp_active": "lp" in sections,
        "hp_active": "hp" in sections,
    }
    if routing:
        out["routing"] = routing
    for s in sections:
        out[f"{s}_circuit"] = circuit
        out[f"{s}_4_pole"] = four_pole
        out[f"{s}_freq"] = round(cut_hz(cut), 2)
        out[f"{s}_res"] = drop_res(res)
    return out


def write(path: Path, comment: str, title: str, about: str, types, reference: bool) -> None:
    """`types` is (short name, our settings, the reference's section and how it's set)."""
    text = comment + "\n\n" + HEADER
    if reference:
        text += REFERENCE.format(plugin=PLUGIN)
    knobs = sorted({f"{t[2][0]}_res" if t[2][0] != "both" else "lp_res" for t in types if t[2]})
    width = [t[1]["filter"] for t in types if t[1]["filter"] in ("BP", "Notch")]
    text += report(title, about, knobs, width)
    for short, ours, theirs in types:
        for cut in CUTS:
            for res in STEPS:
                settings = dict(ours) | {"cut": cut, "res": res}
                setting = drop(*theirs[:3], cut, res, *theirs[3:]) if theirs else None
                text += case(f"{short}-{cut:02X}-res{res:02X}", settings, setting)
    path.write_text(text)
    print(f"wrote {path.relative_to(HERE)}")


ABOUT_KNOB = (
    "Res is turned in 33 steps, 00 to FF, at three cutoffs, about 200 Hz, 988 Hz and 5 kHz, with \\\n"
    "the same quiet sine sweep as the other filter reports. The Drop's resonance runs from 0 to \\\n"
    "125% and self-oscillates above 100, so our knob's first 85%, 00 to D9, is set against its 0 to \\\n"
    "100%, and the last 15% against 100 to 125%."
)


def main() -> None:
    write(
        SPECS / "filter-taper-svf.toml",
        "# The clean types' Res knob against Cytomic's The Drop's OSR, an OTA state-variable filter like\n"
        "# the OSCar's, at no drive: peak height, its frequency, the bass and self-oscillation, over 33\n"
        "# steps at three cutoffs (#333). Written by taper_specs.py.\n#\n"
        "#   uv run run.py specs/filter-taper-svf.toml --report out/filter-taper-svf.html",
        "Clean filters' Res against The Drop's OSR",
        "Our track filter's clean types, the state-variable low-pass, high-pass, band-pass and notch at \\\n"
        "12 dB, against the OSR circuit of Cytomic's The Drop, an OTA state-variable filter like the \\\n"
        "OSCar's, at no drive. The band-pass is The Drop's high-pass and low-pass in series at the same \\\n"
        "frequency, and the notch the two in parallel, as its manual makes them.\n\n" + ABOUT_KNOB,
        [
            ("lp12", {"filter": "LP", "mode": "12 dB"}, ("lp", "OSR", False)),
            ("hp12", {"filter": "HP", "mode": "12 dB"}, ("hp", "OSR", False)),
            ("bp12", {"filter": "BP", "mode": "12 dB"}, ("both", "OSR", False, "Serial")),
            ("notch12", {"filter": "Notch", "mode": "12 dB"}, ("both", "OSR", False, "Parallel")),
        ],
        reference=True,
    )
    write(
        SPECS / "filter-taper-ladder.toml",
        "# The Ladder's Res knob, at its 24 dB low-pass, against Cytomic's The Drop's PRD, a BJT ladder\n"
        "# like the Prodigy's, at four poles and no drive, over 33 steps at three cutoffs (#333). Written\n"
        "# by taper_specs.py.\n#\n"
        "#   uv run run.py specs/filter-taper-ladder.toml --report out/filter-taper-ladder.html",
        "Ladder's Res against The Drop's PRD",
        "Our track filter's Ladder in its 24 dB low-pass mode against the PRD circuit of Cytomic's The \\\n"
        "Drop, a four-pole BJT transistor ladder like Moog's Prodigy's, at no drive.\n\n" + ABOUT_KNOB,
        [("ladder", {"filter": "Ladder", "mode": "LP24"}, ("lp", "PRD", True))],
        reference=True,
    )
    for circuit, what in (("MS1", "the MS-20's first Korg 35, with its hard clipper"), ("MS2", "the MS-20's later LM13700 OTA")):
        suffix = "" if circuit == "MS1" else "-ms2"
        write(
            SPECS / f"filter-taper-ms20{suffix}.toml",
            f"# The MS20 types' Res knob against Cytomic's The Drop's {circuit}, {what}, at two poles and\n"
            "# no drive, over 33 steps at three cutoffs (#333). Written by taper_specs.py.\n#\n"
            f"#   uv run run.py specs/filter-taper-ms20{suffix}.toml --report out/filter-taper-ms20{suffix}.html",
            f"MS20 types' Res against The Drop's {circuit}",
            "Our track filter's MS20 LP and MS20 HP, Korg 35s, against the "
            f"{circuit} circuit of Cytomic's The \\\nDrop, {what}, at two poles and no drive.\n\n" + ABOUT_KNOB,
            [
                ("ms20lp", {"filter": "MS20 LP"}, ("lp", circuit, False)),
                ("ms20hp", {"filter": "MS20 HP"}, ("hp", circuit, False)),
            ],
            reference=True,
        )
    write(
        SPECS / "filter-taper-ours.toml",
        "# The Diode's and Dirty's Res knob, which have no circuit of The Drop to compare with, over 33\n"
        "# steps at three cutoffs, measured against their own drawings (#333). Written by taper_specs.py.\n#\n"
        "#   uv run run.py specs/filter-taper-ours.toml --report out/filter-taper-ours.html",
        "Diode's and Dirty's Res",
        "Our track filter's Diode, a 303's diode ladder, and Vital's Dirty filter, each at its 24 dB \\\n"
        "low-pass, which The Drop has no circuit like. Each is measured against its own drawing on the \\\n"
        "Sound page, and its taper set to be like the others'.\n\n"
        "Res is turned in 33 steps, 00 to FF, at three cutoffs, about 200 Hz, 988 Hz and 5 kHz, with \\\n"
        "the same quiet sine sweep as the other filter reports.",
        [
            ("diode", {"filter": "Diode", "mode": "Full"}, None),
            ("dirty", {"filter": "Dirty", "mode": "LP24"}, None),
        ],
        reference=False,
    )


if __name__ == "__main__":
    main()
