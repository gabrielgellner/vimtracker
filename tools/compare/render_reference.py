"""Render a spec's reference side: its `[reference]` plugin, hosted by pedalboard, playing the
spec's notes or given its input.

Usage: render_reference.py <spec.toml> <out.wav> [--case NAME]

Only reads the plugin. Some plugins write their own caches as they load (u-he's do), which is
theirs to do; nothing here writes in a plugin's folder or its preferences.
"""

from __future__ import annotations

import argparse
import difflib
import sys
from pathlib import Path

import numpy as np
import pedalboard
from scipy.io import wavfile

import arturia
import spec as specs

# Arturia's plugins put out silence at pedalboard's default buffer size of 8192 (#186), so a host's
# buffer size is used unless a spec gives its own.
DEFAULT_BUFFER_SIZE = 512


class Reference:
    """A loaded plugin, kept between cases, with the values its parameters had as it loaded so each
    case starts from them."""

    def __init__(self, path: str, plugin_name: str | None = None, preset=None, state=None):
        self.path = path
        self.plugin = pedalboard.load_plugin(path, plugin_name=plugin_name)
        if preset or state:
            # Acid V, at least, plays from a state only once it's been given it a second time: the
            # first leaves it silent (found on 2026-11-10).
            for _ in range(2):
                arturia.start_from(self.plugin, path, preset, state)
        self.loaded = {}

    @classmethod
    def of(cls, section: dict) -> "Reference":
        """The plugin a spec's `[reference]` names, started from its preset and state if it gives
        them."""
        return cls(section["plugin"], section.get("plugin_name"), section.get("preset"), section.get("state"))

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


def render(spec: specs.Spec, case: str | None, reference: Reference | None = None) -> np.ndarray:
    """The reference's output for a spec, or one of its cases, as stereo samples (2, frames)."""
    section = spec.reference(case)
    if reference is None:
        reference = Reference.of(section)
    reference.set(section["parameters"])
    buffer_size = int(section.get("buffer_size", DEFAULT_BUFFER_SIZE))
    plugin, rate = reference.plugin, spec.rate
    if "notes" in spec.data:
        frames = specs.duration_frames(spec)
        out = plugin(
            specs.midi(spec),
            duration=frames / rate,
            sample_rate=rate,
            num_channels=2,
            buffer_size=buffer_size,
            reset=True,
        )
    else:
        given = specs.signal(spec, case)
        frames = specs.duration_frames(spec, given.shape[1])
        padded = np.zeros((2, frames), dtype=np.float32)
        padded[:, : min(frames, given.shape[1])] = given[:, :frames]
        out = plugin(padded, rate, buffer_size=buffer_size, reset=True)
    out = np.asarray(out, dtype=np.float32)
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
    args = parser.parse_args()
    spec = specs.read(args.spec)
    out = render(spec, args.case)
    write(args.out, spec.rate, out)
    print(f"Rendered {args.out} ({out.shape[1] / spec.rate:.2f} s)", file=sys.stderr)


if __name__ == "__main__":
    main()
