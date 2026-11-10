"""A comparison's report: one self-contained web page for a spec, with both sides plotted together
by matplotlib, the plots embedded as PNGs, and the measurements in tables.

`run.py <spec> --report page.html` writes it. The spec's `[report]` says what kind of test it is
and how to lay it out:

    [report]
    kind = "response"     # response, harmonics, oscillation, static, timing, distortion, knob, velocity,
                          # taper, notes, transition, gate or chirp
    title = "..."         # the page's heading; the spec's file name if not given
    about = "..."         # what the test is, in a paragraph or two, for a reader without our code
    figure = ["filter"]   # our settings that give each figure, for response and harmonics
    row = "res"           # the setting that gives each row of a figure's panels
    column = "cut"        # and each column
    line = []             # and each line within a panel, with neither side's own (ours alone)
    window = [1.0, 2.0]   # seconds to measure a held note or a ringing filter over
    knob = "acid.cutoff"  # our knob a knob sweep turns, and the references', one name for all or a
    reference_knob = "cutoff"   # table by reference: { "Acid V" = "envelope_mod", ABL3 = "envmod" }
    measure = "note"      # what a knob sweep measures: note, filter, decay or accents
    columns = 3           # how many panels go in a row, for notes, transition and gate
    step = 0.125          # a step's length in seconds, for notes, so rests show

The kinds:

- `response`: a sweep's magnitude response, dB against Hz, with the peak, the gain at Cut, and
  the gain two octaves either side of Cut, which for a low-pass is its bass.
- `harmonics`: a held sawtooth's harmonics, each times its number so a sawtooth's are level,
  which shows a voice's filter where it can't be given a sweep.
- `oscillation`: a filter ringing on its own from one click, its pitch in cents off its cutoff.
- `static`: a compressor's static curve, from stepped levels.
- `timing`: a compressor's gain over a tone burst, with attack and release times.
- `distortion`: the harmonics an effect adds to a sine at stepped levels, as THD and the 2nd and
  3rd harmonics against the input level.
- `knob`: one knob swept on both sides, each step's spectral centroid and peak level, or with
  `measure` its filter's peak and bass, its brightness's decay, or its accents (see `Knob`).
- `velocity`: one render of notes at different velocities, each note's level and brightness
  against the first's, for how a voice answers velocity (see `Velocity`).
- `taper`: a filter's Res knob in steps, each a sweep: its peak's height and place, its bass loss
  and where it starts to self-oscillate, at each Cut (see `Taper`).
- `notes`: a pattern of notes in each case, each note's brightness and level against the first's,
  for accents in a row and knobs' extremes (see `Notes`).
- `transition`: two notes in each case, the glide, dip and retrigger between them, for slides and
  legato (see `Transition`).
- `gate`: one note in each case, its level as it's held and its release (see `Gate`).
- `chirp`: each note's resonant peak over time, its place and its height, as the filter envelope
  sweeps it, for the 303's squelch at high Res (see `Chirp`).

Every side is drawn on the same plots: ours, and each of the spec's references. A kind that gives
its measurements as `measures` gets a consensus view as well (`consensus.py`): where the
references agree, that's the target, and our gaps from it are ranked; where they disagree, both
are shown and the measure is flagged.
"""

from __future__ import annotations

import base64
import datetime
import html
import io
import math
from collections import defaultdict
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
from matplotlib.ticker import FuncFormatter, LogLocator, NullFormatter  # noqa: E402

import measure  # noqa: E402

# Each side's colour, the same on every plot: ours blue, the first reference orange and the second
# green, which stay apart for colour-blind readers too. Ours alone, its lines take the categorical
# colours in this order.
OURS = "#2a78d6"
REFERENCE = "#eb6834"
REFERENCES = [REFERENCE, "#1baf7a", "#eda100", "#6250d6"]
# The references' consensus, where they agree, drawn as a band under the sides.
CONSENSUS = "#b8b6b0"
CATEGORICAL = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"]
# Steps of one blue, light to dark, for a setting's levels such as Res 0, 50% and 90%.
SEQUENTIAL = ["#8fbcee", "#2a78d6", "#123f75"]
INK = "#1f1f1e"
MUTED = "#6b6a66"
RULE = "#e4e3df"
# A filter's cutoff on the 00 to FF scale runs from 20 Hz to 20 kHz, evenly in octaves.
LOWEST_CUT_HZ = 20.0
CUT_RANGE = 1000.0
# What a table calls a layout's settings.
HEADINGS = {"cut": "Cut", "res": "Res", "input.level": "Input", "acid.cutoff": "Cutoff", "acid.resonance": "Res"}
# A response's levels that are too far down to read are drawn at this floor.
LOWEST_DB = -60.0
# A ringing filter quieter than this, in dBFS, isn't ringing.
RINGING_DB = -60.0
# The highest a high-pass's passband is read at, under where a sweep to 20 kHz ends.
TOP_PASSBAND_HZ = 16_000.0
# How far from Cut a filter's passband is read, in octaves, for its peak's height over it.
PASSBAND_OCTAVES = 3.0

plt.rcParams.update(
    {
        "font.family": ["Helvetica Neue", "Helvetica", "Arial", "DejaVu Sans"],
        "font.size": 8.5,
        "axes.edgecolor": MUTED,
        "axes.labelcolor": INK,
        "axes.titlesize": 9,
        "axes.titleweight": "normal",
        "axes.spines.top": False,
        "axes.spines.right": False,
        "axes.grid": True,
        "grid.color": RULE,
        "grid.linewidth": 0.6,
        "xtick.color": MUTED,
        "ytick.color": MUTED,
        "legend.frameon": False,
        "lines.linewidth": 1.4,
        "figure.dpi": 100,
        "savefig.dpi": 100,
        "savefig.facecolor": "white",
    }
)


# What's shown of a setting and its value.


def passband_hz(nominal: float, high: bool) -> float:
    """Where a filter's passband is read for how far its peak stands over it: three octaves under
    Cut, or over it for a high-pass, though no higher than a sweep reaches."""
    if high:
        return min(nominal * 2.0**PASSBAND_OCTAVES, TOP_PASSBAND_HZ)
    return nominal * 2.0**-PASSBAND_OCTAVES


def cut_hz(value: int) -> float:
    """The track filter's cutoff in Hz for its `cut` on the 00 to FF scale."""
    return LOWEST_CUT_HZ * CUT_RANGE ** (value / 255.0)


def hertz(value: float) -> str:
    if not math.isfinite(value):
        return "–"
    return f"{value / 1000:.2f} kHz" if value >= 1000 else f"{value:.0f} Hz"


def number(value: float, digits: int = 1, sign: bool = False) -> str:
    if value is None or not math.isfinite(value):
        return "–"
    text = f"{value:+.{digits}f}" if sign else f"{value:.{digits}f}"
    return text.replace("-", "−")


def ratio_text(ratio: float, digits: int = 1) -> str:
    """A compressor's ratio, where one that turns its output down as its input rises, or holds
    it, is a limiter's."""
    return "∞:1" if math.isinf(ratio) else f"{number(ratio, digits)}:1"


def setting_label(key: str, value) -> str:
    """A setting as the report shows it, with what its value means where that's known."""
    if value is None:
        return ""
    if isinstance(value, str):
        return f"{key} {value}"
    if key == "input.level":
        return f"Input {number(value, 0)} dBFS"
    if key == "input.on":
        return f"Burst {value * 1000:g} ms"
    if isinstance(value, float):
        return f"{key} {value:g}"
    shown = f"0x{value:02X}"
    if key == "cut":
        return f"Cut {shown} ({hertz(cut_hz(value))})"
    if key in ("res", "acid.resonance"):
        return f"Res {shown} ({value / 255:.0%})"
    if key == "acid.cutoff":
        return f"Cutoff {shown}"
    return f"{key} {shown}"


def group_label(keys: list[str], settings: dict) -> str:
    parts = []
    for key in keys:
        value = settings.get(key)
        if value is None:
            continue
        parts.append(str(value) if key in ("filter", "mode") else setting_label(key, value))
    return " ".join(parts) or "All cases"


def as_list(value) -> list[str]:
    if value is None:
        return []
    return [value] if isinstance(value, str) else list(value)


# Pieces of the page.


def image(figure) -> str:
    buffer = io.BytesIO()
    figure.savefig(buffer, format="png", bbox_inches="tight")
    plt.close(figure)
    encoded = base64.b64encode(buffer.getvalue()).decode("ascii")
    return f'<img src="data:image/png;base64,{encoded}" alt="">'


def table(headers: list[str], rows: list[list], numeric_from: int = 1) -> str:
    head = "".join(
        f'<th class="n">{html.escape(h)}</th>' if i >= numeric_from else f"<th>{html.escape(h)}</th>"
        for i, h in enumerate(headers)
    )
    body = []
    for row in rows:
        cells = "".join(
            f'<td class="n">{html.escape(str(c))}</td>' if i >= numeric_from else f"<td>{html.escape(str(c))}</td>"
            for i, c in enumerate(row)
        )
        body.append(f"<tr>{cells}</tr>")
    return f'<div class="scroll"><table><thead><tr>{head}</tr></thead><tbody>{"".join(body)}</tbody></table></div>'


def bullets(items: list[str]) -> str:
    if not items:
        return ""
    return "<ul>" + "".join(f"<li>{item}</li>" for item in items) + "</ul>"


def legend_note(entries: list[tuple[str, str, str]]) -> str:
    """What each line is, as a key under a heading: (colour, line style, what it is)."""
    items = []
    for colour, style, what in entries:
        dash = {"solid": "solid", "dashed": "dashed", "dotted": "dotted", "marker": "solid"}[style]
        items.append(
            f'<span class="key"><span class="swatch" style="border-top: 2px {dash} {colour}"></span>'
            f"{html.escape(what)}</span>"
        )
    return '<p class="keys">' + "".join(items) + "</p>"


def describe_input(given: dict | None, notes: list | None) -> str:
    if notes:
        played = ", ".join(
            f"MIDI note {n['note']} at {n['time']:g} s for {n['length']:g} s, velocity {n.get('velocity', 100)}"
            for n in notes
        )
        return f"Each side plays {played}."
    if not given:
        return ""
    kind = given["signal"]
    if kind == "sweep":
        return (
            f"The input is an exponential sine sweep (Farina's) from {given['from']:g} Hz to "
            f"{given['to']:g} Hz over {given['seconds']:g} s at {given['level']:g} dBFS."
        )
    if kind == "steps":
        return (
            f"The input is a {given['frequency']:g} Hz sine stepped from {given['from']:g} to "
            f"{given['to']:g} dBFS, {given.get('step', 1):g} dB at a time, each held "
            f"{given['hold']:g} s."
        )
    if kind == "burst":
        return (
            f"The input is a {given['frequency']:g} Hz sine at {given['rest']:g} dBFS for "
            f"{given['before']:g} s, then {given['level']:g} dBFS for a burst (its length is in the "
            f"cases), then {given['rest']:g} dBFS for {given['after']:g} s."
        )
    if kind == "impulse":
        return (
            f"The input is one sample at {given['level']:g} dBFS, then silence, for "
            f"{given['seconds']:g} s: a click for the filter to ring from."
        )
    return f"The input is a {kind}."


def settings_table(spec) -> str:
    ours = spec.data.get("ours", {})
    rows = []
    if ours.get("machine"):
        rows.append(["ours", "machine", ours["machine"]])
    if ours.get("effects"):
        rows.append(["ours", "effects", ", ".join(ours["effects"])])
    import spec as specs

    for key, value in specs.flatten(ours.get("settings", {})).items():
        rows.append(["ours", key, f"0x{value:02X}" if isinstance(value, int) else value])
    for name, section in spec._references():
        for key in ("channels", "accent_from", "preroll", "wake_note"):
            if key in section:
                rows.append([name, key, section[key]])
        for key, value in (section.get("state") or {}).items():
            rows.append([name, f"state {key}", value])
        for key, value in (section.get("parameters") or {}).items():
            rows.append([name, key, value])
    if not rows:
        return ""
    return table(["Side", "Setting", "Value"], rows, numeric_from=3)


def cases_table(spec) -> str:
    rows = []
    for case in spec.data.get("cases", []):
        import spec as specs

        ours = ", ".join(
            f"{k} = {f'0x{v:02X}' if isinstance(v, int) else v}" for k, v in specs.flatten(case.get("ours", {})).items()
        )
        theirs = ", ".join(f"{k} = {v}" for k, v in (case.get("reference") or {}).items())
        for name, values in (case.get("references") or {}).items():
            theirs += ("; " if theirs else "") + f"{name}: " + ", ".join(f"{k} = {v}" for k, v in values.items())
        given = ", ".join(f"{k} = {v}" for k, v in (case.get("input") or {}).items())
        if "notes" in case:
            given = (given + "; " if given else "") + notes_text(case["notes"])
        rows.append([case["name"], ours, theirs, given] if spec.has_reference else [case["name"], ours, given])
    if not rows:
        return ""
    headers = ["Case", "Ours", "References", "Input"] if spec.has_reference else ["Case", "Ours", "Input"]
    if not any(row[-1] for row in rows):
        headers, rows = headers[:-1], [row[:-1] for row in rows]
    return table(headers, rows, numeric_from=len(headers))


def notes_text(notes: list) -> str:
    """A case's notes, short: each one's MIDI note, start and length in ms, and its accent (A) or
    slide (S)."""
    parts = []
    for note in notes:
        marks = ("A" if note.get("accent") else "") + ("S" if note.get("slide") else "")
        parts.append(f"{note['note']} at {note['time'] * 1000:g} ms for {note['length'] * 1000:g}{(' ' + marks) if marks else ''}")
    return "notes " + ", ".join(parts)


