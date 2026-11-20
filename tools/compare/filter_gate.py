"""The filter gate: each track filter circuit against its references while its cutoff and resonance
move, beside `filter_refs.py`'s static measures, with the biggest differences listed first (#1081,
decision 453).

Usage: filter_gate.py [--circuit svf|obx|ladder|ms20 ...] [--binary PATH] [--fresh] [--no-static]
                      [--page PAGE] [--json DATA]

| Ours | The Drop | Arturia |
|---|---|---|
| `LP`, `HP` at 12 dB, the state-variable filter | OSR, two poles | Filter SEM |
| `OB-X` `LP12` | OSR, two poles | Filter SEM |
| `Ladder` `LP24` | PRD, four poles | Filter MINI |
| `MS20 LP`, `MS20 HP` | MS1, two poles | Filter MS-20 |

The Drop is the main reference; the Arturia filter is a second opinion. `Ladder`'s Xpander modes
(decision 184) have only Filter M12 to go by: `--circuit xpander` reads their static responses
against its pole mixes, as its cutoff knob moves in whole semitones and can't be swept smoothly.

**How a sweep is made the same on both sides.** Ours moves through its own Mod page: an LFO or an
envelope in a mod slot, played by `vimtracker probe`, which writes down the cutoff and resonance
the filter heard every 32 frames (`--controls`), after the mod slots' smoothing. Each reference is
then rendered in blocks of 32 frames, its cutoff and resonance set before each block to what ours
heard in it, so both sides follow one path. A reference's own smoothing of a setting comes on top:
the step measure shows how much. The Drop follows within about 1.5 ms; Filter SEM and Filter MS-20
take 10 to 35 ms, so their envelope sweeps lag, and are shown as such, not as fair pairs. Our knob
and a step's lock reach the cutoff through a 10 ms smoother rather than the mod slots' 1 ms one;
the gate measures the mod path, which is how a sweep is played.

**The measures**, each side's own where the references' consensus can be read (as
`filter_refs.py` has it), and each side against The Drop's where only a curve compares:

- tracking: on white noise at high resonance, the peak's pitch in cents off the cutoff it was
  given, frame by frame, its median;
- the peak while moving: how far it stands over the band two octaves either side, against the
  same height read from the same render with the cutoff held still (the `held` case);
- level and brightness over the sweep against The Drop's, level-matched: the RMS of the
  difference of each frame's level (dB) and spectral centroid (octaves), and the whole render's
  level, unmatched, against the input;
- zipper: a 1 kHz sine under a slow or medium sweep, what comes out that isn't at its harmonics, in
  dB under the whole output;
- stability: the peak in dBFS of a fast sweep at a resonance past where each side rings, and any
  sample that isn't a number;
- near audio rate: a 110 Hz saw with a 100 Hz sine LFO a little over an octave either way, the third-octave
  band levels against The Drop's, RMS dB;
- the step: a 1 kHz sine with the cutoff stepped between 250 Hz and 4 kHz, how long the sine
  takes to rise or fall 90% of the way to its new level, in ms from the block the step came in;
- the knobs: hertz per step of each side's cutoff knob, and each side's resonance taper, which
  `filter_refs.py` reads as the peak's height along the way to ringing.

Run from `tools/compare` with `uv run filter_gate.py`. Renders are cached in
`out/filter-gate/cache/` as `filter_refs.py`'s are; the page and JSON go beside them.
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
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
from scipy.io import wavfile
from scipy.signal import hilbert

import consensus
import filter_refs as F
import render_reference
import report
from run import read_wav

HERE = Path(__file__).resolve().parent
OUT = HERE / "out" / "filter-gate"
CACHE = OUT / "cache"
INPUTS = OUT / "inputs"
RATE = F.RATE
# The block a reference's settings are changed on, and how often the probe writes down ours.
BLOCK = 32
# Our knob's share where the references start to ring, as filter_refs.py pairs them.
KNEE = F.KNEE
# Where a moving filter is set: low and high resonance as a share of the way to ringing, and just
# past it.
RES_LOW, RES_HIGH = 0.25, 0.9
RES_RINGING = F.JUST_PAST
# The frames the measures read, in samples.
FRAME, HOP = 2048, 512
# How far from The Drop counts as audibly different, over a curve, where the other reference isn't
# further: 1.5 dB of level, 0.15 octaves of brightness, 3 dB across third-octave bands.
AUDIBLE = {"level": 1.5, "brightness": 0.15, "bands": 3.0}
# A peak this far over full scale, or any sample that isn't a number, has blown up.
BLOWN_DB = 12.0


def cut_value(hertz: float) -> float:
    """Our `Cut` for a frequency, on its `00` to `FF` scale, unrounded."""
    return 255.0 * math.log(hertz / 20.0) / math.log(1000.0)


def res_value(along: float) -> int:
    """Our `Res` for a place along the way to where the references ring."""
    return min(255, int(round(255.0 * KNEE * along)))


# The inputs, made here so every side gets the same samples.


def _write(path: Path, samples: np.ndarray) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        wavfile.write(path, RATE, np.ascontiguousarray(np.vstack([samples, samples]).T, dtype=np.float32))
    return path


def saw(frequency: float, seconds: float, level_db: float) -> np.ndarray:
    """A band-limited saw, its harmonics up to 20 kHz, at a peak of about `level_db`."""
    t = np.arange(int(seconds * RATE)) / RATE
    out = np.zeros_like(t)
    for k in range(1, int(20_000 / frequency) + 1):
        out += np.sin(2 * np.pi * k * frequency * t) / k
    return out / np.max(np.abs(out)) * 10 ** (level_db / 20)


def inputs() -> dict:
    """Each input by name: its path and its samples."""
    rng = np.random.default_rng(1081)
    seconds = 10.0
    t = np.arange(int(seconds * RATE)) / RATE
    made = {
        "noise": rng.standard_normal(t.size) * 10 ** (-24 / 20),
        "sine": np.sin(2 * np.pi * 1000.0 * t) * 10 ** (-20 / 20),
        "saw": saw(110.0, seconds, -12.0),
        "hot saw": saw(55.0, seconds, 0.0),
    }
    return {name: (_write(INPUTS / f"{name.replace(' ', '-')}.wav", x), x) for name, x in made.items()}


# How ours moves: a mod slot, as a song stores it.


def lfo(destination: str, shape: str, centihertz: int, depth: int, phase: int = 0) -> dict:
    return {
        "kind": "Lfo",
        "destination": destination,
        "depth": depth,
        "lfo": {"shape": shape, "rate": {"Hertz": centihertz}, "phase": phase, "mode": "Free"},
    }


def adsr(depth: int, decay_ms: int, attack_ms: int = 1, sustain: int = 0, release_ms: int = 60) -> dict:
    return {
        "kind": "Adsr",
        "destination": "cut",
        "depth": depth,
        "adsr": {"attack_ms": attack_ms, "decay_ms": decay_ms, "sustain": sustain, "release_ms": release_ms, "reset": True},
    }


def sweep(low: float, high: float, shape: str, centihertz: int) -> tuple[int, dict]:
    """Our `Cut` and an LFO on it that sweeps from `low` to `high` hertz and back."""
    a, b = cut_value(low), cut_value(high)
    base = int(round((a + b) / 2))
    return base, lfo("cut", shape, centihertz, int(round((b - a) / 2)))


def _toml(value) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return repr(value)
    if isinstance(value, str):
        return json.dumps(value)
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{k} = {_toml(v)}" for k, v in value.items()) + " }"
    if isinstance(value, list):
        return "[ " + ", ".join(_toml(v) for v in value) + " ]"
    raise TypeError(value)


def _key(*parts) -> str:
    return hashlib.sha1(json.dumps(parts, sort_keys=True, default=str).encode()).hexdigest()[:20]


@dataclass
class Played:
    """A render and the path its filter took: per block of 32 frames, the cutoff in hertz and
    the resonance as our knob's share."""

    samples: np.ndarray
    hertz: np.ndarray
    share: np.ndarray


