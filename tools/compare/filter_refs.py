"""The track filter's circuits against two independent references each: the circuit of Cytomic's
The Drop it's paired with (decision 120), and an Arturia filter modelled on the same circuit, with
where the two agree as the target, as the Acid's nuance suite has it (decision 125, `consensus.py`).

Usage: filter_refs.py [--circuit svf|obx|ladder|ms20 ...] [--page PAGE] [--json DATA] [--binary PATH]
                     [--fresh] [--write-specs]

| Ours | The Drop | Arturia |
|---|---|---|
| `LP`, `HP`, `BP`, `Notch` at 12 dB, the state-variable filter | OSR, two poles | Filter SEM, the Oberheim SEM's state-variable filter |
| `OB-X` in `LP12`, `HP12`, `BP12`, `N12`, OB-Xd's two-pole | OSR, two poles | Filter SEM |
| `Ladder`, `LP24` | PRD, four poles | Filter MINI, the Minimoog's transistor ladder |
| `MS20 LP`, `MS20 HP`, the Korg 35 | MS1, two poles | Filter MS-20 |

Every side's knobs are mapped to hertz and resonance by measuring them first, then each circuit is
measured the same way on every side:

- its response to a quiet sine sweep at three cutoffs, about 200 Hz, 988 Hz and 5 kHz, at Res 0,
  for the shape of its slope: the gain at Cut, an octave under it and an octave and two over it,
  each against its passband three octaves away;
- its Res knob in 33 steps at each cutoff, paired as `taper_specs.py` pairs them, our knob's
  first 85% against each reference's knob up to where it starts to self-oscillate, which this
  measures: how tall the peak stands over the passband at a quarter, half, three quarters and 95%
  of the way there, the tallest before it rings, and how much of the passband it takes away;
- where the peak sits against Cut, how wide it is, how steeply it falls past it and what's left of
  the passband, at the same height of peak, 3, 6 and 12 dB over the passband, read between the
  steps that stand either side of it, so the circuits' shapes are compared with their knobs' tapers
  taken out;
- its ringing on its own after a click at six cutoffs, just past where it starts and at the top of
  the knob: its pitch in cents off Cut and its level;
- a sine in its passband stepped from −36 to 0 dBFS at Cut 988 Hz, with no resonance and half way
  to ringing: its total harmonic distortion, its 2nd and 3rd harmonics and how far its gain falls
  from the quietest step's.

Reference renders are kept in `out/filter-refs/cache/`, so a run after a change to our code renders
only ours again and every run compares with the same reference renders. `--fresh` renders them
anew. Ours is kept by the binary's time and size, so a new build renders again.

The page and the JSON have each measure's consensus, as `run.py --suite` gives it: where the
references agree within a tolerance, their mean is the target and our gap is ours less it; where
they disagree, both are shown and the measure is flagged.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import html
import json
import math
import subprocess
import sys
import tempfile
import textwrap
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

import numpy as np

import consensus
import measure
import render_reference
import report
import spec as specs
from run import read_wav

HERE = Path(__file__).resolve().parent
REPOSITORY = HERE.parent.parent
OUT = HERE / "out" / "filter-refs"
CACHE = OUT / "cache"
RATE = 48_000
VST3 = "/Library/Audio/Plug-Ins/VST3"

# The quiet sweep every response is read from, as the `filter-*` specs have it.
SWEEP = {"signal": "sweep", "from": 20.0, "to": 20_000.0, "level": -30.0, "seconds": 2.0}
# A click for a filter to ring from.
CLICK = {"signal": "impulse", "level": -20.0, "seconds": 2.0}
# The three cutoffs, about 200 Hz, 988 Hz and 5 kHz, and Res's 33 steps, as `taper_specs.py` has.
CUTS = [0x55, 0x90, 0xCC]
STEPS = list(range(0x00, 0x100, 0x08)) + [0xFF]
# Where our knob is set against where each reference starts to ring, as a share of its turn.
KNEE = 0.85
# The heights of peak, over the passband, each circuit's shape is compared at.
RISES = [3.0, 6.0, 12.0]
# How far along the way to ringing the peak's height is read.
ALONG = [0.25, 0.5, 0.75, 0.95]
# The cutoffs ringing is measured at, an eighth of our knob apart.
RINGING_CUTS = [0x40, 0x60, 0x80, 0xA0, 0xC0, 0xE0]
# Just past where each side starts to ring, as a share of the way there: our `E8`.
JUST_PAST = 0xE8 / 255.0 / KNEE
# A filter rings when what's left after the click, a second on, is louder than this, in dBFS.
RINGING_DB = -60.0
# The stepped sine for drive, at Cut 988 Hz, two octaves into the passband.
DRIVE_CUT = 0x90
DRIVE_LEVELS = [-18.0, -12.0, -6.0, 0.0]
# Under this, a reference's response with its filter wide open is taken out of every other, for the
# coupling at its input, which takes 2 to 3 dB at 20 Hz on Filter SEM and Filter MINI.
OPEN_UNDER = 1_000.0
# Distortion under this is shown as this, as nothing under it is heard.
DISTORTION_FLOOR = -80.0


def cut_hz(value: int) -> float:
    return 20.0 * 1000.0 ** (value / 255.0)


def paired(share: float, onset: float) -> float:
    """A reference's knob, as a share of its turn, for ours: our first 85% against its turn up to
    where it starts to ring, and the rest against the rest, as `taper_specs.py` pairs The Drop's."""
    if share <= KNEE:
        return onset * share / KNEE
    return min(1.0, onset + (1.0 - onset) * (share - KNEE) / (1.0 - KNEE))


# The sides.


def _key(*parts) -> str:
    return hashlib.sha1(json.dumps(parts, sort_keys=True, default=str).encode()).hexdigest()[:20]


def _spec(given: dict, ours: dict | None = None, references: list | None = None) -> specs.Spec:
    data = {"rate": RATE, "input": dict(given)}
    if ours is not None:
        data["ours"] = {"settings": ours}
    if references:
        data["references"] = references
    return specs.Spec(path=CACHE / "filter-refs.toml", data=data)


def made(given: dict) -> np.ndarray:
    """The input every side is given, made as the probe makes it."""
    return specs.signal(_spec(given))


