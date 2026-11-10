"""Measurements of a rendered side, ours or the reference's, from its samples alone.

Every function takes stereo samples, shape (2, frames), and measures their mono mix. Nothing here
knows which side made them, so both are measured the same way.
"""

from __future__ import annotations

import math

import numpy as np
from scipy.signal import hilbert

# Where a level of nothing is shown from.
FLOOR_DB = -150.0
# How finely a sweep's response is read, in points to an octave, from 20 Hz to 20 kHz.
RESPONSE_POINTS_PER_OCTAVE = 48
# The cents in an octave.
CENTS = 1200.0


def db(value: float) -> float:
    return 20.0 * math.log10(max(value, 10.0 ** (FLOOR_DB / 20.0)))


def mono(samples: np.ndarray) -> np.ndarray:
    return samples.mean(axis=0).astype(np.float64)


def cents(frequency: float, from_frequency: float) -> float:
    return CENTS * math.log2(frequency / from_frequency)


def centroid(samples: np.ndarray, rate: int) -> float:
    """The spectral centroid of the whole render, in Hz, weighted by magnitude."""
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


def bin_response(given: np.ndarray, out: np.ndarray, rate: int, low: float, high: float):
    """The gain from a sweep's input to the output in each bin of their spectra, in dB, as (Hz, dB)
    arrays over the sweep's range. The bins are a fraction of a hertz apart, so even a sharp
    resonance's peak is read whole."""
    length = max(given.shape[1], out.shape[1])
    power_in = np.abs(np.fft.rfft(mono(given), length)) ** 2
    power_out = np.abs(np.fft.rfft(mono(out), length)) ** 2
    bins = np.fft.rfftfreq(length, 1.0 / rate)
    inside = (bins >= low) & (bins <= high) & (power_in > 0)
    gains = 10.0 * np.log10(np.maximum(power_out[inside], 1e-30) / power_in[inside])
    return bins[inside], gains


def response(bins: np.ndarray, gains: np.ndarray, points_per_octave=RESPONSE_POINTS_PER_OCTAVE):
    """A bin response read at log-spaced frequencies, each the mean power gain of the bins within
    a point's width of it, or the nearest bin where the bins are wider than that, for plotting."""
    low, high = bins[0], bins[-1]
    count = int(math.floor(math.log2(high / low) * points_per_octave)) + 1
    frequencies = low * 2.0 ** (np.arange(count) / points_per_octave)
    half = 2.0 ** (0.5 / points_per_octave)
    power = 10.0 ** (gains / 10.0)
    starts = np.searchsorted(bins, frequencies / half)
    ends = np.searchsorted(bins, frequencies * half)
    nearest = np.clip(np.searchsorted(bins, frequencies), 0, bins.size - 1)
    out = np.empty(count)
    for index in range(count):
        if ends[index] > starts[index]:
            out[index] = power[starts[index] : ends[index]].mean()
        else:
            out[index] = power[nearest[index]]
    return frequencies, 10.0 * np.log10(np.maximum(out, 1e-30))


def gain_at(frequencies: np.ndarray, gains: np.ndarray, hertz: float) -> float:
    """A response's gain at a frequency, read between its points on a log scale."""
    if not frequencies[0] <= hertz <= frequencies[-1]:
        return float("nan")
    return float(np.interp(math.log2(hertz), np.log2(frequencies), gains))


def peak(bins: np.ndarray, gains: np.ndarray, around: float, octaves: float = 2.0):
    """The highest bin within so many octaves of a frequency, as (Hz, dB), or None when the
    highest is at either end, so the response has no peak there, only a slope."""
    inside = np.where((bins >= around * 2.0**-octaves) & (bins <= around * 2.0**octaves))[0]
    if inside.size < 3:
        return None
    at = inside[np.argmax(gains[inside])]
    if at in (inside[0], inside[-1]):
        return None
    # A parabola through the top bin and its neighbours, for a peak between them.
    y0, y1, y2 = gains[at - 1], gains[at], gains[at + 1]
    bend = y0 - 2.0 * y1 + y2
    shift = 0.5 * (y0 - y2) / bend if bend < 0 else 0.0
    step = bins[at + 1] - bins[at]
    return float(bins[at] + shift * step), float(y1 - 0.25 * (y0 - y2) * shift)