class Ours:
    """Our track filter through `vimtracker probe`, moved by its mod slots."""

    def __init__(self, binary: Path):
        self.binary = binary
        stat = binary.stat()
        self.build = (stat.st_mtime_ns, stat.st_size)

    def render(self, settings: dict, mods: dict, given: Path | None = None, notes: list | None = None, seconds: float = 0.0) -> Played:
        key = _key(self.build, settings, mods, str(given), notes, seconds)
        path = CACHE / f"ours-{key}.npz"
        if path.exists():
            data = np.load(path)
            return Played(data["samples"], data["hertz"], data["share"])
        lines = [f"rate = {RATE}"]
        if seconds:
            lines.append(f"duration = {seconds}")
        if notes is not None:
            lines.append("notes = [")
            lines += [f"  {_toml(note)}," for note in notes]
            lines.append("]")
        else:
            lines += ["[input]", 'signal = "file"', f"path = {json.dumps(str(given))}"]
        lines.append("[ours]")
        if notes is not None:
            lines.append('machine = "va"')
        lines.append("[ours.settings]")
        lines += [f'"{k}" = {_toml(v)}' for k, v in settings.items()]
        for slot, table in mods.items():
            lines.append(f"[ours.mods.{slot}]")
            lines += [f"{k} = {_toml(v)}" for k, v in table.items()]
        with tempfile.TemporaryDirectory() as folder:
            spec, out, controls = Path(folder) / "gate.toml", Path(folder) / "ours.wav", Path(folder) / "controls.csv"
            spec.write_text("\n".join(lines) + "\n")
            result = subprocess.run([str(self.binary), "probe", str(spec), str(out), "--controls", str(controls)], capture_output=True, text=True)
            if result.returncode != 0:
                raise SystemExit(f"{result.stderr.strip()}\n{spec.read_text()}")
            samples = read_wav(out)
            heard = np.loadtxt(controls, delimiter=",", skiprows=1, ndmin=2)
        played = Played(samples, heard[:, 1], heard[:, 2])
        np.savez(path, samples=played.samples, hertz=played.hertz, share=played.share)
        return played