def _toml(value) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return json.dumps(value)
    return repr(value)


class Ours:
    """Our track filter, through `vimtracker probe`."""

    name = "ours"

    def __init__(self, binary: Path):
        self.binary = binary
        stat = binary.stat()
        self.build = (stat.st_mtime_ns, stat.st_size)

    def render(self, settings: dict, given: dict) -> np.ndarray:
        path = CACHE / f"ours-{_key(self.build, settings, given)}.npy"
        if path.exists():
            return np.load(path)
        lines = [f"rate = {RATE}", "[input]"]
        lines += [f"{key} = {_toml(value)}" for key, value in given.items()]
        lines.append("[ours.settings]")
        lines += [f'"{key}" = {_toml(value)}' for key, value in settings.items()]
        with tempfile.TemporaryDirectory() as folder:
            spec_path, out = Path(folder) / "probe.toml", Path(folder) / "ours.wav"
            spec_path.write_text("\n".join(lines) + "\n")
            result = subprocess.run([str(self.binary), "probe", str(spec_path), str(out)], capture_output=True, text=True)
            if result.returncode != 0:
                raise SystemExit(result.stderr.strip())
            samples = read_wav(out)
        np.save(path, samples)
        return samples


@dataclass
class Reference:
    """A reference plugin, loaded once, started from its factory preset and state if it has them,
    with `base` set for every render. A plugin that takes a new setting only at its next render,
    as Filter MS-20 does, is first given a buffer of silence with it (`settle`)."""

    name: str
    plugin: str
    base: dict
    preset: str | None = None
    state: dict | None = None
    settle: bool = False
    fresh: bool = False
    loaded: object = field(default=None, repr=False)

    def section(self, parameters: dict) -> dict:
        section = {"name": self.name, "plugin": f"{VST3}/{self.plugin}.vst3", "buffer_size": 512, "parameters": parameters}
        if self.preset:
            section["preset"] = self.preset
        if self.state:
            section["state"] = self.state
        if self.settle:
            section["settle"] = True
        return section

    def render(self, parameters: dict, given: dict) -> np.ndarray:
        parameters = self.base | parameters
        path = CACHE / f"{self.name.replace(' ', '-').lower()}-{_key(self.plugin, self.preset, self.state, parameters, given)}.npy"
        if path.exists() and not self.fresh:
            return np.load(path)
        section = self.section(parameters)
        if self.loaded is None:
            self.loaded = render_reference.Reference(section)
        samples = render_reference.render(_spec(given, references=[section]), None, self.loaded)
        np.save(path, samples)
        return samples


# How each side is set, for one of a circuit's responses (`kind`), at a cutoff in hertz and its
# Res knob as a share of its turn.
Setter = Callable[[str, float, float], dict]


@dataclass
class Side:
    """One side of a circuit: ours or a reference, how it's set, where it starts to ring, and the
    settings with its filter wide open, whose response is taken out of every other: all of it, for
    a second filter that can't be switched off, or only under `open_under` hertz, and its level
    there over it, for an input's coupling that takes a little of the deepest bass."""

    name: str
    source: object
    setter: Setter
    onset: float = KNEE
    open: Callable[[str], dict] | None = None
    open_under: float | None = None
    about: str = ""

    # Where it's measured to start ringing, for each kind, when `onset` isn't given.
    onsets: dict = field(default_factory=dict)

    def onset_of(self, kind: str) -> float:
        return self.onsets.get(kind, self.onset)

    def render(self, kind: str, hertz: float, share: float, given: dict) -> np.ndarray:
        return self.source.render(self.setter(kind, hertz, share), given)


@dataclass
class Circuit:
    key: str
    title: str
    kinds: dict  # kind: (label, high, width), width "band", "dip" or None
    sides: list
    about: str
    # The kinds driven and rung, which must have a peak that grows into ringing.
    rings: list = field(default_factory=list)
    driven: list = field(default_factory=list)


# The Drop's sections, set for a kind.


def the_drop(circuit: str, poles: bool) -> Setter:
    def set_it(kind: str, hertz: float, share: float) -> dict:
        res = round(125.0 * share, 3)
        sections = {"lp": ["lp"], "hp": ["hp"], "bp": ["lp", "hp"], "notch": ["lp", "hp"]}[kind]
        out = {"lp_active": "lp" in sections, "hp_active": "hp" in sections}
        if kind in ("bp", "notch"):
            out["routing"] = "Serial" if kind == "bp" else "Parallel"
        for section in sections:
            out |= {
                f"{section}_circuit": circuit,
                f"{section}_4_pole": poles,
                f"{section}_freq": round(hertz, 3),
                f"{section}_res": res,
            }
        return out

    return set_it


DROP_BASE = {
    "lp_drive": 0.0,
    "hp_drive": 0.0,
    "lp_hd": True,
    "hp_hd": True,
    "lp_safe_res": False,
    "hp_safe_res": False,
    "lp_resmax": 125.0,
    "hp_resmax": 125.0,
    "lp_keytrack": False,
    "hp_keytrack": False,
    "mod_active": False,
    "glide": "0.000 mS",
    "mix": 100.0,
    "pad": 0.0,
}
DROP_ABOUT = (
    "The Drop, as the filter-taper specs set it: no drive, nothing modulating, no glide, its HD "
    "circuits, its resonance's whole turn at 125%, which self-oscillates over 100%, and only the "
    "section measured switched on."
)


def our_setter(base: dict, types: dict) -> Setter:
    def set_it(kind: str, hertz: float, share: float) -> dict:
        cut = int(round(255.0 * math.log(hertz / 20.0) / math.log(1000.0)))
        return base | types[kind] | {"cut": cut, "res": int(round(255.0 * share))}

    return set_it


def drop_side(circuit: str, poles: bool, fresh: bool) -> Side:
    return Side(
        name="The Drop",
        source=Reference("The Drop", "The Drop", DROP_BASE, fresh=fresh),
        setter=the_drop(circuit, poles),
        onset=100.0 / 125.0,
        about=f"{DROP_ABOUT} Circuit {circuit}, {'four' if poles else 'two'} poles.",
    )


# Filter SEM.

