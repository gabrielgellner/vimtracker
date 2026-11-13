"""A take's report: our Acid against Acid V over the whole riff, note by note, as `report.md` with
its plots beside it and every number in `measures.json` (#565).

Acid V's side is Live's own render of the clip if there is one, `acid-v-live.wav`, and otherwise
Acid V hosted here playing the take again, `acid-v.wav`; with both, how close the two are is
given first, as a check on the export and the replay.

Each note is read on both sides at the time the take has it:

- its **level**, the RMS over its first 60 ms, and its **brightness**, the spectral centroid over
  its first 2 to 42 ms, as the `notes` reports read them (`measure.note_marks`);
- its **body**, the centroid over the whole note, which the filter envelope's decay and Env Mod
  move more than its start;
- its **harmonics** over time, the first eight at the note's pitch, every 5 ms through a window of
  two of its cycles, each times its number, so a sawtooth's are level and what's left is the
  filter.

The gaps are ours less Acid V's: brightness in octaves, levels in dB once the take's overall
difference in level is taken out, as the two sides' masters aren't matched. Then they're set
against the knobs each note was played with, Acid V's five as shares of their turn, and against
accent and slide, to say where and with which knobs we drift.
"""

from __future__ import annotations

import json
import math
from pathlib import Path

import numpy as np
from scipy.io import wavfile

import measure
# The harness's colours, and matplotlib as report.py sets it up.
from report import CATEGORICAL, INK, MUTED, OURS, REFERENCE, plt

HARMONICS = 8
HOP = 0.005
NOTE_WINDOW = 0.06
BRIGHT = (0.002, 0.042)
# The longest a note's body is read for, in seconds.
LONGEST_BODY = 0.5
WORST_NOTES = 12


def read(path: Path) -> np.ndarray:
    rate, samples = wavfile.read(path)
    samples = np.asarray(samples, dtype=np.float32)
    samples = samples.T if samples.ndim == 2 else np.vstack([samples, samples])
    return samples


def frequency(midi: float) -> float:
    return 440.0 * 2.0 ** ((midi - 69) / 12)


def harmonic_track(samples: np.ndarray, rate: int, start: float, end: float, f0: float) -> tuple[np.ndarray, np.ndarray]:
    """The first harmonics' levels between two times, in dB, each times its number, every `HOP`
    seconds through a Hann window two cycles long, which puts each harmonic on its neighbours'
    zeros, as `chirp` reads them: (times, levels[time, harmonic])."""
    signal = measure.mono(samples)
    width = max(16, int(2 * rate / f0))
    window = np.hanning(width)
    gain = 2.0 / window.sum()
    numbers = np.arange(1, HARMONICS + 1)
    phases = np.exp(-2j * np.pi * np.outer(numbers * f0 / rate, np.arange(width)))
    times, levels = [], []
    at = start
    while at + width / rate <= end and int(at * rate) + width <= signal.size:
        frame = signal[int(at * rate) : int(at * rate) + width] * window
        amplitude = np.abs(phases @ frame) * gain * numbers
        times.append(at + width / rate / 2)
        levels.append(20 * np.log10(np.maximum(amplitude, 1e-9)))
        at += HOP
    return np.array(times), np.array(levels).reshape(-1, HARMONICS)


def span(take, index: int) -> tuple[float, float]:
    """Where a note sounds, in seconds: from its start to its end and a little release, or the next
    note's start, and no longer than `LONGEST_BODY`."""
    note = take.notes[index]
    start = take.seconds(note.start)
    following = take.seconds(take.notes[index + 1].start) if index + 1 < len(take.notes) else take.duration
    return start, min(following, start + take.seconds(note.duration) + 0.02, start + LONGEST_BODY, take.duration)


def note_measures(take, transpose: int, samples: np.ndarray, rate: int) -> list[dict]:
    out = []
    starts = [take.seconds(note.start) for note in take.notes]
    marks = measure.note_marks(samples, rate, starts, bright=BRIGHT, length=NOTE_WINDOW)
    for index, (note, mark) in enumerate(zip(take.notes, marks)):
        start, end = span(take, index)
        f0 = frequency(note.pitch + transpose)
        times, levels = harmonic_track(samples, rate, start, min(end, start + 0.2), f0)
        out.append({
            "level dB": mark["RMS dBFS"],
            "bright Hz": mark["bright Hz"],
            "body Hz": measure.windowed_centroid(samples, rate, start, end) if end - start > 0.01 else float("nan"),
            "harmonics dB": levels.mean(axis=0).tolist() if levels.size else [float("nan")] * HARMONICS,
        })
    return out


