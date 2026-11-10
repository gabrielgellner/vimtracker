"""The Live bridge without Live: its value mapping, its protocol and a whole render, against
stand-ins for the Live Object Model's song, tracks, clip slots, devices and parameters (#364).

    cd tools/compare && uv run python -m unittest discover tests

The stand-in "records" by reading the clip it was given, putting it through the device's own
sum (a gain) and delaying it, so the harness's framing, alignment and cache are checked end to
end. What Live itself does is in the README's checklist, as only Live can check it.
"""

from __future__ import annotations

import contextlib
import io
import math
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path

import numpy as np
from scipy.io import wavfile

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))

import live_bridge  # noqa: E402
import report  # noqa: E402
import spec as specs  # noqa: E402
from VimtrackerBridge import bridge, values  # noqa: E402

RATE = 48_000


# Stand-ins for Live's objects, with the names and behaviour the LOM documents.


class Parameter:
    def __init__(self, name, show, low=0.0, high=1.0, value=0.0, items=None, default=None):
        self.name = name
        self.show = show
        self.min, self.max, self.value = low, high, value
        self.is_quantized = items is not None
        self.value_items = items or []
        self.default_value = default if default is not None else value

    def str_for_value(self, value):
        return self.show(value)

    def __str__(self):
        return self.show(self.value)


def listed(items):
    return lambda value: items[int(round(value))]


def decibels(low, high, digits=1):
    return lambda value: f"{low + (high - low) * value:.{digits}f} dB"


def seconds_or_ms(value):
    """A time from 1 ms to 10 s on a log scale, shown in ms under a second, as Live shows times."""
    seconds = 0.001 * 10_000.0**value
    return f"{seconds * 1000:.1f} ms" if seconds < 1.0 else f"{seconds:.2f} s"


class Device:
    def __init__(self, name, parameters, gain_from=None, latency=0):
        self.name = self.class_display_name = name
        self.class_name = name.replace(" ", "")
        self.parameters = parameters
        self.gain_from = gain_from
        self.latency_in_samples = latency

    def process(self, samples):
        gain = 1.0
        if self.gain_from:
            shown = values.parse(str(next(p for p in self.parameters if p.name == self.gain_from)))
            gain = 10.0 ** (shown[0] / 20.0)
        out = samples * gain
        return np.pad(out, ((0, 0), (self.latency_in_samples, 0)))[:, : samples.shape[1]]


def glue():
    return Device(
        "Glue Compressor",
        [
            Parameter("Device On", listed(["Off", "On"]), 0, 1, 1, items=["Off", "On"]),
            Parameter("Threshold", decibels(-40, 0), value=1.0),
            Parameter("Ratio", listed(["2", "4", "10"]), 0, 2, 0, items=["2", "4", "10"]),
            Parameter("Attack", listed(["0.01 ms", "0.1 ms", "0.3 ms", "1.00 ms", "3.00 ms", "10.0 ms", "30.0 ms"]), 0, 6, 0,
                      items=["0.01 ms", "0.1 ms", "0.3 ms", "1.00 ms", "3.00 ms", "10.0 ms", "30.0 ms"]),
            Parameter("Release", listed(["0.1 s", "0.2 s", "0.4 s", "0.6 s", "0.8 s", "1.2 s", "A"]), 0, 6, 0,
                      items=["0.1 s", "0.2 s", "0.4 s", "0.6 s", "0.8 s", "1.2 s", "A"]),
            Parameter("Makeup", decibels(0, 20, 2), value=0.0),
            Parameter("Dry/Wet", lambda value: f"{value * 100:.0f} %", value=1.0),
            Parameter("Hold", seconds_or_ms, value=0.5),
        ],
        gain_from="Makeup",
    )


def pedal():
    return Device(
        "Pedal",
        [
            Parameter("Type", listed(["Overdrive", "Distortion", "Fuzz"]), 0, 2, 0, items=["Overdrive", "Distortion", "Fuzz"]),
            Parameter("Gain", lambda value: f"{value * 100:.0f} %", value=0.5),
            Parameter("Output", decibels(-20, 20), value=0.5),
            Parameter("Dry/Wet", lambda value: f"{value * 100:.0f} %", value=1.0),
        ],
        gain_from="Output",
        latency=37,
    )


DEVICES = {"Glue Compressor": glue, "Pedal": pedal}