SEM_BASE = {
    "input_gain_db": 0.0,
    "noise_volume_db": -120.0,
    "filter_out_db": 0.0,
    "dry_wet": 100.0,
    "soft_clip": False,
    "lfo_freq": 0.0,
    "lfo_res": 0.0,
    "lfo_mode": 0.0,
    "lfo_noise": 0.0,
    "lfo_filter_out": 0.0,
    "env_freq": 0.0,
    "env_res": 0.0,
    "env_mode": 0.0,
    "env_noise": 0.0,
    "env_filter_out": 0.0,
}
SEM_ABOUT = (
    "Filter SEM from its Default preset, its state given twice: input gain and filter output at 0 dB, "
    "dry/wet 100%, soft clip off, its noise at its lowest, −120 dB, and every LFO and envelope "
    "routing at 0, so its LFO, envelope and sequencer move nothing. Its cutoff knob is in hertz, from "
    "20 Hz to 15 kHz. Its mode knob morphs low-pass at 0, through a notch at 0.5, to high-pass at 1, "
    "and its band-pass is a switch. It doesn't self-oscillate at any resonance (measured): so its "
    "whole turn is set against ours up to 85%. Under 1 kHz, its response as a low-pass at 15 kHz is "
    "taken out of every other, for its input's coupling, which takes 3.2 dB at 20 Hz."
)


def sem(kind: str, hertz: float, share: float) -> dict:
    out = {"cutoff_hz": round(hertz, 3), "resonance": round(share, 4), "bandpass_on_off": kind == "bp"}
    out["multi_mode"] = {"lp": 0.0, "hp": 1.0, "notch": 0.5, "bp": 0.0}[kind]
    return out


# Filter MINI, whose cutoff knob has no hertz on it: it's mapped by measuring.

MINI_BASE = {
    "output_volume": 0.37,
    "drive": 0.0,
    "dry_wet": 1.0,
    "limit_resonance": True,
    "lfo_cut_mod_amount": 0.0,
    "lfo_res_mod_amount": 0.0,
    "lfo_seq_cut_mod": 0.0,
    "ef_cut_mod_amount": 0.0,
    "ef_res_mod_amount": 0.0,
    "ef_lfo_rate_mod": 0.0,
    "ss_cut_mod_amount": 0.0,
    "ss_res_mod_amount": 0.0,
    "ss_lfo_rate_mod": 0.0,
}
MINI_ABOUT = (
    "Filter MINI from its Default preset, its state given twice: output volume 0.37, as it loads, "
    "drive 0, dry/wet 1, and its LFO's, envelope follower's and step sequencer's modulation all at "
    "0. Its `limit_resonance` switch, measured, does the opposite of its name: off, which it loads "
    "with, the resonance stops growing at 0.74 and never rings; on, it rings from 0.75. So it's on, "
    "and its knob up to where it's measured to start ringing is set against ours up to 85%. Its cutoff knob runs 0 to 1 with no "
    "hertz on it, so it's mapped by measuring: at no resonance, a four-pole ladder's four equal "
    "poles each pass 3 dB less at its cutoff, 12 dB in all, so the knob is set where its response "
    "is 12 dB under its passband three octaves lower at the frequency asked for: measured at 51 "
    "points of its turn, with a parabola in octaves fitted through those from 400 Hz to 12 kHz and "
    "carried on under them, as at lower cutoffs a broad rise over its deepest bass moves the 12 dB "
    "point from one to the next. Under 1 kHz, its response with the knob fully open is taken out of every "
    "other, for its input's coupling, which takes 2.3 dB at 20 Hz."
)


class MiniTaper:
    """Filter MINI's cutoff knob against hertz, measured at Res 0 where it passes 12 dB under its
    passband, at 51 points of its turn, and a parabola in octaves fitted through those from 400 Hz
    to 12 kHz. Under 400 Hz its response at Res 0 has a broad rise over its deepest bass, 2.5 dB at
    a 200 Hz cutoff, which moves where it's 12 dB down from point to point, so the fit is carried
    on down from the points over it."""

    LOWEST, HIGHEST = 400.0, 12_000.0

    def __init__(self, side: "Side"):
        knobs = np.linspace(0.0, 1.0, 51)
        found = []
        given = made(SWEEP)
        for knob in knobs:
            samples = side.source.render({"cutoff": round(float(knob), 4), "resonance": 0.0}, SWEEP)
            f, g = measure.response(*response(side, "lp", samples, given))
            found.append(twelve_down(f, g))
        found = np.array(found)
        fitted = np.isfinite(found) & (found >= self.LOWEST) & (found <= self.HIGHEST)
        self.fit = np.polyfit(knobs[fitted], np.log2(found[fitted]), 2)
        self.knobs = np.linspace(0.0, 1.0, 10_001)
        self.octaves = np.polyval(self.fit, self.knobs)
        if np.any(np.diff(self.octaves) <= 0):
            raise SystemExit("Filter MINI's fitted cutoff doesn't rise all the way along its knob")

    def knob(self, hertz: float) -> float:
        octave = math.log2(hertz)
        if not self.octaves[0] <= octave <= self.octaves[-1]:
            raise SystemExit(f"Filter MINI's cutoff reaches {2 ** self.octaves[0]:.0f} to {2 ** self.octaves[-1]:.0f} Hz, not {hertz:.0f}")
        return float(np.interp(octave, self.octaves, self.knobs))


def twelve_down(f: np.ndarray, g: np.ndarray) -> float:
    """Where a low-pass first passes 12 dB under its passband three octaves under it, or at the
    sweep's lowest frequency, or NaN if it never does within the sweep."""
    passband = np.interp(np.maximum(np.log2(f) - 3.0, math.log2(f[0])), np.log2(f), g)
    under = np.where(g - passband < -12.0)[0]
    if under.size == 0 or under[0] == 0 or not np.isfinite(passband[under[0] - 1]):
        return float("nan")
    i = under[0]
    a, b = g[i - 1] - passband[i - 1], g[i] - passband[i]
    x0, x1 = math.log2(f[i - 1]), math.log2(f[i])
    return 2.0 ** (x0 + (x1 - x0) * (-12.0 - a) / (b - a))


# Filter MS-20.