def harmonics(samples: np.ndarray, rate: int, start: float, end: float, expected: float):
    """A held note's harmonics between two times, as (Hz, dB) for each up to 16 kHz, with each
    harmonic's level times its number, so an ideal sawtooth's are level and what's left is the
    filter's shape, and the fundamental found near the frequency expected."""
    part = mono(samples)[int(start * rate) : int(end * rate)]
    window = np.hanning(part.size)
    padding = 4
    spectrum = np.abs(np.fft.rfft(part * window, padding * part.size))
    bins = np.fft.rfftfreq(padding * part.size, 1.0 / rate)
    near = (bins > expected * 2.0 ** (-3 / 12)) & (bins < expected * 2.0 ** (3 / 12))
    fundamental = float(bins[near][np.argmax(spectrum[near])])
    found = []
    number = 1
    while number * fundamental < min(16_000.0, rate * 0.45):
        centre = number * fundamental
        around = np.abs(bins - centre) < fundamental * 0.25
        level = float(spectrum[around].max()) * 2.0 / window.sum()
        found.append((centre, db(level * number)))
        number += 1
    return fundamental, np.array(found)


def pitch(samples: np.ndarray, rate: int, start: float, end: float):
    """The strongest frequency between two times, and its level, as (Hz, dBFS peak), from a
    windowed spectrum with the top bin's neighbours fitted by a parabola, which is good to well
    under a cent here."""
    part = mono(samples)[int(start * rate) : int(end * rate)]
    if part.size == 0:
        return float("nan"), FLOOR_DB
    window = np.hanning(part.size)
    padding = 8
    spectrum = np.abs(np.fft.rfft(part * window, padding * part.size))
    bins = np.fft.rfftfreq(padding * part.size, 1.0 / rate)
    spectrum[bins < 10.0] = 0.0
    at = int(np.argmax(spectrum))
    if 0 < at < spectrum.size - 1:
        y0, y1, y2 = (math.log(max(v, 1e-30)) for v in spectrum[at - 1 : at + 2])
        bend = y0 - 2.0 * y1 + y2
        shift = 0.5 * (y0 - y2) / bend if bend < 0 else 0.0
    else:
        shift = 0.0
    frequency = float((at + shift) * rate / (padding * part.size))
    return frequency, db(float(np.abs(part).max(initial=0.0)))


