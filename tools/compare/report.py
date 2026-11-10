"""A comparison's report: one self-contained web page for a spec, with both sides plotted together
by matplotlib, the plots embedded as PNGs, and the measurements in tables.

`run.py <spec> --report page.html` writes it. The spec's `[report]` says what kind of test it is
and how to lay it out:

    [report]
    kind = "response"     # response, harmonics, oscillation, static, timing, distortion or knob
    title = "..."         # the page's heading; the spec's file name if not given
    about = "..."         # what the test is, in a paragraph or two, for a reader without our code
    figure = ["filter"]   # our settings that give each figure, for response and harmonics
    row = "res"           # the setting that gives each row of a figure's panels
    column = "cut"        # and each column
    line = []             # and each line within a panel, with neither side's own (ours alone)
    window = [1.0, 2.0]   # seconds to measure a held note or a ringing filter over
    knob = "acid.cutoff"  # our knob a knob sweep turns, and the reference's
    reference_knob = "cutoff"
    measure = "note"      # what a knob sweep measures: note, filter, decay or accents

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

# Each side's colour, the same on every plot: ours blue and the reference orange, which stay apart
# for colour-blind readers too. Ours alone, its lines take the categorical colours in this order.
OURS = "#2a78d6"
REFERENCE = "#eb6834"
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
    if spec.has_reference:
        for key, value in (spec.data["reference"].get("parameters") or {}).items():
            rows.append(["reference", key, value])
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
        given = ", ".join(f"{k} = {v}" for k, v in (case.get("input") or {}).items())
        rows.append([case["name"], ours, theirs, given] if spec.has_reference else [case["name"], ours, given])
    if not rows:
        return ""
    headers = ["Case", "Ours", "Reference", "Input"] if spec.has_reference else ["Case", "Ours", "Input"]
    if not any(row[-1] for row in rows):
        headers, rows = headers[:-1], [row[:-1] for row in rows]
    return table(headers, rows, numeric_from=len(headers))


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
        self.reference_name = (
            Path(spec.data["reference"]["plugin"]).stem if spec.has_reference else None
        )
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
        self.inputs = {r.name: spec.input(r.case) for r in rendered}

    def sides(self, r):
        """Each side's samples with its name and colour."""
        out = [("ours", "vimtracker", OURS, r.ours)]
        if r.reference is not None:
            out.append(("reference", self.reference_name, REFERENCE, r.reference))
        return out

    def keys(self) -> list[tuple[str, str, str]]:
        entries = [(OURS, "solid", "vimtracker (ours)")]
        if self.reference_name:
            entries.append((REFERENCE, "solid", f"{self.reference_name} (the reference)"))
        return entries


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
                    colour = (OURS if side == "ours" else REFERENCE) if not line_keys else SEQUENTIAL[line_index % len(SEQUENTIAL)]
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
                    ours, theirs = results[name]["sides"]["ours"][2], results[name]["sides"]["reference"][2]
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
                        "vimtracker" if side == "ours" else self.reference_name,
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
                colour = OURS if side == "ours" else REFERENCE
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
                    "vimtracker" if side == "ours" else self.reference_name,
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
                    ours, theirs = measured[name]["ours"], measured[name]["reference"]
                    summary.append(
                        html.escape(
                            f"{setting_label(column_key, column)}, {setting_label(row_key, row)}: ours peaks at "
                            f"{hertz(ours['peak Hz'])} ({number(ours['peak dB'], 1, True)} dB) and keeps "
                            f"{number(ours['bass dB'], 1, True)} dB of its bass; {self.reference_name} peaks at "
                            f"{hertz(theirs['peak Hz'])} ({number(theirs['peak dB'], 1, True)} dB) and keeps "
                            f"{number(theirs['bass dB'], 1, True)} dB."
                        )
                    )
        fundamentals = {side: [measured[n][side]["fundamental"] for n in names] for side in measured[names[0]]}
        notes = "; ".join(
            f"{'vimtracker' if side == 'ours' else self.reference_name} plays it at {np.median(values):.1f} Hz"
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
                    "line": group_label(line_keys, settings) if side == "ours" else self.reference_name,
                    "knob": settings.get("cut") if side == "ours" else self.parameters[r.name].get(knob),
                    "nominal": cut_hz(settings.get("cut", 255)) if side == "ours" else float("nan"),
                    "frequency": frequency,
                    "level": level,
                    "rings": level > RINGING_DB,
                })
        fit_note = ""
        theirs = [p for p in points if p["side"] == "reference" and p["rings"]]
        if self.layout.get("reference_knob_is") == "midi":
            # The reference's knob is a MIDI note number, so it says its own frequency.
            for p in points:
                if p["side"] == "reference":
                    p["nominal"] = 440.0 * 2.0 ** ((p["knob"] - 69.0) / 12.0)
            fit_note = f"{self.reference_name}'s {knob} is a MIDI note number, so it's set to that note's frequency."
        elif len(theirs) >= 2:
            slope, offset = np.polyfit([p["knob"] for p in theirs], [math.log2(p["frequency"]) for p in theirs], 1)
            for p in points:
                if p["side"] == "reference":
                    p["nominal"] = 2.0 ** (offset + slope * p["knob"])
            fit_note = (
                f"{self.reference_name}'s fitted line: {2 ** offset:.1f} Hz at {knob} 0, rising "
                f"{slope:.2f} octaves over its knob's whole turn."
            )
        for p in points:
            p["cents"] = measure.cents(p["frequency"], p["nominal"]) if p["rings"] else float("nan")

        lines = list(dict.fromkeys(p["line"] for p in points))
        figure, (left, right) = plt.subplots(1, 2, figsize=(10.0, 3.2))
        for index, line in enumerate(lines):
            chosen = [p for p in points if p["line"] == line]
            ours_only = not self.reference_name
            colour = CATEGORICAL[index % len(CATEGORICAL)] if ours_only else (OURS if chosen[0]["side"] == "ours" else REFERENCE)
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
            if len(found) == 2:
                ours, theirs = found["ours"], found["reference"]
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
                who = "ours" if side == "ours" else self.reference_name
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
                    who = "ours" if side == "ours" else self.reference_name
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
                who = "ours" if side == "ours" else self.reference_name
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
    """One knob swept on both sides, measured as the spec's `[report] measure` says:

    - `note`, the default: the whole render's spectral centroid and peak level.
    - `filter`: a held note's filter read from its harmonics over `window`, as `harmonics` does:
      its peak's frequency and height over the bass, and the bass against the knob's bottom.
    - `decay`: how the note's brightness falls, fitted with an exponential, for a decay knob.
    - `accents`: a pattern's notes, a plain one first and then accents: the first accent's
      brightness and level over the plain note's, and how much the last accent builds over the
      first.
    """

    # Each measure's columns: (name, axis label, whether it's read in octaves, digits).
    COLUMNS = {
        "note": [
            ("centroid Hz", "Spectral centroid, Hz", True, 0),
            ("peak dBFS", "Peak level, dBFS", False, 1),
        ],
        "filter": [
            ("peak Hz", "Filter peak, Hz", True, 0),
            ("peak dB", "Peak over the bass, dB", False, 1),
            ("bass dB", "Bass against the knob's bottom, dB", False, 1),
        ],
        "decay": [
            ("decay ms", "Brightness decay, ms", True, 0),
            ("fall octaves", "Brightness fall, octaves", False, 2),
        ],
        "accents": [
            ("accent octaves", "First accent over the plain note, octaves", False, 2),
            ("accent dB", "First accent over the plain note, dB", False, 1),
            ("build octaves", "Last accent over the first, octaves", False, 2),
        ],
    }
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
            "FF, the reference's from 0 to 1. "
            + self.READINGS[self.kind]
            + " Where the two curves part is where the knobs map differently; the summary gives "
            "how far apart they are on average over the knob's steps."
        )

    def measured(self, samples) -> dict:
        rate, notes = self.spec.rate, self.spec.data.get("notes", [])
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
        their_knob = self.layout.get("reference_knob")
        columns = self.COLUMNS[self.kind]
        points = []
        for r in self.rendered:
            values = {side: self.measured(samples) for side, _, _, samples in self.sides(r)}
            points.append((r.name, self.settings[r.name][ours_knob], self.parameters[r.name].get(their_knob), values))
        if self.kind == "filter":
            # The bass against the same side's at the knob's bottom.
            for side in ("ours", "reference"):
                chosen = [p[3][side] for p in points if side in p[3]]
                if chosen:
                    bottom = chosen[0]["bass dB"]
                    for values in chosen:
                        values["bass dB"] -= bottom

        figure, panels = plt.subplots(1, len(columns), figsize=(5.0 * len(columns), 3.2))
        for (key, label, octaves, _), panel in zip(columns, panels):
            for side, colour in (("ours", OURS), ("reference", REFERENCE)):
                chosen = [p for p in points if side in p[3]]
                if not chosen:
                    continue
                x = [p[1] / 255.0 if side == "ours" else p[2] for p in chosen]
                panel.plot(x, [p[3][side][key] for p in chosen], color=colour, marker="o", markersize=3.5)
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
        for name, ours_value, their_value, values in points:
            row = [name, f"0x{ours_value:02X}"] + [shown(values["ours"][key], digits) for key, _, _, digits in columns]
            if "reference" in values:
                row += [f"{their_value:g}"] + [shown(values["reference"][key], digits) for key, _, _, digits in columns]
            rows.append(row)

        summary = []
        for key, label, octaves, digits in columns:
            unit = "octaves" if octaves else label.rsplit(", ", 1)[-1]
            ends = []
            for side in ("ours", "reference"):
                chosen = [p[3][side][key] for p in points if side in p[3]]
                if chosen:
                    who = "ours" if side == "ours" else self.reference_name
                    ends.append(f"{who} from {shown(chosen[0], digits)} to {shown(chosen[-1], digits)}")
            line = f"{label}: {', '.join(ends)}."
            if self.reference_name:
                apart = []
                for _, _, _, values in points:
                    a, b = values["ours"][key], values["reference"][key]
                    if math.isfinite(a) and math.isfinite(b) and (not octaves or (a > 0 and b > 0)):
                        apart.append(abs(math.log2(a / b)) if octaves else abs(a - b))
                if apart:
                    line += (
                        f" The two are {number(float(np.mean(apart)), 2)} {unit} apart on average over "
                        f"{len(apart)} steps, {number(max(apart), 2)} at most."
                    )
            summary.append(html.escape(line))

        names = [label for _, label, _, _ in columns]
        headers = ["Case", f"Ours {ours_knob}"] + names
        if self.reference_name:
            headers += [f"{self.reference_name} {their_knob}"] + names
        notes = {
            "note": "The centroid and peak are of the whole render.",
            "filter": "Measured over the held note's harmonics; a dash where no harmonic stands a decibel over the bass.",
            "decay": "From the brightness of one held note.",
            "accents": "From each note's first 40 ms for its brightness and the RMS of its first 60 ms for its level.",
        }
        return {
            "keys": [(c, "marker", w) for c, _, w in self.keys()],
            "summary": summary,
            "figures": [image(figure)],
            "numbers": table(headers, rows, numeric_from=1),
            "numbers note": notes[self.kind],
        }