MS20_BASE = {
    "fx_level_db": 0.0,
    "movement": 100.0,
    "global_mix": 100.0,
    "master_cutoff": 0.0,
    "stereo": 0.0,
    "limit_resonance": False,
    "filter_mode": "Clean",
    "distortion_drive_db": 0.0,
    "distortion_dry_wet": 0.0,
}
MS20_ABOUT = (
    "Filter MS-20 from its Default preset, its state given twice: its output at 0 dB rather than "
    "the preset's −1.52, its distortion's dry/wet at 0, as at 100% even with no drive its own "
    "filters dull the top by 10 dB at 16 kHz, its `Clean` mode, which is linear until it clips at "
    "0 dBFS, where `Dirty` clips everything over −12 dBFS, and `limit_resonance` off, which lets it "
    "ring, as its name says. Its function generator, envelope follower and sequencer modulate "
    "nothing as it loads. It has a high-pass and a low-pass in series, so the one not measured is "
    "left open, the high-pass at 20 Hz or the low-pass at 20 kHz, with no peak, and that open "
    "response is taken out of every response measured: it lifts the bass by up to 4.6 dB around "
    "60 Hz and takes 13 dB off at 16 kHz. It takes a new setting only at its next render, so each "
    "render is given a buffer of silence first."
)


def ms20(kind: str, hertz: float, share: float) -> dict:
    if kind == "lp":
        return {"lpf_cutoff_frequency_hz": round(hertz, 3), "lpf_peak": round(100.0 * share, 2), "hpf_cutoff_frequency_hz": 20.0, "hpf_peak": 0.0}
    return {"hpf_cutoff_frequency_hz": round(hertz, 3), "hpf_peak": round(100.0 * share, 2), "lpf_cutoff_frequency_hz": 20_000.0, "lpf_peak": 0.0}


def ms20_open(kind: str) -> dict:
    return {"lpf_cutoff_frequency_hz": 20_000.0, "lpf_peak": 0.0, "hpf_cutoff_frequency_hz": 20.0, "hpf_peak": 0.0}


def circuits(binary: Path, fresh: bool) -> dict:
    ours = Ours(binary)
    mini_source = Reference("Filter MINI", "Filter MINI", MINI_BASE, preset="Default", fresh=fresh)
    taper = {}

    def mini(kind: str, hertz: float, share: float) -> dict:
        if "map" not in taper:
            taper["map"] = MiniTaper(mini_side)
        return {"cutoff": round(taper["map"].knob(hertz), 4), "resonance": round(share, 4)}

    mini_side = Side(
        "Filter MINI", mini_source, mini, onset=float("nan"), open=lambda kind: {"cutoff": 1.0, "resonance": 0.0}, open_under=OPEN_UNDER, about=MINI_ABOUT
    )
    svf_types = {kind: {"filter": name, "mode": "12 dB"} for kind, name in (("lp", "LP"), ("hp", "HP"), ("bp", "BP"), ("notch", "Notch"))}
    obx_types = {kind: {"filter": "OB-X", "mode": mode} for kind, mode in (("lp", "LP12"), ("hp", "HP12"), ("bp", "BP12"), ("notch", "N12"))}
    return {
        "svf": Circuit(
            key="svf",
            title="The clean state-variable filter against OSR and Filter SEM",
            kinds={"lp": ("LP", False, None), "hp": ("HP", True, None), "bp": ("BP", False, "band"), "notch": ("Notch", False, "dip")},
            sides=[
                Side("ours", ours, our_setter({}, svf_types), about="Our `LP`, `HP`, `BP` and `Notch` at 12 dB, no drive."),
                drop_side("OSR", False, fresh),
                Side(
                    "Filter SEM",
                    Reference("Filter SEM", "Filter SEM", SEM_BASE, preset="Default", fresh=fresh),
                    sem,
                    onset=1.0,
                    open=lambda kind: {"cutoff_hz": 15_000.0, "resonance": 0.0, "multi_mode": 0.0, "bandpass_on_off": False},
                    open_under=OPEN_UNDER,
                    about=SEM_ABOUT,
                ),
            ],
            about=(
                "The Drop's band-pass is its high-pass and low-pass in series and its notch the two "
                "in parallel, as its manual makes them, so its band-pass falls at 12 dB an octave "
                "either side where a two-pole band-pass, ours and the SEM's, falls at 6. The SEM's "
                "notch, half way along its mode knob, is half its low-pass and half its high-pass, "
                "6 dB down, so a notch's width is read against its own passband."
            ),
            rings=["lp"],
            driven=["lp", "hp"],
        ),
        "obx": Circuit(
            key="obx",
            title="OB-X against OSR and Filter SEM",
            kinds={"lp": ("OB-X LP12", False, None), "hp": ("OB-X HP12", True, None), "bp": ("OB-X BP12", False, "band"), "notch": ("OB-X N12", False, "dip")},
            sides=[
                Side("ours", ours, our_setter({}, obx_types), about="Our `OB-X` in `LP12`, `HP12`, `BP12` and `N12`, no drive."),
                drop_side("OSR", False, fresh),
                Side(
                    "Filter SEM",
                    Reference("Filter SEM", "Filter SEM", SEM_BASE, preset="Default", fresh=fresh),
                    sem,
                    onset=1.0,
                    open=lambda kind: {"cutoff_hz": 15_000.0, "resonance": 0.0, "multi_mode": 0.0, "bandpass_on_off": False},
                    open_under=OPEN_UNDER,
                    about=SEM_ABOUT,
                ),
            ],
            about=(
                "OB-Xd's two-pole is an SEM-style state-variable filter whose damping grows with its "
                "level, so it's set against the clean types' pair. Unlike the clean types it rings "
                "by itself, as OSR does over its 100%, so its ringing's pitch and level are measured "
                "too. The SEM never rings."
            ),
            rings=["lp", "hp"],
            driven=["lp", "hp"],
        ),
        "ladder": Circuit(
            key="ladder",
            title="The Ladder against PRD and Filter MINI",
            kinds={"lp": ("Ladder", False, None)},
            sides=[
                Side("ours", ours, our_setter({}, {"lp": {"filter": "Ladder", "mode": "LP24"}}), about="Our `Ladder` in `LP24`, no drive."),
                drop_side("PRD", True, fresh),
                mini_side,
            ],
            about="",
            rings=["lp"],
            driven=["lp"],
        ),
        "ms20": Circuit(
            key="ms20",
            title="The MS20 types against MS1 and Filter MS-20",
            kinds={"lp": ("MS20 LP", False, None), "hp": ("MS20 HP", True, None)},
            sides=[
                Side("ours", ours, our_setter({}, {"lp": {"filter": "MS20 LP"}, "hp": {"filter": "MS20 HP"}}), about="Our `MS20 LP` and `MS20 HP`, no drive."),
                drop_side("MS1", False, fresh),
                Side(
                    "Filter MS-20",
                    Reference("Filter MS-20", "Filter MS-20", MS20_BASE, preset="Default", settle=True, fresh=fresh),
                    ms20,
                    onset=float("nan"),
                    open=ms20_open,
                    about=MS20_ABOUT,
                ),
            ],
            about=(
                "The Drop's MS1 is the MS-20's first filter, the Korg 35, which ours is; its MS2, the "
                "later OTA one, is in the filter-taper-ms20-ms2 spec."
            ),
            rings=["lp", "hp"],
            driven=["lp", "hp"],
        ),
    }