class Routing:
    def __init__(self, name):
        self.display_name = name


class Clip:
    def __init__(self, path, rate=RATE):
        self.file_path = path
        self.warping = True
        self.looping = True
        self.gain_display_string = "0.0 dB"
        self.sample_rate = rate


class ClipSlot:
    def __init__(self, track):
        self.track = track
        self.clip = None
        self.is_recording = False
        self.fired = False

    @property
    def has_clip(self):
        return self.clip is not None

    @property
    def will_record_on_start(self):
        return self.track.arm and not self.has_clip

    def create_audio_clip(self, path):
        assert self.clip is None and os.path.isabs(path)
        self.clip = Clip(path)

    def delete_clip(self):
        self.clip = None

    def fire(self):
        self.fired = True
        if self.will_record_on_start:
            self.is_recording = True
        self.track.song.is_playing = True


class Track:
    def __init__(self, song, name):
        self.song = song
        self.name = name
        self.devices = []
        self.clip_slots = [ClipSlot(self) for _ in range(8)]
        self.arm = False
        self.can_be_armed = True
        self.has_audio_input = True
        self.current_monitoring_state = 1
        self.input_routing_type = Routing("Ext. In")
        self.input_routing_channel = Routing("1/2")

    @property
    def available_input_routing_types(self):
        return [Routing("Ext. In"), Routing("Resampling")] + [Routing(t.name) for t in self.song.tracks if t is not self] + [Routing("No Input")]

    @property
    def available_input_routing_channels(self):
        if self.input_routing_type.display_name in ("Ext. In", "No Input", "Resampling"):
            return [Routing("1/2")]
        return [Routing("Pre FX"), Routing("Post FX"), Routing("Post Mixer")]

    def insert_device(self, name):
        if name not in DEVICES:
            raise RuntimeError(f"Couldn't find {name}")
        self.devices.append(DEVICES[name]())

    def delete_device(self, index):
        del self.devices[index]