def octaves(ours: float, theirs: float) -> float:
    return float(math.log2(ours / theirs)) if ours > 0 and theirs > 0 else float("nan")


def gaps(take, ours: list[dict], theirs: list[dict]) -> tuple[list[dict], float]:
    """Each note's gaps, ours less theirs, and the overall level difference taken out of them."""
    raw = [a["level dB"] - b["level dB"] for a, b in zip(ours, theirs)]
    offset = float(np.nanmedian(raw)) if raw else 0.0
    rows = []
    for index, (note, a, b) in enumerate(zip(take.notes, ours, theirs)):
        rows.append({
            "note": index,
            "time s": round(take.seconds(note.start), 4),
            "pitch": note.pitch,
            "velocity": note.velocity,
            "accent": note.accent,
            "slid into": index > 0 and take.notes[index - 1].slide,
            "slides": note.slide,
            "knobs": {name: round(value, 3) for name, value in take.knobs_at(note.start).items()},
            "bright octaves": octaves(a["bright Hz"], b["bright Hz"]),
            "body octaves": octaves(a["body Hz"], b["body Hz"]),
            "level dB": raw[index] - offset,
            "harmonics dB": (np.array(a["harmonics dB"]) - np.array(b["harmonics dB"]) - offset).tolist(),
        })
    return rows, offset


def by_knob(rows: list[dict]) -> list[dict]:
    """The gaps' means over the notes in each third of each knob's turn the take used, or at each
    place it was set to if only a few, and for plain, accented and slid-into notes."""
    groups = []
    names = sorted({name for row in rows for name in row["knobs"]})
    for name in names:
        values = np.array([row["knobs"][name] for row in rows])
        if np.ptp(values) < 1e-3:
            continue
        distinct = np.unique(np.round(values, 3))
        if distinct.size <= 3:
            # A knob set to a few places, each one's notes.
            for place in distinct:
                groups.append(summary(f"{name} at {place:.2f}", [row for row, v in zip(rows, values) if round(v, 3) == place]))
            continue
        edges = np.quantile(values, [0, 1 / 3, 2 / 3, 1])
        for third in range(3):
            low, high = edges[third], edges[third + 1]
            inside = [row for row, v in zip(rows, values) if (low <= v <= high if third == 2 else low <= v < high)]
            if inside:
                groups.append(summary(f"{name} {low:.2f} to {high:.2f}", inside))
    for label, test in (("plain", lambda r: not r["accent"]), ("accented", lambda r: r["accent"]),
                        ("slid into", lambda r: r["slid into"])):
        inside = [row for row in rows if test(row)]
        if inside:
            groups.append(summary(label, inside))
    return groups


def summary(label: str, rows: list[dict]) -> dict:
    def mean(key):
        values = np.array([row[key] for row in rows], dtype=float)
        return float(np.nanmean(values)) if np.isfinite(values).any() else float("nan")

    return {"notes": label, "count": len(rows), "bright octaves": mean("bright octaves"), "body octaves": mean("body octaves"),
            "level dB": mean("level dB")}


def accent_slide(take, measures: list[dict], name: str) -> dict:
    """How much louder and brighter a side's accented notes are than its plain ones."""
    accented = [m for note, m in zip(take.notes, measures) if note.accent]
    plain = [m for note, m in zip(take.notes, measures) if not note.accent]
    if not accented or not plain:
        return {"side": name}
    return {
        "side": name,
        "accent louder dB": float(np.median([m["level dB"] for m in accented]) - np.median([m["level dB"] for m in plain])),
        "accent brighter octaves": float(np.median([np.log2(m["bright Hz"]) for m in accented]) - np.median([np.log2(m["bright Hz"]) for m in plain])),
    }