# Measuring.


def response(side: Side, kind: str, samples: np.ndarray, given: np.ndarray):
    bins, gains = measure.bin_response(given, samples, RATE, SWEEP["from"], SWEEP["to"])
    if side.open is not None:
        open_samples = side.source.render(side.open(kind), SWEEP)
        _, open_gains = measure.bin_response(given, open_samples, RATE, SWEEP["from"], SWEEP["to"])
        if side.open_under is not None:
            over = bins >= side.open_under
            open_gains = np.where(over, open_gains[np.argmax(over)], open_gains)
        gains = gains - open_gains
    return bins, gains


def read_sweep(side: Side, kind: str, samples: np.ndarray, given: np.ndarray, nominal: float, high: bool, width: str | None) -> dict:
    """What a sweep says of one setting, each level against the passband three octaves away, or for
    a band-pass, against its peak."""
    bins, gains = response(side, kind, samples, given)
    f, g = measure.response(bins, gains)
    passband = measure.gain_at(f, g, report.passband_hz(nominal, high))
    level = measure.highest(bins, gains, nominal)[1] if width == "band" else passband
    away = 2.0 if high else 0.5
    values = {
        "passband": passband,
        "at Cut": measure.gain_at(f, g, nominal) - level,
        "octave in": measure.gain_at(f, g, nominal * away) - level,
        "octave out": measure.gain_at(f, g, min(nominal / away, report.TOP_PASSBAND_HZ)) - level,
        "two octaves out": measure.gain_at(f, g, min(nominal / away**2, report.TOP_PASSBAND_HZ)) - level,
        "rings": measure.rings_on(samples, RATE, SWEEP["seconds"], nominal),
    }
    if width == "dip":
        inside = np.where((bins >= nominal / 2.0) & (bins <= nominal * 2.0))[0]
        at = inside[np.argmin(gains[inside])]
        values["centre"] = float(bins[at])
        values["width"] = measure.width(bins, gains - passband, nominal, dip=True)
    elif width == "band":
        peak_hz, _ = measure.highest(bins, gains, nominal)
        values["centre"] = peak_hz
        values["width"] = measure.width(bins, gains, nominal, dip=False)
    else:
        peak_hz, peak_db = measure.highest(bins, gains, nominal)
        values["peak"] = peak_hz
        values["rise"] = peak_db - passband
        values["width"] = measure.width(bins, gains, nominal, dip=False)
        past = min(peak_hz / away, report.TOP_PASSBAND_HZ)
        values["past peak"] = measure.gain_at(f, g, past) - peak_db
    if "centre" in values:
        # Read between steps in octaves.
        values["centre octave"] = math.log2(values["centre"]) if values["centre"] > 0 else float("nan")
    if values["rings"]:
        for key in ("peak", "rise", "width", "past peak", "centre", "centre octave"):
            if key in values:
                values[key] = float("nan")
    return values


def between(steps: list[dict], key: str, target: float, wanted: list[str]) -> dict:
    """The wanted values where `key` first reaches `target`, read between the two steps either side
    of it, or NaN if it never does before ringing."""
    for before, after in zip(steps, steps[1:]):
        a, b = before.get(key, float("nan")), after.get(key, float("nan"))
        if not (math.isfinite(a) and math.isfinite(b)):
            continue
        if a < target <= b:
            t = (target - a) / (b - a)
            out = {}
            for name in wanted:
                x, y = before.get(name, float("nan")), after.get(name, float("nan"))
                if name in ("peak", "centre"):
                    out[name] = 2.0 ** (math.log2(x) + t * (math.log2(y) - math.log2(x))) if x > 0 and y > 0 else float("nan")
                else:
                    out[name] = x + t * (y - x)
            return out
    return {name: float("nan") for name in wanted}


def along(steps: list[dict], key: str, place: float) -> float:
    """A value at a place along the way to ringing, read between steps."""
    xs = [s["along"] for s in steps]
    ys = [s.get(key, float("nan")) for s in steps]
    for i in range(len(xs) - 1):
        if xs[i] <= place <= xs[i + 1]:
            if not (math.isfinite(ys[i]) and math.isfinite(ys[i + 1])):
                return float("nan")
            t = (place - xs[i]) / (xs[i + 1] - xs[i])
            return ys[i] + t * (ys[i + 1] - ys[i])
    return float("nan")


def find_onset(side: Side, kind: str) -> float:
    """Where a side starts to ring at Cut 988 Hz, as a share of its knob, by halving between a
    setting that doesn't ring and one that does."""
    nominal = cut_hz(0x90)
    low, high = 0.5, 1.0
    if not measure.rings_on(side.render(kind, nominal, high, SWEEP), RATE, SWEEP["seconds"], nominal):
        return float("nan")
    for _ in range(9):
        middle = (low + high) / 2.0
        if measure.rings_on(side.render(kind, nominal, middle, SWEEP), RATE, SWEEP["seconds"], nominal):
            high = middle
        else:
            low = middle
    return high


def record(records: list, name: str, case: str, unit: str, values: dict) -> None:
    records.append({"measure": name, "case": case, "unit": unit, "values": values})