def log_frequency_axis(axes, low=20.0, high=20_000.0):
    axes.set_xscale("log")
    axes.set_xlim(low, high)
    axes.xaxis.set_major_locator(LogLocator(base=10, subs=(1.0, 2.0, 5.0)))
    axes.xaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v / 1000:g}k" if v >= 1000 else f"{v:g}"))
    axes.xaxis.set_minor_formatter(NullFormatter())


def grid_of(cases, settings, row_key, column_key):
    """The distinct values of the row and column settings among some cases, in order."""
    rows = sorted({settings[c].get(row_key) for c in cases}, key=str) if row_key else [None]
    columns = sorted({settings[c].get(column_key) for c in cases}, key=str) if column_key else [None]

    def order(values):
        return sorted(values, key=lambda v: (isinstance(v, str), v if v is not None else -1))

    return order(rows), order(columns)


# Each kind of report: what it measures, plots and says.


class Report:
    def __init__(self, spec, rendered):
        self.spec = spec
        self.rendered = rendered
        self.layout = spec.report
        # The references by name, in the spec's order. The kinds that compare ours with one
        # reference compare it with the first, and plot them all.
        self.reference_names = spec.reference_names
        self.reference_name = self.reference_names[0] if self.reference_names else None
        self.first = self.reference_name
        # Our settings, and the input's fields as `input.level` and so on, so a layout can lay
        # cases out by either.
        self.settings = {
            r.name: spec.settings(r.case)
            | {f"input.{k}": v for k, v in (spec.input(r.case) or {}).items()}
            for r in rendered
        }
        self.parameters = {
            r.name: (spec.reference(r.case)["parameters"] if spec.has_reference else {}) for r in rendered
        }
        # Each reference's parameters for each case.
        self.reference_parameters = {
            r.name: {name: spec.reference(r.case, name)["parameters"] for name in self.reference_names}
            for r in rendered
        }
        self.inputs = {r.name: spec.input(r.case) for r in rendered}

    def sides(self, r):
        """Each side's key, name, colour and samples: ours, then each reference by its name."""
        out = [("ours", "vimtracker", OURS, r.ours)]
        for name, samples in r.references.items():
            out.append((name, name, self.colour(name), samples))
        return out

    def colour(self, side: str) -> str:
        if side == "ours":
            return OURS
        return REFERENCES[self.reference_names.index(side) % len(REFERENCES)]

    def label(self, side: str) -> str:
        return "vimtracker" if side == "ours" else side

    def keys(self) -> list[tuple[str, str, str]]:
        entries = [(OURS, "solid", "vimtracker (ours)")]
        for name in self.reference_names:
            entries.append((self.colour(name), "solid", f"{name} (a reference)"))
        return entries

    def reference_knob(self, name: str) -> str | None:
        """The knob a knob sweep turns on a reference: the layout's `reference_knob`, one name for
        every reference or a table of them by reference."""
        knob = self.layout.get("reference_knob")
        return knob.get(name) if isinstance(knob, dict) else knob


class Response(Report):
    """A filter's magnitude response from a sweep."""

    reading = (
        "Each panel is one filter setting. The curves are the gain from input to output, in dB, "
        "at each frequency, read from the sweep. The grey line marks our Cut, the frequency both "
        "sides were set to. The table gives the peak's frequency and height, the gain at Cut, and "
        "the gain two octaves under and over Cut; for a low-pass, the gain under Cut is its bass, "
        "and the bass loss is how much of it a setting loses against the same filter at Res 0."
    )

    def measures(self, bins, bin_gains, frequencies, gains, nominal):
        under = measure.gain_at(frequencies, gains, nominal / 4)
        over = measure.gain_at(frequencies, gains, nominal * 4)
        found = measure.peak(bins, bin_gains, nominal)
        # A peak only if it stands a decibel over the response an octave either side of it, where
        # the sweep reaches, so a flat passband's highest point isn't one.
        if found:
            beside = [measure.gain_at(frequencies, gains, found[0] * f) for f in (0.5, 2.0)]
            if any(math.isfinite(b) and b > found[1] - 1.0 for b in beside):
                found = None
        return {
            "peak Hz": found[0] if found else float("nan"),
            "peak dB": found[1] if found else float("nan"),
            "at Cut dB": measure.gain_at(frequencies, gains, nominal),
            "under dB": under,
            "over dB": over,
        }

    def build(self):
        figure_keys = as_list(self.layout.get("figure", ["filter", "mode"]))
        row_key = self.layout.get("row", "res")
        column_key = self.layout.get("column", "cut")
        line_keys = as_list(self.layout.get("line"))
        # The setting a bass loss is measured against at 0.
        resonance_key = self.layout.get("resonance", "res")
        given = self.inputs[self.rendered[0].name]
        low, high = given["from"], given["to"]

        results = {}
        for r in self.rendered:
            settings = self.settings[r.name]
            nominal = cut_hz(settings.get("cut", 255))
            entry = {"nominal": nominal, "sides": {}}
            for side, name, colour, samples in self.sides(r):
                bins, bin_gains = measure.bin_response(r.given, samples, self.spec.rate, low, high)
                f, g = measure.response(bins, bin_gains)
                entry["sides"][side] = (f, g, self.measures(bins, bin_gains, f, g, nominal))
                if side == "ours":
                    ours_bins = (bins, bin_gains)
            if r.drawn is not None:
                # Against the bins themselves, at the drawing's points, so a narrow peak or dip
                # isn't averaged away.
                drawn_hz, drawn_db = r.drawn
                heard = np.interp(drawn_hz, *ours_bins)
                usable = (drawn_db > -40.0) & (drawn_hz >= 30.0) & (drawn_hz <= 16_000.0)
                entry["drawn"] = r.drawn
                entry["drawn difference"] = (
                    float(np.max(np.abs(heard[usable] - drawn_db[usable]))) if usable.any() else float("nan")
                )
            results[r.name] = entry

        # The bass loss: the gain under Cut against the same filter and Cut at Res 0.
        for r in self.rendered:
            settings = self.settings[r.name]
            sibling = next(
                (
                    other.name
                    for other in self.rendered
                    if self.settings[other.name].get(resonance_key) == 0
                    and all(
                        self.settings[other.name].get(k) == settings.get(k)
                        for k in figure_keys + [row_key, column_key] + line_keys
                        if k and k != resonance_key
                    )
                ),
                None,
            )
            for side, values in results[r.name]["sides"].items():
                base = results[sibling]["sides"][side][2]["under dB"] if sibling else float("nan")
                # Only a filter that passes bass at Res 0 can lose it.
                passes = math.isfinite(base) and base > -3.0
                values[2]["bass loss dB"] = values[2]["under dB"] - base if passes else float("nan")

        figures, rows_out, summary = [], [], []
        groups = defaultdict(list)
        for r in self.rendered:
            groups[tuple(self.settings[r.name].get(k) for k in figure_keys)].append(r.name)
        for key, names in groups.items():
            title = group_label(figure_keys, self.settings[names[0]])
            rows, columns = grid_of(names, self.settings, row_key, column_key)
            figure, axes = plt.subplots(
                len(rows), len(columns), figsize=(3.4 * len(columns), 2.5 * len(rows) + 0.3), squeeze=False, sharey=True
            )
            lowest = 0.0
            for name in names:
                settings = self.settings[name]
                i = rows.index(settings.get(row_key)) if rows != [None] else 0
                j = columns.index(settings.get(column_key)) if columns != [None] else 0
                panel = axes[i][j]
                entry = results[name]
                line_index = 0
                if line_keys:
                    values = sorted({self.settings[n].get(line_keys[0]) for n in names}, key=lambda v: (isinstance(v, str), v))
                    line_index = values.index(settings.get(line_keys[0]))
                for side, (f, g, _) in entry["sides"].items():
                    colour = self.colour(side) if not line_keys else SEQUENTIAL[line_index % len(SEQUENTIAL)]
                    shown = np.maximum(g, LOWEST_DB)
                    label = setting_label(line_keys[0], settings.get(line_keys[0])) if line_keys else None
                    panel.plot(f, shown, color=colour, label=label)
                    lowest = min(lowest, float(np.nanmin(shown)))
                if "drawn" in entry and not self.reference_name:
                    colour = SEQUENTIAL[line_index % len(SEQUENTIAL)] if line_keys else OURS
                    panel.plot(entry["drawn"][0], np.maximum(entry["drawn"][1], LOWEST_DB), color=colour, linestyle=":", linewidth=1.1)
                panel.axvline(entry["nominal"], color=MUTED, linewidth=0.7, alpha=0.6)
                log_frequency_axis(panel, low, high)
                heading = []
                if row_key and rows != [None]:
                    heading.append(setting_label(row_key, settings.get(row_key)))
                if column_key and columns != [None]:
                    heading.append(setting_label(column_key, settings.get(column_key)))
                panel.set_title(", ".join(heading), loc="left")
            for i in range(len(rows)):
                axes[i][0].set_ylabel("Gain, dB")
            for j in range(len(columns)):
                axes[-1][j].set_xlabel("Hz")
            for row in axes:
                for panel in row:
                    panel.set_ylim(max(LOWEST_DB, lowest - 3.0), None)
            if line_keys:
                axes[0][-1].legend(loc="lower left", fontsize=7.5)
            figure.tight_layout()
            figures.append(f"<h3>{html.escape(title)}</h3>{image(figure)}")

            # What differs most in this figure.
            if self.reference_name:
                worst_peak = worst_height = worst_bass = None
                for name in names:
                    ours, theirs = results[name]["sides"]["ours"][2], results[name]["sides"][self.first][2]
                    if math.isfinite(ours["peak Hz"]) and math.isfinite(theirs["peak Hz"]):
                        off = measure.cents(ours["peak Hz"], theirs["peak Hz"])
                        if worst_peak is None or abs(off) > abs(worst_peak[0]):
                            worst_peak = (off, name, ours["peak Hz"], theirs["peak Hz"])
                        height = ours["peak dB"] - theirs["peak dB"]
                        if worst_height is None or abs(height) > abs(worst_height[0]):
                            worst_height = (height, name, ours["peak dB"], theirs["peak dB"])
                    bass = ours["bass loss dB"] - theirs["bass loss dB"]
                    if math.isfinite(bass) and (worst_bass is None or abs(bass) > abs(worst_bass[0])):
                        worst_bass = (bass, name, ours["bass loss dB"], theirs["bass loss dB"])
                parts = []
                if worst_peak:
                    off, name, a, b = worst_peak
                    parts.append(
                        f"peaks differ most at {self.case_label(name, figure_keys)}: ours at {hertz(a)}, "
                        f"{self.reference_name} at {hertz(b)} ({number(off, 0, True)} cents)"
                    )
                if worst_height:
                    diff, name, a, b = worst_height
                    parts.append(
                        f"heights most at {self.case_label(name, figure_keys)}: {number(a, 1, True)} dB "
                        f"against {number(b, 1, True)} dB"
                    )
                if worst_bass and abs(worst_bass[0]) >= 0.5:
                    diff, name, a, b = worst_bass
                    parts.append(
                        f"bass loss most at {self.case_label(name, figure_keys)}: {number(a, 1)} dB against "
                        f"{number(b, 1)} dB"
                    )
                if parts:
                    summary.append(f"<b>{html.escape(title)}</b>: " + "; ".join(html.escape(p) for p in parts) + ".")
            else:
                # The most it strays at each resonance, as a saturating filter strays most where
                # its peak is tallest.
                by_resonance = defaultdict(list)
                for n in names:
                    by_resonance[self.settings[n].get(resonance_key)].append(results[n].get("drawn difference", float("nan")))
                parts = [
                    f"{number(max(values), 1)} dB at {setting_label(resonance_key, level)}"
                    for level, values in sorted(by_resonance.items(), key=lambda item: (item[0] is None, item[0]))
                ]
                summary.append(
                    f"<b>{html.escape(title)}</b>: what's heard strays from what the Sound page draws by at "
                    f"most {html.escape(', '.join(parts))}."
                )

            for name in names:
                settings = self.settings[name]
                for side, (_, _, values) in results[name]["sides"].items():
                    row = [
                        title if side == "ours" else "",
                        setting_label(column_key, settings.get(column_key)) if side == "ours" else "",
                        setting_label(row_key, settings.get(row_key)) if side == "ours" else "",
                        self.label(side),
                        hertz(values["peak Hz"]),
                        number(values["peak dB"], 1, True),
                        number(values["at Cut dB"], 1, True),
                        number(values["under dB"], 1, True),
                        number(values["over dB"], 1, True),
                        number(values["bass loss dB"], 1, True),
                    ]
                    if side == "ours":
                        row.append(number(results[name].get("drawn difference", float("nan")), 1))
                    else:
                        row.append("")
                    rows_out.append(row)
        headers = [
            "Filter", HEADINGS.get(column_key, column_key or ""), HEADINGS.get(row_key, row_key or ""), "Side", "Peak", "Peak dB", "At Cut dB",
            "2 oct under dB", "2 oct over dB", "Bass loss dB", "Off drawing, dB",
        ]
        keys = self.keys() if not line_keys else [(SEQUENTIAL[1], "solid", "vimtracker, measured, by Res")]
        if not self.reference_name:
            keys.append((SEQUENTIAL[1] if line_keys else OURS, "dotted", "vimtracker, as the Sound page draws it, in the same colour"))
        return {
            "keys": keys,
            "summary": summary,
            "figures": figures,
            "numbers": table(headers, rows_out, numeric_from=4),
            "numbers note": (
                "Off drawing is the most the measured response strays from the Sound page's drawing of "
                "the same filter, where the drawing is above −40 dB."
            ),
        }

    def case_label(self, name, figure_keys):
        settings = self.settings[name]
        keys = [k for k in (self.layout.get("column", "cut"), self.layout.get("row", "res")) if k]
        parts = [setting_label(k, settings.get(k)) for k in keys if k not in figure_keys]
        return ", ".join(parts) or name


