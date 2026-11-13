"""Takes without Live (#565): reading a clip, playing it again and probing an instrument through the
bridge, against stand-ins for Live's MIDI clips, envelopes and mixer; and what the harness makes
of a take, its accents and slides, and its rows and lanes for our song.

    cd tools/compare && uv run python -m unittest discover tests
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np
from scipy.io import wavfile

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import live_bridge  # noqa: E402
import take  # noqa: E402
import take_rows  # noqa: E402
import test_live_bridge as stand_ins  # noqa: E402
from VimtrackerBridge import bridge  # noqa: E402

RATE = stand_ins.RATE


class Envelope:
    def __init__(self, shape):
        self.shape = shape

    def value_at_time(self, time):
        return self.shape(time)


class MidiNote:
    def __init__(self, pitch, start_time, duration, velocity, mute=False):
        self.pitch, self.start_time, self.duration, self.velocity, self.mute = pitch, start_time, duration, velocity, mute


class MidiClip:
    is_midi_clip = True

    def __init__(self, name, notes, length, envelopes=None, start=0.0):
        self.name = name
        self.notes = [MidiNote(*note) for note in notes]
        self.looping = True
        self.start_marker = self.loop_start = start
        self.loop_end = self.end_marker = start + length
        self.envelopes = envelopes or {}

    def get_notes_extended(self, from_pitch, pitch_span, from_time, time_span):
        return tuple(n for n in self.notes if from_time <= n.start_time < from_time + time_span and from_pitch <= n.pitch < from_pitch + pitch_span)

    def automation_envelope(self, parameter):
        shape = self.envelopes.get(parameter.name)
        return None if shape is None else Envelope(shape)

    def set_notes(self, notes):
        self.notes.extend(MidiNote(*note) for note in notes)

    def add_new_notes(self, specifications):
        self.notes.extend(MidiNote(s["pitch"], s["start"], s["duration"], s["velocity"]) for s in specifications)


class TakeSlot(stand_ins.ClipSlot):
    def create_clip(self, length):
        assert self.clip is None
        self.clip = MidiClip("", [], length)


class Instrument(stand_ins.Device):
    type = 1


def acid_v():
    knob = lambda value: f"{value:.2f}"  # noqa: E731
    return Instrument("Acid V", [
        stand_ins.Parameter("Device On", stand_ins.listed(["Off", "On"]), 0, 1, 1, items=["Off", "On"]),
        stand_ins.Parameter("Cutoff", knob, value=0.5),
        stand_ins.Parameter("Resonance", knob, value=0.25),
        stand_ins.Parameter("Envelope Mod", knob, value=0.5),
        stand_ins.Parameter("Osc Pitch", knob, value=0.5),
    ])


class MidiTrack(stand_ins.Track):
    def __init__(self, song, name):
        super().__init__(song, name)
        self.has_audio_input = False
        self.clip_slots = [TakeSlot(self) for _ in range(8)]
        self.devices = [acid_v()]
        self.arm = True


class Volume:
    def __init__(self):
        self.min, self.max, self.value = 0.0, 1.0, 0.85


class Master:
    def __init__(self):
        self.mixer_device = type("Mixer", (), {"volume": Volume()})()


class Song(stand_ins.Song):
    """A set with a MIDI track, whose recording is a sine for each note of the clip it plays,
    `delay` frames late, and which notes whether the master was down while it recorded."""

    def __init__(self, folder):
        super().__init__(folder)
        self.master_track = Master()
        self.session_record = True
        self.tracks.insert(0, MidiTrack(self, "Acid"))
        self.master_while_recording = []
        self.armed_while_recording = []
        self.re_enabled = 0

    def re_enable_automation(self):
        self.re_enabled += 1

    def stop_playing(self):
        self.is_playing = False
        for sink in self.tracks:
            slot = sink.clip_slots[0]
            if not slot.is_recording:
                continue
            source = next(t for t in self.tracks if t.name == sink.input_routing_type.display_name)
            assert sink.input_routing_channel.display_name == "Post FX"
            played = next(s for s in source.clip_slots if s.fired)
            self.master_while_recording.append(self.master_track.mixer_device.volume.value)
            self.armed_while_recording.append((source.arm, self.session_record))
            seconds_per_beat = 60.0 / self.tempo
            clip = played.clip
            frames = int((clip.loop_end - clip.start_marker) * seconds_per_beat * RATE) + RATE // 2
            out = np.zeros(frames)
            for note in clip.notes:
                start = int((note.start_time - clip.start_marker) * seconds_per_beat * RATE)
                length = int(note.duration * seconds_per_beat * RATE)
                t = np.arange(length) / RATE
                out[start : start + length] += 0.2 * np.sin(2 * np.pi * 440 * 2 ** ((note.pitch - 69) / 12) * t)
            out = np.pad(out, (self.delay, 0))
            self.recordings += 1
            path = self.folder / f"recorded-{self.recordings}.wav"
            wavfile.write(path, RATE, np.ascontiguousarray(np.vstack([out, out]).T, dtype=np.float32))
            slot.is_recording = False
            slot.clip = stand_ins.Clip(str(path))
            played.fired = False


NOTES = [(48, 0.0, 0.25, 90), (51, 0.5, 0.4, 110), (53, 0.75, 0.25, 90), (36, 1.0, 0.25, 64, True)]


class Bridge(unittest.TestCase):
    def setUp(self):
        self.folder = Path(tempfile.mkdtemp(prefix="vt-take-test-"))
        self.song = Song(self.folder)
        self.clip = MidiClip("riff", NOTES, 2.0, {"Cutoff": lambda beat: 0.25 + 0.25 * beat})
        self.song.tracks[0].clip_slots[1].clip = self.clip
        self.running = stand_ins.Running(self.song)
        self.address, self.takes = live_bridge.ADDRESS, take.TAKES
        live_bridge.ADDRESS = self.running.address
        take.TAKES = self.folder / "takes"

    def tearDown(self):
        live_bridge.ADDRESS, take.TAKES = self.address, self.takes
        self.running.stop()
        shutil.rmtree(self.folder)

    def test_a_take_is_read_from_its_clip(self):
        answer = live_bridge.request("take", clip="riff", step=0.25)
        self.assertEqual(answer["track"], "Acid")
        self.assertEqual(answer["device"], "Acid V")
        self.assertEqual((answer["start"], answer["end"], answer["tempo"]), (0.0, 2.0, 120.0))
        self.assertEqual([n["pitch"] for n in answer["notes"]], [48, 51, 53, 36])
        self.assertTrue(answer["notes"][3]["mute"])
        self.assertEqual(list(answer["envelopes"]), ["Cutoff"])
        self.assertEqual(answer["envelopes"]["Cutoff"], [0.25 + 0.25 * 0.25 * k for k in range(9)])
        names = [p["name"] for p in answer["parameters"]]
        self.assertIn("Resonance", names)

    def test_a_take_starts_at_its_start_marker(self):
        self.clip.start_marker = 0.5
        answer = live_bridge.request("take", clip="riff", step=0.5)
        self.assertEqual([n["start"] for n in answer["notes"]], [0.0, 0.25, 0.5])
        self.assertEqual(answer["envelopes"]["Cutoff"][0], 0.375)

    def test_what_isnt_a_take_is_refused(self):
        with self.assertRaisesRegex(live_bridge.LiveError, "no Session View clip called nothing"):
            live_bridge.request("take", clip="nothing")
        self.song.tracks[1].arrangement_clips = [MidiClip("arranged", [], 1.0)]
        with self.assertRaisesRegex(live_bridge.LiveError, "Arrangement View"):
            live_bridge.request("take", clip="arranged")

    def test_a_take_plays_again_silent_and_puts_back_what_it_moved(self):
        answer = live_bridge.request("render take", clip="riff", seconds=1.5, timeout=30)
        self.assertTrue(Path(answer["recorded"]).exists())
        sink = next(t for t in self.song.tracks if t.name == bridge.SINK)
        self.assertEqual(sink.input_routing_type.display_name, "Acid")
        self.assertEqual(self.song.master_while_recording, [0.0])
        self.assertEqual(self.song.armed_while_recording, [(False, False)])
        self.assertEqual(self.song.master_track.mixer_device.volume.value, 0.85)
        self.assertTrue(self.song.tracks[0].arm)
        self.assertTrue(self.song.session_record)
        self.assertEqual(self.song.re_enabled, 1)

    def test_a_probe_plays_from_a_clip_it_then_deletes(self):
        notes = [{"pitch": 60, "start": 0.0, "duration": 0.5, "velocity": 100}]
        answer = live_bridge.request("probe take", track="Acid", notes=notes, length=1.0, seconds=1.0, timeout=30)
        _, recorded = wavfile.read(answer["recorded"])
        self.assertGreater(np.abs(recorded).max(), 0.1)
        self.assertFalse(self.song.tracks[0].clip_slots[0].has_clip)
        self.assertIs(self.song.tracks[0].clip_slots[1].clip, self.clip)

    def test_export_and_render_live_line_up_with_the_take(self):
        take.export("riff")
        path = take.render_live("riff", tail=0.25)
        _, samples = wavfile.read(path)
        found = take.Take.read(take.folder_of("riff"))
        self.assertEqual(samples.shape[0], int(round(found.duration * RATE)))
        # The first note starts where the take has it, Live's delay taken out.
        first = int(np.argmax(np.abs(samples[:, 0]) > 1e-3))
        self.assertLessEqual(first, 2)


def a_take(notes, envelopes=None, parameters=None, tempo=120.0, length=4.0, midi=None):
    data = {
        "clip": "test", "tempo": tempo, "start": 0.0, "end": length, "step": 0.25,
        "notes": [{"pitch": p, "start": s, "duration": d, "velocity": v, "mute": False} for p, s, d, v in notes],
        "parameters": parameters or [{"name": "Cutoff", "value": 0.5, "min": 0.0, "max": 1.0, "shown": "0.5"}],
        "envelopes": envelopes or {},
    }
    return take.Take(data, midi=midi or {})


class Classify(unittest.TestCase):
    def test_accent_from_the_threshold_and_slide_from_an_overlap(self):
        found = a_take([(48, 0.0, 0.3, 99), (50, 0.25, 0.25, 100), (52, 0.5, 0.25, 127)])
        self.assertEqual([n.accent for n in found.notes], [False, True, True])
        self.assertEqual([n.slide for n in found.notes], [True, False, False])
        # Notes that only touch don't slide; a probe that found a wider overlap moves the line.
        found = a_take([(48, 0.0, 0.25, 99), (50, 0.25, 0.26, 99), (52, 0.5, 0.25, 99)], midi={"slide from seconds": 0.01})
        self.assertEqual([n.slide for n in found.notes], [False, False, False])
        self.assertFalse(a_take([(48, 0.0, 0.25, 100)], midi={"accent from": 101}).notes[0].accent)

    def test_knob_values_between_the_points_read(self):
        found = a_take([(48, 0.0, 0.25, 99)], envelopes={"Cutoff": [0.0, 1.0, 1.0]})
        self.assertAlmostEqual(found.value_at("Cutoff", 0.125), 0.5)
        self.assertEqual(found.moved(), ["Cutoff"])
        self.assertEqual(found.static("Cutoff"), 0.0)

    def test_what_ours_cant_follow_is_flagged(self):
        parameters = [{"name": "Distortion Dry/Wet", "value": 0.4, "min": 0.0, "max": 1.0, "shown": "40 %"}]
        found = a_take([(48, 0.0, 0.25, 99)], envelopes={"Brightness": [0.1, 0.9]}, parameters=parameters)
        flags = " ".join(found.flags())
        self.assertIn("Brightness", flags)
        self.assertIn("Distortion Dry/Wet", flags)


class Rows(unittest.TestCase):
    def test_notes_land_on_rows_with_their_nudges_gates_accents_and_slides(self):
        found = a_take([
            (48, 0.0, 0.125, 99),              # half a step
            (60, 0.26, 0.6, 112),              # nudged late, sliding into the next
            (55, 0.75, 0.25 * 3.5, 90),        # three and a half steps
            (50, 2.0, 0.25 * 11, 90),          # over eight steps, ended by an OFF
        ], length=8.0)
        layout = take_rows.layout(found)
        self.assertEqual(layout.transpose, -12)
        steps = layout.steps
        self.assertEqual(steps[0], "C-1 63 SGL1 80")
        self.assertEqual(steps[1], "C-2 7F +1 HLD2")
        self.assertEqual(steps[2], "G-1 5A SGL4 80")
        self.assertEqual(steps[3], "--- --")
        self.assertEqual(steps[4], "D-1 5A SGL8 FF")
        # Eight steps on from step 8 is step 16; the OFF lands where the note ends, step 19.
        self.assertEqual(steps[5:8], ["--- --"] * 3)
        self.assertEqual(steps[8], "OFF --")
        self.assertEqual(layout.rows, 9 + 32 - 20)
        self.assertEqual(len(steps), 32)

    def test_a_note_too_low_for_us_is_left_out(self):
        layout = take_rows.layout(a_take([(20, 0.0, 0.25, 99), (48, 0.25, 0.25, 99)]))
        self.assertTrue(layout.steps[0].startswith("--- --"))
        self.assertIn("out of our range", " ".join(layout.warnings))

    def test_moving_knobs_take_a_lane_each_and_glide(self):
        envelopes = {"Cutoff": list(np.linspace(0.0, 1.0, 17)), "Resonance": [0.5] * 8 + [1.0] * 9}
        layout = take_rows.layout(a_take([(48, 0.0, 0.25, 99)], envelopes=envelopes))
        self.assertEqual(layout.lanes, 2)
        cutoff, resonance = layout.columns
        # A straight sweep keeps only its ends.
        # It reaches the top at the take's end, after its last step, 15/16 of the way: EF10.
        self.assertEqual([c for c in cutoff if c != "---"], ["acid.cutoff:00g", "acid.cutoff:EF10h"])
        self.assertEqual(cutoff.index("acid.cutoff:EF10h"), 15)
        kept = [(at, c) for at, c in enumerate(resonance) if c != "---"]
        self.assertEqual(kept[0], (0, "acid.resonance:7F80g"))
        self.assertEqual(kept[-1], (15, "acid.resonance:FFh"))
        self.assertEqual(layout.settings["acid.cutoff"], [0, 0])
        self.assertEqual(layout.steps[1], "--- -- --- ---")

    def test_more_than_three_knobs_share_the_columns(self):
        envelopes = {name: list(np.linspace(0.1, 0.9, 17) ** (1 + index)) for index, name in enumerate(take.KNOBS)}
        layout = take_rows.layout(a_take([(48, 0.0, 0.25, 99)], envelopes=envelopes))
        self.assertEqual(layout.lanes, 3)
        first = [c for c in layout.columns[0] if c != "---"]
        self.assertTrue(all(c.startswith(("acid.cutoff:", "acid.decay:")) for c in first))
        self.assertTrue(any(c.startswith("acid.decay:") for c in first))
        self.assertTrue(all(layout.columns[0][at] == "---" or layout.columns[0][at].startswith("acid.cutoff:") for at in range(0, 16, 2)))

    def test_the_rows_file_reads_as_toml(self):
        import tomllib

        found = a_take([(48, 0.0, 0.25, 99)], envelopes={"Cutoff": [0.2, 0.8]})
        layout = take_rows.layout(found)
        read = tomllib.loads(take_rows.toml_of(layout, found))
        self.assertEqual(read["rows"], layout.rows)
        self.assertEqual(read["settings"]["acid.cutoff"], [51, 0])


@unittest.skipUnless(take.SONG_BUILDER.exists() and take.BINARY.exists(), "needs cargo build --release, and its take_song example")
class OurSong(unittest.TestCase):
    def test_our_song_builds_and_plays(self):
        folder = Path(tempfile.mkdtemp(prefix="vt-take-song-"))
        try:
            found = a_take([(48, 0.0, 0.3, 99), (55, 0.25, 0.25, 112), (48, 1.0, 1.0, 90)],
                           envelopes={"Cutoff": list(np.linspace(0.2, 0.8, 17))})
            project = take.build_song(found, folder)
            take.render_ours(found, project, folder / "ours.wav", 16)
            rate, samples = wavfile.read(folder / "ours.wav")
            self.assertEqual(rate, RATE)
            self.assertEqual(samples.shape[0], int(16 * 0.125 * RATE))
            self.assertGreater(np.abs(samples).max(), 0.01)
            listed = subprocess.run([str(take.BINARY), "render", str(project), str(folder / "again.wav"), "--rows", "16"], capture_output=True)
            self.assertEqual(listed.returncode, 0)
        finally:
            shutil.rmtree(folder)


if __name__ == "__main__":
    unittest.main()