def measure_circuit(circuit: Circuit, log=print) -> dict:
    given = made(SWEEP)
    names = [side.name for side in circuit.sides]
    records, steps_by, notes = [], {}, []
    onsets = {}
    for side in circuit.sides:
        for kind in circuit.kinds:
            onsets[(side.name, kind)] = find_onset(side, kind) if kind in circuit.rings else float("nan")
            if side.name != "ours" and not math.isfinite(side.onset):
                # Set against where it's measured to start ringing, in each kind.
                side.onsets[kind] = onsets[(side.name, kind)]
                notes.append(f"{side.name} {circuit.kinds[kind][0]} is set against ours by where it starts to ring, {side.onsets[kind]:.3f} of its knob.")
    for kind, (label, high, width) in circuit.kinds.items():
        for cut in CUTS:
            nominal = cut_hz(cut)
            case = f"{label} at {report.hertz(nominal)}"
            per_side = {}
            for side in circuit.sides:
                steps = []
                for step in STEPS:
                    share = step / 255.0
                    knob = share if side.name == "ours" else paired(share, side.onset_of(kind))
                    samples = side.render(kind, nominal, knob, SWEEP)
                    values = read_sweep(side, kind, samples, given, nominal, high, width)
                    values |= {"step": step, "knob": knob, "along": share / KNEE}
                    steps.append(values)
                base = steps[0]["passband"]
                for values in steps:
                    values["bass"] = values["passband"] - base
                per_side[side.name] = steps
                log(f"{circuit.key} {kind} {cut:02X} {side.name}: measured", file=sys.stderr)
            steps_by[(kind, cut)] = per_side

            def each(key, steps_of=lambda s: s[0]):
                return {name: steps_of(per_side[name]).get(key, float("nan")) for name in names}

            # The slope's shape with no resonance.
            for key, text in (("at Cut", "at Cut"), ("octave in", "an octave into the passband"), ("octave out", "an octave past Cut"), ("two octaves out", "two octaves past Cut")):
                record(records, f"Res 0: gain {text}, against the {'peak' if width == 'band' else 'passband'}", case, "dB", each(key))
            if width:
                record(records, "Res 0: width, 3 dB down", case, "octaves", each("width"))
                for place in ALONG:
                    record(
                        records,
                        f"width {int(place * 100)}% of the way to ringing",
                        case,
                        "octaves",
                        {name: along(per_side[name], "width", place) for name in names},
                    )
                    record(
                        records,
                        f"centre {int(place * 100)}% of the way to ringing",
                        case,
                        "Hz",
                        {name: 2.0 ** along(per_side[name], "centre octave", place) for name in names},
                    )
                continue
            # The taper: the peak's height along the way to ringing, and the passband it takes.
            for place in ALONG:
                record(records, f"peak over the passband {int(place * 100)}% of the way to ringing", case, "dB", {name: along(per_side[name], "rise", place) for name in names})
                record(records, f"passband against Res 0, {int(place * 100)}% of the way to ringing", case, "dB", {name: along(per_side[name], "bass", place) for name in names})
            tallest = {}
            for name in names:
                standing = [s for s in per_side[name] if math.isfinite(s.get("rise", float("nan")))]
                tallest[name] = max(standing, key=lambda s: s["rise"]) if standing else {}
            record(records, "the tallest peak before ringing, over the passband", case, "dB", {n: tallest[n].get("rise", float("nan")) for n in names})
            record(records, "the passband against Res 0 at the tallest peak", case, "dB", {n: tallest[n].get("bass", float("nan")) for n in names})
            # The shape at the same height of peak.
            for rise in RISES:
                found = {name: between(per_side[name], "rise", rise, ["peak", "bass", "width", "past peak", "along"]) for name in names}
                record(records, f"peak's place at a {rise:g} dB peak", case, "Hz", {n: found[n]["peak"] for n in names})
                record(records, f"passband against Res 0 at a {rise:g} dB peak", case, "dB", {n: found[n]["bass"] for n in names})
                if rise >= 6.0:
                    record(records, f"peak's width at a {rise:g} dB peak", case, "octaves", {n: found[n]["width"] for n in names})
                    record(records, f"an octave past the peak at a {rise:g} dB peak, under it", case, "dB", {n: found[n]["past peak"] for n in names})
                record(records, f"how far to ringing a {rise:g} dB peak is", case, "octaves", {n: found[n]["along"] for n in names})

    # Ringing on its own after a click.
    ringing = []
    for kind in circuit.rings:
        label = circuit.kinds[kind][0]
        for cut in RINGING_CUTS:
            nominal = cut_hz(cut)
            for place, text in ((JUST_PAST, "just past where it starts"), (1.0 / KNEE, "at the top of the knob")):
                values_hz, values_db = {}, {}
                for side in circuit.sides:
                    share = place * KNEE
                    knob = min(1.0, share) if side.name == "ours" else paired(min(1.0, share), side.onset_of(kind))
                    samples = side.render(kind, nominal, knob, CLICK)
                    frequency, level = measure.pitch(samples, RATE, 1.0, 2.0)
                    rings = level > RINGING_DB
                    values_hz[side.name] = measure.cents(frequency, nominal) if rings else float("nan")
                    values_db[side.name] = level if rings else float("nan")
                    ringing.append({"kind": label, "cut": nominal, "place": text, "side": side.name, "Hz": frequency if rings else float("nan"), "dBFS": level})
                case = f"{label} at {report.hertz(nominal)}"
                record(records, f"ringing {text}: its pitch off Cut", case, "cents", values_hz)
                record(records, f"ringing {text}: its level, dBFS peak", case, "dB", values_db)
        log(f"{circuit.key} {kind}: ringing measured", file=sys.stderr)

    # Drive: a sine stepped in level in the passband.
    distortion = []
    for kind in circuit.driven:
        label, high, _ = circuit.kinds[kind]
        nominal = cut_hz(DRIVE_CUT)
        sine = round(nominal * (4.0 if high else 0.25), 2)
        steps = {"signal": "steps", "frequency": sine, "from": -36.0, "to": 0.0, "step": 6.0, "hold": 0.3}
        for place, text in ((0.0, "with no resonance"), (0.5, "half way to ringing")):
            for side in circuit.sides:
                share = place * KNEE
                knob = share if side.name == "ours" else paired(share, side.onset_of(kind))
                samples = side.render(kind, nominal, knob, steps)
                found = measure.step_harmonics(steps, samples, RATE)
                quiet = found[0]["out dBFS"] - found[0]["level"]
                for f in found:
                    distortion.append({"kind": label, "place": text, "side": side.name, "level": f["level"], "THD": f["THD dB"], "2nd": f["harmonics dB"][0], "3rd": f["harmonics dB"][1], "gain": f["out dBFS"] - f["level"] - quiet})
            case = f"{label} {text}, {report.hertz(sine)} at Cut {report.hertz(nominal)}"
            for level in DRIVE_LEVELS:
                chosen = {d["side"]: d for d in distortion if d["kind"] == label and d["place"] == text and d["level"] == level}
                floor = lambda v: max(v, DISTORTION_FLOOR)  # noqa: E731
                record(records, f"THD at {level:g} dBFS", case, "dB", {n: floor(chosen[n]["THD"]) for n in names})
                record(records, f"2nd harmonic at {level:g} dBFS", case, "dB", {n: floor(chosen[n]["2nd"]) for n in names})
                record(records, f"3rd harmonic at {level:g} dBFS", case, "dB", {n: floor(chosen[n]["3rd"]) for n in names})
                record(records, f"gain at {level:g} dBFS against at −36", case, "dB", {n: chosen[n]["gain"] for n in names})
        log(f"{circuit.key} {kind}: drive measured", file=sys.stderr)

    references = names[1:]
    verdicts = [consensus.judge(r, references) for r in records]
    return {
        "circuit": circuit,
        "records": records,
        "verdicts": verdicts,
        "rows": consensus.summarize(verdicts),
        "steps": steps_by,
        "onsets": {f"{side} {kind}": value for (side, kind), value in onsets.items()},
        "ringing": ringing,
        "distortion": distortion,
        "notes": notes,
    }


