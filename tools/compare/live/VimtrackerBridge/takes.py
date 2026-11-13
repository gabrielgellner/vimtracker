"""Takes: a riff the author played on an instrument in Live, read back and played again for the
harness to record, so `take.py` can play the same riff on our Acid and compare the two (#565).

A take is a Session View MIDI clip, recorded with Automation Arm on, so its notes and the moves
of the instrument's knobs are in the clip: the notes, and an automation envelope for each
parameter that moved. Three requests work on one:

- `take`, with `clip`, the clip's name, and `step`, in beats: the clip's notes, the instrument's
  parameters as they are, and each envelope read every `step` beats over one pass of the clip
  (`Clip.automation_envelope(parameter).value_at_time`), with the tempo and where the pass starts
  and ends. Nothing in the set changes.
- `render take`, with `clip` and `seconds`: plays the clip from its start and records the track
  it's on after its devices, as `render` records the source track, then puts back everything it
  moved. Its envelopes play the instrument's knobs as they were recorded: automation a knob
  touched since has overridden is enabled again first, as Live's Re-Enable Automation does.
- `probe take`, with `track`, `notes` (each `pitch`, `start`, `duration` and `velocity`, in
  beats), `length` in beats and `seconds`: puts the notes in a new clip in the track's first empty
  slot, records it as `render take` does, and deletes the clip, for finding out by its output what
  the instrument makes of a velocity or of notes that overlap.

While the harness records, the master's fader is at its bottom, so nothing is heard; the
recording is taken before the track's own fader and the master, so it's the same either way. The
track is disarmed and Session Record is off while it plays, so playing the clip never records
into it, and its arm, the master and Session Record are put back however the render ends.

Arrangement View's recorded automation is the track's, not a clip's, and Live gives a script no
way to read it, so a take is recorded into a Session View clip.
"""

from . import bridge as bridges

# The most points one envelope may be read at, so a request can't ask for millions.
MOST_POINTS = 65536
# The requests this module answers.
COMMANDS = ("take", "render take", "probe take")


def handle(bridge, command, request):
    """Answer one of `COMMANDS`; a render is a generator the bridge moves on as Live polls it."""
    if command == "take":
        return export(bridge.song(), request.get("clip"), request.get("step", 1 / 64), bridge.live_version())
    if bridge.rendering:
        raise bridges.Refused("a render is already running")
    if command == "render take":
        job = render(bridge, request.get("clip"), float(request["seconds"]))
    else:
        job = probe(bridge, request.get("track"), request["notes"], request["length"], float(request["seconds"]),
                    getattr(bridge, "make_note", None))
    bridge.rendering = True
    return job


def find_clip(song, name):
    """The track, slot and Session View clip called `name`, refusing if there are none or several,
    and naming an Arrangement View clip of that name as the wrong kind."""
    found = []
    for track in song.tracks:
        for slot in track.clip_slots:
            if slot.has_clip and slot.clip.name == name:
                found.append((track, slot, slot.clip))
    if len(found) > 1:
        tracks = ", ".join(track.name for track, _, _ in found)
        raise bridges.Refused(f"there are {len(found)} clips called {name}, on {tracks}; rename all but one")
    if found:
        track, slot, clip = found[0]
        if not getattr(clip, "is_midi_clip", True):
            raise bridges.Refused(f"the clip called {name} is an audio clip; a take is a MIDI clip")
        return found[0]
    for track in song.tracks:
        for clip in getattr(track, "arrangement_clips", []):
            if clip.name == name:
                raise bridges.Refused(
                    f"{name} is in Arrangement View, whose automation Live doesn't let a script read; "
                    "record the take into a Session View clip slot"
                )
    raise bridges.Refused(f"there's no Session View clip called {name}")