class Moved:
    """A reference plugin rendered in blocks of 32 frames, its cutoff and resonance set before
    each from a path ours took."""

    def __init__(self, side: F.Side, setter=None):
        self.side = side
        self.setter = setter or side.setter
        self.loaded = None

    def render(self, kind: str, path: Played, samples: np.ndarray, extra: dict | None = None, fresh: bool = False) -> np.ndarray:
        source = self.side.source
        onset = self.side.onset_of(kind)
        knobs = np.array([F.paired(min(1.0, float(s)), onset) for s in path.share])
        extra = extra or {}
        key = _key(source.plugin, source.preset, source.base, kind, extra, path.hertz.round(4).tolist(), knobs.round(5).tolist(), hashlib.sha1(samples.tobytes()).hexdigest())
        out_path = CACHE / f"{self.side.name.replace(' ', '-').lower()}-{key}.npy"
        if out_path.exists() and not fresh:
            return np.load(out_path)
        if self.loaded is None:
            self.loaded = render_reference.Reference(source.section(source.base))
        plugin = self.loaded.plugin
        first = source.base | self.setter(kind, float(path.hertz[0]), float(knobs[0])) | extra
        self.loaded.set(first)
        given = np.asarray(samples, dtype=np.float32)
        if given.ndim == 1:
            given = np.vstack([given, given])
        out = np.zeros_like(given)
        frames = given.shape[1]
        if source.settle:
            plugin(np.zeros((2, BLOCK), dtype=np.float32), RATE, buffer_size=BLOCK, reset=True)
        now = dict(first)
        for block, start in enumerate(range(0, frames, BLOCK)):
            at = min(block, len(knobs) - 1)
            wanted = self.setter(kind, float(path.hertz[at]), float(knobs[at]))
            for name, value in wanted.items():
                low, high = plugin.parameters[name].range[:2]
                if isinstance(value, float) and isinstance(low, (int, float)) and isinstance(high, (int, float)):
                    # Past the end of a reference's knob, as Filter SEM's 15 kHz, it stays at its end.
                    value = min(max(value, low), high)
                if now.get(name) != value:
                    setattr(plugin, name, value)
                    now[name] = value
            out[:, start : start + BLOCK] = plugin(given[:, start : start + BLOCK], RATE, buffer_size=BLOCK, reset=(block == 0 and not source.settle))
        np.save(out_path, out)
        return out


# Measuring.