# Showing it.


def plots(result: dict) -> list[str]:
    import matplotlib.pyplot as plt

    circuit = result["circuit"]
    names = [side.name for side in circuit.sides]
    colours = {name: report.OURS if name == "ours" else report.REFERENCES[i % len(report.REFERENCES)] for i, name in enumerate(names[1:])} | {"ours": report.OURS}
    figures = []
    for kind, (label, high, width) in circuit.kinds.items():
        key = "width" if width else "rise"
        figure, axes = plt.subplots(2, len(CUTS), figsize=(3.6 * len(CUTS), 5.4), squeeze=False)
        for j, cut in enumerate(CUTS):
            per_side = result["steps"][(kind, cut)]
            for name in names:
                steps = per_side[name]
                axes[0][j].plot([s["along"] for s in steps], [s.get(key, float("nan")) for s in steps], color=colours[name], marker="o", markersize=2.5, label="vimtracker" if name == "ours" else name)
                axes[1][j].plot([s["along"] for s in steps], [s["bass"] for s in steps], color=colours[name], marker="o", markersize=2.5)
            axes[0][j].set_title(f"{label}, Cut {report.hertz(cut_hz(cut))}", loc="left")
            axes[1][j].set_xlabel("Of the way to ringing")
        axes[0][0].set_ylabel("Width, octaves" if width else "Peak over passband, dB")
        axes[1][0].set_ylabel("Passband against Res 0, dB")
        axes[0][-1].legend(fontsize=7.5)
        figure.tight_layout()
        figures.append(f"<h3>{html.escape(label)}: the peak and the passband along Res</h3>{report.image(figure)}")
    if result["distortion"]:
        groups = sorted({(d["kind"], d["place"]) for d in result["distortion"]})
        figure, axes = plt.subplots(1, len(groups), figsize=(3.4 * len(groups), 3.0), squeeze=False, sharey=True)
        for i, (label, place) in enumerate(groups):
            for name in names:
                chosen = [d for d in result["distortion"] if d["kind"] == label and d["place"] == place and d["side"] == name]
                axes[0][i].plot([d["level"] for d in chosen], [max(d["THD"], -120.0) for d in chosen], color=colours[name])
            axes[0][i].set_title(f"{label}, {place}", loc="left", fontsize=8)
            axes[0][i].set_xlabel("Input, dBFS")
        axes[0][0].set_ylabel("THD, dB")
        figure.tight_layout()
        figures.append(f"<h3>Distortion against level</h3>{report.image(figure)}")
    return figures


def page(results: list[dict], path: Path) -> None:
    sections = []
    for result in results:
        circuit = result["circuit"]
        references = [side.name for side in circuit.sides[1:]]
        consensus_html, _ = consensus.section(result["records"], references)
        about = "".join(f"<li><b>{html.escape(side.name)}</b>: {html.escape(side.about)}</li>" for side in circuit.sides)
        onsets = ", ".join(f"{name}: {value:.3f}" for name, value in result["onsets"].items() if math.isfinite(value))
        sections.append(
            f"<h2>{html.escape(circuit.title)}</h2><ul>{about}</ul><p>{html.escape(circuit.about)}</p>"
            f"<p class=\"note\">Where each side starts to ring at 988 Hz, as a share of its knob's turn: {html.escape(onsets or 'none')}. "
            f"{html.escape(' '.join(result['notes']))}</p>"
            f"<h3>The consensus</h3>{consensus_html}{''.join(plots(result))}"
        )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Filter references</title>
<style>{report.STYLE}</style>
</head>
<body>
<main>
<h1>The track filter against two references each</h1>
<p class="meta">filter_refs.py · {RATE} Hz · made {datetime.date.today().isoformat()}</p>
<p>Each circuit of our track filter against two independent references, The Drop's circuit and an
Arturia filter modelled on the same one. Where the two agree within a tolerance, their mean is the
target and our gap is ours less it; where they disagree, both are shown and the measure is flagged
(⚑). Every response is read from the same quiet sine sweep at −30 dBFS; each side's Res knob is
turned in 33 steps, our first 85% against each reference's turn up to where it starts to
self-oscillate. See <code>filter_refs.py</code>'s docstring for every measure.</p>
{"".join(sections)}
</main>
</body>
</html>
"""
    )


def summary_text(results: list[dict]) -> str:
    """Each circuit's measures in plain text: how many cases the references agree on, our mean
    signed gap there, and how far apart they are where they don't."""
    lines = []
    for result in results:
        lines.append(f"\n== {result['circuit'].title}")
        lines.append(f"{'measure':72} {'agree':>7} {'gap':>8} {'tol':>6} {'apart':>8}")
        for row in result["rows"]:
            gap = row.get("mean signed gap", float("nan"))
            apart = row.get("mean apart", float("nan"))
            lines.append(
                f"{row['measure'][:72]:72} {row['agreed']:>3}/{row['cases']:<3} {gap:+8.2f} {row.get('score', float('nan')):6.2f} {apart:8.2f} {row['gap unit']}"
            )
    return "\n".join(lines)