def slide_pitch(take, samples: np.ndarray, rate: int, transpose: int) -> list[float]:
    """For each slid-into note, how much of the way from the last note's pitch it still has to go
    5 to 25 ms in: 1 still at the last note, 0 there, so a glide on its way is in between and a
    note played afresh is at 0. Pitch is looked for only around the two notes, so a resonant
    peak can't pass for it."""
    out = []
    for index, note in enumerate(take.notes[1:], start=1):
        last = take.notes[index - 1]
        if not last.slide or last.pitch == note.pitch:
            continue
        start = take.seconds(note.start)
        low, high = sorted((frequency(last.pitch + transpose), frequency(note.pitch + transpose)))
        times, found = measure.pitch_track(samples, rate, start - 0.02, start + 0.06, low / 1.25, high * 1.25)
        early = found[(times >= start + 0.005) & (times <= start + 0.025)]
        early = early[np.isfinite(early)]
        if not early.size:
            out.append(float("nan"))
            continue
        cents = 1200 * np.log2(np.median(early) / frequency(note.pitch + transpose))
        out.append(float(cents / (100.0 * (last.pitch - note.pitch))))
    return out


def level_apart(ours: np.ndarray, theirs: np.ndarray, rate: int, hop: float = 0.005, within: float = 40.0) -> dict:
    """How far apart the two levels are over time, every `hop` seconds over 10 ms: where both are
    within `within` dB of the loudest, the RMS of their difference once its median is out; and how
    long one plays while the other has fallen under that, as one's release outlasts the other's."""
    _, a = measure.level_envelope(ours, rate, width=0.01, hop=hop)
    _, b = measure.level_envelope(theirs, rate, width=0.01, hop=hop)
    count = min(a.size, b.size)
    a, b = a[:count], b[:count]
    floor = max(a.max(initial=-150.0), b.max(initial=-150.0)) - within
    both = (a > floor) & (b > floor)
    apart = a[both] - b[both]
    apart = apart - np.median(apart) if apart.size else apart
    return {
        "both playing dB": float(np.sqrt(np.mean(apart**2))) if apart.size else float("nan"),
        "ours alone s": float(((a > floor) & ~(b > floor)).sum() * hop),
        "theirs alone s": float(((b > floor) & ~(a > floor)).sum() * hop),
    }


# Plots.


def save(figure, path: Path) -> str:
    figure.savefig(path, bbox_inches="tight")
    plt.close(figure)
    return path.name


def brightness_over_time(samples: np.ndarray, rate: int, hop: float = 0.01, width: float = 0.04):
    times = np.arange(0.0, samples.shape[1] / rate - width, hop)
    return times + width / 2, np.array([measure.windowed_centroid(samples, rate, t, t + width) for t in times])


def overview(take, rows, ours, theirs, rate, folder: Path, reference_name: str) -> str:
    figure, axes = plt.subplots(4, 1, figsize=(11.0, 9.0), sharex=True, gridspec_kw={"height_ratios": [1.2, 1.2, 1.0, 1.0]})
    level, bright, gap, knobs = axes
    for samples, colour, label in ((theirs, REFERENCE, reference_name), (ours, OURS, "ours")):
        t, levels = measure.level_envelope(samples, rate, width=0.01, hop=0.005)
        level.plot(t, levels, color=colour, lw=0.9, label=label)
        t, centroid = brightness_over_time(samples, rate)
        bright.plot(t, centroid, color=colour, lw=0.9, label=label)
    level.set_ylim(-70, None)
    level.set_ylabel("level, dBFS RMS")
    level.legend(loc="upper right", ncols=2)
    bright.set_yscale("log")
    bright.set_ylabel("brightness, Hz")
    times = [row["time s"] for row in rows]
    gap.bar(times, [row["bright octaves"] for row in rows], width=0.03, color=OURS, label="brightness, octaves")
    gap.plot(times, [row["level dB"] / 6.0 for row in rows], "o", ms=3, color=INK, label="level, dB / 6")
    gap.axhline(0.0, color=MUTED, lw=0.6)
    gap.set_ylabel("ours less Acid V")
    gap.legend(loc="upper right", ncols=2)
    beats = np.arange(0.0, take.length, take.step)
    seconds = [take.seconds(b) for b in beats]
    from take import KNOBS

    for index, name in enumerate(KNOBS):
        if take.static(name) is None:
            continue
        knobs.plot(seconds, [take.value_at(name, b) for b in beats], color=CATEGORICAL[index % len(CATEGORICAL)], lw=1.1, label=name)
    for row in rows:
        if row["accent"]:
            knobs.plot(row["time s"], 1.04, "v", ms=4, color=INK)
        if row["slides"]:
            knobs.plot(row["time s"], -0.04, "^", ms=4, color=MUTED)
    knobs.set_ylim(-0.08, 1.08)
    knobs.set_ylabel("Acid V's knobs")
    knobs.set_xlabel("seconds; accents marked over the knobs, slides under")
    knobs.legend(loc="upper left", ncols=5, fontsize=7.5)
    return save(figure, folder / "overview.png")