def frames_of(samples: np.ndarray):
    x = F.measure.mono(samples) if samples.ndim == 2 else samples
    x = np.nan_to_num(np.asarray(x, dtype=np.float64))
    window = np.hanning(FRAME)
    count = max(1, (x.size - FRAME) // HOP)
    spectra = np.array([np.abs(np.fft.rfft(x[i * HOP : i * HOP + FRAME] * window)) for i in range(count)])
    centres = (np.arange(count) * HOP + FRAME / 2) / RATE
    return spectra, centres


def path_at(path: Played, times: np.ndarray, values: np.ndarray | None = None) -> np.ndarray:
    """The path's value at each time, a block's value standing for its 32 frames."""
    values = path.hertz if values is None else values
    blocks = np.clip((times * RATE / BLOCK).astype(int), 0, len(values) - 1)
    return values[blocks]


def tracking(samples: np.ndarray, path: Played, skip: float = 0.2) -> dict:
    """The resonant peak against the cutoff it was given, frame by frame: its median offset in
    cents, and its height over the band two octaves either side."""
    spectra, times = frames_of(samples)
    hz = np.fft.rfftfreq(FRAME, 1 / RATE)
    db = 20 * np.log10(spectra + 1e-12)
    smooth = np.apply_along_axis(lambda row: np.convolve(row, np.ones(5) / 5, mode="same"), 1, db)
    given = path_at(path, times)
    offsets, heights = [], []
    for row, raw, nominal, t in zip(smooth, db, given, times):
        if t < skip or not 60.0 <= nominal <= 16_000.0 or raw.max() < -100:
            continue
        near = (hz >= nominal / 2**0.75) & (hz <= nominal * 2**0.75)
        if near.sum() < 3:
            continue
        at = np.flatnonzero(near)[np.argmax(row[near])]
        offsets.append(1200 * math.log2(hz[at] / nominal))
        band = (hz >= nominal / 4) & (hz <= nominal * 4)
        heights.append(row[at] - np.median(row[band]))
    if not offsets:
        return {"cents": float("nan"), "height": float("nan")}
    return {"cents": float(np.median(offsets)), "height": float(np.mean(heights))}


def level_curve(samples: np.ndarray) -> np.ndarray:
    x = np.nan_to_num(F.measure.mono(samples))
    count = max(1, (x.size - FRAME) // HOP)
    return np.array([10 * math.log10(np.mean(x[i * HOP : i * HOP + FRAME] ** 2) + 1e-20) for i in range(count)])


def centroid_curve(samples: np.ndarray) -> np.ndarray:
    spectra, _ = frames_of(samples)
    hz = np.fft.rfftfreq(FRAME, 1 / RATE)
    power = spectra**2
    c = (power * hz).sum(1) / np.maximum(power.sum(1), 1e-20)
    return np.log2(np.maximum(c, 20.0))


def curve_apart(a: np.ndarray, b: np.ndarray, matched: bool, quiet: np.ndarray | None = None) -> float:
    n = min(len(a), len(b))
    d = a[:n] - b[:n]
    if quiet is not None:
        d = d[~quiet[:n]]
    if d.size == 0:
        return float("nan")
    if matched:
        d = d - np.mean(d)
    return float(np.sqrt(np.mean(d**2)))


def residual(samples: np.ndarray, fundamental: float, skip: float = 0.2, width: float = 25.0) -> float:
    """What isn't within `width` Hz of a harmonic of a sine, in dB under the whole output."""
    x = np.nan_to_num(F.measure.mono(samples))[int(skip * RATE) :]
    spectrum = np.abs(np.fft.rfft(x * np.hanning(x.size))) ** 2
    hz = np.fft.rfftfreq(x.size, 1 / RATE)
    harmonic = np.zeros_like(hz, dtype=bool)
    for k in range(1, int(RATE / 2 / fundamental) + 1):
        harmonic |= np.abs(hz - k * fundamental) <= width
    total = spectrum.sum()
    return 10 * math.log10(max(spectrum[~harmonic].sum(), 1e-30) / max(total, 1e-30))


def bands(samples: np.ndarray, skip: float = 0.2) -> np.ndarray:
    x = np.nan_to_num(F.measure.mono(samples))[int(skip * RATE) :]
    spectrum = np.abs(np.fft.rfft(x)) ** 2
    hz = np.fft.rfftfreq(x.size, 1 / RATE)
    centres = 1000.0 * 2.0 ** (np.arange(-14, 13) / 3)
    out = []
    for c in centres:
        inside = (hz >= c * 2 ** (-1 / 6)) & (hz < c * 2 ** (1 / 6))
        out.append(10 * math.log10(spectrum[inside].sum() + 1e-20))
    out = np.array(out)
    return out - np.max(out)


def step_times(samples: np.ndarray, path: Played) -> dict:
    """How long a sine takes to rise or fall 90% of the way to its new level as the cutoff steps up
    and down, in ms from the block the step came in."""
    x = np.nan_to_num(F.measure.mono(samples).astype(np.float64))
    env = np.abs(hilbert(x))
    env = np.convolve(env, np.ones(48) / 48, mode="same")
    hz = path.hertz
    jumps = np.flatnonzero(np.abs(np.diff(np.log2(hz))) > 0.25)
    out = {"rise": [], "fall": []}
    last = -RATE
    for jump in jumps:
        start = jump * BLOCK
        # A step smoothed over a few blocks is one step.
        if start - last < 0.1 * RATE:
            continue
        last = start
        if start < 0.1 * RATE or start + int(0.3 * RATE) > x.size:
            continue
        up = hz[jump + 1] > hz[jump]
        before = np.median(env[start - int(0.05 * RATE) : start])
        after = np.median(env[start + int(0.2 * RATE) : start + int(0.3 * RATE)])
        if after == before:
            continue
        segment = (env[start : start + int(0.2 * RATE)] - before) / (after - before)
        out["rise" if up else "fall"].append(np.argmax(segment > 0.9) / RATE * 1000)
    return {k: float(np.median(v)) if v else float("nan") for k, v in out.items()}


def peak_dbfs(samples: np.ndarray) -> float:
    if not np.all(np.isfinite(samples)):
        return float("inf")
    return 20 * math.log10(max(float(np.max(np.abs(samples))), 1e-12))


# The gate's cases.


@dataclass
class Case:
    name: str
    about: str
    given: str
    settings: dict
    mods: dict
    measures: list
    seconds: float = 5.0
    kinds: tuple = ()


def cases(kind: str, high: bool) -> list[Case]:
    """The moving cases for one kind: `high` for a high-pass."""
    out = []
    ranges = [("200 Hz-2 kHz", 200.0, 2000.0), ("1-6 kHz", 1000.0, 6000.0), ("80-800 Hz", 80.0, 800.0)]
    for label, low, top in ranges:
        for rate_name, centihertz, seconds in (("slow, 5 s a cycle", 20, 5.0), ("medium, a bar of 120 BPM a cycle", 50, 4.0)):
            for res_name, along in (("low Res", RES_LOW), ("high Res", RES_HIGH)):
                cut, mod = sweep(low, top, "Tri", centihertz)
                out.append(
                    Case(
                        f"sweep {label}, {rate_name.split(',')[0]}, {res_name}",
                        f"White noise, the cutoff swept {label} and back, {rate_name}, {res_name}.",
                        "noise",
                        {"cut": cut, "res": res_value(along)},
                        {4: mod},
                        ["tracking", "level", "brightness"] if along == RES_HIGH else ["level", "brightness"],
                        seconds,
                    )
                )
    for label, centihertz in (("100 ms", 1000), ("33 ms", 3000)):
        cut, mod = sweep(200.0, 5000.0, "Saw", centihertz)
        out.append(Case(f"fast falls of {label}, high Res", f"White noise, the cutoff falling from 5 kHz to 200 Hz in {label}, over and over, as an envelope does, at high Res.", "noise", {"cut": cut, "res": res_value(RES_HIGH)}, {4: mod}, ["tracking", "level", "brightness"], 3.0))
        out.append(Case(f"fast falls of {label}, ringing", f"A 110 Hz saw, the cutoff falling from 5 kHz to 200 Hz in {label}, at a resonance just past where each side rings.", "saw", {"cut": cut, "res": res_value(RES_RINGING)}, {4: mod}, ["stability", "level"], 3.0))
    cut, mod = sweep(2000.0, 20_000.0, "Tri", 50)
    out.append(Case("sweep to the top, high Res", "White noise, the cutoff swept 2-20 kHz and back, a bar a cycle, at high Res: what happens near Nyquist.", "noise", {"cut": cut, "res": res_value(RES_HIGH)}, {4: mod}, ["tracking", "level", "brightness", "stability"], 4.0))
    for label, centihertz in (("slow", 20), ("medium", 50)):
        cut, mod = sweep(300.0, 3000.0, "Tri", centihertz)
        out.append(Case(f"zipper, {label}", f"A 1 kHz sine at -20 dBFS, the cutoff swept 300 Hz-3 kHz across it, {label}, at low Res: anything but its harmonics is stepping or noise.", "sine", {"cut": cut, "res": res_value(RES_LOW)}, {4: mod}, ["zipper"], 5.0))
    cut = int(round(cut_value(1000.0)))
    out.append(Case("near audio rate", "A 110 Hz saw, the cutoff moved by a 100 Hz sine LFO, a little over an octave either side of 1 kHz, at half way to ringing: the growl it makes.", "saw", {"cut": cut, "res": res_value(0.5)}, {4: lfo("cut", "Sine", 10_000, 26)}, ["bands"], 3.0))
    top = res_value(1.0)
    out.append(Case("Res swept", "White noise, the cutoff at 1 kHz, Res swept from 0 to where each side rings and back, 4 s a cycle.", "noise", {"cut": cut, "res": top // 2}, {4: lfo("res", "Tri", 25, top // 2, phase=270)}, ["level", "brightness"], 4.0))
    both_cut, both_mod = sweep(200.0, 2000.0, "Tri", 50)
    out.append(Case("Cut and Res swept", "White noise, the cutoff swept 200 Hz-2 kHz, a bar a cycle, and Res from 0 to where each side rings, 3 s a cycle, together.", "noise", {"cut": both_cut, "res": top // 2}, {4: both_mod, 5: lfo("res", "Tri", 33, top // 2, phase=270)}, ["level", "brightness"], 6.0))
    cut, mod = sweep(200.0, 5000.0, "Tri", 50)
    out.append(Case("driven sweep", "A 55 Hz saw at 0 dBFS, the cutoff swept 200 Hz-5 kHz, a bar a cycle, half way to ringing: the circuit's own saturation as it moves.", "hot saw", {"cut": cut, "res": res_value(0.5)}, {4: mod}, ["level", "brightness"], 4.0))
    cut, mod = sweep(250.0, 4000.0, "Square", 100)
    out.append(Case("step", "A 1 kHz sine at -20 dBFS, the cutoff stepped between 250 Hz and 4 kHz once a second, at no Res: how quickly each side follows.", "sine", {"cut": cut, "res": 0}, {4: mod}, ["step"], 4.0))
    held_cut = int(round(cut_value(1000.0)))
    out.append(Case("held", "White noise, the cutoff held at 1 kHz at high Res, for the peak's height still.", "noise", {"cut": held_cut, "res": res_value(RES_HIGH)}, {4: lfo("cut", "Tri", 20, 0)}, ["tracking", "level"], 2.0))
    for case in out:
        case.kinds = (kind,)
    return out


@dataclass
class Gated:
    """What one kind of a circuit is measured as."""

    circuit: str
    kind: str
    label: str
    records: list = field(default_factory=list)
    against: list = field(default_factory=list)  # each a row against The Drop


def measure_kind(circuit: F.Circuit, kind: str, ours: Ours, moved: dict, made: dict, fresh: bool, log=print) -> Gated:
    label, high, _ = circuit.kinds[kind]
    out = Gated(circuit.key, kind, label)
    our_side = circuit.sides[0]
    names = [side.name for side in circuit.sides]
    drop = names[1]
    for case in cases(kind, high):
        path_given, samples = made[case.given]
        settings = our_side.setter(kind, 1000.0, 0.0) | case.settings
        played = ours.render(settings, case.mods, given=path_given, seconds=case.seconds)
        frames = played.samples.shape[1]
        rendered = {"ours": played.samples}
        for side in circuit.sides[1:]:
            rendered[side.name] = moved[side.name].render(kind, played, samples[:frames], fresh=fresh)
        levels = {name: level_curve(x) for name, x in rendered.items()}
        quiet = levels[drop] < np.max(levels[drop]) - 50
        for measure in case.measures:
            if measure == "tracking":
                found = {name: tracking(x, played) for name, x in rendered.items()}
                out.records.append({"measure": "the peak off the cutoff, median", "case": case.name, "unit": "cents", "values": {n: v["cents"] for n, v in found.items()}})
                out.records.append({"measure": "the peak over the band around it", "case": case.name, "unit": "dB", "values": {n: v["height"] for n, v in found.items()}})
            elif measure == "level":
                gain = {name: 10 * math.log10(np.mean(np.nan_to_num(x[:, int(0.2 * RATE) :]) ** 2) + 1e-20) - 10 * math.log10(np.mean(samples[int(0.2 * RATE) : frames] ** 2)) for name, x in rendered.items()}
                out.records.append({"measure": "level against the input, the whole render", "case": case.name, "unit": "dB", "values": gain})
                for name in names[2:] + ["ours"]:
                    out.against.append({"case": case.name, "what": "level over time, level-matched", "unit": "dB", "side": name, "apart": curve_apart(levels[name], levels[drop], True, quiet)})
            elif measure == "brightness":
                bright = {name: centroid_curve(x) for name, x in rendered.items()}
                for name in names[2:] + ["ours"]:
                    out.against.append({"case": case.name, "what": "brightness over time", "unit": "octaves", "side": name, "apart": curve_apart(bright[name], bright[drop], False, quiet)})
            elif measure == "zipper":
                out.records.append({"measure": "zipper and noise under a moving sine", "case": case.name, "unit": "dB", "values": {n: residual(x, 1000.0) for n, x in rendered.items()}})
            elif measure == "stability":
                out.records.append({"measure": "the peak while moving fast", "case": case.name, "unit": "dB", "values": {n: peak_dbfs(x) for n, x in rendered.items()}})
            elif measure == "bands":
                found = {name: bands(x) for name, x in rendered.items()}
                for name in names[2:] + ["ours"]:
                    out.against.append({"case": case.name, "what": "third-octave bands", "unit": "dB", "side": name, "apart": float(np.sqrt(np.mean((found[name] - found[drop]) ** 2)))})
            elif measure == "step":
                found = {name: step_times(x, played) for name, x in rendered.items()}
                out.records.append({"measure": "the step's rise, to 90%", "case": case.name, "unit": "ms", "values": {n: v["rise"] for n, v in found.items()}})
                out.records.append({"measure": "the step's fall, to 90%", "case": case.name, "unit": "ms", "values": {n: v["fall"] for n, v in found.items()}})
        log(f"{circuit.key} {kind}: {case.name}", file=sys.stderr)
    return out


# The Xpander's pole mixes, against Filter M12's.

# Our `Ladder` mode and Filter M12's type for it. `HP24`, `N24` and `Fat24` have none.
XPANDER = [
    ("LP24", "LP4P"),
    ("LP18", "LP3P"),
    ("LP12", "LP2P"),
    ("LP6", "LP1P"),
    ("BP24", "BP4P"),
    ("BP12", "BP2P"),
    ("HP6", "HP1P"),
    ("HP12", "HP2P"),
    ("HP18", "HP3P"),
    ("N12", "Notch"),
    ("PH18", "Phase"),
    ("HP12+LP6", "HP2P+LP1P"),
    ("HP18+LP6", "HP3P+LP1P"),
    ("N12+LP6", "Notch+LP1P"),
    ("PH18+LP6", "Phase+LP1P"),
]
M12_BASE = {"filter_routing": "A Only", "limit_resonance": False, "dry_wet": 100.0, "filter_out_db": 0.0, "filter_a_out_db": 0.0, "filter_a_pan": 0.0, "master_cutoff": 0.0}
# Our `Cut` and M12's, which is in semitones as MIDI numbers notes: about 200 Hz, 988 Hz and 5 kHz.
XPANDER_CUTS = [(0x55, 55.0), (0x90, 83.0), (0xCC, 111.0)]
# Our `Res` and M12's, as `filter-clean.toml` sets them: none and about half. Not fitted.
XPANDER_RES = [(0x00, 0.0), (0x80, 32.0)]


def measure_xpander(binary: Path, fresh: bool, log=print) -> Gated:
    """Each pole mix's response to the quiet sweep, ours against Filter M12's, its shape from two
    octaves under `Cut` to two over, level-matched, and its level there, at three cutoffs."""
    ours = F.Ours(binary)
    m12 = F.Reference("Filter M12", "Filter M12", M12_BASE, fresh=fresh)
    given = F.made(F.SWEEP)
    out = Gated("xpander", "modes", "Ladder's Xpander modes against Filter M12")
    for mode, theirs in XPANDER:
        for cut, note in XPANDER_CUTS:
            for res, m12_res in XPANDER_RES:
                nominal = F.cut_hz(cut)
                curves = {}
                for name, samples in (
                    ("ours", ours.render({"filter": "Ladder", "mode": mode, "cut": cut, "res": res}, F.SWEEP)),
                    ("Filter M12", m12.render({"filter_a_type": theirs, "filter_a_cutoff": note, "filter_a_resonance": m12_res}, F.SWEEP)),
                ):
                    bins, gains = F.measure.bin_response(given, samples, RATE, F.SWEEP["from"], F.SWEEP["to"])
                    f, g = F.measure.response(bins, gains)
                    points = nominal * 2.0 ** np.arange(-2.0, 2.01, 0.25)
                    curves[name] = np.array([F.measure.gain_at(f, g, min(p, 18_000.0)) for p in points])
                d = curves["ours"] - curves["Filter M12"]
                seen = (curves["ours"] > -40) | (curves["Filter M12"] > -40)
                d = d[seen]
                case = f"{mode} as {theirs}, Cut {report.hertz(nominal)}, Res {res:02X}"
                out.against.append({"case": case, "what": "response's shape, two octaves either side", "unit": "dB", "side": "ours", "apart": float(np.sqrt(np.mean((d - d.mean()) ** 2))) if d.size else float("nan")})
                out.records.append({"measure": "level, two octaves either side, mean", "case": case, "unit": "dB", "values": {"ours": float(np.mean(curves["ours"][seen])), "Filter M12": float(np.mean(curves["Filter M12"][seen]))}})
        log(f"xpander {mode}: measured", file=sys.stderr)
    return out


# The knobs.


def knob_maps() -> list[dict]:
    """Hertz per step of each cutoff knob, by reading each plugin's own parameter as its knob
    turns: the step is a 255th of its turn, as ours is. Only the parameters' values are read."""
    import pedalboard

    rows = [{"side": "ours", "knob": "Cut", "steps": {f: 1200 * math.log2(1000.0) / 255 for f in (200.0, 1000.0, 5000.0)}}]
    plugins = [("The Drop", "lp_freq"), ("Filter SEM", "cutoff_hz"), ("Filter MS-20", "lpf_cutoff_frequency_hz")]
    for name, parameter in plugins:
        plugin = pedalboard.load_plugin(f"{F.VST3}/{name}.vst3")
        p = plugin.parameters[parameter]
        raws = np.linspace(0.0, 1.0, 1021)
        values = []
        for raw in raws:
            p.raw_value = float(raw)
            values.append(float(getattr(plugin, parameter)))
        values = np.array(values)
        steps = {}
        for f in (200.0, 1000.0, 5000.0):
            i = int(np.argmin(np.abs(values - f)))
            j = min(i + 4, len(raws) - 1)
            steps[f] = 1200 * math.log2(values[j] / values[i]) / ((raws[j] - raws[i]) * 255) if values[i] > 0 else float("nan")
        rows.append({"side": name, "knob": parameter, "steps": steps, "range": [float(values.min()), float(values.max())]})
    return rows


# Showing it.


def flagged(gated: Gated) -> list[dict]:
    """Every difference from The Drop, biggest first: a consensus gap past its tolerance, or a
    curve further from The Drop's than is audible and than the other reference is."""
    out = []
    references = sorted({k for r in gated.records for k in r["values"] if k != "ours"})
    # The main reference: The Drop, or for the Xpander's modes, Filter M12.
    drop = [name for name in references if name.startswith("The Drop")] or references[:1]
    other = [name for name in references if name not in drop]
    for record in gated.records:
        values = record["values"]
        ours = values.get("ours", float("nan"))
        row = {"case": record["case"], "what": record["measure"], "unit": record["unit"], "ours": ours, "main": drop[0], "reference": values.get(drop[0]), "second": {n: values.get(n) for n in other}}
        if record["measure"].startswith("zipper"):
            # Only worse than every reference, and loud enough to hear, counts.
            worst = max((v for n, v in values.items() if n != "ours" and math.isfinite(v)), default=float("nan"))
            if ours > -60.0 and ours > worst + 6.0:
                out.append(row | {"size": (ours - worst) / 6.0})
            continue
        if record["measure"].startswith("the peak while moving"):
            if not math.isfinite(ours) or ours > BLOWN_DB:
                out.append(row | {"size": 99.0})
            continue
        verdict = consensus.judge(record, drop)
        if verdict["status"] in ("agree", "one") and verdict["size"] > 1.0:
            out.append(row | {"size": verdict["size"]})
    by_case = {}
    for row in gated.against:
        by_case.setdefault((row["case"], row["what"]), {})[row["side"]] = row
    for (case, what), sides in by_case.items():
        ours = sides.get("ours", {}).get("apart", float("nan"))
        other = [row["apart"] for name, row in sides.items() if name != "ours" and math.isfinite(row["apart"])]
        limit = AUDIBLE["level" if "level" in what else "brightness" if "brightness" in what else "bands"]
        if math.isfinite(ours) and ours > limit and (not other or ours > max(other)):
            out.append({"case": case, "what": what + f" against {drop[0]}'s", "unit": sides["ours"]["unit"], "ours": ours, "main": drop[0], "reference": 0.0, "second": {name: row["apart"] for name, row in sides.items() if name != "ours"}, "size": ours / limit})
    return sorted(out, key=lambda row: -row["size"])


def text_table(gated: Gated) -> str:
    lines = [f"\n== {gated.circuit} {gated.label}"]
    references = sorted({k for r in gated.records for k in r["values"] if k != "ours"}, key=lambda n: not n.startswith("The Drop"))
    lines.append(f"{'case':44} {'measure':44} {'ours':>9} " + " ".join(f"{n[:12]:>12}" for n in references))
    for record in gated.records:
        values = record["values"]
        lines.append(f"{record['case'][:44]:44} {record['measure'][:44]:44} {values.get('ours', float('nan')):9.2f} " + " ".join(f"{values.get(n, float('nan')):12.2f}" for n in references) + f" {record['unit']}")
    lines.append(f"{'case':44} {'against the main reference':44} {'ours':>9} {'2nd ref':>12}")
    by_case = {}
    for row in gated.against:
        by_case.setdefault((row["case"], row["what"]), {})[row["side"]] = row
    for (case, what), sides in by_case.items():
        other = [row["apart"] for name, row in sides.items() if name != "ours"]
        lines.append(f"{case[:44]:44} {what[:44]:44} {sides['ours']['apart']:9.2f} {other[0] if other else float('nan'):12.2f} {sides['ours']['unit']}")
    flags = flagged(gated)
    lines.append(f"-- flags ({len(flags)}), biggest first:")
    for row in flags:
        second = ", ".join(f"{n} {v:.2f}" for n, v in row["second"].items())
        lines.append(f"  {row['size']:5.1f}x  {row['case']}: {row['what']}: ours {row['ours']:.2f}, {row['main']} {row['reference']:.2f}{', ' + second if second else ''} {row['unit']}")
    return "\n".join(lines)


def page(results: list[Gated], static: list, knobs: list, path: Path) -> None:
    sections = []
    if knobs:
        rows = [[k["side"], k["knob"]] + [f"{k['steps'][f]:.1f}" for f in (200.0, 1000.0, 5000.0)] for k in knobs]
        sections.append("<h2>The cutoff knobs</h2><p>Cents of cutoff per 255th of each knob's turn, at three cutoffs.</p>" + report.table(["Side", "Knob", "200 Hz", "1 kHz", "5 kHz"], rows, numeric_from=2))
    for gated in results:
        flags = flagged(gated)
        rows = [[f"{r['size']:.1f}×", html.escape(r["case"]), html.escape(r["what"]), consensus.shown(r["ours"], r["unit"]), f"{html.escape(r['main'])} {consensus.shown(r['reference'], r['unit'])}", ", ".join(f"{html.escape(n)} {consensus.shown(v, r['unit'])}" for n, v in r["second"].items())] for r in flags]
        records = "".join(
            f"<tr><td>{html.escape(r['case'])}</td><td>{html.escape(r['measure'])}</td>" + "".join(f"<td class=\"n\">{consensus.shown(v, r['unit'])}</td>" for v in r["values"].values()) + f"<td>{r['unit']}</td></tr>"
            for r in gated.records
        )
        sections.append(
            f"<h2>{html.escape(gated.circuit)} {html.escape(gated.label)}</h2><h3>Flags, biggest first</h3>"
            + report.table(["How far", "Case", "What", "Ours", "Main reference", "Second"], rows, numeric_from=3)
            + f"<h3>Every measure</h3><table>{records}</table>"
        )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        f"""<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Filter gate</title><style>{report.STYLE}</style></head><body><main>
<h1>The track filter, moving, against The Drop and Arturia</h1>
<p class="meta">filter_gate.py · {RATE} Hz · made {datetime.date.today().isoformat()}</p>
<p>See <code>filter_gate.py</code>'s docstring for how each sweep is made the same on every side and what each measure is.{' The static measures, each circuit held still, are on <a href="static.html">their own page</a>.' if static else ''}</p>
{''.join(sections)}
</main></body></html>
"""
    )


def circuits_for_gate(binary: Path, fresh: bool) -> dict:
    every = F.circuits(binary, fresh)
    # Filter MINI's knob mapped by measuring, kept within what it reaches.
    ladder = every["ladder"]
    mini = next(side for side in ladder.sides if side.name == "Filter MINI")
    taper = {}

    def mini_setter(kind: str, hertz: float, share: float) -> dict:
        if "map" not in taper:
            taper["map"] = F.MiniTaper(mini)
        fit = taper["map"]
        return {"cutoff": round(float(np.interp(math.log2(hertz), fit.octaves, fit.knobs)), 5), "resonance": round(share, 4)}

    mini.setter = mini_setter
    return every


# Which kinds of each circuit move.
MOVING = {"svf": ["lp", "hp"], "obx": ["lp"], "ladder": ["lp"], "ms20": ["lp", "hp"]}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--circuit", action="append", choices=list(MOVING) + ["xpander"])
    parser.add_argument("--binary", type=Path, default=F.REPOSITORY / "target" / "release" / "vimtracker")
    parser.add_argument("--page", type=Path, default=OUT / "index.html")
    parser.add_argument("--json", type=Path, default=OUT / "gate.json")
    parser.add_argument("--fresh", action="store_true", help="render the references again, not from the cache")
    parser.add_argument("--no-static", action="store_true", help="leave out filter_refs.py's static measures")
    parser.add_argument("--no-knobs", action="store_true", help="leave out reading the plugins' cutoff knobs")
    args = parser.parse_args()
    if not args.binary.exists():
        raise SystemExit(f"{args.binary} isn't there: run `cargo build --release` first")
    CACHE.mkdir(parents=True, exist_ok=True)
    F.CACHE.mkdir(parents=True, exist_ok=True)
    every = circuits_for_gate(args.binary, args.fresh)
    chosen = args.circuit or list(MOVING) + ["xpander"]
    ours = Ours(args.binary)
    made = {name: (path, samples) for name, (path, samples) in inputs().items()}
    results, static = [], []
    for key in chosen:
        if key == "xpander":
            gated = measure_xpander(args.binary, args.fresh)
            results.append(gated)
            print(text_table(gated), flush=True)
            continue
        circuit = every[key]
        # Where the references that are set by it start to ring.
        for side in circuit.sides[1:]:
            for kind in MOVING[key]:
                if not math.isfinite(side.onset) and kind not in side.onsets:
                    side.onsets[kind] = F.find_onset(side, kind) if kind in circuit.rings else 1.0
        moved = {side.name: Moved(side) for side in circuit.sides[1:]}
        for kind in MOVING[key]:
            gated = measure_kind(circuit, kind, ours, moved, made, args.fresh)
            results.append(gated)
            print(text_table(gated), flush=True)
        if not args.no_static:
            result = F.measure_circuit(circuit, log=print)
            static.append(result)
    if static:
        print(F.summary_text(static))
        F.page(static, args.page.with_name("static.html"))
    knobs = [] if args.no_knobs else knob_maps()
    for k in knobs:
        print(f"{k['side']:14} {k['knob']:26} " + " ".join(f"{f:>6.0f} Hz {c:6.1f} cents/step" for f, c in k["steps"].items()))
    page(results, static, knobs, args.page)
    data = {
        "moving": [{"circuit": g.circuit, "kind": g.kind, "records": g.records, "against": g.against, "flags": flagged(g)} for g in results],
        "static": {r["circuit"].key: {"rows": r["rows"], "onsets": r["onsets"]} for r in static},
        "knobs": knobs,
    }
    args.json.parent.mkdir(parents=True, exist_ok=True)
    args.json.write_text(json.dumps(data, indent=1, default=float))
    print(f"\nPage in {args.page}", file=sys.stderr)


if __name__ == "__main__":
    main()