# The specs, so `run.py` draws each circuit's responses on every side, and each reference's state is
# written down where the other specs keep theirs.

# How far along the way to ringing a spec's rows are: Res 0, half way and 95%.
SPEC_ALONG = [0.0, 0.5, 0.95]


def toml_inline(table: dict) -> str:
    def key(name: str) -> str:
        return name if name.replace("_", "").isalnum() else json.dumps(name)

    def value(item) -> str:
        if isinstance(item, dict):
            return toml_inline(item)
        if isinstance(item, int) and not isinstance(item, bool):
            return f"0x{item:02X}" if item <= 0xFF else str(item)
        return _toml(item)

    return "{ " + ", ".join(f"{key(k)} = {value(v)}" for k, v in table.items()) + " }"


def write_spec(circuit: Circuit) -> Path:
    """`specs/filter-refs-<circuit>.toml`: each kind at three cutoffs, at Res 0, half way to ringing
    and 95% of the way, ours and both references, as `run.py`'s response report reads them."""
    path = HERE / "specs" / f"filter-refs-{circuit.key}.toml"
    head = (
        f"{circuit.title}, written by filter_refs.py --write-specs, which measures far more of each "
        "circuit; this draws the responses, as each side gives them, with nothing taken out. Each "
        "reference's state is as filter_refs.py sets it."
    )
    lines = [f"# {line}" for line in textwrap.wrap(head, 96)]
    for side in circuit.sides[1:]:
        lines.append("#")
        lines += [f"# {line}" for line in textwrap.wrap(f"{side.name}: {side.about}", 96)]
    lines += [
        "#",
        f"#   uv run run.py specs/{path.name} --report out/{path.stem}.html",
        "",
        f"rate = {RATE}",
        "duration = 2.5",
        "",
        "[input]",
        *[f"{k} = {_toml(v)}" for k, v in SWEEP.items()],
        "",
        "[ours]",
    ]
    for side in circuit.sides[1:]:
        source = side.source
        lines += ["", "[[references]]", f"name = {json.dumps(side.name)}", f'plugin = "{VST3}/{source.plugin}.vst3"', "buffer_size = 512"]
        if source.preset:
            lines.append(f"preset = {json.dumps(source.preset)}")
        if source.settle:
            lines.append("settle = true")
        lines.append("[references.parameters]")
        lines += [f"{k} = {_toml(v)}" for k, v in source.base.items()]
    about = (
        f"{circuit.title}, by a quiet sine sweep's response at three cutoffs, at Res 0, half way to "
        "where each side starts to ring and 95% of the way, our first 85% of Res set against each "
        "reference's knob up to where it starts to ring. filter_refs.py measures each circuit "
        "further and gives the references' consensus. " + circuit.about
    )
    lines += [
        "",
        "[report]",
        'kind = "response"',
        f"title = {json.dumps(circuit.title)}",
        'figure = ["filter"]',
        'row = "res"',
        'column = "cut"',
        'about = """',
        *textwrap.wrap(about, 96),
        '"""',
    ]
    for kind, (label, _, _) in circuit.kinds.items():
        for cut in CUTS:
            hertz = cut_hz(cut)
            for place in SPEC_ALONG:
                share = place * KNEE
                ours = circuit.sides[0].setter(kind, hertz, share)
                theirs = {side.name: side.source.base | side.setter(kind, hertz, paired(share, side.onset_of(kind))) for side in circuit.sides[1:]}
                theirs = {name: {k: v for k, v in table.items() if k not in side.source.base or side.source.base[k] != v} for (name, table), side in zip(theirs.items(), circuit.sides[1:])}
                lines += [
                    "",
                    "[[cases]]",
                    f'name = "{kind}-{cut:02X}-res{ours["res"]:02X}"',
                    f"ours = {toml_inline(ours)}",
                    f"references = {toml_inline(theirs)}",
                ]
    path.write_text("\n".join(lines) + "\n")
    return path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--circuit", action="append", choices=["svf", "obx", "ladder", "ms20"])
    parser.add_argument("--binary", type=Path, default=REPOSITORY / "target" / "release" / "vimtracker")
    parser.add_argument("--page", type=Path, default=OUT / "index.html")
    parser.add_argument("--json", type=Path, default=OUT / "consensus.json")
    parser.add_argument("--fresh", action="store_true", help="render the references again, not from the cache")
    parser.add_argument("--write-specs", action="store_true", help="write specs/filter-refs-<circuit>.toml too")
    args = parser.parse_args()
    if not args.binary.exists():
        raise SystemExit(f"{args.binary} isn't there: run `cargo build --release` first")
    CACHE.mkdir(parents=True, exist_ok=True)
    every = circuits(args.binary, args.fresh)
    chosen = args.circuit or list(every)
    results = [measure_circuit(every[key], log=print) for key in chosen]
    if args.write_specs:
        for key in chosen:
            print(f"wrote {write_spec(every[key]).relative_to(HERE)}", file=sys.stderr)
    page(results, args.page)
    data = {
        result["circuit"].key: {
            "rows": result["rows"],
            "verdicts": result["verdicts"],
            "onsets": result["onsets"],
            "notes": result["notes"],
        }
        for result in results
    }
    args.json.parent.mkdir(parents=True, exist_ok=True)
    args.json.write_text(json.dumps(data, indent=1, default=float))
    print(summary_text(results))
    print(f"\nPage in {args.page}", file=sys.stderr)


if __name__ == "__main__":
    main()
