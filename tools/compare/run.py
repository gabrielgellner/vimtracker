"""Render a spec through both sides, ours with `vimtracker probe` and the reference with its plugin,
print a first comparison of the two, and write a report of it as a web page.

Usage: run.py <spec.toml> [--case NAME ...] [--binary PATH] [--out FOLDER] [--report PAGE]

Every case is rendered unless some are named. The WAVs go in `out/<spec>/` beside this script,
as `<case>-ours.wav` and `<case>-reference.wav`. Each case prints both sides' peak, RMS and
spectral centroid; a sweep's input adds the magnitude response in octave bands, and stepped levels
add the static curve, the output level at each input level. A spec with no `[reference]` renders
ours alone. `--report` writes one self-contained page with both sides plotted and measured, laid
out as the spec's `[report]` says (see `report.py`).
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from scipy.io import wavfile

import render_reference
import spec as specs
from measure import octave_response, static_curve, summary

HERE = Path(__file__).resolve().parent
REPOSITORY = HERE.parent.parent


@dataclass
class Rendered:
    """One case as both sides rendered it, as stereo samples (2, frames)."""

    case: str | None
    ours: np.ndarray
    reference: np.ndarray | None
    # The input both were given, as made here, for an effect.
    given: np.ndarray | None
    # The track filter's response as the Sound page draws it, as (Hz, dB) arrays, for a sweep.
    drawn: tuple[np.ndarray, np.ndarray] | None

    @property
    def name(self) -> str:
        return self.case or "probe"


def read_wav(path: Path) -> np.ndarray:
    _, samples = wavfile.read(path)
    samples = np.asarray(samples, dtype=np.float32)
    return samples.T if samples.ndim == 2 else np.vstack([samples, samples])


def probe(binary: Path, spec: Path, out: Path, case: str | None, write_input=None, response=None):
    command = [str(binary), "probe", str(spec), str(out)]
    if case is not None:
        command += ["--case", case]
    if write_input is not None:
        command += ["--write-input", str(write_input)]
    if response is not None:
        command += ["--response", str(response)]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(result.stderr.strip() or f"{binary} probe failed")


def print_sides(sides: list[tuple[str, dict]]) -> None:
    """Each side's summary in a column, with their difference when there are two."""
    header = f"{'':14}" + "".join(f"{side:>12}" for side, _ in sides)
    print(header + (f"{'difference':>12}" if len(sides) == 2 else ""))
    for key in sides[0][1]:
        values = [values[key] for _, values in sides]
        line = f"{key:14}" + "".join(f"{value:12.2f}" for value in values)
        if len(values) == 2:
            line += f"{values[0] - values[1]:12.2f}"
        print(line)


def compare_case(spec, case, binary, folder, reference) -> Rendered:
    name = case or "probe"
    ours_path = folder / f"{name}-ours.wav"
    reference_path = folder / f"{name}-reference.wav"
    given = spec.input(case) or {}
    input_path = folder / f"{name}-input.wav" if given else None
    response_path = folder / f"{name}-drawn.csv" if given.get("signal") == "sweep" else None
    probe(binary, spec.path, ours_path, case, input_path, response_path)
    ours = read_wav(ours_path)
    theirs = None
    if reference is not None:
        theirs = render_reference.render(spec, case, reference)
        render_reference.write(reference_path, spec.rate, theirs)

    print(f"\n== {spec.path.stem}: {name}")
    made = None
    if input_path is not None:
        made = specs.signal(spec, case)
        probed = read_wav(input_path)
        if made.shape == probed.shape:
            difference = float(np.abs(made - probed).max(initial=0.0))
            print(f"input: the two sides' signals differ by at most {difference:.1e}")
        else:
            print(f"input: the two sides' signals differ in length, {made.shape} and {probed.shape}")
    drawn = None
    if response_path is not None:
        table = np.loadtxt(response_path, delimiter=",", skiprows=1)
        drawn = (table[:, 0], table[:, 1])

    sides = [("ours", summary(ours, spec.rate))]
    if theirs is not None:
        sides.append(("reference", summary(theirs, spec.rate)))
    print_sides(sides)

    if given.get("signal") == "sweep" and theirs is not None:
        low, high = given["from"], given["to"]
        print("\nmagnitude response, dB in octave bands")
        print(f"{'Hz':>8}{'ours':>10}{'reference':>12}{'difference':>12}")
        ours_bands = octave_response(made, ours, spec.rate, low, high)
        theirs_bands = octave_response(made, theirs, spec.rate, low, high)
        for (centre, a), (_, b) in zip(ours_bands, theirs_bands):
            print(f"{centre:8.0f}{a:10.2f}{b:12.2f}{a - b:12.2f}")
    if given.get("signal") == "steps" and theirs is not None:
        print("\nstatic curve, output level in dBFS at each input level")
        print(f"{'in':>6}{'ours':>10}{'reference':>12}{'difference':>12}")
        ours_curve = static_curve(given, ours, spec.rate)
        theirs_curve = static_curve(given, theirs, spec.rate)
        for (level, a), (_, b) in zip(ours_curve, theirs_curve):
            print(f"{level:6.0f}{a:10.2f}{b:12.2f}{a - b:12.2f}")
    return Rendered(case=case, ours=ours, reference=theirs, given=made, drawn=drawn)


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
    parser.add_argument("--report", type=Path, help="write a report of the spec to this page")
    args = parser.parse_args()
    if not args.binary.exists():
        raise SystemExit(f"{args.binary} isn't there: run `cargo build --release` first")
    spec = specs.read(args.spec.resolve())
    folder = args.out or HERE / "out" / spec.path.stem
    folder.mkdir(parents=True, exist_ok=True)
    cases = args.case or spec.cases or [None]
    reference = None
    if spec.has_reference:
        section = spec.reference(cases[0])
        reference = render_reference.Reference.of(section)
    rendered = [compare_case(spec, case, args.binary, folder, reference) for case in cases]
    sys.stdout.flush()
    print(f"\nWAVs in {folder}", file=sys.stderr)
    if args.report is not None:
        import report

        report.write(spec, rendered, args.report)
        print(f"Report in {args.report}", file=sys.stderr)


if __name__ == "__main__":
    main()
