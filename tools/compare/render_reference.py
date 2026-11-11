"""Render a spec's reference side: one of its references' plugins, hosted by pedalboard, playing
the spec's notes or given its input, or one of Ableton Live's devices through `live_bridge.py`.

Usage: render_reference.py <spec.toml> <out.wav> [--case NAME] [--reference NAME]

Only reads the plugin. Some plugins write their own caches as they load (u-he's do), which is
theirs to do; nothing here writes in a plugin's folder or its preferences.

A reference's section can say, beside its plugin, parameters and state:

- `channels`, how many it puts out, 2 if not given; ABL3's output is mono, so 1.
- `accent_from`, the velocity it accents a note from, so a note's `accent` reaches it as an
  accent, or a plain note, however its threshold sits against ours.
- `preroll`, seconds it plays before the notes, which are cut from what it renders, for a plugin
  that makes a sound of its own as it starts, rounded up to whole buffers so its buffers fall
  where they would without it.
- `wake_note`, a MIDI note played for a tenth of a second at the pre-roll's start, for a plugin
  that stays silent until it's had one.
- `settle = true`, for a plugin that takes a new setting only at its next render, as Arturia's
  Filter MS-20 does: its cutoff stays where the last case left it for a whole render, and a
  pre-roll doesn't help. It's given one buffer of silence with the case's settings first.
"""

from __future__ import annotations

import argparse
import difflib
import math
import sys
from pathlib import Path

import numpy as np
import pedalboard
from mido import Message
from scipy.io import wavfile

import plugin_state
import spec as specs

# Arturia's plugins put out silence at pedalboard's default buffer size of 8192 (#186), so a host's
# buffer size is used unless a spec gives its own.
DEFAULT_BUFFER_SIZE = 512
# How long a wake note is held, in seconds.
WAKE_SECONDS = 0.1


class Reference:
    """A loaded plugin, kept between cases, with the values its parameters had as it loaded so each
    case starts from them."""

    def __init__(self, section: dict):
        self.section = section
        self.name = section.get("name") or Path(section["plugin"]).stem
        self.path = section["plugin"]
        self.plugin = pedalboard.load_plugin(self.path, plugin_name=section.get("plugin_name"))
        preset, state = section.get("preset"), section.get("state")
        if preset or state:
            # Acid V, at least, plays from a state only once it's been given it a second time: the
            # first leaves it silent (found on 2026-11-10).
            for _ in range(2):
                plugin_state.start_from(self.plugin, self.path, preset, state)
        self.loaded = {}

    @classmethod
    def of(cls, section: dict):
        """The plugin a spec's reference section names, started from its preset and state if it
        gives them, or the Live device it names, which `live_bridge.py` renders through Live."""
        if section.get("live"):
            import live_bridge

            return live_bridge.LiveReference.of(section)
        return cls(section)

    def set(self, parameters: dict) -> None:
        known = self.plugin.parameters
        for name in parameters:
            if name not in known:
                near = difflib.get_close_matches(name, list(known), n=5)
                hint = f"; did you mean {', '.join(near)}?" if near else ""
                raise SystemExit(f"{self.path} has no parameter called {name}{hint}")
        # Anything an earlier case set goes back to how it loaded.
        for name, value in self.loaded.items():
            if name not in parameters:
                setattr(self.plugin, name, value)
        for name, value in parameters.items():
            self.loaded.setdefault(name, getattr(self.plugin, name))
            setattr(self.plugin, name, value)


def render(spec: specs.Spec, case: str | None, reference: Reference | None = None, name: str | None = None) -> np.ndarray:
    """A reference's output for a spec, or one of its cases, as stereo samples (2, frames): the
    one called `name`, or the reference given, or the spec's first."""
    section = spec.reference(case, name or (reference.name if reference else None))
    if reference is None:
        reference = Reference.of(section)
    if getattr(reference, "renders_itself", False):
        return reference.render(spec, case, section)
    reference.set(section["parameters"])
    buffer_size = int(section.get("buffer_size", DEFAULT_BUFFER_SIZE))
    channels = int(section.get("channels", 2))
    plugin, rate = reference.plugin, spec.rate
    if section.get("settle"):
        plugin(np.zeros((channels, buffer_size), dtype=np.float32), rate, buffer_size=buffer_size, reset=True)
    preroll = math.ceil(float(section.get("preroll", 0.0)) * rate / buffer_size) * buffer_size
    if spec.plays_notes:
        frames = specs.duration_frames(spec)
        messages = specs.midi(spec, case, int(section.get("accent_from", specs.ACCENT_VELOCITY)), preroll / rate)
        if "wake_note" in section:
            note = int(section["wake_note"])
            messages = [
                Message("note_on", note=note, velocity=specs.DEFAULT_VELOCITY, time=0.0),
                Message("note_off", note=note, velocity=0, time=WAKE_SECONDS),
            ] + messages
            # In time order, keeping each time's note-offs before its note-ons.
            messages.sort(key=lambda message: message.time)
        out = plugin(
            messages,
            duration=(preroll + frames) / rate,
            sample_rate=rate,
            num_channels=channels,
            buffer_size=buffer_size,
            reset=True,
        )
    else:
        given = specs.signal(spec, case)
        frames = specs.duration_frames(spec, given.shape[1])
        padded = np.zeros((channels, preroll + frames), dtype=np.float32)
        padded[:, preroll : preroll + min(frames, given.shape[1])] = given[:channels, :frames]
        out = plugin(padded, rate, buffer_size=buffer_size, reset=True)
    out = np.asarray(out, dtype=np.float32)[:, preroll:]
    if out.shape[0] == 1:
        out = np.vstack([out, out])
    if out.shape[1] < frames:
        out = np.pad(out, ((0, 0), (0, frames - out.shape[1])))
    return out[:2, :frames]


def write(path: Path, rate: int, samples: np.ndarray) -> None:
    """Write stereo samples (2, frames) as a 32-bit float WAV, as the probe writes its own."""
    wavfile.write(path, rate, np.ascontiguousarray(samples.T, dtype=np.float32))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("spec", type=Path)
    parser.add_argument("out", type=Path)
    parser.add_argument("--case")
    parser.add_argument("--reference", help="which of the spec's references; the first if not given")
    args = parser.parse_args()
    import song_line

    try:
        spec = specs.read(args.spec)
    except song_line.Skipped as skipped:
        print(skipped, file=sys.stderr)
        return
    out = render(spec, args.case, name=args.reference)
    write(args.out, spec.rate, out)
    print(f"Rendered {args.out} ({out.shape[1] / spec.rate:.2f} s)", file=sys.stderr)


if __name__ == "__main__":
    main()
