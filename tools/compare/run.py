"""Render a spec through both sides, ours with `vimtracker probe` and the reference with its plugin,
and print a first comparison of the two.

Usage: run.py <spec.toml> [--case NAME ...] [--binary PATH] [--out FOLDER]

Every case is rendered unless some are named. The WAVs go in `out/<spec>/` beside this script,
as `<case>-ours.wav` and `<case>-reference.wav`. Each case prints both sides' peak, RMS and
spectral centroid; a sweep's input adds the magnitude response in octave bands, and stepped levels
add the static curve, the output level at each input level.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

import numpy as np
from scipy.io import wavfile

import render_reference
import spec as specs

HERE = Path(__file__).resolve().parent
REPOSITORY = HERE.parent.parent
# Where a level of nothing is shown from.
FLOOR_DB = -150.0


def db(value: float) -> float:
    return 20.0 * np.log10(max(value, 10.0 ** (FLOOR_DB / 20.0)))


def read_wav(path: Path) -> np.ndarray:
    _, samples = wavfile.read(path)
    samples = np.asarray(samples, dtype=np.float32)
    return samples.T if samples.ndim == 2 else np.vstack([samples, samples])


def mono(samples: np.ndarray) -> np.ndarray:
    return samples.mean(axis=0).astype(np.float64)


def centroid(samples: np.ndarray, rate: int) -> float:
    magnitude = np.abs(np.fft.rfft(mono(samples)))
    frequencies = np.fft.rfftfreq(samples.shape[1], 1.0 / rate)
    total = magnitude.sum()
    return float((magnitude * frequencies).sum() / total) if total > 0 else 0.0


def summary(samples: np.ndarray, rate: int) -> dict:
    return {
        "peak dBFS": db(float(np.abs(samples).max(initial=0.0))),
        "RMS dBFS": db(float(np.sqrt(np.mean(samples.astype(np.float64) ** 2)))),
        "centroid Hz": centroid(samples, rate),
    }


def octave_response(given: np.ndarray, out: np.ndarray, rate: int, low: float, high: float):
    """The gain from input to output in octave bands, by the ratio of their spectra's power."""
    length = max(given.shape[1], out.shape[1])
    spectrum_in = np.abs(np.fft.rfft(mono(given), length)) ** 2
    spectrum_out = np.abs(np.fft.rfft(mono(out), length)) ** 2
    frequencies = np.fft.rfftfreq(length, 1.0 / rate)
    bands = []
    centre = 31.25
    while centre <= 16_000:
        lower, upper = centre / np.sqrt(2.0), centre * np.sqrt(2.0)
        if lower >= low and upper <= high:
            inside = (frequencies >= lower) & (frequencies < upper)
            power_in, power_out = spectrum_in[inside].sum(), spectrum_out[inside].sum()
            bands.append((centre, 10.0 * np.log10(max(power_out, 1e-30) / max(power_in, 1e-30))))
        centre *= 2.0
    return bands


def static_curve(given: dict, out: np.ndarray, rate: int):
    """The output's level at each input level, as a sine's peak level from the RMS over the last
    half of each hold, so a compressor has settled."""
    held = specs.frames_in(given["hold"], rate)
    step = given.get("step", 1.0)
    count = int(np.floor((given["to"] - given["from"]) / step + 1e-9)) + 1
    curve = []
    signal = mono(out)
    for index in range(count):
        part = signal[index * held + held // 2 : (index + 1) * held]
        rms = float(np.sqrt(np.mean(part**2))) if part.size else 0.0
        curve.append((given["from"] + step * index, db(rms * np.sqrt(2.0))))
    return curve


def probe(binary: Path, spec: Path, out: Path, case: str | None, write_input: Path | None):
    command = [str(binary), "probe", str(spec), str(out)]
    if case is not None:
        command += ["--case", case]
    if write_input is not None:
        command += ["--write-input", str(write_input)]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(result.stderr.strip() or f"{binary} probe failed")


def compare_case(spec, case, binary, folder, reference):
    name = case or "probe"
    ours_path = folder / f"{name}-ours.wav"
    reference_path = folder / f"{name}-reference.wav"
    input_path = folder / f"{name}-input.wav" if "input" in spec.data else None
    probe(binary, spec.path, ours_path, case, input_path)
    rendered = render_reference.render(spec, case, reference)
    render_reference.write(reference_path, spec.rate, rendered)
    ours, theirs = read_wav(ours_path), read_wav(reference_path)

    print(f"\n== {spec.path.stem}: {name}")
    if input_path is not None:
        made = specs.signal(spec)
        probed = read_wav(input_path)
        if made.shape == probed.shape:
            difference = float(np.abs(made - probed).max(initial=0.0))
            print(f"input: the two sides' signals differ by at most {difference:.1e}")
        else:
            print(f"input: the two sides' signals differ in length, {made.shape} and {probed.shape}")
    ours_summary, theirs_summary = summary(ours, spec.rate), summary(theirs, spec.rate)
    print(f"{'':14}{'ours':>12}{'reference':>12}{'difference':>12}")
    for key in ours_summary:
        a, b = ours_summary[key], theirs_summary[key]
        print(f"{key:14}{a:12.2f}{b:12.2f}{a - b:12.2f}")

    given = spec.data.get("input", {})
    if given.get("signal") == "sweep":
        made = specs.signal(spec)
        low, high = given["from"], given["to"]
        print("\nmagnitude response, dB in octave bands")
        print(f"{'Hz':>8}{'ours':>10}{'reference':>12}{'difference':>12}")
        ours_bands = octave_response(made, ours, spec.rate, low, high)
        theirs_bands = octave_response(made, theirs, spec.rate, low, high)
        for (centre, a), (_, b) in zip(ours_bands, theirs_bands):
            print(f"{centre:8.0f}{a:10.2f}{b:12.2f}{a - b:12.2f}")
    if given.get("signal") == "steps":
        print("\nstatic curve, output level in dBFS at each input level")
        print(f"{'in':>6}{'ours':>10}{'reference':>12}{'difference':>12}")
        ours_curve = static_curve(given, ours, spec.rate)
        theirs_curve = static_curve(given, theirs, spec.rate)
        for (level, a), (_, b) in zip(ours_curve, theirs_curve):
            print(f"{level:6.0f}{a:10.2f}{b:12.2f}{a - b:12.2f}")
    return ours_summary, theirs_summary


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("spec", type=Path)
    parser.add_argument("--case", action="append", help="a case to render; every case if none")
    parser.add_argument(
        "--binary",
        type=Path,
        default=REPOSITORY / "target" / "release" / "vimtracker",
        help="the vimtracker to probe with, the release build by default",
    )
    parser.add_argument("--out", type=Path, help="where the WAVs go")
    args = parser.parse_args()
    if not args.binary.exists():
        raise SystemExit(f"{args.binary} isn't there: run `cargo build --release` first")
    spec = specs.read(args.spec.resolve())
    folder = args.out or HERE / "out" / spec.path.stem
    folder.mkdir(parents=True, exist_ok=True)
    cases = args.case or spec.cases or [None]
    section = spec.reference(cases[0])
    reference = render_reference.Reference(section["plugin"], section.get("plugin_name"))
    for case in cases:
        compare_case(spec, case, args.binary, folder, reference)
    sys.stdout.flush()
    print(f"\nWAVs in {folder}", file=sys.stderr)


if __name__ == "__main__":
    main()
