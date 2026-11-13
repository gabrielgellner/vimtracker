"""Rendering a spec's input through one of Ableton Live's built-in devices, by way of the bridge, a
control surface script running inside Live (`live/VimtrackerBridge`, #364).

Usage: live_bridge.py ping
       live_bridge.py params <device>
       live_bridge.py null
       live_bridge.py remove

`ping` asks whether Live is answering. `params` loads a device, such as `"Glue Compressor"`, and
lists its parameters: their names, ranges, what they show and a list's items, which a spec's
parameters are written with. `null` renders a sweep through the probe with no device, which should
come back as it went, and says whether it did: the first thing to run once Live is open. `remove`
deletes the bridge's two tracks from the set and puts back the launch quantization it changed:
the last thing to run, so the set is left as it was.

A spec names a Live device as a reference:

    [[references]]
    name = "Glue"
    live = "Glue Compressor"        # the device's name as Live shows it
    [references.parameters]         # by Live's names, as Live shows them
    Threshold = "-20 dB"
    Ratio = 4
    Attack = "10 ms"

Live plays in real time, so a render takes as long as what's played. What goes to Live is the
input with a marker before it: a quarter second of silence, one sample at -30 dBFS, a second of
silence, the input, and silence after it for the device to ring out. The marker's place in what
comes back gives Live's delay, the device's latency with it, which is cut off, so the output lines
up with the input as ours does. The marker is quiet enough that a compressor or a pedal passes it
much as it is, and the second after it lets a compressor recover before the input starts.

Each render is kept in `out/live-cache/`, by a hash of the device, its parameters and the samples
sent, with what was played beside it, so a spec runs again without Live; `run.py --fresh-live`
renders again anyway.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import math
import os
import socket
import sys
import time
from pathlib import Path

import numpy as np
from scipy.io import wavfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "live"))
from VimtrackerBridge import values  # noqa: E402
from VimtrackerBridge.bridge import HOST, PORT  # noqa: E402

import spec as specs  # noqa: E402

# Where the bridge listens; VT_LIVE_PORT changes it, for the tests' stand-in.
ADDRESS = (HOST, int(os.environ.get("VT_LIVE_PORT", PORT)))
CACHE = HERE / "out" / "live-cache"
# The framing around the input, in seconds and dBFS.
LEAD_SECONDS = 0.25
MARKER_DB = -30.0
GAP_SECONDS = 1.0
TAIL_SECONDS = 1.0
# How much longer than the framed input Live records, for the start of its playing and recording.
SPARE_SECONDS = 0.5
# A marker quieter than this coming back, in dBFS, wasn't heard.
HEARD_DB = -80.0
# What the framing and the bridge's protocol are, in the cache's key, so a change to either renders
# again.
FRAMING = {"lead": LEAD_SECONDS, "marker": MARKER_DB, "gap": GAP_SECONDS, "protocol": 1}
# Set by `run.py --fresh-live`: render even what's cached.
FRESH = False
# How close the null check must come: the level within this many dB, and what's left over under
# this many dBFS.
NULL_GAIN_DB = 0.01
NULL_RESIDUAL_DB = -90.0

UNREACHABLE = (
    "Live isn't answering on {host}:{port}. Open Ableton Live with VimtrackerBridge chosen as a "
    "control surface in Settings, Link, Tempo & MIDI; tools/compare/README.md, 'Ableton Live's "
    "devices', has the steps. A render made before is read from out/live-cache/ without Live."
)


class LiveError(SystemExit):
    """Something Live or the bridge refused, with why."""


# Live fills a new track's list of inputs a moment after making it, so a request that finds it
# empty is tried again after this long, this many times.
ROUTING_RETRY_SECONDS = 1.0
ROUTING_RETRIES = 3


def request(command: str, timeout: float = 15.0, address=None, **fields) -> dict:
    """Send the bridge one request and wait for its answer, trying again while a new track's inputs
    aren't listed yet."""
    for _ in range(ROUTING_RETRIES):
        try:
            return request_once(command, timeout, address, **fields)
        except LiveError as error:
            if "can't take its input from" not in str(error):
                raise
            time.sleep(ROUTING_RETRY_SECONDS)
    return request_once(command, timeout, address, **fields)