def instrument(track):
    """The track's instrument, the first device that plays notes, or its first device."""
    devices = bridges.chain(track)
    if not devices:
        raise bridges.Refused(f"{track.name} has no instrument on it")
    # Live's `Device.type`: 1 is an instrument, 2 an audio effect and 4 a MIDI effect.
    return next((device for device in devices if getattr(device, "type", None) == 1), devices[0])


def window(clip):
    """Where one pass of a clip starts and ends, in beats of the clip's own time: from its start
    marker to its loop's end if it loops, or to its end marker."""
    start = float(clip.start_marker)
    end = float(clip.loop_end if clip.looping else clip.end_marker)
    if end <= start:
        raise bridges.Refused(f"{clip.name} plays nothing: it starts at beat {start:g} and ends at {end:g}")
    return start, end


def notes_of(clip, start, end):
    """The clip's notes between two times, each a table, its start from `start`, in beats."""
    span = end - start
    if hasattr(clip, "get_notes_extended"):
        found = clip.get_notes_extended(0, 128, start, span)
        out = [
            {
                "pitch": int(note.pitch),
                "start": float(note.start_time) - start,
                "duration": float(note.duration),
                "velocity": float(note.velocity),
                "mute": bool(note.mute),
            }
            for note in found
        ]
    else:
        out = [
            {"pitch": int(pitch), "start": float(time) - start, "duration": float(length), "velocity": float(velocity), "mute": bool(mute)}
            for pitch, time, length, velocity, mute in clip.get_notes(start, 0, span, 128)
        ]
    return sorted(out, key=lambda note: (note["start"], note["pitch"]))


def export(song, name, step, live=None):
    """Everything a take is: its clip's notes, its instrument's parameters and their envelopes."""
    step = float(step)
    if step <= 0:
        raise bridges.Refused("`step` is a number of beats over 0")
    track, _, clip = find_clip(song, name)
    start, end = window(clip)
    points = int((end - start) / step) + 1
    if points > MOST_POINTS:
        raise bridges.Refused(f"{name} would be read at {points} points; give a longer `step`")
    device = instrument(track)
    parameters, envelopes = [], {}
    for parameter in device.parameters:
        parameters.append(bridges.describe_parameter(parameter))
        envelope = clip.automation_envelope(parameter)
        if envelope is None:
            continue
        envelopes[parameter.name] = [float(envelope.value_at_time(start + index * step)) for index in range(points)]
    return {
        "clip": clip.name,
        "track": track.name,
        "device": device.name,
        "device class": getattr(device, "class_display_name", device.name),
        "live": live,
        "tempo": float(song.tempo),
        "signature": [int(getattr(song, "signature_numerator", 4)), int(getattr(song, "signature_denominator", 4))],
        "start": start,
        "end": end,
        "looping": bool(clip.looping),
        "step": step,
        "notes": notes_of(clip, start, end),
        "parameters": parameters,
        "envelopes": envelopes,
    }


class Silenced:
    """The master's fader at its bottom and the take's track disarmed, with Session Record off,
    and how to put them back."""

    def __init__(self, song, track):
        self.song, self.track = song, track
        volume = song.master_track.mixer_device.volume
        self.volume = volume, volume.value
        self.arm = track.arm if getattr(track, "can_be_armed", False) else None
        self.session_record = getattr(song, "session_record", None)
        volume.value = volume.min
        if self.arm:
            track.arm = False
        if self.session_record:
            song.session_record = False

    def restore(self):
        volume, value = self.volume
        volume.value = value
        if self.arm:
            self.track.arm = True
        if self.session_record:
            self.song.session_record = True