class Harmonics(Report):
    """A voice's filter, read from a held sawtooth's harmonics."""

    reading = (
        "Each panel is one setting of the voice. Each point is one harmonic of the held note, its "
        "level times its number, so an unfiltered sawtooth would be a level line and the shape is "
        "the filter's, as heard through the voice. Levels are in dB against the same side's Res 0 "
        "at the same cutoff, over its 2nd to 4th harmonics, so a fall at the left is bass lost to "
        "resonance. The table gives each side's peak, how much bass it keeps against Res 0, and "
        "where it has fallen 12 dB under its bass."
    )

    def build(self):
        row_key = self.layout.get("row", "acid.resonance")
        column_key = self.layout.get("column", "acid.cutoff")
        start, end = self.layout.get("window", [1.0, 2.0])
        note = self.spec.data["notes"][0]["note"]
        expected = 440.0 * 2.0 ** ((note - 69) / 12)

        found = {}
        for r in self.rendered:
            found[r.name] = {}
            for side, name, colour, samples in self.sides(r):
                fundamental, levels = measure.harmonics(samples, self.spec.rate, start, end, expected)
                found[r.name][side] = (fundamental, levels)

        def base(name, side):
            settings = self.settings[name]
            sibling = next(
                (
                    n
                    for n in found
                    if self.settings[n].get(row_key) == 0 and self.settings[n].get(column_key) == settings.get(column_key)
                ),
                name,
            )
            levels = found[sibling][side][1]
            return float(np.mean(levels[1:4, 1]))

        measured = {}
        for name in found:
            measured[name] = {}
            for side, (fundamental, levels) in found[name].items():
                relative = levels[:, 1] - base(name, side)
                own = float(np.mean(relative[1:4]))
                top = 1 + int(np.argmax(relative[1:]))
                is_peak = relative[top] > own + 1.0
                under = np.where((np.arange(len(relative)) > top) & (relative < own - 12.0))[0]
                measured[name][side] = {
                    "fundamental": fundamental,
                    "levels": np.column_stack([levels[:, 0], relative]),
                    "peak Hz": levels[top, 0] if is_peak else float("nan"),
                    "peak dB": relative[top] - own if is_peak else float("nan"),
                    "bass dB": own,
                    "fall Hz": levels[under[0], 0] if under.size else float("nan"),
                }

        names = [r.name for r in self.rendered]
        rows, columns = grid_of(names, self.settings, row_key, column_key)
        figure, axes = plt.subplots(len(rows), len(columns), figsize=(3.4 * len(columns), 2.5 * len(rows) + 0.3), squeeze=False, sharey=True)
        for name in names:
            settings = self.settings[name]
            panel = axes[rows.index(settings.get(row_key))][columns.index(settings.get(column_key))]
            for side, values in measured[name].items():
                colour = self.colour(side)
                levels = values["levels"]
                panel.plot(levels[:, 0], np.maximum(levels[:, 1], LOWEST_DB), color=colour, marker="o", markersize=2.0, linewidth=1.0)
            log_frequency_axis(panel, 30.0, 16_000.0)
            panel.set_title(f"{setting_label(column_key, settings[column_key])}, {setting_label(row_key, settings[row_key])}", loc="left")
        for row in axes:
            row[0].set_ylabel("Level, dB")
        for panel in axes[-1]:
            panel.set_xlabel("Hz")
        figure.tight_layout()

        table_rows, summary = [], []
        for name in names:
            settings = self.settings[name]
            for side, values in measured[name].items():
                table_rows.append([
                    setting_label(column_key, settings[column_key]) if side == "ours" else "",
                    setting_label(row_key, settings[row_key]) if side == "ours" else "",
                    self.label(side),
                    f"{values['fundamental']:.1f}",
                    hertz(values["peak Hz"]),
                    number(values["peak dB"], 1, True),
                    number(values["bass dB"], 1, True),
                    hertz(values["fall Hz"]),
                ])
        if self.reference_name:
            for column in columns:
                for row in rows:
                    name = next((n for n in names if self.settings[n].get(column_key) == column and self.settings[n].get(row_key) == row), None)
                    if name is None or row == 0:
                        continue
                    parts = [
                        f"{'ours' if side == 'ours' else side} peaks at {hertz(values['peak Hz'])} "
                        f"({number(values['peak dB'], 1, True)} dB) and keeps {number(values['bass dB'], 1, True)} dB of its bass"
                        for side, values in measured[name].items()
                    ]
                    summary.append(html.escape(f"{setting_label(column_key, column)}, {setting_label(row_key, row)}: " + "; ".join(parts) + "."))
        fundamentals = {side: [measured[n][side]["fundamental"] for n in names] for side in measured[names[0]]}
        notes = "; ".join(
            f"{self.label(side)} plays it at {np.median(values):.1f} Hz"
            for side, values in fundamentals.items()
        )
        return {
            "keys": [(c, "marker", w) for c, _, w in self.keys()],
            "summary": summary,
            "figures": [image(figure)],
            "numbers": table(["Cutoff", "Res", "Side", "Fundamental Hz", "Peak", "Peak dB over bass", "Bass dB vs Res 0", "12 dB under bass at"], table_rows, numeric_from=3),
            "numbers note": f"The note is MIDI {note}, {expected:.1f} Hz; {notes}. Measured from {start:g} s to {end:g} s, after the filter's envelope has decayed.",
        }


class Oscillation(Report):
    """A filter ringing on its own at full resonance."""

    @property
    def reading(self) -> str:
        if self.layout.get("reference_knob_is") == "midi":
            theirs = (
                "The reference's cutoff is a MIDI note number, so its cents are off that note's "
                "frequency."
            )
        else:
            theirs = (
                "The reference's knob has no frequency printed on it, so its cents are off a "
                "straight line, in octaves against the knob, fitted through its own points: that "
                "measures how evenly it tracks, not where it sits."
            )
        return (
            "Each point is one cutoff. Its frequency is the strongest in the filter's ringing, "
            "after a single click, over the measuring window. Ours is in cents off the frequency "
            "its Cut is set to, so a filter that tracks its knob sits on 0. "
            + (theirs + " " if self.reference_name else "")
            + "The right panel is how loud the ringing is."
        )

    def build(self):
        line_keys = as_list(self.layout.get("line", ["filter"]))
        knob = self.layout.get("reference_knob", "cutoff")
        start, end = self.layout.get("window", [1.0, 2.0])
        points = []
        for r in self.rendered:
            settings = self.settings[r.name]
            for side, name, colour, samples in self.sides(r):
                frequency, level = measure.pitch(samples, self.spec.rate, start, end)
                points.append({
                    "case": r.name,
                    "side": side,
                    "line": group_label(line_keys, settings) if side == "ours" else side,
                    "knob": settings.get("cut") if side == "ours" else self.reference_parameters[r.name][side].get(knob),
                    "nominal": cut_hz(settings.get("cut", 255)) if side == "ours" else float("nan"),
                    "frequency": frequency,
                    "level": level,
                    "rings": level > RINGING_DB,
                })
        fit_note = ""
        notes = []
        for reference in self.reference_names:
            theirs = [p for p in points if p["side"] == reference and p["rings"]]
            if self.layout.get("reference_knob_is") == "midi":
                # The reference's knob is a MIDI note number, so it says its own frequency.
                for p in points:
                    if p["side"] == reference:
                        p["nominal"] = 440.0 * 2.0 ** ((p["knob"] - 69.0) / 12.0)
                notes.append(f"{reference}'s {knob} is a MIDI note number, so it's set to that note's frequency.")
            elif len(theirs) >= 2:
                slope, offset = np.polyfit([p["knob"] for p in theirs], [math.log2(p["frequency"]) for p in theirs], 1)
                for p in points:
                    if p["side"] == reference:
                        p["nominal"] = 2.0 ** (offset + slope * p["knob"])
                notes.append(
                    f"{reference}'s fitted line: {2 ** offset:.1f} Hz at {knob} 0, rising "
                    f"{slope:.2f} octaves over its knob's whole turn."
                )
        fit_note = " ".join(notes)
        for p in points:
            p["cents"] = measure.cents(p["frequency"], p["nominal"]) if p["rings"] else float("nan")

        lines = list(dict.fromkeys(p["line"] for p in points))
        figure, (left, right) = plt.subplots(1, 2, figsize=(10.0, 3.2))
        for index, line in enumerate(lines):
            chosen = [p for p in points if p["line"] == line]
            ours_only = not self.reference_name
            colour = CATEGORICAL[index % len(CATEGORICAL)] if ours_only else self.colour(chosen[0]["side"])
            ringing = [p for p in chosen if p["rings"]]
            left.plot([p["nominal"] for p in ringing], [p["cents"] for p in ringing], color=colour, marker="o", markersize=3.5, label=line)
            right.plot([p["nominal"] for p in chosen if math.isfinite(p["nominal"])], [p["level"] for p in chosen if math.isfinite(p["nominal"])], color=colour, marker="o", markersize=3.5, label=line)
        for panel in (left, right):
            log_frequency_axis(panel, 20.0, 20_000.0)
            panel.set_xlabel("The frequency it's set to, Hz")
        left.axhline(0.0, color=MUTED, linewidth=0.7)
        left.set_ylabel("Cents off")
        right.set_ylabel("Level, dBFS peak")
        right.legend(loc="lower left", fontsize=7.5)
        figure.tight_layout()

        summary = []
        for line in lines:
            chosen = [p for p in points if p["line"] == line]
            ringing = [p for p in chosen if p["rings"]]
            silent = len(chosen) - len(ringing)
            if not ringing:
                summary.append(f"<b>{html.escape(line)}</b> doesn't ring at any cutoff measured.")
                continue
            offs = [p["cents"] for p in ringing]
            text = (
                f"<b>{html.escape(line)}</b>: from {number(min(offs), 0, True)} to {number(max(offs), 0, True)} "
                f"cents off, at {number(min(p['level'] for p in ringing), 1)} to "
                f"{number(max(p['level'] for p in ringing), 1)} dBFS"
            )
            if silent:
                text += f"; silent at {silent} of {len(chosen)} cutoffs"
            summary.append(text + ".")
        rows = [
            [
                p["line"],
                p["case"],
                (f"0x{p['knob']:02X}" if p["side"] == "ours" else f"{p['knob']:g}") if p["knob"] is not None else "",
                hertz(p["nominal"]),
                f"{p['frequency']:.1f}" if p["rings"] else "–",
                number(p["cents"], 1, True),
                number(p["level"], 1),
            ]
            for p in points
        ]
        keys = (
            self.keys()
            if self.reference_name
            else [(CATEGORICAL[i % len(CATEGORICAL)], "marker", line) for i, line in enumerate(lines)]
        )
        return {
            "keys": keys,
            "summary": summary,
            "figures": [image(figure)],
            "numbers": table(["Line", "Case", "Knob", "Set to", "Rings at Hz", "Cents off", "Level dBFS"], rows, numeric_from=2),
            "numbers note": f"Measured from {start:g} s to {end:g} s. {fit_note}",
        }