class Song:
    """A song whose recording is its source track's clip, through its devices, `delay` frames
    late and times `gain`, written when the transport stops."""

    def __init__(self, folder, delay=211, gain=1.0):
        self.folder = Path(folder)
        self.delay, self.gain = delay, gain
        self.tracks = [Track(self, "1-Audio")]
        self.tempo = 120.0
        self.is_playing = False
        self.clip_trigger_quantization = 4
        self.recordings = 0

    def create_audio_track(self, index):
        self.tracks.insert(index, Track(self, f"{index + 1}-Audio"))

    def stop_playing(self):
        self.is_playing = False
        for sink in self.tracks:
            slot = sink.clip_slots[0]
            if not slot.is_recording:
                continue
            source = next(t for t in self.tracks if t.name == sink.input_routing_type.display_name)
            assert sink.input_routing_channel.display_name == "Post FX"
            assert source.clip_slots[0].fired
            _, played = wavfile.read(source.clip_slots[0].clip.file_path)
            samples = played.T.astype(np.float64)
            for device in source.devices:
                samples = device.process(samples)
            samples = np.pad(samples * self.gain, ((0, 0), (self.delay, RATE // 2)))
            self.recordings += 1
            path = self.folder / f"recorded-{self.recordings}.wav"
            wavfile.write(path, RATE, np.ascontiguousarray(samples.T, dtype=np.float32))
            slot.is_recording = False
            slot.clip = Clip(str(path))
            source.clip_slots[0].fired = False


class Application:
    def get_major_version(self):
        return 12

    def get_minor_version(self):
        return 4

    def get_bugfix_version(self):
        return 16


class Clock:
    def __init__(self):
        self.now = 0.0

    def __call__(self):
        return self.now


class Running:
    """The bridge serving a stand-in song on a port of its own, polled from a thread, its clock
    moving a tenth of a second each poll as Live's would."""

    def __init__(self, song):
        self.clock = Clock()
        self.bridge = bridge.Bridge(song=lambda: song, application=Application(), log=lambda text: None, port=0, clock=self.clock)
        self.bridge.start()
        self.stopping = threading.Event()
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()
        self.address = (bridge.HOST, self.bridge.port)

    def serve(self):
        while not self.stopping.wait(0.002):
            self.bridge.poll()
            self.clock.now += 0.1

    def stop(self):
        self.stopping.set()
        self.thread.join()
        self.bridge.stop()


class Values(unittest.TestCase):
    def test_parse(self):
        self.assertEqual(values.parse("-20.0 dB")[:2], (-20.0, "db"))
        self.assertEqual(values.parse("100 ms")[3], 0.1)
        self.assertEqual(values.parse("1.20 s")[3], 1.2)
        self.assertEqual(values.parse("1.00 kHz")[3], 1000.0)
        self.assertEqual(values.parse("-inf dB")[0], -math.inf)
        self.assertEqual(values.parse("4")[:2], (4.0, None))
        self.assertIsNone(values.parse("A"))
        self.assertIsNone(values.parse("Overdrive"))

    def test_matches_to_the_shown_rounding(self):
        self.assertTrue(values.matches("-20 dB", "-20.0 dB"))
        self.assertTrue(values.matches(-20, "-20.0 dB"))
        self.assertTrue(values.matches("-20.04 dB", "-20.0 dB"))
        self.assertFalse(values.matches("-20.1 dB", "-20.0 dB"))
        self.assertTrue(values.matches("100 ms", "0.10 s"))
        self.assertFalse(values.matches("100 Hz", "0.10 s"))
        self.assertTrue(values.matches("Distortion", "Distortion"))
        self.assertTrue(values.matches(True, "On"))
        self.assertFalse(values.matches(False, "On"))

    def test_a_continuous_parameter_is_set_by_what_it_shows(self):
        threshold = glue().parameters[1]
        threshold.value = values.value_for(threshold, "-20 dB")
        self.assertEqual(str(threshold), "-20.0 dB")
        self.assertAlmostEqual(threshold.value, 0.5, places=3)
        makeup = glue().parameters[5]
        makeup.value = values.value_for(makeup, 6)
        self.assertEqual(str(makeup), "6.00 dB")

    def test_a_time_meets_its_unit_across_the_display_change(self):
        hold = glue().parameters[7]
        hold.value = values.value_for(hold, "100 ms")
        self.assertEqual(str(hold), "100.0 ms")
        hold.value = values.value_for(hold, "1.5 s")
        self.assertEqual(str(hold), "1.50 s")
        hold.value = values.value_for(hold, "1500 ms")
        self.assertEqual(str(hold), "1.50 s")

    def test_lists_and_switches(self):
        device = glue()
        on, ratio, attack, release = device.parameters[0], device.parameters[2], device.parameters[3], device.parameters[4]
        self.assertEqual(values.value_for(ratio, 4), 1)
        self.assertEqual(values.value_for(ratio, "10"), 2)
        self.assertEqual(values.value_for(attack, "10 ms"), 5)
        self.assertEqual(values.value_for(attack, "0.01 ms"), 0)
        self.assertEqual(values.value_for(release, "A"), 6)
        self.assertEqual(values.value_for(release, "0.4 s"), 2)
        self.assertEqual(values.value_for(on, False), 0)
        kind = pedal().parameters[0]
        self.assertEqual(values.value_for(kind, "distort"), 1)
        self.assertEqual(values.value_for(kind, "max"), 2)

    def test_what_cant_be_set_says_what_can(self):
        device = glue()
        with self.assertRaisesRegex(values.Unsettable, "its values are 2, 4, 10"):
            values.value_for(device.parameters[2], 3)
        with self.assertRaisesRegex(values.Unsettable, "runs from -40.0 dB to 0.0 dB"):
            values.value_for(device.parameters[1], "-50 dB")
        with self.assertRaisesRegex(values.Unsettable, "takes a number"):
            values.value_for(device.parameters[1], "loud")


class Protocol(unittest.TestCase):
    def setUp(self):
        self.folder = Path(tempfile.mkdtemp(prefix="vt-live-test-"))
        self.song = Song(self.folder)
        self.running = Running(self.song)
        self.address = live_bridge.ADDRESS
        live_bridge.ADDRESS = self.running.address

    def tearDown(self):
        live_bridge.ADDRESS = self.address
        self.running.stop()
        shutil.rmtree(self.folder)

    def test_ping(self):
        answer = live_bridge.request("ping")
        self.assertEqual(answer["bridge"], bridge.VERSION)
        self.assertEqual(answer["live"], "12.4.16")

    def test_an_unknown_command_is_refused(self):
        with self.assertRaisesRegex(live_bridge.LiveError, "no command called fly"):
            live_bridge.request("fly")

    def test_loading_makes_and_routes_the_probe_tracks(self):
        answer = live_bridge.request("load", device="Glue Compressor")
        self.assertEqual(answer["device"]["class"], "Glue Compressor")
        names = [track.name for track in self.song.tracks]
        self.assertEqual(names, ["1-Audio", bridge.SOURCE, bridge.SINK])
        source, sink = self.song.tracks[1], self.song.tracks[2]
        self.assertEqual(sink.input_routing_type.display_name, bridge.SOURCE)
        self.assertEqual(sink.input_routing_channel.display_name, "Post FX")
        self.assertTrue(sink.arm)
        self.assertFalse(source.arm)
        self.assertEqual(sink.current_monitoring_state, bridge.MONITOR_OFF)
        self.assertEqual(self.song.clip_trigger_quantization, bridge.NO_QUANTIZATION)
        # Loading again starts a fresh device, alone, on the same tracks.
        live_bridge.request("load", device="Pedal")
        self.assertEqual([d.name for d in source.devices], ["Pedal"])
        self.assertEqual(len(self.song.tracks), 3)

    def test_an_unknown_device_is_refused(self):
        with self.assertRaisesRegex(live_bridge.LiveError, "couldn't put Glue Compresor"):
            live_bridge.request("load", device="Glue Compresor")

    def test_set_answers_with_what_live_shows(self):
        live_bridge.request("load", device="Glue Compressor")
        answer = live_bridge.request("set", parameters={"Threshold": "-20 dB", "Ratio": 4, "Attack": "10 ms", "Release": "A"})
        self.assertEqual(answer["set"], {"Threshold": "-20.0 dB", "Ratio": "4", "Attack": "10.0 ms", "Release": "A"})
        self.assertEqual(answer["shown"]["Dry/Wet"], "100 %")

    def test_an_unknown_parameter_is_refused_with_its_neighbours(self):
        live_bridge.request("load", device="Glue Compressor")
        with self.assertRaisesRegex(live_bridge.LiveError, "no parameter called Treshold; did you mean Threshold"):
            live_bridge.request("set", parameters={"Treshold": "-20 dB"})

    def test_a_render_comes_back_lined_up_and_cached(self):
        cache = self.folder / "cache"
        t = np.arange(RATE // 2) / RATE
        mono = 0.25 * np.sin(2 * np.pi * 440.0 * t)
        given = np.vstack([mono, mono]).astype(np.float32)
        frames = given.shape[1] + RATE // 10
        out, notes = live_bridge.render_through("Pedal", {"Output": "6 dB", "Type": "Distortion"}, given, RATE, frames, cache=cache, quiet=True)
        self.assertEqual(notes["lag"], self.song.delay + 37)
        self.assertEqual(notes["shown"]["Type"], "Distortion")
        self.assertEqual(out.shape, (2, frames))
        expected = np.pad(given * 10 ** (6 / 20), ((0, 0), (0, RATE // 10)))
        self.assertLess(np.abs(out - expected).max(), 1e-6)
        # Without Live, the same render is read back from the cache.
        self.running.stop()
        again, _ = live_bridge.render_through("Pedal", {"Output": "6 dB", "Type": "Distortion"}, given, RATE, frames, cache=cache, quiet=True)
        self.assertTrue(np.array_equal(out, again))
        with self.assertRaisesRegex(live_bridge.LiveError, "Live isn't answering"):
            live_bridge.render_through("Pedal", {"Output": "0 dB"}, given, RATE, frames, cache=cache, quiet=True)
        self.running = Running(Song(self.folder))

    def test_the_null_check(self):
        live_bridge.CACHE, cache = self.folder / "cache", live_bridge.CACHE
        printed = io.StringIO()
        try:
            with contextlib.redirect_stdout(printed), contextlib.redirect_stderr(io.StringIO()):
                self.assertTrue(live_bridge.null_check())
                self.song.gain = 0.5
                self.assertFalse(live_bridge.null_check())
        finally:
            live_bridge.CACHE = cache
        self.assertIn("null: OK", printed.getvalue())
        self.assertIn("level: -6.0206 dB", printed.getvalue())


class Harness(unittest.TestCase):
    def test_live_isnt_there(self):
        with self.assertRaisesRegex(live_bridge.LiveError, "Live isn't answering on 127.0.0.1:9"):
            live_bridge.request("ping", address=("127.0.0.1", 9))

    def test_what_live_shows_is_checked(self):
        live_bridge.check_shown("Glue", {"Threshold": "-20 dB"}, {"Threshold": "-20.0 dB"})
        with self.assertRaisesRegex(live_bridge.LiveError, "Threshold shows '-19.5 dB', not '-20 dB'"):
            live_bridge.check_shown("Glue", {"Threshold": "-20 dB"}, {"Threshold": "-19.5 dB"})

    def test_the_marker_finds_a_lag_either_way(self):
        given = np.full((2, 1000), 0.5, dtype=np.float32)
        sent, marker, start = live_bridge.framed(given, RATE, 1000)
        for lag in (0, 17, 4000, -300):
            received = np.roll(sent, lag, axis=1) if lag >= 0 else sent[:, -lag:]
            self.assertEqual(live_bridge.lag_of(received, marker, start, RATE), lag)
            self.assertTrue(np.array_equal(live_bridge.aligned(received, start, lag, 1000), given))

    def test_silence_is_refused(self):
        sent, marker, start = live_bridge.framed(np.zeros((2, 100), dtype=np.float32), RATE, 100)
        with self.assertRaisesRegex(live_bridge.LiveError, "marker didn't come back"):
            live_bridge.lag_of(np.zeros_like(sent), marker, start, RATE)

    def test_the_live_specs_read(self):
        paths = sorted((HERE / "specs").glob("live-*.toml"))
        self.assertTrue(paths)
        for path in paths:
            spec = specs.read(path)
            self.assertTrue(spec.input(spec.cases[0] if spec.cases else None), path.name)
            for case in spec.cases or [None]:
                for name in spec.reference_names:
                    section = spec.reference(case, name)
                    if section.get("live"):
                        self.assertIn(section["live"], ("Glue Compressor", "Pedal"), path.name)
                        for value in section["parameters"].values():
                            self.assertIsInstance(value, (str, int, float, bool), path.name)
            self.assertIn("Ableton Live's", report.source_of(dict(spec._references())[spec.reference_names[0]]))

    def test_a_single_live_reference_is_named_by_its_device(self):
        spec = specs.Spec(path=Path("x.toml"), data={"reference": {"live": "Pedal", "parameters": {"Gain": "50 %"}}})
        self.assertEqual(spec.reference_names, ["Pedal"])
        self.assertEqual(spec.reference(None)["parameters"], {"Gain": "50 %"})


class Install(unittest.TestCase):
    """install.sh against a User Library of its own, never the real one."""

    def setUp(self):
        self.library = Path(tempfile.mkdtemp(prefix="vt-library-"))
        self.environment = {**os.environ, "ABLETON_USER_LIBRARY": str(self.library)}
        self.target = self.library / "Remote Scripts" / "VimtrackerBridge"

    def tearDown(self):
        shutil.rmtree(self.library)

    def run_script(self, *arguments):
        return subprocess.run(["bash", str(HERE / "live" / "install.sh"), *arguments], env=self.environment, capture_output=True, text=True)

    def test_link_and_unlink(self):
        self.assertEqual(self.run_script().returncode, 0)
        self.assertTrue(self.target.is_symlink())
        self.assertEqual(self.target.resolve(), (HERE / "live" / "VimtrackerBridge").resolve())
        self.assertIn("links to", self.run_script("status").stdout)
        self.assertEqual(self.run_script("uninstall").returncode, 0)
        self.assertFalse(self.target.exists() or self.target.is_symlink())
        self.assertTrue((self.library / "Remote Scripts").is_dir())

    def test_copy_and_remove(self):
        self.assertEqual(self.run_script("--copy").returncode, 0)
        self.assertTrue((self.target / "bridge.py").is_file())
        self.assertFalse(self.target.is_symlink())
        self.assertEqual(self.run_script("uninstall").returncode, 0)
        self.assertFalse(self.target.exists())

    def test_someone_elses_folder_is_left_alone(self):
        self.target.mkdir(parents=True)
        (self.target / "theirs.py").write_text("")
        self.assertNotEqual(self.run_script().returncode, 0)
        self.run_script("uninstall")
        self.assertTrue((self.target / "theirs.py").exists())

    def test_no_library(self):
        self.environment["ABLETON_USER_LIBRARY"] = str(self.library / "missing")
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no User Library", result.stderr)


if __name__ == "__main__":
    unittest.main()