def route_sink(bridge, track):
    """The sink, armed and recording `track` after its devices, as it records the source."""
    sink = bridge.track(bridges.SINK)
    kinds = list(sink.available_input_routing_types)
    kind = bridges.pick(kinds, track.name)
    if kind is None:
        names = ", ".join(item.display_name for item in kinds)
        raise bridges.Refused(f"{bridges.SINK} can't take its input from {track.name}; it can from {names}")
    if sink.input_routing_type.display_name != kind.display_name:
        sink.input_routing_type = kind
    channels = list(sink.available_input_routing_channels)
    channel = bridges.pick(channels, bridges.TAP)
    if channel is None:
        names = ", ".join(item.display_name for item in channels)
        raise bridges.Refused(f"{bridges.SINK} can't take {track.name}'s {bridges.TAP}; it can take {names}")
    if sink.input_routing_channel.display_name != channel.display_name:
        sink.input_routing_channel = channel
    sink.current_monitoring_state = bridge.monitor_off
    bridges.clear(sink)
    if not sink.arm:
        sink.arm = True
    bridge.song().clip_trigger_quantization = bridge.no_quantization
    return sink


def record(bridge, track, slot, seconds):
    """Play a clip slot and record its track for `seconds`, silent, then wait for the recording."""
    song = bridge.song()
    sink = route_sink(bridge, track)
    if song.is_playing:
        song.stop_playing()
        yield
    sink_slot = sink.clip_slots[0]
    if sink_slot.has_clip:
        sink_slot.delete_clip()
    silenced = Silenced(song, track)
    try:
        sink_slot.fire()
        deadline = bridge.clock() + bridges.ARM_SECONDS
        while not (sink_slot.is_recording or getattr(sink_slot, "is_triggered", False) or sink_slot.will_record_on_start or sink_slot.has_clip):
            if bridge.clock() > deadline:
                song.stop_playing()
                raise bridges.Refused(f"{bridges.SINK} won't record: {bridges.recording_state(sink, sink_slot)}")
            yield
        slot.fire()
        started = bridge.clock()
        while bridge.clock() < started + seconds:
            yield
        song.stop_playing()
        deadline = bridge.clock() + bridges.WRITE_SECONDS
        size = None
        while True:
            yield
            if bridge.clock() > deadline:
                raise bridges.Refused("Live didn't finish writing the recording")
            if not sink_slot.has_clip or sink_slot.is_recording:
                continue
            recorded = sink_slot.clip.file_path
            if not recorded:
                continue
            now = file_size(recorded)
            if now and now == size:
                break
            size = now
    finally:
        silenced.restore()
    return {"recorded": recorded, "rate": getattr(sink_slot.clip, "sample_rate", None), "track": track.name}


def file_size(path):
    import os

    return os.path.getsize(path) if os.path.isfile(path) else None


def render(bridge, name, seconds):
    song = bridge.song()
    track, slot, _ = find_clip(song, name)
    # A knob touched since the take was recorded overrides its envelope until automation is
    # enabled again, as Live's Re-Enable Automation button does, so the take plays as recorded.
    if hasattr(song, "re_enable_automation"):
        song.re_enable_automation()
    return record(bridge, track, slot, float(seconds))


def probe(bridge, track_name, notes, length, seconds, make_note=None):
    """Play notes from a clip made for them in the track's first empty slot, then delete it."""
    song = bridge.song()
    tracks = [track for track in song.tracks if track.name == track_name]
    if len(tracks) != 1:
        raise bridges.Refused(f"there's no one track called {track_name}")
    track = tracks[0]
    slot = next((slot for slot in track.clip_slots if not slot.has_clip), None)
    if slot is None:
        raise bridges.Refused(f"{track_name} has no empty clip slot for the probe's notes")
    slot.create_clip(float(length))
    clip = slot.clip
    try:
        clip.name = "vt bridge probe"
        clip.looping = False
        if make_note is not None and hasattr(clip, "add_new_notes"):
            clip.add_new_notes(tuple(make_note(note) for note in notes))
        else:
            clip.set_notes(tuple((int(n["pitch"]), float(n["start"]), float(n["duration"]), float(n["velocity"]), False) for n in notes))
        answer = yield from record(bridge, track, slot, float(seconds))
    finally:
        if slot.has_clip:
            slot.delete_clip()
    return answer