class Static(Report):
    """A compressor's static curve."""

    reading = (
        "Each panel is one setting. The curves are the gain reduction at each input level: how "
        "much quieter than its input the output is once the compressor has settled, against the "
        "same side's quietest steps, where nothing is reduced. The table gives the ratio over the "
        "top 10 dB of input, the reduction at 0 dBFS, and the input levels where the reduction "
        "first reaches 0.5 and 1 dB, which shows where the knee starts to bend."
    )

    def build(self):
        names = [r.name for r in self.rendered]
        figure, axes = plt.subplots(1, len(names), figsize=(3.4 * len(names), 3.0), squeeze=False, sharey=True)
        rows, summary = [], []
        for index, r in enumerate(self.rendered):
            given = self.inputs[r.name]
            panel = axes[0][index]
            found = {}
            for side, name, colour, samples in self.sides(r):
                curve = measure.static_curve(given, samples, self.spec.rate)
                levels = np.array([c[0] for c in curve])
                outs = np.array([c[1] for c in curve])
                reduction = levels - outs
                reduction -= np.median(reduction[: max(3, len(reduction) // 6)])
                panel.plot(levels, reduction, color=colour)
                found[side] = measure.curve_measures(curve)
                rows.append([
                    r.name if side == "ours" else "",
                    name,
                    ratio_text(found[side]["ratio"], 2),
                    number(found[side]["reduction at top dB"], 1),
                    number(found[side]["0.5 dB onset dBFS"], 0),
                    number(found[side]["1 dB onset dBFS"], 0),
                ])
            panel.set_title(r.name, loc="left")
            panel.set_xlabel("Input, dBFS")
            if self.first in found:
                ours, theirs = found["ours"], found[self.first]
                apart = measure.curve_distance(
                    measure.static_curve(given, r.ours, self.spec.rate),
                    measure.static_curve(given, r.reference, self.spec.rate),
                )
                summary.append(html.escape(
                    f"{r.name}: ours {ratio_text(ours['ratio'])}, reducing from "
                    f"{number(ours['0.5 dB onset dBFS'], 0)} dBFS; {self.reference_name} "
                    f"{ratio_text(theirs['ratio'])}, from {number(theirs['0.5 dB onset dBFS'], 0)} dBFS; "
                    f"at 0 dBFS, {number(ours['reduction at top dB'], 1)} dB against "
                    f"{number(theirs['reduction at top dB'], 1)} dB; the curves are "
                    f"{number(apart, 2)} dB apart, RMS, from -40 to 0 dBFS."
                ))
        axes[0][0].set_ylabel("Gain reduction, dB")
        figure.tight_layout()
        return {
            "keys": self.keys(),
            "summary": summary,
            "figures": [image(figure)],
            "numbers": table(["Case", "Side", "Ratio", "Reduction at 0 dBFS", "0.5 dB from dBFS", "1 dB from dBFS"], rows, numeric_from=2),
            "numbers note": "Each level's output is read from the second half of its hold, as an RMS level turned back into a sine's peak.",
        }


class Timing(Report):
    """A compressor's attack and release, from a tone burst."""

    reading = (
        "Each panel is one setting and one burst. The curves are the gain from input to output over "
        "time, from the output's envelope over the input's known level, around the burst, whose "
        "start and end the grey lines mark. As the burst starts the gain falls (the attack); after "
        "it ends the gain recovers (the release). The table gives the time to 63% and 90% of each "
        "move, and how far the gain fell by the burst's end. A short burst against a long one at the "
        "same settings shows program dependence: a compressor whose release depends on how long it "
        "was compressing recovers at a different speed after each."
    )

    def build(self):
        names = [r.name for r in self.rendered]
        columns = min(3, len(names))
        count_rows = math.ceil(len(names) / columns)
        figure, axes = plt.subplots(count_rows, columns, figsize=(3.4 * columns, 2.6 * count_rows + 0.3), squeeze=False, sharey=True)
        rows, timings = [], {}
        for index, r in enumerate(self.rendered):
            given = self.inputs[r.name]
            panel = axes[index // columns][index % columns]
            timings[r.name] = {}
            for side, name, colour, samples in self.sides(r):
                times, gain, starts, ends = measure.gain_trace(given, r.given, samples, self.spec.rate)
                begin = max(0, starts - int(0.05 * self.spec.rate))
                stop = min(gain.size, ends + int(0.9 * self.spec.rate))
                shown = slice(begin, stop)
                panel.plot((times[shown] - times[starts]) * 1000.0, gain[shown], color=colour, linewidth=1.1)
                found = measure.burst_times(given, r.given, samples, self.spec.rate)
                timings[r.name][side] = found
                rows.append([
                    r.name if side == "ours" else "",
                    name,
                    f"{given['on'] * 1000:g}",
                    number(found["reduction dB"], 1),
                    number(found["attack 63% ms"], 1),
                    number(found["attack 90% ms"], 1),
                    number(found["release 63% ms"], 0),
                    number(found["release 90% ms"], 0),
                ])
            panel.axvline(0.0, color=MUTED, linewidth=0.7, alpha=0.6)
            panel.axvline(given["on"] * 1000.0, color=MUTED, linewidth=0.7, alpha=0.6)
            panel.set_title(r.name, loc="left")
        for panel in axes[-1]:
            panel.set_xlabel("ms from the burst's start")
        for row in axes:
            row[0].set_ylabel("Gain, dB")
        for extra in range(len(names), count_rows * columns):
            axes[extra // columns][extra % columns].set_visible(False)
        figure.tight_layout()

        summary = []
        for name in names:
            parts = []
            for side, found in timings[name].items():
                who = "ours" if side == "ours" else side
                parts.append(
                    f"{who} attacks in {number(found['attack 63% ms'], 1)} ms and releases in "
                    f"{number(found['release 63% ms'], 0)} ms (to 63%), reducing {number(found['reduction dB'], 1)} dB"
                )
            summary.append(html.escape(f"{name}: " + "; ".join(parts) + "."))
        # Program dependence: the same settings, a short burst against a long one.
        for name in names:
            for other in names:
                same = (
                    self.settings[name] == self.settings[other]
                    and self.parameters[name] == self.parameters[other]
                    and self.inputs[name]["on"] < self.inputs[other]["on"]
                )
                if not same:
                    continue
                parts = []
                for side in timings[name]:
                    short = timings[name][side]["release 63% ms"]
                    long = timings[other][side]["release 63% ms"]
                    who = "ours" if side == "ours" else side
                    parts.append(f"{who} {number(short, 0)} ms against {number(long, 0)} ms")
                summary.append(html.escape(
                    f"Program dependence, {name} against {other} (release to 63% after a "
                    f"{self.inputs[name]['on'] * 1000:g} ms burst and a {self.inputs[other]['on'] * 1000:g} ms one): "
                    + "; ".join(parts) + "."
                ))
        return {
            "keys": self.keys(),
            "summary": summary,
            "figures": [image(figure)],
            "numbers": table(["Case", "Side", "Burst ms", "Reduction dB", "Attack 63% ms", "Attack 90% ms", "Release 63% ms", "Release 90% ms"], rows, numeric_from=2),
            "numbers note": "Times are from the burst's start, for the attack, and its end, for the release, to 63% and 90% of the way to where the gain settles.",
        }


class Distortion(Report):
    """The harmonics an effect adds to a sine at stepped levels."""

    reading = (
        "Each panel is one setting. The solid curves are the total harmonic distortion at each "
        "input level, the 2nd to 9th harmonics' power under the fundamental's, in dB; the dashed "
        "ones are the 2nd harmonic alone, which a VCA's or a transformer's colour shows most. Each "
        "level is read once it's settled, from the second half of its hold. A curve near the "
        "bottom is the arithmetic's own floor, so no colour at all."
    )

    def build(self):
        names = [r.name for r in self.rendered]
        figure, axes = plt.subplots(1, len(names), figsize=(3.4 * len(names), 3.0), squeeze=False, sharey=True)
        rows, summary = [], []
        for index, r in enumerate(self.rendered):
            given = self.inputs[r.name]
            panel = axes[0][index]
            worst = {}
            for side, name, colour, samples in self.sides(r):
                found = measure.step_harmonics(given, samples, self.spec.rate)
                levels = [f["level"] for f in found]
                panel.plot(levels, [f["THD dB"] for f in found], color=colour)
                panel.plot(levels, [f["harmonics dB"][0] for f in found], color=colour, linestyle="--", linewidth=1.0)
                for f in found:
                    rows.append([
                        r.name if side == "ours" and f is found[0] else "",
                        name if f is found[0] else "",
                        number(f["level"], 0),
                        number(f["out dBFS"], 1),
                        number(f["THD dB"], 1),
                        number(f["harmonics dB"][0], 1),
                        number(f["harmonics dB"][1], 1),
                    ])
                worst[side] = max(found, key=lambda f: f["THD dB"])
            panel.set_title(r.name, loc="left")
            panel.set_xlabel("Input, dBFS")
            parts = []
            for side, f in worst.items():
                who = "ours" if side == "ours" else side
                parts.append(
                    f"{who} at most {number(f['THD dB'], 1)} dB at {number(f['level'], 0)} dBFS, "
                    f"its 2nd harmonic {number(f['harmonics dB'][0], 1)} dB and 3rd {number(f['harmonics dB'][1], 1)} dB"
                )
            summary.append(html.escape(f"{r.name}: " + "; ".join(parts) + "."))
        axes[0][0].set_ylabel("Under the fundamental, dB")
        figure.tight_layout()
        keys = self.keys() + [(MUTED, "dashed", "the 2nd harmonic alone")]
        return {
            "keys": keys,
            "summary": summary,
            "figures": [image(figure)],
            "numbers": table(["Case", "Side", "Input dBFS", "Out dBFS", "THD dB", "2nd dB", "3rd dB"], rows, numeric_from=2),
            "numbers note": "Harmonics are in dB under the fundamental, from a Blackman-windowed spectrum of each level's settled half.",
        }


class Knob(Report):
    """One knob swept on every side, measured as the spec's `[report] measure` says:

    - `note`, the default: the whole render's spectral centroid and peak level.
    - `filter`: a held note's filter read from its harmonics over `window`, as `harmonics` does:
      its peak's frequency and height over the bass, and the bass against the knob's bottom.
    - `decay`: how the note's brightness falls, fitted with an exponential, for a decay knob.
    - `accents`: a pattern's notes, a plain one first and then accents: the first accent's
      brightness and level over the plain note's, and how much the last accent builds over the
      first.

    A reference's knob is the layout's `reference_knob`, one name for all of them or a table by
    reference, as Acid V's Env Mod is `envelope_mod` and ABL3's `envmod`.
    """

    # Each measure's columns: (name, axis label, whether it's read in octaves, digits, the unit
    # the consensus compares it in).
    COLUMNS = {
        "note": [
            ("centroid Hz", "Spectral centroid, Hz", True, 0, "Hz"),
            ("peak dBFS", "Peak level, dBFS", False, 1, "dB"),
        ],
        "filter": [
            ("peak Hz", "Filter peak, Hz", True, 0, "Hz"),
            ("peak dB", "Peak over the bass, dB", False, 1, "dB"),
            ("bass dB", "Bass against the knob's bottom, dB", False, 1, "dB"),
        ],
        "decay": [
            ("decay ms", "Brightness decay, ms", True, 0, "ms"),
            ("fall octaves", "Brightness fall, octaves", False, 2, "octaves"),
        ],
        "accents": [
            ("accent octaves", "First accent over the plain note, octaves", False, 2, "octaves"),
            ("accent dB", "First accent over the plain note, dB", False, 1, "dB"),
            ("build octaves", "Last accent over the first, octaves", False, 2, "octaves"),
        ],
    }
    # Measures each side has its own level for, so the consensus compares them against the same
    # side's at the knob's bottom.
    ABSOLUTE = {"peak dBFS"}
    READINGS = {
        "note": (
            "The left panel is the spectral centroid of the whole note, the centre of its "
            "spectrum's weight, a measure of brightness; the right is its peak level."
        ),
        "filter": (
            "Each step holds one note and reads its harmonics, each times its number so a "
            "sawtooth's are level, once the filter's envelope has decayed. The panels are the "
            "highest harmonic's frequency, where it stands a decibel over the bass; how far it "
            "stands over the bass, the 2nd to 4th harmonics; and the bass against the knob's "
            "bottom, so a fall is bass lost to resonance."
        ),
        "decay": (
            "The note's brightness, its spectral centroid every 5 ms, is fitted with an "
            "exponential falling to a resting brightness. The left panel is the fit's time "
            "constant, which follows the filter envelope's decay; the right is how far it falls."
        ),
        "accents": (
            "The pattern plays a plain note and then accents. Each note's brightness is the "
            "centroid over its first 40 ms, and its level the RMS over its first 60 ms. The "
            "panels are the first accent's brightness and level over the plain note's, and how "
            "much brighter the last accent is than the first, which is the accent sweep building "
            "up."
        ),
    }

    @property
    def kind(self) -> str:
        return self.layout.get("measure", "note")

    @property
    def reading(self) -> str:
        return (
            "Each point is one step of the knob, placed by how far it's turned: ours from 00 to "
            "FF, the references' from 0 to 1. "
            + self.READINGS[self.kind]
            + " Where the curves part is where the knobs map differently; the summary gives how "
            "far apart ours and each reference are on average over the knob's steps. The grey "
            "band under them is the references' consensus, where they agree."
        )

    def measured(self, samples, case) -> dict:
        rate, notes = self.spec.rate, self.spec.notes(case)
        if self.kind == "filter":
            start, end = self.layout.get("window", [1.0, 2.0])
            expected = 440.0 * 2.0 ** ((notes[0]["note"] - 69) / 12)
            return measure.filter_shape(samples, rate, start, end, expected)
        if self.kind == "decay":
            return measure.brightness_decay(samples, rate, notes[0]["time"])
        if self.kind == "accents":
            starts = [note["time"] for note in notes]
            marks = measure.note_marks(samples, rate, starts)
            accented = [m for m, note in zip(marks, notes) if note.get("accent")]
            plain = next(m for m, note in zip(marks, notes) if not note.get("accent"))
            first, last = accented[0], accented[-1]
            return {
                "accent octaves": math.log2(first["bright Hz"] / plain["bright Hz"]),
                "accent dB": first["RMS dBFS"] - plain["RMS dBFS"],
                "build octaves": math.log2(last["bright Hz"] / first["bright Hz"]),
            }
        return measure.summary(samples, rate)

    def build(self):
        ours_knob = self.layout["knob"]
        columns = self.COLUMNS[self.kind]
        sides = ["ours"] + self.reference_names
        # Each step: its case, and each side's knob and measures.
        points = []
        for r in self.rendered:
            values = {side: self.measured(samples, r.case) for side, _, _, samples in self.sides(r)}
            knobs = {"ours": self.settings[r.name][ours_knob]}
            for name in self.reference_names:
                knobs[name] = self.reference_parameters[r.name][name].get(self.reference_knob(name))
            points.append((r.name, knobs, values))
        if self.kind == "filter":
            # The bass against the same side's at the knob's bottom.
            for side in sides:
                bottom = points[0][2][side]["bass dB"]
                for _, _, values in points:
                    values[side]["bass dB"] -= bottom

        def x_of(knobs, side):
            return knobs[side] / 255.0 if side == "ours" else knobs[side]

        # The consensus's records, with absolute levels against each side's first step.
        records, by_step = [], {}
        for key, label, _, _, unit in columns:
            for name, _, values in points:
                record = {}
                for side in sides:
                    value = values[side][key]
                    if key in self.ABSOLUTE:
                        value -= points[0][2][side][key]
                    record[side] = value
                shown_label = label.replace(", dBFS", ", dB against the knob's bottom") if key in self.ABSOLUTE else label
                records.append({"measure": shown_label, "case": name, "unit": unit, "values": record})
                by_step[key, name] = records[-1]

        import consensus

        figure, panels = plt.subplots(1, len(columns), figsize=(5.0 * len(columns), 3.2))
        panels = np.atleast_1d(panels)
        for (key, label, octaves, _, unit), panel in zip(columns, panels):
            # The band where the references agree, but not for a level each side sets for itself.
            if len(self.reference_names) > 1 and key not in self.ABSOLUTE:
                targets = []
                for name, knobs, values in points:
                    verdict = consensus.judge(by_step[key, name], self.reference_names)
                    targets.append(verdict["target"] if verdict["status"] == "agree" else float("nan"))
                panel.plot([x_of(p[1], "ours") for p in points], targets, color=CONSENSUS, linewidth=7, alpha=0.6, solid_capstyle="round", zorder=0)
            for side in sides:
                panel.plot(
                    [x_of(p[1], side) for p in points], [p[2][side][key] for p in points],
                    color=self.colour(side), marker="o", markersize=3.5,
                )
            if octaves:
                panel.set_yscale("log")
                panel.yaxis.set_major_locator(LogLocator(base=10, subs=(1.0, 2.0, 3.0, 5.0, 7.0)))
                panel.yaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v / 1000:g}k" if v >= 1000 else f"{v:g}"))
                panel.yaxis.set_minor_formatter(NullFormatter())
            panel.set_ylabel(label)
            panel.set_xlabel("How far the knob is turned")
            panel.set_xlim(-0.02, 1.02)
        figure.tight_layout()

        def shown(value, digits):
            return number(value, digits) if digits else (f"{value:.0f}" if math.isfinite(value) else "–")

        rows = []
        for name, knobs, values in points:
            row = [name]
            for side in sides:
                knob = knobs[side]
                row += [f"0x{knob:02X}" if side == "ours" else (f"{knob:g}" if knob is not None else "–")]
                row += [shown(values[side][key], digits) for key, _, _, digits, _ in columns]
            rows.append(row)

        summary = []
        for key, label, octaves, digits, _ in columns:
            unit = "octaves" if octaves else label.rsplit(", ", 1)[-1]
            ends = []
            for side in sides:
                chosen = [p[2][side][key] for p in points]
                ends.append(f"{'ours' if side == 'ours' else side} from {shown(chosen[0], digits)} to {shown(chosen[-1], digits)}")
            line = f"{label}: {', '.join(ends)}."
            for reference in self.reference_names:
                apart = []
                for _, _, values in points:
                    a, b = values["ours"][key], values[reference][key]
                    if math.isfinite(a) and math.isfinite(b) and (not octaves or (a > 0 and b > 0)):
                        apart.append(abs(math.log2(a / b)) if octaves else abs(a - b))
                if apart:
                    line += (
                        f" Ours and {reference} are {number(float(np.mean(apart)), 2)} {unit} apart on average over "
                        f"{len(apart)} steps, {number(max(apart), 2)} at most."
                    )
            summary.append(html.escape(line))

        names = [label for _, label, _, _, _ in columns]
        headers = ["Case"]
        for side in sides:
            knob = ours_knob if side == "ours" else self.reference_knob(side)
            headers += [f"{self.label(side)} {knob}"] + names
        notes = {
            "note": "The centroid and peak are of the whole render.",
            "filter": "Measured over the held note's harmonics; a dash where no harmonic stands a decibel over the bass.",
            "decay": "From the brightness of one held note.",
            "accents": "From each note's first 40 ms for its brightness and the RMS of its first 60 ms for its level.",
        }
        return {
            "keys": [(c, "marker", w) for c, _, w in self.keys()] + ([(CONSENSUS, "solid", "the references' consensus")] if len(self.reference_names) > 1 else []),
            "summary": summary,
            "figures": [image(figure)],
            "numbers": table(headers, rows, numeric_from=1),
            "numbers note": notes[self.kind],
            "measures": records,
        }


class Velocity(Report):
    """Notes at different velocities in one render, each far enough from the last to start from
    rest, measured one by one: its level over its first 60 ms and its peak over its first 250 ms,
    both against the first note's, and its brightness over its first 40 ms."""

    reading = (
        "Each point is one note, placed by its MIDI velocity. The left panel is its RMS level over "
        "its first 60 ms against the first note's, which says how loud it sounds; the middle its "
        "peak over its first 250 ms against the first note's; the right its brightness, the "
        "spectral centroid over its first 40 ms. A voice whose level follows velocity climbs "
        "across the left panel; one that only accents is flat, then steps up where its accent "
        "starts."
    )
    # How long after a note starts its peak is looked for.
    PEAK_SECONDS = 0.25

    def notes(self, samples) -> list[dict]:
        rate, notes = self.spec.rate, self.spec.data.get("notes", [])
        starts = [note["time"] for note in notes]
        marks = measure.note_marks(samples, rate, starts)
        signal = measure.mono(samples)
        out = []
        for note, mark in zip(notes, marks):
            start = int(note["time"] * rate)
            part = signal[start : start + int(self.PEAK_SECONDS * rate)]
            out.append({
                "velocity": int(note.get("velocity", 100)),
                "RMS dBFS": mark["RMS dBFS"],
                "peak dBFS": measure.db(float(np.abs(part).max(initial=0.0))),
                "bright Hz": mark["bright Hz"],
            })
        for value in out:
            value["RMS dB"] = value["RMS dBFS"] - out[0]["RMS dBFS"]
            value["peak dB"] = value["peak dBFS"] - out[0]["peak dBFS"]
        return out

    def build(self):
        columns = [
            ("RMS dB", "Level over the first note's, dB", False),
            ("peak dB", "Peak over the first note's, dB", False),
            ("bright Hz", "Brightness, Hz", True),
        ]
        rows, summary = [], []
        figure, panels = plt.subplots(1, len(columns), figsize=(5.0 * len(columns), 3.2))
        for r in self.rendered:
            for side, name, colour, samples in self.sides(r):
                found = self.notes(samples)
                velocities = [n["velocity"] for n in found]
                for (key, label, log), panel in zip(columns, panels):
                    panel.plot(velocities, [n[key] for n in found], color=colour, marker="o", markersize=3.5)
                    if log:
                        panel.set_yscale("log")
                        panel.yaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v:g}"))
                        panel.yaxis.set_minor_formatter(NullFormatter())
                    panel.set_ylabel(label)
                    panel.set_xlabel("Velocity")
                    panel.set_xlim(0, 128)
                for n in found:
                    rows.append([
                        name,
                        str(n["velocity"]),
                        number(n["RMS dBFS"], 1),
                        number(n["RMS dB"], 2, sign=True),
                        number(n["peak dBFS"], 1),
                        number(n["peak dB"], 2, sign=True),
                        f"{n['bright Hz']:.0f}",
                    ])
                levels = [n["RMS dB"] for n in found]
                summary.append(html.escape(
                    f"{name}: from velocity {velocities[0]} to {velocities[-1]}, its level runs from "
                    f"{number(min(levels), 2, sign=True)} to {number(max(levels), 2, sign=True)} dB "
                    f"over the first note's."
                ))
        figure.tight_layout()
        return {
            "keys": [(c, "marker", w) for c, _, w in self.keys()],
            "summary": summary,
            "figures": [image(figure)],
            "numbers": table(["Side", "Velocity", "RMS dBFS", "RMS over the first dB", "Peak dBFS", "Peak over the first dB", "Brightness Hz"], rows, numeric_from=1),
            "numbers note": "Each note's RMS is over its first 60 ms, its peak over its first 250 ms and its brightness over its first 40 ms.",
        }


class Taper(Report):
    """A filter's Res knob turned in steps, each step a sweep: how tall its peak stands and where,
    the bass it loses, and the step it starts to self-oscillate from, at each Cut (#333).

    The layout's `knob` is our setting the steps turn, `reference_knob` the reference's, as a list
    when it's named differently for some cases, such as a low-pass's and a high-pass's; `width`
    names the types, by our filter's name, measured by how wide their peak or dip is rather than
    how tall, a band-pass's and a notch's, whose height doesn't follow the knob.
    """

    reading = (
        "Each point is one step of the knob, placed by how far ours is turned, with the reference "
        "set to the step it's paired with. Each step is a sweep, read as the response report reads "
        "it. The top row is the peak: how far it stands over the passband, three octaves under "
        "Cut for a low-pass and over it for a high-pass, the peak being the highest gain within an "
        "octave of Cut, so the first steps, before a peak stands out, give the top of the slope; "
        "or for a band-pass and a notch, how many octaves wide its peak or dip is, 3 dB down. The "
        "table has the peak's own height too. The middle row is where the "
        "peak sits, in cents off Cut, and the bottom the bass loss, two octaves under Cut against "
        "Res 0, or for a high-pass two octaves over. A dashed line marks the first step a side "
        "is ringing by itself near Cut as the sweep ends, two octaves and more past it: from there on it's self-oscillating, so its "
        "response isn't read."
    )

    def knob_of(self, name, side):
        if side == "ours":
            return self.settings[name].get(self.layout.get("knob", "res"))
        for key in as_list(self.layout.get("reference_knob")):
            if key in self.reference_parameters[name][side]:
                return self.reference_parameters[name][side][key]
        return None

    def measured(self, r, samples, nominal, dip_or_band, high) -> dict:
        given = self.inputs[r.name]
        bins, gains = measure.bin_response(r.given, samples, self.spec.rate, given["from"], given["to"])
        f, g = measure.response(bins, gains)
        peak_hz, peak_db = measure.highest(bins, gains, nominal)
        values = {
            "peak Hz": peak_hz,
            "peak dB": peak_db,
            "peak cents": measure.cents(peak_hz, nominal) if math.isfinite(peak_hz) else float("nan"),
            "under dB": measure.gain_at(f, g, nominal / 4),
            # No higher than 16 kHz, where the sweep still reaches at Cut 5 kHz.
            "over dB": measure.gain_at(f, g, min(nominal * 4, TOP_PASSBAND_HZ)),
            # What it passes, three octaves away, under Cut or over it for a high-pass, far enough
            # that the diode's early slope has passed.
            "passband dB": measure.gain_at(f, g, passband_hz(nominal, high)),
            "width octaves": (
                measure.width(bins, gains, nominal, dip=dip_or_band == "Notch") if dip_or_band else float("nan")
            ),
            "rings": measure.rings_on(samples, self.spec.rate, given["seconds"], nominal),
        }
        if values["rings"]:
            for key in ("peak Hz", "peak dB", "peak cents", "width octaves"):
                values[key] = float("nan")
        return values, (bins, gains)

    def build(self):
        figure_keys = as_list(self.layout.get("figure", ["filter", "mode"]))
        column_key = self.layout.get("column", "cut")
        knob = self.layout.get("knob", "res")
        widths = set(as_list(self.layout.get("width")))

        results = {}
        for r in self.rendered:
            settings = self.settings[r.name]
            nominal = cut_hz(settings.get("cut", 255))
            kind = settings.get("filter")
            by = kind if kind in widths else None
            # A high-pass's passband is over Cut.
            high = "HP" in str(kind)
            entry = {"sides": {}, "nominal": nominal}
            for side, _, _, samples in self.sides(r):
                values, ours_bins = self.measured(r, samples, nominal, by, high)
                # How far the peak stands over what the filter passes, which is how much
                # resonance is heard, whether or not the bass is thinned.
                values["rise dB"] = values["peak dB"] - values["passband dB"] if not by else float("nan")
                values["knob"] = self.knob_of(r.name, side)
                entry["sides"][side] = values
                if side == "ours" and r.drawn is not None:
                    drawn_hz, drawn_db = r.drawn
                    heard = np.interp(drawn_hz, *ours_bins)
                    usable = (drawn_db > -40.0) & (drawn_hz >= 30.0) & (drawn_hz <= 16_000.0)
                    near = (drawn_hz >= nominal / 2) & (drawn_hz <= nominal * 2)
                    values["drawn peak dB"] = float(np.max(drawn_db[near])) if near.any() else float("nan")
                    passband = measure.gain_at(drawn_hz, drawn_db, passband_hz(nominal, high))
                    values["drawn rise dB"] = values["drawn peak dB"] - passband if not by else float("nan")
                    values["off drawing dB"] = (
                        float(np.max(np.abs(heard[usable] - drawn_db[usable])))
                        if usable.any() and not values["rings"]
                        else float("nan")
                    )
            results[r.name] = entry
        self.results = results

        groups = defaultdict(list)
        for r in self.rendered:
            groups[tuple(self.settings[r.name].get(k) for k in figure_keys)].append(r.name)

        # The bass loss: against the same type, Cut and side at Res 0.
        for names in groups.values():
            for name in names:
                cut = self.settings[name].get(column_key)
                bottom = next(n for n in names if self.settings[n].get(column_key) == cut and self.settings[n].get(knob) == 0)
                for side, values in results[name]["sides"].items():
                    base = results[bottom]["sides"][side]
                    # A high-pass's bass is what it lets through, over Cut.
                    key = "over dB" if "HP" in str(self.settings[name].get("filter")) else "under dB"
                    passes = math.isfinite(base[key]) and base[key] > -3.0
                    values["bass loss dB"] = values[key] - base[key] if passes and not values["rings"] else float("nan")

        sides = ["ours"] + self.reference_names
        figures, summary, rows_out = [], [], []
        self.gaps = {}
        for key, names in groups.items():
            title = group_label(figure_keys, self.settings[names[0]])
            by_width = self.settings[names[0]].get("filter") in widths
            height = "width octaves" if by_width else "rise dB"
            _, columns = grid_of(names, self.settings, None, column_key)
            figure, axes = plt.subplots(3, len(columns), figsize=(3.6 * len(columns), 7.6), squeeze=False, sharex=True)
            onsets = {side: {} for side in sides}
            gaps, changes = [], []
            for j, cut in enumerate(columns):
                steps = sorted((n for n in names if self.settings[n].get(column_key) == cut), key=lambda n: self.settings[n].get(knob))
                x = np.array([self.settings[n].get(knob) / 255.0 for n in steps])
                for side in sides:
                    colour = self.colour(side)
                    chosen = [results[n]["sides"][side] for n in steps]
                    for row, measure_key in enumerate((height, "peak cents", "bass loss dB")):
                        axes[row][j].plot(x, [v[measure_key] for v in chosen], color=colour, marker="o", markersize=2.5)
                    if side == "ours" and not self.reference_name and not by_width:
                        axes[0][j].plot(x, [v.get("drawn rise dB", float("nan")) for v in chosen], color=colour, linestyle=":")
                    onset = next((xi for xi, v in zip(x, chosen) if v["rings"]), None)
                    onsets[side][cut] = onset
                    if onset is not None:
                        for row in range(3):
                            axes[row][j].axvline(onset, color=colour, linestyle="--", linewidth=0.9)
                if self.reference_name:
                    # Apart, and apart in how far each has come from its own Res 0, which is what
                    # the knob's taper sets, while the filters' shapes set where they start.
                    first = results[steps[0]]["sides"]
                    for n in steps:
                        a, b = results[n]["sides"]["ours"][height], results[n]["sides"][self.first][height]
                        if math.isfinite(a) and math.isfinite(b):
                            gaps.append((abs(a - b), a - b, n))
                            change = (a - first["ours"][height]) - (b - first[self.first][height])
                            if math.isfinite(change):
                                changes.append((abs(change), change, n))
                axes[0][j].set_title(setting_label(column_key, cut), loc="left")
                axes[-1][j].set_xlabel("How far Res is turned")
            axes[0][0].set_ylabel("Peak width, octaves" if by_width else "Peak over the passband, dB")
            axes[1][0].set_ylabel("Peak, cents off Cut")
            axes[2][0].set_ylabel("Bass loss, dB")
            for row in axes:
                for panel in row:
                    panel.set_xlim(-0.02, 1.02)
            figure.tight_layout()
            figures.append(f"<h3>{html.escape(title)}</h3>{image(figure)}")

            def onset_text(side):
                found = [f"{setting_label(column_key, cut)} from {number(100 * at, 0)}%" for cut, at in onsets[side].items() if at is not None]
                return ", ".join(found) if found else "not at all"

            parts = [f"ours self-oscillates {onset_text('ours')}"]
            if self.reference_name:
                for reference in self.reference_names:
                    parts.append(f"{reference} {onset_text(reference)}")
                if gaps:
                    unit = "octaves" if by_width else "dB"
                    mean = float(np.mean([g[0] for g in gaps]))
                    worst = max(gaps)
                    changed = float(np.mean([c[0] for c in changes])) if changes else float("nan")
                    most = max(changes) if changes else (float("nan"), float("nan"), gaps[0][2])
                    self.gaps[title] = {
                        "mean": mean, "worst": worst[1], "worst at": worst[2], "steps": len(gaps),
                        "change mean": changed, "change worst": most[1], "change worst at": most[2],
                    }
                    measured = "width" if by_width else "height over the passband"
                    parts.insert(0, (
                        f"the peak's {measured} is {number(mean, 2)} {unit} apart on average over "
                        f"{len(gaps)} steps, {number(worst[1], 2, True)} at most, at {self.case_label(worst[2])}; "
                        f"how far it has come from Res 0 is {number(changed, 2)} {unit} apart on average, "
                        f"{number(most[1], 2, True)} at most, at {self.case_label(most[2])}"
                    ))
            else:
                strays = [results[n]["sides"]["ours"].get("off drawing dB", float("nan")) for n in names]
                strays = [s for s in strays if math.isfinite(s)]
                if strays:
                    parts.append(f"what's heard strays from the drawing by at most {number(max(strays), 1)} dB before it does")
            summary.append(f"<b>{html.escape(title)}</b>: " + "; ".join(html.escape(p) for p in parts) + ".")

            for n in sorted(names, key=lambda n: (self.settings[n].get(column_key), self.settings[n].get(knob))):
                for side in sides:
                    v = results[n]["sides"][side]
                    knob_value = v["knob"]
                    shown_knob = f"0x{knob_value:02X}" if side == "ours" else f"{knob_value:g}"
                    row = [
                        title if side == "ours" else "",
                        setting_label(column_key, self.settings[n].get(column_key)) if side == "ours" else "",
                        self.label(side),
                        shown_knob,
                        hertz(v["peak Hz"]),
                        number(v["peak dB"], 1, True),
                        number(v["rise dB"], 1, True),
                        number(v["width octaves"], 2),
                        number(v["bass loss dB"], 1, True),
                        "yes" if v["rings"] else "",
                        number(v.get("off drawing dB", float("nan")), 1) if side == "ours" else "",
                    ]
                    rows_out.append(row)
        headers = [
            "Filter", HEADINGS.get(column_key, column_key), "Side", "Res", "Peak", "Peak dB", "Over passband dB", "Width, oct",
            "Bass loss dB", "Rings on", "Off drawing, dB",
        ]
        keys = self.keys()
        if not self.reference_name:
            keys.append((OURS, "dotted", "vimtracker's peak as the Sound page draws it"))
        return {
            "keys": keys,
            "summary": summary,
            "figures": figures,
            "numbers": table(headers, rows_out, numeric_from=3),
            "numbers note": (
                "Peak is the highest gain within an octave of Cut, and over passband its height over "
                "the gain three octaves under Cut, or over it for a high-pass, up to 16 kHz. Width is how wide a band-pass's "
                "peak or a notch's dip is, 3 dB down. A dash where a side is self-oscillating. Off "
                "drawing is the most the measured response strays from the Sound page's drawing, "
                "where the drawing is above −40 dB."
            ),
        }

    def case_label(self, name):
        settings = self.settings[name]
        return ", ".join(setting_label(k, settings.get(k)) for k in (self.layout.get("column", "cut"), self.layout.get("knob", "res")))


def panel_grid(count: int, columns: int, width: float = 3.2, height: float = 2.2, sharey: bool = True):
    """A figure of `count` panels, `columns` to a row, with the spares hidden."""
    columns = max(1, min(columns, count))
    rows = math.ceil(count / columns)
    figure, axes = plt.subplots(rows, columns, figsize=(width * columns, height * rows + 0.3), squeeze=False, sharey=sharey)
    for extra in range(count, rows * columns):
        axes[extra // columns][extra % columns].set_visible(False)
    return figure, [axes[i // columns][i % columns] for i in range(count)]


def midi_hz(note: float) -> float:
    return 440.0 * 2.0 ** ((note - 69.0) / 12.0)


class Notes(Report):
    """A pattern of notes in each case, each note measured as `accents` measures them: its
    brightness, the centroid over its first 40 ms, and its level, the RMS over its first 60 ms,
    both against the case's first note, which is plain. For accents in a row and apart, and for
    knobs' extremes.

    Each case gives the consensus: the first note's brightness, and its level against the first
    case; the first accent's brightness and level over the first note; the last accent's over the
    first accent's, which is the accent sweep building up; and the plain notes after the first,
    over it. The layout's `columns` says how many panels go in a row, 3 if not given.
    """

    reading = (
        "Each panel is one case, and each point one of its notes, in order; a filled point is an "
        "accent and a hollow one a plain note, and a missing step a rest. The top figure is each "
        "note's brightness, the centroid over its first 40 ms, in octaves over the case's first "
        "note; the bottom its level, the RMS over its first 60 ms, in dB over the first note's. "
        "A 303's accents a sixteenth apart build on each other, the accent sweep's capacitor "
        "charging, which shows as each accent brighter than the last."
    )

    def per_note(self, samples, notes) -> list[dict]:
        marks = measure.note_marks(samples, self.spec.rate, [note["time"] for note in notes])
        first = marks[0]
        return [
            {
                "bright Hz": mark["bright Hz"],
                "RMS dBFS": mark["RMS dBFS"],
                "octaves": math.log2(mark["bright Hz"] / first["bright Hz"]),
                "dB": mark["RMS dBFS"] - first["RMS dBFS"],
                "accent": bool(note.get("accent")),
            }
            for mark, note in zip(marks, notes)
        ]

    def build(self):
        sides = ["ours"] + self.reference_names
        found = {}
        for r in self.rendered:
            notes = sorted(self.spec.notes(r.case), key=lambda note: note["time"])
            found[r.name] = (notes, {side: self.per_note(samples, notes) for side, _, _, samples in self.sides(r)})
        names = [r.name for r in self.rendered]
        first_case = names[0]

        records = []

        def add(measure_name, case, unit, values):
            records.append({"measure": measure_name, "case": case, "unit": unit, "values": values})

        for name in names:
            notes, per_side = found[name]
            add("first note's brightness, Hz", name, "Hz", {s: per_side[s][0]["bright Hz"] for s in sides})
            if len(names) > 1:
                add(
                    "first note's level, dB against the first case", name, "dB",
                    {s: per_side[s][0]["RMS dBFS"] - found[first_case][1][s][0]["RMS dBFS"] for s in sides},
                )
            accents = [i for i, note in enumerate(notes) if note.get("accent")]
            plains = [i for i, note in enumerate(notes) if not note.get("accent") and i > 0]
            if accents:
                a = accents[0]
                add("first accent, octaves over the first note", name, "octaves", {s: per_side[s][a]["octaves"] for s in sides})
                add("first accent, dB over the first note", name, "dB", {s: per_side[s][a]["dB"] for s in sides})
            if len(accents) > 1:
                a, b = accents[0], accents[-1]
                add(
                    "last accent over the first, octaves", name, "octaves",
                    {s: math.log2(per_side[s][b]["bright Hz"] / per_side[s][a]["bright Hz"]) for s in sides},
                )
                add(
                    "last accent over the first, dB", name, "dB",
                    {s: per_side[s][b]["RMS dBFS"] - per_side[s][a]["RMS dBFS"] for s in sides},
                )
            if plains:
                add(
                    "later plain notes, octaves over the first note", name, "octaves",
                    {s: float(np.mean([per_side[s][i]["octaves"] for i in plains])) for s in sides},
                )
                add(
                    "later plain notes, dB over the first note", name, "dB",
                    {s: float(np.mean([per_side[s][i]["dB"] for i in plains])) for s in sides},
                )

        columns = int(self.layout.get("columns", 3))
        figures = []
        for key, label in (("octaves", "Brightness over the first note, octaves"), ("dB", "Level over the first note, dB")):
            figure, panels = panel_grid(len(names), columns)
            for name, panel in zip(names, panels):
                notes, per_side = found[name]
                # Each note at its step, so rests show.
                steps = self.steps(notes, self.layout.get("step"))
                for side in sides:
                    values = [n[key] for n in per_side[side]]
                    colour = self.colour(side)
                    panel.plot(steps, values, color=colour, linewidth=1.0)
                    filled = [i for i, n in enumerate(per_side[side]) if n["accent"]]
                    hollow = [i for i, n in enumerate(per_side[side]) if not n["accent"]]
                    panel.plot([steps[i] for i in filled], [values[i] for i in filled], linestyle="none", marker="o", markersize=3.5, color=colour)
                    panel.plot([steps[i] for i in hollow], [values[i] for i in hollow], linestyle="none", marker="o", markersize=3.5, markerfacecolor="white", color=colour)
                panel.set_title(name, loc="left")
                panel.axhline(0.0, color=MUTED, linewidth=0.6)
            for panel in panels[-columns:]:
                panel.set_xlabel("Step")
            panels[0].set_ylabel(label)
            figure.tight_layout()
            figures.append(f"<h3>{html.escape(label.split(',')[0])}</h3>{image(figure)}")

        rows = []
        for name in names:
            notes, per_side = found[name]
            for side in sides:
                rows.append([
                    name if side == "ours" else "",
                    self.label(side),
                    " ".join(("A" if n["accent"] else "·") for n in per_side[side]),
                    f"{per_side[side][0]['bright Hz']:.0f}",
                    " ".join(number(n["octaves"], 2, True) for n in per_side[side][1:]),
                    " ".join(number(n["dB"], 1, True) for n in per_side[side][1:]),
                ])
        import consensus

        summary = []
        for row in consensus.summarize([consensus.judge(r, self.reference_names) for r in records]):
            if "score" not in row:
                continue
            unit = row["gap unit"]
            summary.append(html.escape(
                f"{row['measure']}: ours is {number(row['mean gap'], 2)} {unit} from the target on average, "
                f"{'higher' if row['mean signed gap'] > 0 else 'lower'}, over the {row['agreed']} cases the references agree on."
            ))
        return {
            "keys": [(c, "marker", w) for c, _, w in self.keys()],
            "summary": summary[:8],
            "figures": figures,
            "numbers": table(["Case", "Side", "Notes", "First note's brightness Hz", "Each later note, octaves over the first", "And dB over it"], rows, numeric_from=3),
            "numbers note": "Brightness is the centroid over each note's first 40 ms, from 2 ms in; level the RMS over its first 60 ms.",
            "measures": records,
        }

    @staticmethod
    def steps(notes, step: float | None = None) -> list[float]:
        """Each note's step from the first, where a step is the layout's `step` in seconds, or the
        shortest gap between notes' starts, so a rest leaves a gap."""
        starts = [note["time"] for note in notes]
        gaps = [b - a for a, b in zip(starts, starts[1:]) if b - a > 1e-6]
        step = step or (min(gaps) if gaps else 1.0)
        return [round((start - starts[0]) / step, 3) for start in starts]


class Transition(Report):
    """Two notes in each case, and how the voice goes from the first to the second: whether it
    glides, how long that takes, whether the second note starts its envelopes again, and the dip
    in level between them. For slides, and for legato against retriggered notes at different
    gaps.

    Each case gives the consensus: for a first note held into the second, the time from the
    second note's start until the pitch has gone 90% of the way, and from 10 to 90%; the level's
    dip around the change, against the level just before it; the brightness just after the
    change over just before, which a retrigger raises; and the second note's level over the
    first's. The layout's `columns` says how many panels go in a row, 3 if not given.
    """

    reading = (
        "Each panel is one case, the time in ms from the second note's start. The top figure is "
        "the pitch in semitones from the first note, tracked by YIN over 20 ms frames, each frame "
        "placed at its centre, so a jump shows 10 ms early; the bottom the level, the RMS over "
        "one cycle of the lower note. A slide glides from one pitch to the other with the gate held, so the level doesn't "
        "dip and the envelopes aren't started again; a retriggered note jumps, and its level dips "
        "as the gate closes and opens."
    )
    # Around the change: how long before it the level and brightness are read, in seconds.
    BEFORE = (0.032, 0.002)
    AFTER = (0.002, 0.032)
    # The deepest a dip is read, in dB under the level before it.
    DEEPEST_DIP_DB = -60.0

    def transition(self, samples, notes) -> dict:
        rate = self.spec.rate
        first, second = notes[0], notes[1]
        change = second["time"]
        end = first["time"] + first["length"]
        held = end > change or bool(first.get("slide"))
        # Where the first note was last left alone: its end, or the change if it's held into it.
        last = min(end, change) if not first.get("slide") else change
        low, high = sorted((midi_hz(first["note"]), midi_hz(second["note"])))
        times, hertz = measure.pitch_track(samples, rate, change - 0.08, change + 0.25, low / 1.6, high * 1.6)
        # A median over 7 ms, so a frame that tracks an octave out doesn't count as a crossing.
        semitones = measure.median(12.0 * np.log2(hertz / midi_hz(first["note"])), 7)
        interval = second["note"] - first["note"]
        values = {"times": times - change, "semitones": semitones}
        # The first note's pitch over the 30 ms before it was last left alone, the frames placed
        # at their centres, so none reaches past it.
        before_change = (times > last - 0.04) & (times < last - 0.01)
        values["pitch cents"] = 100.0 * float(np.nanmedian(semitones[before_change])) if before_change.any() else float("nan")
        # The RMS over one cycle of the lower note, which a periodic wave's phase doesn't move.
        env_times, levels = measure.level_envelope(samples, rate, width=1.0 / low)
        values["envelope"] = (env_times - change, levels)
        before = (env_times >= last - 0.015) & (env_times < last - 0.005)
        around = (env_times >= last - 0.005) & (env_times <= change + 0.015)
        level_before = float(np.mean(levels[before])) if before.any() else float("nan")
        # No deeper than the floor, as a note that has died away has nothing more to lose.
        dip = float(np.min(levels[around])) - level_before if around.any() else float("nan")
        values["dip dB"] = max(dip, self.DEEPEST_DIP_DB)
        bright_before = measure.windowed_centroid(samples, rate, last - self.BEFORE[0], last - self.BEFORE[1])
        bright_after = measure.windowed_centroid(samples, rate, change + self.AFTER[0], change + self.AFTER[1])
        values["retrigger octaves"] = math.log2(bright_after / bright_before) if bright_before > 0 and bright_after > 0 else float("nan")
        marks = measure.note_marks(samples, rate, [first["time"], second["time"]])
        values["level dB"] = marks[1]["RMS dBFS"] - marks[0]["RMS dBFS"]
        values["glide 90 ms"] = values["glide 10-90 ms"] = float("nan")
        if held and interval:
            rising = interval > 0
            at = lambda share: measure.crossing(times - change, semitones, -0.02, share * interval, rising)  # noqa: E731
            ten, ninety = at(0.1), at(0.9)
            values["glide 90 ms"] = ninety * 1000.0
            values["glide 10-90 ms"] = (ninety - ten) * 1000.0
        values["held"] = held
        return values

    def build(self):
        sides = ["ours"] + self.reference_names
        names = [r.name for r in self.rendered]
        found = {}
        for r in self.rendered:
            notes = sorted(self.spec.notes(r.case), key=lambda note: note["time"])
            found[r.name] = (notes, {side: self.transition(samples, notes) for side, _, _, samples in self.sides(r)})

        records = []
        measures = [
            ("pitch cents", "the first note's pitch, cents off its note", "cents"),
            ("glide 90 ms", "a held note's glide to 90% of the way, ms", "ms"),
            ("glide 10-90 ms", "a held note's glide from 10 to 90%, ms", "ms"),
            ("dip dB", "the level's dip at the change, dB", "dB"),
            ("retrigger octaves", "brightness just after the change over just before, octaves", "octaves"),
            ("level dB", "second note's level over the first's, dB", "dB"),
        ]
        for name in names:
            _, per_side = found[name]
            for key, label, unit in measures:
                values = {s: per_side[s][key] for s in sides}
                if all(not math.isfinite(v) for v in values.values()):
                    continue
                records.append({"measure": label, "case": name, "unit": unit, "values": values})

        columns = int(self.layout.get("columns", 3))
        figures = []
        for what in ("pitch", "level"):
            figure, panels = panel_grid(len(names), columns, sharey=what == "level")
            for name, panel in zip(names, panels):
                notes, per_side = found[name]
                for side in sides:
                    values = per_side[side]
                    if what == "pitch":
                        panel.plot(values["times"] * 1000.0, values["semitones"], color=self.colour(side), linewidth=1.1)
                    else:
                        t, levels = values["envelope"]
                        shown = (t >= -0.08) & (t <= 0.2)
                        panel.plot(t[shown] * 1000.0, np.maximum(levels[shown], -80.0), color=self.colour(side), linewidth=1.0)
                panel.axvline(0.0, color=MUTED, linewidth=0.6)
                end = (notes[0]["time"] + notes[0]["length"] - notes[1]["time"]) * 1000.0
                if abs(end) > 0.5:
                    panel.axvline(end, color=MUTED, linewidth=0.6, linestyle=":")
                panel.set_title(name, loc="left")
                panel.set_xlim(-80, 200)
            for panel in panels[-columns:]:
                panel.set_xlabel("ms from the second note")
            panels[0].set_ylabel("Semitones from the first note" if what == "pitch" else "Level, dBFS")
            figure.tight_layout()
            heading = "Pitch" if what == "pitch" else "Level"
            figures.append(f"<h3>{heading}</h3>{image(figure)}")

        rows = []
        for name in names:
            _, per_side = found[name]
            for side in sides:
                v = per_side[side]
                rows.append([
                    name if side == "ours" else "",
                    self.label(side),
                    number(v["glide 90 ms"], 1),
                    number(v["glide 10-90 ms"], 1),
                    number(v["dip dB"], 1, True),
                    number(v["retrigger octaves"], 2, True),
                    number(v["level dB"], 1, True),
                ])
        summary = []
        for name in names:
            _, per_side = found[name]
            parts = []
            for side in sides:
                v = per_side[side]
                glide = f"glides to 90% in {number(v['glide 90 ms'], 0)} ms, " if math.isfinite(v["glide 90 ms"]) else ""
                parts.append(f"{'ours' if side == 'ours' else side} {glide}dips {number(v['dip dB'], 1, True)} dB and brightens {number(v['retrigger octaves'], 2, True)} octaves")
            summary.append(html.escape(f"{name}: " + "; ".join(parts) + "."))
        return {
            "keys": self.keys() + [(MUTED, "solid", "the second note's start"), (MUTED, "dotted", "the first note's end")],
            "summary": summary,
            "figures": figures,
            "numbers": table(["Case", "Side", "Glide to 90% ms", "Glide 10 to 90% ms", "Dip dB", "Brightening octaves", "Second over first dB"], rows, numeric_from=2),
            "numbers note": (
                "A glide is timed only where the first note is held into the second. The dip is the "
                "lowest level, the RMS over a cycle, down to 60 dB under, from 5 ms before the first note's end, or the change if it's "
                "held, to 15 ms after the change, against the 10 ms before that; the brightening, "
                "the centroid over the 30 ms after the change over the 30 ms before it."
            ),
            "measures": records,
        }


class Gate(Report):
    """One note in each case, held for a different length: how its level falls while it's held,
    and how quickly it dies once let go, which is the gate's and the VCA's timing.

    Each case gives the consensus: the level just before the note's end, under its first 60 ms;
    and the time from its end until it's 20 and 40 dB under that. The layout's `columns` says how
    many panels go in a row, 3 if not given.
    """

    reading = (
        "Each panel is one case, a note held for the length its name gives. The curves are the "
        "level, the RMS over one cycle of the note, against the time from the note's start; the dotted line is "
        "where it's let go. A 303's VCA holds a note's level while its gate is open, falling "
        "slowly, and lets go within a few milliseconds."
    )

    def released(self, samples, note) -> dict:
        rate = self.spec.rate
        start, end = note["time"], note["time"] + note["length"]
        times, levels = measure.level_envelope(samples, rate, width=1.0 / midi_hz(note["note"]))
        early = (times >= start) & (times < start + 0.06)
        late = (times >= end - 0.01) & (times < end)
        first = float(10.0 * np.log10(np.mean(10.0 ** (levels[early] / 10.0))))
        at_end = float(10.0 * np.log10(np.mean(10.0 ** (levels[late] / 10.0)))) if late.any() else float("nan")
        found = {"envelope": (times - start, levels), "held dB": at_end - first}
        for drop in (20, 40):
            when = measure.crossing(times, levels, end, at_end - drop, rising=False)
            found[f"release {drop} ms"] = (when - end) * 1000.0 if math.isfinite(when) else float("nan")
        return found

    def build(self):
        sides = ["ours"] + self.reference_names
        names = [r.name for r in self.rendered]
        found = {}
        for r in self.rendered:
            note = sorted(self.spec.notes(r.case), key=lambda n: n["time"])[0]
            found[r.name] = (note, {side: self.released(samples, note) for side, _, _, samples in self.sides(r)})
        records = []
        for name in names:
            _, per_side = found[name]
            for key, label, unit in (
                ("held dB", "level at the note's end under its start, dB", "dB"),
                ("release 20 ms", "release to 20 dB under the note's end, ms", "ms"),
                ("release 40 ms", "release to 40 dB under the note's end, ms", "ms"),
            ):
                records.append({"measure": label, "case": name, "unit": unit, "values": {s: per_side[s][key] for s in sides}})
        columns = int(self.layout.get("columns", 3))
        figure, panels = panel_grid(len(names), columns)
        for name, panel in zip(names, panels):
            note, per_side = found[name]
            for side in sides:
                t, levels = per_side[side]["envelope"]
                shown = (t >= -0.01) & (t <= note["length"] + 0.15)
                panel.plot(t[shown] * 1000.0, np.maximum(levels[shown], -90.0), color=self.colour(side), linewidth=1.0)
            panel.axvline(note["length"] * 1000.0, color=MUTED, linewidth=0.6, linestyle=":")
            panel.set_title(name, loc="left")
        for panel in panels[-columns:]:
            panel.set_xlabel("ms from the note's start")
        panels[0].set_ylabel("Level, dBFS")
        figure.tight_layout()
        rows = []
        for name in names:
            _, per_side = found[name]
            for side in sides:
                v = per_side[side]
                rows.append([name if side == "ours" else "", self.label(side), number(v["held dB"], 1, True), number(v["release 20 ms"], 1), number(v["release 40 ms"], 1)])
        summary = []
        for name in names:
            _, per_side = found[name]
            parts = [
                f"{'ours' if side == 'ours' else side} {number(per_side[side]['held dB'], 1, True)} dB by its end, "
                f"then 20 dB down in {number(per_side[side]['release 20 ms'], 1)} ms"
                for side in sides
            ]
            summary.append(html.escape(f"{name}: " + "; ".join(parts) + "."))
        return {
            "keys": self.keys() + [(MUTED, "dotted", "the note's end")],
            "summary": summary,
            "figures": [image(figure)],
            "numbers": table(["Case", "Side", "At its end, dB under its start", "20 dB down, ms", "40 dB down, ms"], rows, numeric_from=2),
            "numbers note": "Levels are the RMS over one cycle of the note, every half a millisecond; the start's over the note's first 60 ms, the end's over its last 10 ms.",
            "measures": records,
        }


class Chirp(Report):
    """Each note's resonant peak over time, the 303's chirp: where the peak is as the filter
    envelope sweeps it down, and how far it stands over the fundamental and over the passband
    under it, from `measure.resonance_track`. Each note is tracked from its start to its end, or
    to the next note's start, at its own pitch on each side: the note's, tuned by the layout's
    `tuning`, cents by side, such as `{ ABL3 = 3.8 }`. The oscillator's pitch is what the
    harmonics sit on; a pitch tracker's reading moves as the sweeping filter's phase moves.

    Each note gives the consensus: the peak's place 20 ms in, where the sweep starts once a
    window of two cycles fits, and at 60 and 150 ms; how far it falls from 20 to 120 ms, in
    octaves; its height over the fundamental at 20, 60 and 150 ms, the first being the ping; its
    height over the passband over the first 200 ms; the level an octave over it, under it, over the
    same time, the filter's skirt past its resonance; and the note's level over its first 20 ms
    over the 40 ms after, which a resonance ringing out at the start raises. Each time is read as
    the median over a window around it, so the height's wavering as the peak crosses the
    harmonics evens out, and a window past a note's end is left out. The layout's `columns` says
    how many panels go in a row, 3 if not given; with one, a case's notes are drawn along one
    line, for a pattern.
    """

    reading = (
        "Each panel is one case, the time in ms from the note's start, or along a pattern in "
        "seconds. The top figure is where the resonant peak is, the loudest harmonic above the "
        "fundamental, each harmonic taken times its number so a sawtooth's are level; the "
        "bottom, how many dB it stands over the fundamental. Each point reads two cycles of the "
        "note, every 2 ms, placed at their centre, so the first is a cycle in. A gap is where "
        "no harmonic stands a dB over the fundamental. The chirp is the peak falling from where "
        "the envelope throws it, standing high over the fundamental as it goes."
    )
    # The times after a note's start its peak is read at, in seconds, each the median of the frames
    # within a window around it: from where the first frame falls, a cycle in, to 30 ms; 40 to
    # 80 ms; and 120 to 180 ms.
    AT = ((0.02, 0.01), (0.06, 0.02), (0.15, 0.03))
    FALL = (0.02, 0.12)
    PASSBAND = (0.02, 0.2)
    PING = (0.0, 0.02, 0.06)

    def tracked(self, samples, notes, side) -> list[dict]:
        rate = self.spec.rate
        tuning = self.layout.get("tuning", {}).get(side, 0.0)
        out = []
        for index, note in enumerate(notes):
            following = notes[index + 1]["time"] if index + 1 < len(notes) else None
            end = note["time"] + note["length"]
            end = min(end, following) if following is not None else end
            f0 = midi_hz(note["note"]) * 2.0 ** (tuning / measure.CENTS)
            track = measure.resonance_track(samples, rate, note["time"], end, f0)
            track["since"] = track["times"] - note["time"]
            track["f0"] = f0
            track["length"] = end - note["time"]
            out.append(track)
        return out

    def at(self, track, key, time) -> float:
        """The median of a track's frames within a window around a time after the note's start."""
        (centre, half) = time if isinstance(time, tuple) else (time, 0.01)
        if centre + half > track["length"]:
            return float("nan")
        near = np.abs(track["since"] - centre) <= half
        values = track[key][near]
        values = values[np.isfinite(values)]
        return float(np.median(values)) if values.size else float("nan")

    def note_measures(self, samples, note, track) -> dict:
        found = {}
        for time in self.AT:
            found[f"Hz {time[0]}"] = self.at(track, "Hz", time)
            found[f"dB {time[0]}"] = self.at(track, "over fundamental", time)
        start, stop = (self.at(track, "Hz", time) for time in self.FALL)
        found["fall"] = math.log2(start / stop) if start > 0 and stop > 0 else float("nan")
        within = (track["since"] >= self.PASSBAND[0]) & (track["since"] <= self.PASSBAND[1])
        passband = track["over passband"][within]
        passband = passband[np.isfinite(passband)]
        found["passband"] = float(np.mean(passband)) if passband.size else float("nan")
        skirt = track["skirt"][within]
        skirt = skirt[np.isfinite(skirt)]
        found["skirt"] = float(np.mean(skirt)) if skirt.size else float("nan")
        first, split, last = (note["time"] + t for t in self.PING)
        signal = measure.mono(samples)
        rate = self.spec.rate
        level = lambda a, b: measure.db(float(np.sqrt(np.mean(signal[int(a * rate) : int(b * rate)] ** 2))))  # noqa: E731
        found["ping"] = level(first, split) - level(split, last) if last - note["time"] <= track["length"] else float("nan")
        return found

    MEASURES = [
        ("Hz 0.02", "the peak's place at 20 ms, Hz", "Hz"),
        ("Hz 0.06", "the peak's place at 60 ms, Hz", "Hz"),
        ("Hz 0.15", "the peak's place at 150 ms, Hz", "Hz"),
        ("fall", "the sweep's fall from 20 to 120 ms, octaves", "octaves"),
        ("dB 0.02", "the peak over the fundamental at 20 ms, dB", "dB"),
        ("dB 0.06", "the peak over the fundamental at 60 ms, dB", "dB"),
        ("dB 0.15", "the peak over the fundamental at 150 ms, dB", "dB"),
        ("passband", "the peak over the passband, mean over 20 to 200 ms, dB", "dB"),
        ("skirt", "an octave over the peak, under it, mean over 20 to 200 ms, dB", "dB"),
        ("ping", "the first 20 ms's level over the next 40 ms', dB", "dB"),
    ]

    def build(self):
        sides = ["ours"] + self.reference_names
        names = [r.name for r in self.rendered]
        found = {}
        for r in self.rendered:
            notes = sorted(self.spec.notes(r.case), key=lambda note: note["time"])
            per_side = {}
            for side, _, _, samples in self.sides(r):
                tracks = self.tracked(samples, notes, side)
                per_side[side] = (tracks, [self.note_measures(samples, n, t) for n, t in zip(notes, tracks)])
            found[r.name] = (notes, per_side)

        records = []
        for name in names:
            notes, per_side = found[name]
            for index, note in enumerate(notes):
                case = name if len(notes) == 1 else f"{name} {index + 1:02d}"
                for key, label, unit in self.MEASURES:
                    values = {side: per_side[side][1][index][key] for side in sides}
                    if all(not math.isfinite(v) for v in values.values()):
                        continue
                    records.append({"measure": label, "case": case, "unit": unit, "values": values})

        columns = int(self.layout.get("columns", 3))
        pattern = columns == 1
        figures = []
        for key, label in (("Hz", "The peak's place, Hz"), ("over fundamental", "The peak over the fundamental, dB")):
            if pattern:
                figure, panels = panel_grid(len(names), 1, width=13.0, height=3.0)
            else:
                figure, panels = panel_grid(len(names), columns, width=2.1, height=1.6)
            for name, panel in zip(names, panels):
                notes, per_side = found[name]
                for side in sides:
                    for track in per_side[side][0]:
                        x = track["times"] if pattern else track["since"] * 1000.0
                        panel.plot(x, track[key], color=self.colour(side), linewidth=1.0 if pattern else 0.9)
                if key == "Hz":
                    panel.set_yscale("log")
                    panel.yaxis.set_major_formatter(FuncFormatter(lambda v, _: f"{v / 1000:g}k" if v >= 1000 else f"{v:g}"))
                if pattern:
                    for note in notes:
                        panel.axvline(note["time"], color=MUTED, linewidth=0.5, linestyle=":" if note.get("accent") is False else "-")
                panel.set_title(name, loc="left", fontsize=7 if not pattern else 9)
            for panel in panels[-columns:]:
                panel.set_xlabel("Seconds" if pattern else "ms from the note's start")
            panels[0].set_ylabel(label)
            figure.tight_layout()
            figures.append(f"<h3>{html.escape(label.split(',')[0])}</h3>{image(figure)}")

        rows = []
        for name in names:
            notes, per_side = found[name]
            for index, note in enumerate(notes):
                for side in sides:
                    m = per_side[side][1][index]
                    rows.append([
                        (name if len(notes) == 1 else f"{name} {index + 1:02d}") if side == "ours" else "",
                        self.label(side),
                        number(per_side[side][0][index]["f0"], 1),
                        *[number(m[f"Hz {t[0]}"], 0) for t in self.AT],
                        number(m["fall"], 2),
                        *[number(m[f"dB {t[0]}"], 1) for t in self.AT],
                        number(m["passband"], 1),
                        number(m["skirt"], 1),
                        number(m["ping"], 1, True),
                    ])
        import consensus

        summary = []
        for row in consensus.summarize([consensus.judge(r, self.reference_names) for r in records]):
            if "score" not in row:
                continue
            unit = row["gap unit"]
            summary.append(html.escape(
                f"{row['measure']}: ours is {number(row['mean gap'], 2)} {unit} from the target on average, "
                f"{'higher' if row['mean signed gap'] > 0 else 'lower'}, over the {row['agreed']} cases the references agree on."
            ))
        notes_key = [(MUTED, "solid", "an accented note's start"), (MUTED, "dotted", "a plain note's start")] if pattern else []
        return {
            "keys": self.keys() + notes_key,
            "summary": summary,
            "figures": figures,
            "numbers": table(
                ["Case", "Side", "Pitch Hz", "Peak at 20 ms, Hz", "60 ms", "150 ms", "Fall 20 to 120 ms, octaves",
                 "Over the fundamental at 20 ms, dB", "60 ms", "150 ms", "Over the passband, dB", "An octave over, dB", "Ping dB"],
                rows,
                numeric_from=2,
            ),
            "numbers note": (
                "The peak's place and height at 20, 60 and 150 ms are the median of the frames from 10 "
                "to 30, 40 to 80 and 120 to 180 ms; the "
                "passband's, the mean from 20 to 200 ms; the ping, the RMS over the note's first 20 ms "
                "against the 40 ms after. A dash is a time past the note's end, or no peak."
            ),
            "measures": records,
        }


KINDS = {
    "response": Response,
    "harmonics": Harmonics,
    "oscillation": Oscillation,
    "static": Static,
    "timing": Timing,
    "distortion": Distortion,
    "knob": Knob,
    "velocity": Velocity,
    "taper": Taper,
    "notes": Notes,
    "transition": Transition,
    "gate": Gate,
    "chirp": Chirp,
}

STYLE = """
:root { color-scheme: light; --ink: #1f1f1e; --muted: #6b6a66; --rule: #e4e3df; --surface: #ffffff; }
* { box-sizing: border-box; }
body { margin: 0; background: var(--surface); color: var(--ink);
  font: 15px/1.55 -apple-system, "Helvetica Neue", Helvetica, Arial, sans-serif; }
main { max-width: 1120px; margin: 0 auto; padding: 32px 16px 64px; }
h1 { font-size: 24px; font-weight: 600; margin: 0 0 4px; }
h2 { font-size: 17px; font-weight: 600; margin: 36px 0 8px; padding-top: 12px; border-top: 1px solid var(--rule); }
h3 { font-size: 14px; font-weight: 600; margin: 24px 0 4px; }
p, li { max-width: 72ch; }
.meta { color: var(--muted); font-size: 13px; margin: 0 0 16px; }
.keys { display: flex; flex-wrap: wrap; gap: 6px 20px; font-size: 13px; }
.key { display: inline-flex; align-items: center; gap: 8px; }
.swatch { display: inline-block; width: 26px; height: 0; }
img { max-width: 100%; height: auto; display: block; }
.scroll { overflow-x: auto; }
table { border-collapse: collapse; font-size: 12.5px; margin: 8px 0; }
th, td { padding: 3px 10px; border-bottom: 1px solid var(--rule); text-align: left; vertical-align: top; }
th { font-weight: 600; color: var(--muted); }
td.n, th.n { text-align: right; font-variant-numeric: tabular-nums; }
details summary { cursor: pointer; color: var(--muted); font-size: 13px; }
code { font: 12.5px ui-monospace, Menlo, monospace; }
.note { color: var(--muted); font-size: 13px; }
"""


def write(spec, rendered, path: Path) -> dict:
    """Write a spec's report page, and give what was built for it, with its consensus's summary
    under `consensus` for a suite's index."""
    import consensus

    layout = spec.report
    kind = layout.get("kind")
    if kind not in KINDS:
        raise SystemExit(f"{spec.path}: [report] needs a `kind`, one of {', '.join(KINDS)}")
    report = KINDS[kind](spec, rendered)
    built = report.build()
    title = layout.get("title") or spec.path.stem
    about = "".join(f"<p>{html.escape(part.strip())}</p>" for part in layout.get("about", "").split("\n\n") if part.strip())
    sections = dict(spec._references())
    reference = (
        ", ".join(f"{html.escape(name)} ({html.escape(sections[name]['plugin'])})" for name in report.reference_names)
        + ", hosted by pedalboard"
        if report.reference_names
        else "none: ours is measured against its own drawing"
    )
    agreed, built["consensus"] = ("", [])
    if built.get("measures") and report.reference_names:
        agreed, built["consensus"] = consensus.section(built["measures"], report.reference_names)
    consensus_html = f"<h2>The consensus</h2>{agreed}" if agreed else ""
    meta = (
        f"Spec <code>{html.escape(spec.path.name)}</code> · {'references' if len(report.reference_names) > 1 else 'reference'}: {reference} · "
        f"{spec.rate} Hz · made {datetime.date.today().isoformat()}"
    )
    page = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(title)}</title>
<style>{STYLE}</style>
</head>
<body>
<main>
<h1>{html.escape(title)}</h1>
<p class="meta">{meta}</p>
<h2>The test</h2>
{about}
<p>{html.escape(describe_input(spec.input(rendered[0].case), spec.data.get("notes")))} Only the
outputs are compared: nothing here looks inside any side.</p>
<details><summary>Settings every case shares, and what each case changes</summary>
{settings_table(spec)}
{cases_table(spec)}
</details>
<h2>How to read it</h2>
{legend_note(built["keys"])}
<p>{html.escape(report.reading)}</p>
<h2>What differs</h2>
{bullets(built["summary"])}
{consensus_html}
<h2>Plots</h2>
{"".join(built["figures"])}
<h2>Numbers</h2>
<p class="note">{html.escape(built.get("numbers note", ""))}</p>
{built["numbers"]}
</main>
</body>
</html>
"""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(page)
    return built