KINDS = {
    "response": Response,
    "harmonics": Harmonics,
    "oscillation": Oscillation,
    "static": Static,
    "timing": Timing,
    "distortion": Distortion,
    "knob": Knob,
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


def write(spec, rendered, path: Path) -> None:
    layout = spec.report
    kind = layout.get("kind")
    if kind not in KINDS:
        raise SystemExit(f"{spec.path}: [report] needs a `kind`, one of {', '.join(KINDS)}")
    report = KINDS[kind](spec, rendered)
    built = report.build()
    title = layout.get("title") or spec.path.stem
    about = "".join(f"<p>{html.escape(part.strip())}</p>" for part in layout.get("about", "").split("\n\n") if part.strip())
    reference = (
        f"{report.reference_name} ({html.escape(spec.data['reference']['plugin'])}), hosted by pedalboard"
        if report.reference_name
        else "none: ours is measured against its own drawing"
    )
    meta = (
        f"Spec <code>{html.escape(spec.path.name)}</code> · reference: {reference} · "
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
outputs are compared: nothing here looks inside either side.</p>
<details><summary>Settings every case shares, and what each case changes</summary>
{settings_table(spec)}
{cases_table(spec)}
</details>
<h2>How to read it</h2>
{legend_note(built["keys"])}
<p>{html.escape(report.reading)}</p>
<h2>What differs</h2>
{bullets(built["summary"])}
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