def request_once(command: str, timeout: float = 15.0, address=None, **fields) -> dict:
    """Send the bridge one request and wait for its answer."""
    host, port = address or ADDRESS
    try:
        connection = socket.create_connection((host, port), timeout=2.0)
    except OSError:
        raise LiveError(UNREACHABLE.format(host=host, port=port)) from None
    with connection:
        connection.settimeout(timeout)
        connection.sendall((json.dumps({"command": command, **fields}) + "\n").encode("utf-8"))
        received = b""
        try:
            while not received.endswith(b"\n"):
                chunk = connection.recv(65536)
                if not chunk:
                    break
                received += chunk
        except socket.timeout:
            raise LiveError(f"Live took more than {timeout:.0f} s to answer `{command}`") from None
    if not received:
        raise LiveError(f"the bridge closed the connection without answering `{command}`")
    answer = json.loads(received.decode("utf-8"))
    if not answer.get("ok"):
        raise LiveError(f"Live: {answer.get('error')}")
    return answer


# Framing the input, and finding it again in what comes back.


def framed(given: np.ndarray, rate: int, frames: int, gap: float = GAP_SECONDS, tail: float = TAIL_SECONDS):
    """The input with the marker before it and silence after, for `frames` frames of output: the
    samples, where the marker is and where the input starts."""
    marker = specs.frames_in(LEAD_SECONDS, rate)
    start = marker + specs.frames_in(gap, rate)
    after = max(frames - given.shape[1], 0) + specs.frames_in(tail, rate)
    sent = np.zeros((2, start + given.shape[1] + after), dtype=np.float32)
    sent[:, marker] = specs.gain_of(MARKER_DB)
    sent[:, start : start + given.shape[1]] = given[:2]
    return sent, marker, start