def spectrograms(ours, theirs, rate, folder: Path, reference_name: str) -> str:
    figure, axes = plt.subplots(3, 1, figsize=(11.0, 8.0), sharex=True)
    size, hop = 2048, 256
    window = np.hanning(size)

    def stft(samples):
        signal = measure.mono(samples)
        frames = np.array([signal[at : at + size] * window for at in range(0, signal.size - size, hop)])
        return 20 * np.log10(np.maximum(np.abs(np.fft.rfft(frames, axis=1)).T * 2 / window.sum(), 1e-7))

    a, b = stft(ours), stft(theirs)
    count = min(a.shape[1], b.shape[1])
    a, b = a[:, :count], b[:, :count]
    bins = np.fft.rfftfreq(size, 1 / rate)
    times = (np.arange(count) * hop + size / 2) / rate
    keep = (bins >= 30) & (bins <= 12_000)
    top = max(a.max(), b.max())
    for axis, data, title in ((axes[0], b, reference_name), (axes[1], a, "ours")):
        mesh = axis.pcolormesh(times, bins[keep], data[keep], shading="auto", cmap="magma", vmin=top - 90, vmax=top)
        axis.set_title(title, loc="left")
    figure.colorbar(mesh, ax=axes[:2], label="dBFS", shrink=0.8)
    difference = np.clip(a - b, -24, 24)
    loud = np.maximum(a, b) > top - 70
    difference = np.where(loud, difference, np.nan)
    mesh = axes[2].pcolormesh(times, bins[keep], difference[keep], shading="auto", cmap="RdBu_r", vmin=-24, vmax=24)
    axes[2].set_title("ours less Acid V, where either is within 70 dB of the loudest", loc="left")
    figure.colorbar(mesh, ax=axes[2], label="dB", shrink=0.8)
    for axis in axes:
        axis.set_yscale("log")
        axis.set_ylabel("Hz")
        axis.grid(False)
    axes[2].set_xlabel("seconds")
    return save(figure, folder / "spectrograms.png")


def harmonic_envelopes(take, rows, ours, theirs, rate, transpose, folder: Path, reference_name: str) -> str:
    """The notes we drift most on in brightness, and the first plain and accented ones, each
    harmonic's level through the note on both sides."""
    chosen = []
    plain = next((row for row in rows if not row["accent"]), None)
    accented = next((row for row in rows if row["accent"]), None)
    worst = sorted(rows, key=lambda row: -abs(row["bright octaves"]) if np.isfinite(row["bright octaves"]) else 0)
    for row in [plain, accented, *worst]:
        if row is not None and row not in chosen:
            chosen.append(row)
        if len(chosen) == 4:
            break
    figure, axes = plt.subplots(1, len(chosen), figsize=(3.2 * len(chosen), 3.2), squeeze=False, sharey=True)
    for axis, row in zip(axes[0], chosen):
        note = take.notes[row["note"]]
        start, end = span(take, row["note"])
        f0 = frequency(note.pitch + transpose)
        for samples, style in ((theirs, "-"), (ours, "--")):
            times, levels = harmonic_track(samples, rate, start, end, f0)
            for number in range(min(5, HARMONICS)):
                axis.plot((times - start) * 1000, levels[:, number] if levels.size else [], style, lw=1.0,
                          color=CATEGORICAL[number], label=f"{number + 1}" if style == "-" else None)
        kind = "accent" if row["accent"] else "plain"
        axis.set_title(f"note {row['note']} at {start:.2f} s, {kind}, {row['bright octaves']:+.2f} oct", loc="left", fontsize=8)
        axis.set_xlabel("ms into the note")
    axes[0][0].set_ylabel("harmonic × its number, dB")
    axes[0][0].set_ylim(-80, None)
    axes[0][0].legend(title=f"harmonic: {reference_name} solid, ours dashed", fontsize=7, title_fontsize=7, ncols=5, loc="lower left")
    return save(figure, folder / "harmonics.png")


