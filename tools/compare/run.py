"""Render a spec through both sides, ours with `vimtracker probe` and the reference with its plugin,
print a first comparison of the two, and write a report of it as a web page.

Usage: run.py <spec.toml> [--case NAME ...] [--binary PATH] [--out FOLDER] [--report PAGE]
       run.py --suite NAME [--binary PATH]

Every case is rendered unless some are named. The WAVs go in `out/<spec>/` beside this script,
as `<case>-ours.wav` and `<case>-<reference>.wav` for each of the spec's references. Each case
prints every side's peak, RMS and spectral centroid; a sweep's input adds the magnitude response
in octave bands, and stepped levels add the static curve, the output level at each input level. A
spec with no reference renders ours alone. `--report` writes one self-contained page with every
side plotted and measured, laid out as the spec's `[report]` says (see `report.py`).

`--suite nuance` runs every `specs/nuance-*.toml`, writes each one's report into `out/nuance/`,
and an `index.html` there ranking our biggest gaps from the references' consensus first
(`consensus.py`).
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
    """One case as every side rendered it, as stereo samples (2, frames)."""

    case: str | None
    ours: np.ndarray
    # Each reference's, by its name, in the spec's order.
    references: dict[str, np.ndarray]
    # The input every side was given, as made here, for an effect.
    given: np.ndarray | None
    # The track filter's response as the Sound page draws it, as (Hz, dB) arrays, for a sweep.
    drawn: tuple[np.ndarray, np.ndarray] | None

    @property
    def name(self) -> str:
        return self.case or "probe"

    @property
    def reference(self) -> np.ndarray | None:
        """The first reference's samples, or None for ours alone."""
        return next(iter(self.references.values()), None)


def slug(name: str) -> str:
    """A reference's name as a file name's part."""
    return "".join(c if c.isalnum() else "-" for c in name.lower()).strip("-")


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
    header = f"{'':14}" + "".join(f"{side[:11]:>12}" for side, _ in sides)
    print(header + (f"{'difference':>12}" if len(sides) == 2 else ""))
    for key in sides[0][1]:
        values = [values[key] for _, values in sides]
        line = f"{key:14}" + "".join(f"{value:12.2f}" for value in values)
        if len(values) == 2:
            line += f"{values[0] - values[1]:12.2f}"
        print(line)


def make_input(spec, case, binary) -> None:
    """Render the file an input names `made_by`, if it does: that spec's case, our side, written
    where the input reads it, so a riff our voice plays can go through an effect on every side."""
    given = spec.input(case) or {}
    made_by = given.get("made_by") if given.get("signal") == "file" else None
    if not made_by:
        return
    target = (spec.folder / given["path"]).resolve()
    target.parent.mkdir(parents=True, exist_ok=True)
    probe(binary, spec.folder / made_by["spec"], target, made_by.get("case"))


def compare_case(spec, case, binary, folder, references, quiet=False) -> Rendered:
    name = case or "probe"
    ours_path = folder / f"{name}-ours.wav"
    make_input(spec, case, binary)
    given = spec.input(case) or {}
    input_path = folder / f"{name}-input.wav" if given else None
    response_path = folder / f"{name}-drawn.csv" if given.get("signal") == "sweep" else None
    probe(binary, spec.path, ours_path, case, input_path, response_path)
    ours = read_wav(ours_path)
    rendered = {}
    for reference in references:
        rendered[reference.name] = render_reference.render(spec, case, reference)
        render_reference.write(folder / f"{name}-{slug(reference.name)}.wav", spec.rate, rendered[reference.name])
    theirs = next(iter(rendered.values()), None)
    if quiet:
        made = specs.signal(spec, case) if input_path is not None else None
        return Rendered(case=case, ours=ours, references=rendered, given=made, drawn=None)

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
    for reference_name, samples in rendered.items():
        sides.append((reference_name, summary(samples, spec.rate)))
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
    return Rendered(case=case, ours=ours, references=rendered, given=made, drawn=drawn)


def run_spec(path: Path, binary: Path, cases=None, folder=None, quiet=False):
    """Render a spec's cases on every side, loading each reference once."""
    spec = specs.read(path.resolve())
    folder = folder or HERE / "out" / spec.path.stem
    folder.mkdir(parents=True, exist_ok=True)
    cases = cases or spec.cases or [None]
    references = [render_reference.Reference.of(spec.reference(cases[0], name)) for name in spec.reference_names]
    rendered = [compare_case(spec, case, binary, folder, references, quiet) for case in cases]
    return spec, folder, rendered


def run_suite(name: str, binary: Path) -> None:
    """Every `specs/<name>-*.toml`, each with its report, and an index ranking the gaps."""
    import consensus
    import report

    paths = sorted((HERE / "specs").glob(f"{name}-*.toml"))
    if not paths:
        raise SystemExit(f"there are no specs/{name}-*.toml")
    folder = HERE / "out" / name
    folder.mkdir(parents=True, exist_ok=True)
    pages = []
    for path in paths:
        print(f"{path.name}: rendering", file=sys.stderr, flush=True)
        spec, _, rendered = run_spec(path, binary, folder=folder / path.stem, quiet=True)
        page = folder / f"{path.stem}.html"
        built = report.write(spec, rendered, page)
        pages.append((spec, page, built))
        print(f"{path.name}: {page}", file=sys.stderr, flush=True)
    index = folder / "index.html"
    consensus.write_index(name, pages, index, folder / "consensus.json")
    print(f"Index in {index}", file=sys.stderr)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("spec", type=Path, nargs="?")
    parser.add_argument("--suite", help="run every specs/<suite>-*.toml and write an index of them")
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
    if args.suite:
        run_suite(args.suite, args.binary)
        return
    if args.spec is None:
        parser.error("give a spec, or a --suite")
    spec, folder, rendered = run_spec(args.spec, args.binary, args.case, args.out)
    sys.stdout.flush()
    print(f"\nWAVs in {folder}", file=sys.stderr)
    if args.report is not None:
        import report

        report.write(spec, rendered, args.report)
        print(f"Report in {args.report}", file=sys.stderr)


if __name__ == "__main__":
    main()
