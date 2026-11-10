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