def balance(ours, theirs, rate, folder: Path, reference_name: str) -> tuple[str, dict]:
    a, b = measure.passage(ours, rate), measure.passage(theirs, rate)
    figure, axis = plt.subplots(figsize=(7.0, 3.2))
    axis.semilogx(b["bands Hz"], b["shape dB"], color=REFERENCE, label=reference_name)
    axis.semilogx(a["bands Hz"], a["shape dB"], color=OURS, label="ours")
    axis.set_xlim(30, 16_000)
    axis.set_ylim(-60, None)
    axis.set_xlabel("third-octave band, Hz")
    axis.set_ylabel("share of the whole, dB")
    axis.legend()
    return save(figure, folder / "balance.png"), measure.passage_gaps(a, b)


# The page.


def table(headings: list[str], rows: list[list]) -> str:
    def cell(value):
        if isinstance(value, float):
            return "" if not np.isfinite(value) else f"{value:+.2f}"
        return str(value)

    lines = ["| " + " | ".join(headings) + " |", "|" + "---|" * len(headings)]
    lines += ["| " + " | ".join(cell(v) for v in row) + " |" for row in rows]
    return "\n".join(lines)


def write(take, folder: Path) -> Path:
    import take as takes
    import take_rows

    rate = takes.RATE
    transpose = take_rows.transpose_of(take)
    ours = read(folder / "ours.wav")
    live = folder / "acid-v-live.wav"
    replay = read(folder / "acid-v.wav")
    theirs, reference_name = (read(live), "Acid V in Live") if live.exists() else (replay, "Acid V replayed here")
    frames = min(ours.shape[1], theirs.shape[1])
    ours, theirs = ours[:, :frames], theirs[:, :frames]

    our_notes = note_measures(take, transpose, ours, rate)
    their_notes = note_measures(take, transpose, theirs, rate)
    rows, offset = gaps(take, our_notes, their_notes)
    groups = by_knob(rows)
    finite = [row for row in rows if np.isfinite(row["bright octaves"])]
    worst = sorted(finite, key=lambda row: -abs(row["bright octaves"]))[:WORST_NOTES]
    accents = [accent_slide(take, their_notes, reference_name), accent_slide(take, our_notes, "ours")]
    slides = {"theirs": slide_pitch(take, theirs, rate, transpose), "ours": slide_pitch(take, ours, rate, transpose)}
    check = None
    if live.exists():
        replay_notes = note_measures(take, transpose, replay[:, :frames], rate)
        replay_rows, replay_offset = gaps(take, replay_notes, their_notes)
        check = {
            "level offset dB": replay_offset,
            "median bright octaves": float(np.nanmedian([abs(r["bright octaves"]) for r in replay_rows])),
            "median level dB": float(np.nanmedian([abs(r["level dB"]) for r in replay_rows])),
        }

    plots = {
        "overview": overview(take, rows, ours, theirs, rate, folder, reference_name),
        "spectrograms": spectrograms(ours, theirs, rate, folder, reference_name),
        "harmonics": harmonic_envelopes(take, rows, ours, theirs, rate, transpose, folder, reference_name),
    }
    plots["balance"], passage = balance(ours, theirs, rate, folder, reference_name)
    apart = level_apart(ours, theirs, rate)
    harmonic_gaps = np.nanmean(np.array([row["harmonics dB"] for row in rows]), axis=0)

    measures = {
        "take": take.name,
        "reference": reference_name,
        "transpose": transpose,
        "level offset dB": offset,
        "notes": rows,
        "by knob": groups,
        "accents": accents,
        "slides still to go": slides,
        "harmonics dB": harmonic_gaps.tolist(),
        "passage": passage,
        "level over time": apart,
        "live against replay": check,
        "midi": {"accent from": take.accent_from, "slide from seconds": take.slide_from},
    }
    (folder / "measures.json").write_text(json.dumps(measures, indent=1, default=float))

    accents_count = sum(note.accent for note in take.notes)
    slides_count = sum(note.slide for note in take.notes)
    bright = np.array([row["bright octaves"] for row in rows])
    level = np.array([row["level dB"] for row in rows])
    lines = [
        f"# Take `{take.name}`: our Acid against {reference_name}",
        "",
        f"{len(take.notes)} notes over {take.length:g} beats at {take.tempo:g} BPM, {take.duration:.2f} s, "
        f"{accents_count} accented and {slides_count} sliding into the next"
        + (" — a take written by `take.py synthetic`, not played in Live." if take.data.get("synthetic") else "."),
        f"Moved: {', '.join(take.moved()) or 'no knob'}. Our notes are the take's moved {transpose:+d} semitones, as Acid V sounds an octave under the note it's sent.",
        f"Accents from velocity {take.accent_from} and slides from an overlap of more than {take.slide_from * 1000:g} ms, as `take.py probe` found Acid V takes Live's MIDI.",
        "",
        "Files: `ours.wav` and " + ("`acid-v-live.wav`, with `acid-v.wav` the replay" if live.exists() else "`acid-v.wav`")
        + " to A/B by ear; our song is `ours/`, which `vimtracker` opens.",
        "",
    ]
    flags = take.flags()
    if flags:
        lines += ["**Not followed:**", "", *(f"- {flag}" for flag in flags), ""]
    if check:
        lines += [
            "## Live against the replay",
            "",
            f"Acid V replayed here from the export, against Live's own render: {check['median bright octaves']:.3f} octaves and "
            f"{check['median level dB']:.2f} dB apart on the median note, {check['level offset dB']:+.2f} dB overall. "
            "Close means the export caught what Live played.",
            "",
        ]
    lines += [
        "## Over the take",
        "",
        f"Ours is {offset:+.2f} dB against {reference_name} on the median note, taken out of the levels below. "
        f"Note by note, our brightness is {np.nanmedian(bright):+.2f} octaves off on the median note, "
        f"{np.nanmedian(np.abs(bright)):.2f} either way, and the level {np.nanmedian(np.abs(level)):.2f} dB either way.",
        f"The whole take's spectrum, as a shape: {passage['shape dB']:.2f} dB RMS apart, widest at {passage['worst Hz']:.0f} Hz "
        f"({passage['worst dB']:+.1f} dB). Over time, where both play, the levels are {apart['both playing dB']:.2f} dB RMS apart "
        f"once the overall difference is out; {reference_name} plays on alone for {apart['theirs alone s']:.2f} s, and ours "
        f"for {apart['ours alone s']:.2f} s, where the other has fallen 40 dB under the loudest, as one's release outlasts "
        "the other's.",
        "",
        f"![The take over time]({plots['overview']})",
        "",
        f"![Spectrograms]({plots['spectrograms']})",
        "",
        f"![Long-term balance]({plots['balance']})",
        "",
        "## Accent and slide, by output",
        "",
        table(["side", "accented notes louder, dB", "brighter, octaves"],
              [[a["side"], a.get("accent louder dB", float("nan")), a.get("accent brighter octaves", float("nan"))] for a in accents]),
        "",
        "How much of the way each slid-into note still has to go 5 to 25 ms in, from the last note's pitch, 1, to its "
        "own, 0: a glide on its way is in between, and a note played afresh at 0.",
        "",
        table(["side", *[str(i + 1) for i in range(len(slides["theirs"]))]],
              [[reference_name, *slides["theirs"]], ["ours", *slides["ours"]]]) if slides["theirs"] else "No note was slid into.",
        "",
        "## Where we drift, by knob",
        "",
        "Each knob's turn over the take in thirds of its notes, and the gaps' means over them:",
        "",
        table(["notes", "count", "brightness, octaves", "body, octaves", "level, dB"],
              [[g["notes"], g["count"], g["bright octaves"], g["body octaves"], g["level dB"]] for g in groups]),
        "",
        f"## The {len(worst)} notes we drift most on",
        "",
        table(["note", "time, s", "MIDI", "vel", "accent", "slid into", "brightness, oct", "body, oct", "level, dB", "knobs"],
              [[r["note"], f"{r['time s']:.3f}", r["pitch"], f"{r['velocity']:g}", "A" if r["accent"] else "", "S" if r["slid into"] else "",
                r["bright octaves"], r["body octaves"], r["level dB"],
                ", ".join(f"{k} {v:.2f}" for k, v in r["knobs"].items())] for r in worst]),
        "",
        "## Harmonics",
        "",
        "Each harmonic's level over a note's first 200 ms, times its number, ours less Acid V's, averaged over the notes "
        "with the overall level difference out:",
        "",
        table(["harmonic", *[str(n + 1) for n in range(HARMONICS)]], [["dB", *[float(v) for v in harmonic_gaps]]]),
        "",
        f"![Harmonic envelopes]({plots['harmonics']})",
        "",
    ]
    page = folder / "report.md"
    page.write_text("\n".join(lines))
    return page