def static_curve(given: dict, out: np.ndarray, rate: int):
    """The output's level at each input level, as a sine's peak level from the RMS over the last
    half of each hold, so a compressor has settled."""
    held = int(math.floor(given["hold"] * rate + 0.5))
    step = given.get("step", 1.0)
    count = int(np.floor((given["to"] - given["from"]) / step + 1e-9)) + 1
    curve = []
    signal = mono(out)
    for index in range(count):
        part = signal[index * held + held // 2 : (index + 1) * held]
        rms = float(np.sqrt(np.mean(part**2))) if part.size else 0.0
        curve.append((given["from"] + step * index, db(rms * np.sqrt(2.0))))
    return curve


def curve_distance(ours, theirs, low: float = -40.0) -> float:
    """How far apart two static curves are, as the RMS of their outputs' differences in dB over the
    input levels from `low` up, the measure #186 asks to be under 0.5 dB."""
    apart = [a - b for (level, a), (_, b) in zip(ours, theirs) if level >= low]
    return float(np.sqrt(np.mean(np.square(apart)))) if apart else float("nan")


def step_harmonics(given: dict, out: np.ndarray, rate: int, highest: int = 9):
    """A stepped sine's harmonics at each input level, from the last half of each hold, so a
    compressor has settled: for each level, the output's fundamental in dBFS, its total harmonic
    distortion in dB under the fundamental, and each harmonic from the 2nd to the `highest` in dB
    under it, read from a Blackman-windowed spectrum."""
    held = int(math.floor(given["hold"] * rate + 0.5))
    step = given.get("step", 1.0)
    count = int(np.floor((given["to"] - given["from"]) / step + 1e-9)) + 1
    frequency = given["frequency"]
    signal = mono(out)
    found = []
    for index in range(count):
        part = signal[index * held + held // 2 : (index + 1) * held]
        window = np.blackman(part.size)
        spectrum = np.abs(np.fft.rfft(part * window)) * 2.0 / window.sum()
        bins = np.fft.rfftfreq(part.size, 1.0 / rate)
        levels = []
        for number in range(1, highest + 1):
            near = np.abs(bins - number * frequency) <= max(3.0, 2.0 * rate / part.size)
            levels.append(float(spectrum[near].max()) if near.any() and number * frequency < rate / 2 else 0.0)
        fundamental = max(levels[0], 1e-30)
        rest = np.sqrt(sum(level * level for level in levels[1:]))
        found.append({
            "level": given["from"] + step * index,
            "out dBFS": db(fundamental),
            "THD dB": db(rest / fundamental),
            "harmonics dB": [db(level / fundamental) for level in levels[1:]],
        })
    return found


def curve_measures(curve) -> dict:
    """What a static curve shows: the ratio over the top 10 dB of input, the gain reduction at
    the top, and the input levels where the reduction first reaches 0.5 and 1 dB, which is where
    the knee starts to bend."""
    levels = np.array([level for level, _ in curve])
    outs = np.array([out for _, out in curve])
    reduction = levels - outs
    # The output at the bottom, where nothing is reduced, sets the gain to measure from.
    reduction -= np.median(reduction[: max(3, len(reduction) // 6)])
    top = levels >= levels.max() - 10.0
    slope = np.polyfit(levels[top], outs[top], 1)[0]
    ratio = 1.0 / slope if slope > 1e-6 else float("inf")

    def onset(amount):
        over = np.where(reduction >= amount)[0]
        return float(levels[over[0]]) if over.size else float("nan")

    return {
        "ratio": ratio,
        "reduction at top dB": float(reduction[-1]),
        "0.5 dB onset dBFS": onset(0.5),
        "1 dB onset dBFS": onset(1.0),
    }


def envelope(samples: np.ndarray, rate: int, smoothing: float = 0.0005) -> np.ndarray:
    """A signal's envelope: its analytic signal's magnitude, smoothed over half a millisecond."""
    width = max(1, int(smoothing * rate))
    return np.convolve(np.abs(hilbert(mono(samples))), np.ones(width) / width, mode="same")


def gain_trace(given: dict, signal: np.ndarray, out: np.ndarray, rate: int):
    """The gain from a burst's input to the output over time, in dB, each sample: the output's
    envelope over the input's, both found the same way, so their blur at the burst's edges cancels
    and what's left is the compressor's own movement."""
    frames = min(signal.shape[1], out.shape[1])
    starts = int(math.floor(given["before"] * rate + 0.5))
    ends = starts + int(math.floor(given["on"] * rate + 0.5))
    ratio = envelope(out[:, :frames], rate) / np.maximum(envelope(signal[:, :frames], rate), 1e-12)
    gain = 20.0 * np.log10(np.maximum(ratio, 1e-12))
    return np.arange(frames) / rate, gain, starts, ends


def burst_times(given: dict, signal: np.ndarray, out: np.ndarray, rate: int) -> dict:
    """Attack and release from a burst: how long the gain takes to move 63% and 90% of the way
    from where it was to where it settles, after the burst starts and after it ends, and how much
    it reduced at the burst's end. A burst too short to settle releases from where it got to."""
    times, gain, starts, ends = gain_trace(given, signal, out, rate)
    span = lambda a, b: gain[max(0, a) : max(a + 1, b)]  # noqa: E731
    settle = int(0.02 * rate)
    before = float(np.median(span(starts - int(0.1 * rate), starts)))
    during = float(np.median(span(ends - min(settle, (ends - starts) // 4), ends)))
    after = float(np.median(gain[-int(0.1 * rate) :]))

    def reach(begin, stop, start_gain, end_gain, share):
        if abs(end_gain - start_gain) < 0.3:
            return float("nan")
        target = start_gain + share * (end_gain - start_gain)
        part = gain[begin:stop]
        crossed = np.where((part - target) * np.sign(end_gain - start_gain) >= 0)[0]
        return float(crossed[0] / rate * 1000.0) if crossed.size else float("nan")

    return {
        "reduction dB": before - during,
        "attack 63% ms": reach(starts, ends, before, during, 0.63),
        "attack 90% ms": reach(starts, ends, before, during, 0.90),
        "release 63% ms": reach(ends, gain.size, during, after, 0.63),
        "release 90% ms": reach(ends, gain.size, during, after, 0.90),
    }


def windowed_centroid(samples: np.ndarray, rate: int, start: float, end: float) -> float:
    """The spectral centroid between two times, in Hz, through a Hann window, so the cut at either
    end doesn't spread energy up the spectrum and brighten it."""
    part = mono(samples)[int(start * rate) : int(end * rate)]
    if part.size < 2:
        return float("nan")
    magnitude = np.abs(np.fft.rfft(part * np.hanning(part.size)))
    frequencies = np.fft.rfftfreq(part.size, 1.0 / rate)
    total = magnitude.sum()
    return float((magnitude * frequencies).sum() / total) if total > 0 else float("nan")


def filter_shape(samples: np.ndarray, rate: int, start: float, end: float, expected: float) -> dict:
    """A voice's filter read from a held sawtooth's harmonics between two times: the highest
    harmonic's frequency and how far it stands over the bass, when it stands a decibel over it,
    and the bass itself, the mean of the 2nd to 4th harmonics, each times its number, in dB."""
    _, levels = harmonics(samples, rate, start, end, expected)
    bass = float(np.mean(levels[1:4, 1]))
    top = 1 + int(np.argmax(levels[1:, 1]))
    height = float(levels[top, 1]) - bass
    is_peak = height > 1.0
    return {
        "peak Hz": float(levels[top, 0]) if is_peak else float("nan"),
        "peak dB": height if is_peak else float("nan"),
        "bass dB": bass,
    }


def brightness_decay(samples: np.ndarray, rate: int, onset: float) -> dict:
    """How a note's brightness falls after it starts: its spectral centroid in octaves, read every
    5 ms over 30 ms windows, fitted with an exponential that falls to a resting brightness. Gives
    the fit's time constant, which follows the filter envelope's decay, and how far it falls, in
    octaves of centroid."""
    from scipy.optimize import curve_fit

    signal = mono(samples)
    width, hop = int(0.03 * rate), int(0.005 * rate)
    window = np.hanning(width)
    frequencies = np.fft.rfftfreq(width, 1.0 / rate)
    times, octaves, levels = [], [], []
    for start in range(0, signal.size - width, hop):
        part = signal[start : start + width] * window
        magnitude = np.abs(np.fft.rfft(part))
        total = magnitude.sum()
        times.append((start + width / 2) / rate)
        octaves.append(math.log2(max((magnitude * frequencies).sum() / total, 1.0)) if total > 0 else 0.0)
        levels.append(float(np.sqrt(np.mean(part**2))))
    times, octaves, levels = np.array(times), np.array(octaves), np.array(levels)
    keep = (times > onset + 0.02) & (levels > levels.max() * 1e-3)
    t, c = times[keep] - onset, octaves[keep]
    if t.size < 4:
        return {"decay ms": float("nan"), "fall octaves": float("nan")}

    def model(t, rest, fall, tau):
        return rest + fall * np.exp(-t / tau)

    try:
        (rest, fall, tau), _ = curve_fit(
            model, t, c, p0=(c[-1], c[0] - c[-1], 0.3), bounds=([0.0, 0.0, 0.005], [20.0, 10.0, 20.0])
        )
    except RuntimeError:
        return {"decay ms": float("nan"), "fall octaves": float("nan")}
    return {"decay ms": float(tau * 1000.0), "fall octaves": float(fall)}


def note_marks(samples: np.ndarray, rate: int, starts: list[float], bright=(0.002, 0.042), length=0.06):
    """Each note's brightness, the windowed centroid over its first few tens of milliseconds, and
    its RMS level in dBFS over `length` seconds from its start, which says how loud it sounds
    better than its peak, as a resonant note's first few cycles can peak far over the rest."""
    signal = mono(samples)
    marks = []
    for start in starts:
        part = signal[int(start * rate) : int((start + length) * rate)]
        marks.append({
            "bright Hz": windowed_centroid(samples, rate, start + bright[0], start + bright[1]),
            "RMS dBFS": db(float(np.sqrt(np.mean(part.astype(np.float64) ** 2))) if part.size else 0.0),
        })
    return marks


def highest(bins: np.ndarray, gains: np.ndarray, around: float, octaves: float = 1.0):
    """The highest bin within so many octaves of a frequency, as (Hz, dB), wherever it is, so a
    response with no peak yet gives the top of its slope, and a knob's first steps are read the
    same way as its last."""
    inside = np.where((bins >= around * 2.0**-octaves) & (bins <= around * 2.0**octaves))[0]
    if inside.size == 0:
        return float("nan"), float("nan")
    at = inside[np.argmax(gains[inside])]
    return float(bins[at]), float(gains[at])


def width(bins: np.ndarray, gains: np.ndarray, around: float, dip: bool, by: float = 3.0) -> float:
    """How wide a band-pass's peak is, in octaves, where it's `by` dB under its top, or a notch's
    dip, where it's `by` dB under 0 dB, around the highest or lowest bin within an octave of a
    frequency."""
    inside = np.where((bins >= around / 2.0) & (bins <= around * 2.0))[0]
    if inside.size == 0:
        return float("nan")
    at = inside[np.argmin(gains[inside])] if dip else inside[np.argmax(gains[inside])]
    within = gains <= -by if dip else gains >= gains[at] - by
    if not within[at]:
        return 0.0
    low = at
    while low > 0 and within[low - 1]:
        low -= 1
    high = at
    while high < bins.size - 1 and within[high + 1]:
        high += 1
    if low == 0 or high == bins.size - 1:
        return float("nan")
    return math.log2(bins[high] / bins[low])


def rings_on(samples: np.ndarray, rate: int, until: float, around: float, span: float = 0.1, floor: float = -60.0) -> bool:
    """Whether a filter is ringing by itself near `around` Hz as a sweep ends at `until` seconds:
    over the sweep's last `span` seconds, which are at the top of its range, the output within an
    octave of `around` is louder than `floor` dBFS. The sweep passed there long before, so a
    resonance that dies away has died away by then. It's read before the sweep ends, as our probe
    stops the track with its input."""
    signal = mono(samples)
    start, end = int((until - span) * rate), int(until * rate)
    if start < 0 or end > signal.size:
        return False
    part = signal[start:end] * np.hanning(end - start)
    spectrum = np.abs(np.fft.rfft(part)) ** 2
    bins = np.fft.rfftfreq(end - start, 1.0 / rate)
    inside = (bins >= around / 2.0) & (bins <= around * 2.0)
    # The power of a sine of amplitude a comes out as (a N / 4)^2 under a Hann window, and its RMS
    # is a / sqrt(2).
    amplitude = 4.0 * np.sqrt(spectrum[inside].sum()) / (end - start)
    return db(amplitude / math.sqrt(2.0)) > floor


def level_envelope(samples: np.ndarray, rate: int, width: float = 0.002, hop: float = 0.0005):
    """A signal's level over time: the RMS over `width` seconds every `hop`, in dBFS, as the
    windows' centre times and their levels."""
    signal = mono(samples)
    size, step = max(1, int(width * rate)), max(1, int(hop * rate))
    squares = np.concatenate([[0.0], np.cumsum(signal**2)])
    starts = np.arange(0, max(1, signal.size - size), step)
    power = (squares[starts + size] - squares[starts]) / size
    levels = 10.0 * np.log10(np.maximum(power, 10.0 ** (FLOOR_DB / 10.0)))
    return (starts + size / 2) / rate, levels


def crossing(times: np.ndarray, values: np.ndarray, after: float, level: float, rising: bool) -> float:
    """The first time from `after` that values reach `level`, rising to it or falling to it, read
    between the two points either side, or NaN if they never do."""
    inside = np.where(times >= after)[0]
    if inside.size == 0:
        return float("nan")
    part, at = values[inside], times[inside]
    reached = np.where(part >= level if rising else part <= level)[0]
    reached = reached[np.isfinite(part[reached])]
    if reached.size == 0:
        return float("nan")
    index = int(reached[0])
    if index == 0 or not math.isfinite(part[index - 1]) or part[index] == part[index - 1]:
        return float(at[index])
    share = (level - part[index - 1]) / (part[index] - part[index - 1])
    return float(at[index - 1] + share * (at[index] - at[index - 1]))


def median(values: np.ndarray, width: int) -> np.ndarray:
    """A running median over `width` points, an odd number, leaving NaNs out, and NaN where all
    of a point's neighbours are."""
    half = width // 2
    padded = np.pad(values.astype(np.float64), half, constant_values=np.nan)
    windows = np.lib.stride_tricks.sliding_window_view(padded, width)
    out = np.full(values.shape, np.nan)
    finite = np.isfinite(windows).any(axis=1)
    out[finite] = np.nanmedian(windows[finite], axis=1)
    return out


def pitch_track(samples: np.ndarray, rate: int, start: float, end: float, low: float, high: float,
                hop: float = 0.001, span: float = 0.02, threshold: float = 0.15):
    """A note's pitch over time between two times, by de Cheveigné and Kawahara's YIN: each frame,
    `span` seconds long and `hop` apart, is compared with itself shifted by every period from one
    over `high` to one over `low` Hz, and the first period whose normalized difference dips under
    `threshold` is taken, or the deepest, read between samples by a parabola. Gives the frames'
    centre times and their frequencies in Hz, NaN where a frame is too quiet to have one."""
    signal = mono(samples)
    width = int(span * rate)
    shortest, longest = max(2, int(rate / high)), int(rate / low) + 2
    step = max(1, int(hop * rate))
    first, last = int(start * rate), min(int(end * rate), signal.size - width - longest - 1)
    size = 1 << int(math.ceil(math.log2(width + longest + 1)))
    loudest = float(np.abs(signal[first : last + width]).max(initial=0.0)) if last > first else 0.0
    lags = np.arange(longest + 1)
    times, found = [], []
    for at in range(first, max(first, last), step):
        frame = signal[at : at + width + longest + 1]
        head = frame[:width]
        times.append((at + width / 2) / rate)
        if np.abs(head).max(initial=0.0) < loudest * 1e-3:
            found.append(float("nan"))
            continue
        correlation = np.fft.irfft(np.conj(np.fft.rfft(head, size)) * np.fft.rfft(frame, size), size)[: longest + 1]
        squares = np.concatenate([[0.0], np.cumsum(frame**2)])
        difference = squares[width] + (squares[lags + width] - squares[lags]) - 2.0 * correlation
        difference[0] = 0.0
        normal = np.ones(longest + 1)
        normal[1:] = difference[1:] * lags[1:] / np.maximum(np.cumsum(difference[1:]), 1e-30)
        below = np.where(normal[shortest:longest] < threshold)[0]
        if below.size:
            lag = shortest + int(below[0])
            while lag + 1 < longest and normal[lag + 1] < normal[lag]:
                lag += 1
        else:
            lag = shortest + int(np.argmin(normal[shortest:longest]))
        y0, y1, y2 = normal[lag - 1], normal[lag], normal[lag + 1]
        bend = y0 - 2.0 * y1 + y2
        shift = min(max(0.5 * (y0 - y2) / bend, -1.0), 1.0) if bend > 0 else 0.0
        found.append(rate / (lag + shift))
    return np.array(times), np.array(found)
