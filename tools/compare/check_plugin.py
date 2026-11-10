"""Load one plugin headless with pedalboard and render a short test, printing what happened: the
load check of #186, kept for trying a new reference plugin.

Usage: check_plugin.py <plugin path> instrument|effect

Writes out/check/<name>.wav. Only reads the plugin.
"""

import sys
import time
from pathlib import Path

import numpy as np
import pedalboard
from mido import Message
from scipy.io import wavfile

RATE = 48_000
# Arturia's plugins put out silence at pedalboard's default of 8192.
BUFFER_SIZE = 512

path, kind = sys.argv[1], sys.argv[2]
name = Path(path).stem.replace(" ", "_") + ("_au" if path.endswith(".component") else "_vst3")

started = time.time()
plugin = pedalboard.load_plugin(path)
print(f"loaded {path} in {time.time() - started:.2f} s: {plugin.name!r}, instrument={plugin.is_instrument}")
parameters = list(plugin.parameters.keys())
print(f"{len(parameters)} parameters, first: {parameters[:12]}")

if kind == "instrument":
    notes = []
    for index, note in enumerate([36, 48, 43, 36]):
        notes.append(Message("note_on", note=note, velocity=100, time=index * 0.25))
        notes.append(Message("note_off", note=note, velocity=0, time=index * 0.25 + 0.2))
    out = plugin(notes, duration=1.5, sample_rate=RATE, num_channels=2, buffer_size=BUFFER_SIZE)
else:
    t = np.arange(int(RATE * 1.5)) / RATE
    burst = np.where((t > 0.25) & (t < 1.0), 0.7, 0.07)
    signal = (np.sin(2 * np.pi * 220 * t) * burst).astype(np.float32)
    out = plugin(np.stack([signal, signal]), RATE, buffer_size=BUFFER_SIZE)

peak = float(np.max(np.abs(out)))
rms = float(np.sqrt(np.mean(out**2)))
print(f"rendered {out.shape}, peak {peak:.4f}, rms {rms:.4f}, nan={bool(np.isnan(out).any())}")
folder = Path(__file__).resolve().parent / "out" / "check"
folder.mkdir(parents=True, exist_ok=True)
wavfile.write(folder / f"{name}.wav", RATE, np.ascontiguousarray(out.T, dtype=np.float32))