def lag_of(received: np.ndarray, marker: int, start: int, rate: int) -> int:
    """How many frames later the marker came back than it was sent: Live's delay and the device's.
    It's the loudest sample from the start to halfway through the gap after the marker."""
    window = np.abs(received[:, : marker + (start - marker) // 2]).max(axis=0)
    if window.size == 0 or window.max() < specs.gain_of(HEARD_DB):
        raise LiveError(
            "the marker didn't come back: Live recorded silence or nothing before the input. "
            "Check that `vt bridge sink`'s input is `vt bridge source`, Post FX, and run "
            "`uv run live_bridge.py null`"
        )
    return int(np.argmax(window)) - marker


def aligned(received: np.ndarray, start: int, lag: int, frames: int) -> np.ndarray:
    """The output from where the input went in, lag taken out, for `frames` frames."""
    begin = start + lag
    out = received[:, begin : begin + frames]
    if out.shape[1] < frames:
        out = np.pad(out, ((0, 0), (0, frames - out.shape[1])))
    return np.ascontiguousarray(out, dtype=np.float32)


def read_recording(path: str, rate: int) -> np.ndarray:
    """Live's recording as stereo float samples (2, frames), refusing one at another rate."""
    from pedalboard.io import AudioFile

    with AudioFile(path) as file:
        if round(file.samplerate) != rate:
            raise LiveError(
                f"Live recorded at {file.samplerate:g} Hz and the spec is at {rate} Hz: set Live's "
                f"Settings, Audio, Sample Rate to {rate}"
            )
        dtype = getattr(file, "file_dtype", "float32")
        audio = file.read(file.frames)
    if audio.shape[0] == 1:
        audio = np.vstack([audio, audio])
    audio = audio[:2].astype(np.float32)
    if "float" not in str(dtype) and np.abs(audio).max(initial=0.0) >= 0.999:
        print(
            f"warning: Live recorded {dtype}, which clips at full scale, and this reaches it; set "
            "Settings, Record, Warp & Launch, Bit Depth to 32",
            file=sys.stderr,
        )
    return audio


# Rendering, cached.


def cache_key(device: str | None, parameters: dict, rate: int, sent: np.ndarray) -> str:
    digest = hashlib.sha256()
    digest.update(json.dumps({"device": device, "parameters": parameters, "rate": rate, **FRAMING}, sort_keys=True).encode())
    digest.update(np.ascontiguousarray(sent, dtype=np.float32).tobytes())
    return digest.hexdigest()[:24]


def check_shown(device: str, wanted: dict, shown: dict) -> None:
    """Refuse a render whose parameters Live shows otherwise than the spec asked."""
    wrong = [f"{name} shows {shown.get(name)!r}, not {value!r}" for name, value in wanted.items() if not values.matches(value, shown.get(name, ""))]
    if wrong:
        raise LiveError(f"{device}: " + "; ".join(wrong))


def render_through(device: str | None, parameters: dict, given: np.ndarray, rate: int, frames: int,
                   gap: float = GAP_SECONDS, tail: float = TAIL_SECONDS, fresh: bool = False, cache: Path | None = None,
                   quiet: bool = False) -> tuple[np.ndarray, dict]:
    """`given` through a Live device set as `parameters`, `frames` frames of it lined up with the
    input, and what's known of the render: what Live showed, the lag and so on. Read from the cache
    if it's been rendered before."""
    cache = cache or CACHE
    sent, marker, start = framed(given, rate, frames, gap, tail)
    key = cache_key(device, parameters, rate, sent)
    wav, notes_path = cache / f"{key}.wav", cache / f"{key}.json"
    if wav.exists() and notes_path.exists() and not (fresh or FRESH):
        _, samples = wavfile.read(wav)
        return np.ascontiguousarray(samples.T, dtype=np.float32), json.loads(notes_path.read_text())

    loaded = request("load", device=device)
    shown = {}
    if parameters:
        answer = request("set", parameters=parameters)
        shown = answer["shown"]
        check_shown(device, parameters, answer["set"])
    elif loaded.get("device"):
        shown = {p["name"]: p["shown"] for p in loaded["device"]["parameters"]}
    cache.mkdir(parents=True, exist_ok=True)
    seconds = sent.shape[1] / rate
    # What's played stays beside the render, as Live reads it from disk as it plays, and its clip
    # points at it until the next render replaces it.
    played = cache / f"{key}-played.wav"
    wavfile.write(played, rate, np.ascontiguousarray(sent.T))
    if not quiet:
        print(f"Live: rendering {seconds:.1f} s through {device or 'no device'}", file=sys.stderr, flush=True)
    answer = request("render", timeout=seconds + 60.0, path=str(played), seconds=seconds + SPARE_SECONDS)
    received = read_recording(answer["recorded"], rate)
    lag = lag_of(received, marker, start, rate)
    out = aligned(received, start, lag, frames)
    notes = {
        "device": device,
        "parameters": parameters,
        "shown": shown,
        "lag": lag,
        "latency": (loaded.get("device") or {}).get("latency"),
        "recorded": answer["recorded"],
        "clip gain": answer.get("clip gain"),
        "rendered": datetime.datetime.now().isoformat(timespec="seconds"),
    }
    wavfile.write(wav, rate, np.ascontiguousarray(out.T))
    notes_path.write_text(json.dumps(notes, indent=1))
    return out, notes


class LiveReference:
    """A reference that's one of Live's devices, in the place of a plugin pedalboard hosts. It
    talks to Live only when a render isn't cached."""

    renders_itself = True

    def __init__(self, section: dict):
        self.section = section
        self.device = section.get("live") or None
        self.name = section.get("name") or self.device or "Live"
        self.notes = {}

    @classmethod
    def of(cls, section: dict) -> "LiveReference":
        return cls(section)

    def render(self, spec: specs.Spec, case: str | None, section: dict) -> np.ndarray:
        if spec.plays_notes:
            raise SystemExit(f"{spec.path}: a Live reference takes an input; Live's instruments aren't bridged yet")
        given = specs.signal(spec, case)
        frames = specs.duration_frames(spec, given.shape[1])
        out, self.notes = render_through(
            self.device,
            dict(section.get("parameters", {})),
            given,
            spec.rate,
            frames,
            gap=float(section.get("marker_gap", GAP_SECONDS)),
            tail=float(section.get("tail", TAIL_SECONDS)),
        )
        return out


# The commands.


def null_check(rate: int = 48_000, fresh: bool = True) -> bool:
    """A sweep through the probe with no device, which should come back as it went: print how
    close it came, and give whether it passed."""
    seconds, low, high, level = 2.0, 20.0, 20_000.0, -6.0
    t = np.arange(specs.frames_in(seconds, rate)) / rate
    ratio = math.log(high / low)
    mono = specs.gain_of(level) * np.sin(2.0 * np.pi * low * seconds / ratio * (np.exp(t / seconds * ratio) - 1.0))
    given = np.vstack([mono, mono]).astype(np.float32)
    out, notes = render_through(None, {}, given, rate, given.shape[1], fresh=fresh)
    gain = 20.0 * math.log10(float(np.sqrt(np.mean(out**2))) / float(np.sqrt(np.mean(given**2))) + 1e-30)
    residual = 20.0 * math.log10(float(np.abs(out - given).max()) + 1e-30)
    channels = 20.0 * math.log10(float(np.abs(out[0] - out[1]).max()) + 1e-30)
    print(f"lag: {notes['lag']} frames ({notes['lag'] / rate * 1000:.2f} ms), clip gain {notes.get('clip gain')}")
    print(f"level: {gain:+.4f} dB against what went in")
    print(f"left over once subtracted: {residual:.1f} dBFS at most")
    print(f"left against right: {channels:.1f} dBFS at most")
    passed = abs(gain) <= NULL_GAIN_DB and residual <= NULL_RESIDUAL_DB
    print(f"null: {'OK' if passed else 'FAILED'}: the probe is {'' if passed else 'not '}unity with no device")
    return passed


def print_parameters(device: str) -> None:
    answer = request("load", device=device)
    found = answer["device"]
    print(f"{found['class']} ({found['name']}), latency {found.get('latency')} samples")
    for parameter in found["parameters"]:
        line = f"  {parameter['name']!r:28} {parameter['shown']:>14}   {parameter['from']} to {parameter['to']}"
        if parameter.get("items"):
            line += f"   items: {', '.join(parameter['items'])}"
        print(line)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("ping", help="ask whether Live is answering")
    listing = commands.add_parser("params", help="load a device and list its parameters")
    listing.add_argument("device")
    commands.add_parser("null", help="render a sweep through no device and check it comes back the same")
    commands.add_parser("reload", help="have the bridge read its code again, without restarting Live")
    commands.add_parser("remove", help="delete the bridge's tracks and put the set back as it was")
    commands.add_parser("introspect", help="list what Live's classes offer scripts, touching nothing in the set")
    tempo = commands.add_parser("tempo", help="load an audio file into a scratch track, read Live's warp markers, delete the track")
    tempo.add_argument("path")
    args = parser.parse_args()
    if args.command == "ping":
        answer = request("ping")
        print(f"Live {answer.get('live')} is answering: bridge {answer['bridge']}, tempo {answer['tempo']:g} BPM")
    elif args.command == "params":
        print_parameters(args.device)
    elif args.command == "reload":
        answer = request("reload")
        print(f"the bridge read its code again: bridge {answer['bridge']}")
    elif args.command == "null":
        sys.exit(0 if null_check() else 1)
    elif args.command == "remove":
        answer = request("remove")
        removed = ", ".join(answer["removed"]) or "no tracks"
        print(f"removed {removed} from the set")
        if answer.get("quantization") is not None:
            print("and put its launch quantization back as it was")
    elif args.command == "tempo":
        answer = request("tempo probe", path=str(Path(args.path).resolve()))
        print(json.dumps(answer, indent=1))
    elif args.command == "introspect":
        for name, members in request("introspect").items():
            print(f"{name}: {', '.join(members) if isinstance(members, list) else members}")


if __name__ == "__main__":
    main()
