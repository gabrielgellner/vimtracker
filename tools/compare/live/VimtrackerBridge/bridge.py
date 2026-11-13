"""The bridge between the comparison harness and Ableton Live: it takes requests on localhost and
does them with Live's Object Model, the API Live gives its control surface scripts (#364).

It keeps two audio tracks in the open set, which it makes if they aren't there:

- `vt bridge source`, which plays the test file as an unwarped audio clip through the device under
  test, its only device;
- `vt bridge sink`, armed, recording the source track's output after its devices (`Post FX`, so
  before its fader and pan), which Live writes to a file.

A request is one line of JSON over TCP on 127.0.0.1, and its answer is one line back,
`{"ok": true, ...}` or `{"ok": false, "error": "..."}`:

- `ping`: the bridge's version, Live's and the set's tempo.
- `load`, with `device`: put a built-in device, by the name Live shows, alone on the source track,
  started fresh, or empty the track when `device` is null. Answers with its parameters.
- `parameters`: the loaded device's parameters, each with its range, items and what it shows.
- `set`, with `parameters`, a table of names and values as Live shows them (`"-20 dB"`, `"4"`,
  `"Overdrive"`, `true`): sets each and answers with what each shows now, and every other one's.
- `render`, with `path` and `seconds`: plays the file through the source track, records the sink
  for that long, and answers with the recording's path once Live has written it.
- `take`, `render take` and `probe take`: read a riff recorded into a clip, play it again and
  record it, or play a probe's notes on its instrument (`takes.py`, #565).

Nothing here waits: Live calls `poll` about ten times a second from its main thread, and a
request that takes time, such as a render, is a generator that yields until it's done. Everything
here uses only the standard library and the objects it's handed, so the tests can drive it with
stand-ins for Live's.
"""

import difflib
import importlib
import json
import os
import socket
import time

from . import takes, values

VERSION = 1
HOST = "127.0.0.1"
PORT = 47364
SOURCE = "vt bridge source"
SINK = "vt bridge sink"
# The sink's input: the source track after its devices, before its mixer.
TAP = "Post FX"
# Track.monitoring_states.OFF and Song's clip quantization `None`, as numbers, for when the
# surface doesn't hand over Live's own.
MONITOR_OFF = 2
NO_QUANTIZATION = 0
# How long Live may take to put a device on a track, and to finish writing a recording, in seconds.
LOAD_SECONDS = 5.0
WRITE_SECONDS = 15.0
# How long a fired empty slot on an armed track may take to start recording, in seconds.
ARM_SECONDS = 2.0


class Refused(Exception):
    """A request the bridge can't do, with why, sent back as its error."""


class Connection:
    def __init__(self, sock):
        self.socket = sock
        self.buffer = b""
        # The request being worked on, a generator, while it takes more than one poll.
        self.job = None


class Bridge:
    """The bridge, given a function that gives Live's current song, its application (for the
    browser and Live's version), a logging function, and Live's own constants when it has them."""

    def __init__(self, song, application=None, log=print, port=PORT, clock=time.monotonic,
                 monitor_off=MONITOR_OFF, no_quantization=NO_QUANTIZATION):
        self.song = song
        self.application = application
        self.log = log
        self.port = port
        self.clock = clock
        self.monitor_off = monitor_off
        self.no_quantization = no_quantization
        self.listener = None
        self.connections = []
        self.rendering = False

    # The server.

    def start(self):
        listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind((HOST, self.port))
        listener.listen(4)
        listener.setblocking(False)
        self.listener = listener
        self.port = listener.getsockname()[1]
        self.log(f"vimtracker bridge {VERSION} listening on {HOST}:{self.port}")

    def stop(self):
        for connection in self.connections:
            connection.socket.close()
        self.connections = []
        if self.listener is not None:
            self.listener.close()
            self.listener = None

    def poll(self):
        """Take new connections, read what's arrived, and move every request on a step."""
        if self.listener is None:
            return
        while True:
            try:
                sock, _ = self.listener.accept()
            except (BlockingIOError, InterruptedError):
                break
            sock.setblocking(False)
            self.connections.append(Connection(sock))
        for connection in list(self.connections):
            try:
                self.step(connection)
            except OSError as error:
                self.log(f"vimtracker bridge: a connection failed: {error}")
                self.close(connection)

    def step(self, connection):
        if connection.job is not None:
            self.advance(connection)
            return
        try:
            data = connection.socket.recv(65536)
        except (BlockingIOError, InterruptedError):
            data = None
        if data == b"":
            self.close(connection)
            return
        if data:
            connection.buffer += data
        if b"\n" in connection.buffer:
            line, connection.buffer = connection.buffer.split(b"\n", 1)
            self.begin(connection, line)

    def begin(self, connection, line):
        try:
            request = json.loads(line.decode("utf-8"))
            if not isinstance(request, dict):
                raise Refused("a request is a JSON object")
            answer = self.handle(request)
        except Refused as error:
            self.answer(connection, {"ok": False, "error": str(error)})
            return
        except Exception as error:  # Live's own errors come back as they are.
            self.answer(connection, {"ok": False, "error": f"{type(error).__name__}: {error}"})
            return
        if hasattr(answer, "send"):
            connection.job = answer
            self.advance(connection)
        else:
            self.answer(connection, {"ok": True, **answer})

    def advance(self, connection):
        try:
            next(connection.job)
        except StopIteration as done:
            connection.job = None
            self.rendering = False
            self.answer(connection, {"ok": True, **(done.value or {})})
        except Refused as error:
            connection.job = None
            self.rendering = False
            self.answer(connection, {"ok": False, "error": str(error)})
        except Exception as error:
            connection.job = None
            self.rendering = False
            self.answer(connection, {"ok": False, "error": f"{type(error).__name__}: {error}"})

    def answer(self, connection, answer):
        sock = connection.socket
        sock.setblocking(True)
        sock.settimeout(2.0)
        try:
            sock.sendall((json.dumps(answer) + "\n").encode("utf-8"))
        finally:
            sock.setblocking(False)

    def close(self, connection):
        connection.socket.close()
        if connection in self.connections:
            self.connections.remove(connection)
        if connection.job is not None:
            self.rendering = False

    def handle(self, request):
        command = request.get("command")
        if command == "ping":
            return {"bridge": VERSION, "live": self.live_version(), "tempo": self.song().tempo}
        if command == "load":
            return self.load(request.get("device"))
        if command == "parameters":
            return {"device": self.describe(self.device())}
        if command == "set":
            return self.set(request.get("parameters") or {})
        if command == "reload":
            return self.reload()
        if command in takes.COMMANDS:
            return takes.handle(self, command, request)
        if command == "render":
            if self.rendering:
                raise Refused("a render is already running")
            self.rendering = True
            return self.render(request["path"], float(request["seconds"]))
        raise Refused(
            f"no command called {command}; there are ping, load, parameters, set, render, reload, take, "
            "render take and probe take"
        )

    def reload(self):
        """Read this module and `values` again from disk, so a change to the bridge takes without
        restarting Live. The listener and the tracks stay as they are; only the code is new."""
        if self.rendering:
            raise Refused("a render is running; reload once it's done")
        importlib.reload(values)
        importlib.reload(takes)
        module = importlib.reload(importlib.import_module(__name__))
        self.__class__ = module.Bridge
        return {"bridge": module.VERSION}

    def live_version(self):
        application = self.application
        try:
            return "{}.{}.{}".format(
                application.get_major_version(), application.get_minor_version(), application.get_bugfix_version()
            )
        except Exception:
            return None

    # The probe's two tracks.

    def tracks(self):
        """The source and sink tracks, made and routed if need be."""
        song = self.song()
        source = self.track(SOURCE)
        sink = self.track(SINK)
        source.current_monitoring_state = self.monitor_off
        if source.arm:
            source.arm = False
        kinds = list(sink.available_input_routing_types)
        kind = pick(kinds, SOURCE)
        if kind is None:
            names = ", ".join(item.display_name for item in kinds)
            raise Refused(f"{SINK} can't take its input from {SOURCE}; it can from {names}")
        if sink.input_routing_type.display_name != kind.display_name:
            sink.input_routing_type = kind
        channels = list(sink.available_input_routing_channels)
        channel = pick(channels, TAP)
        if channel is None:
            names = ", ".join(item.display_name for item in channels)
            raise Refused(f"{SINK} can't take {SOURCE}'s {TAP}; it can take {names}")
        if sink.input_routing_channel.display_name != channel.display_name:
            sink.input_routing_channel = channel
        sink.current_monitoring_state = self.monitor_off
        clear(sink)
        if not sink.arm:
            sink.arm = True
        song.clip_trigger_quantization = self.no_quantization
        return source, sink

    def track(self, name):
        song = self.song()
        for track in song.tracks:
            if track.name == name:
                if not getattr(track, "has_audio_input", True) or not track.can_be_armed:
                    raise Refused(f"the track called {name} isn't an audio track; rename or delete it")
                return track
        song.create_audio_track(len(song.tracks))
        track = song.tracks[-1]
        track.name = name
        self.log(f"vimtracker bridge: made the track {name}")
        return track

    # Devices and their parameters.

    def device(self):
        source = self.track(SOURCE)
        devices = chain(source)
        if not devices:
            raise Refused(f"there's no device on {SOURCE}; load one first")
        return devices[0]

    def load(self, name):
        source, _ = self.tracks()
        if not name:
            clear(source)
            return {"device": None}
        if hasattr(source, "insert_device"):
            clear(source)
            try:
                source.insert_device(name)
            except Exception as error:
                raise Refused(f"Live couldn't put {name} on {SOURCE} ({error}); give the name Live shows")
            return {"device": self.describe(self.device())}
        return self.load_from_browser(source, name)

    def load_from_browser(self, source, name):
        """Before Live 12.3 there's no `insert_device`: the browser loads the device onto the
        selected track, and it appears a moment later."""
        browser = getattr(self.application, "browser", None)
        item = find_in_browser(browser.audio_effects, name) if browser is not None else None
        if item is None:
            if not any(is_called(device, name) for device in chain(source)):
                raise Refused(f"Live's browser has no audio effect called {name}, and {SOURCE} hasn't one")
            clear(source, keep=name)
            return {"device": self.describe(self.device())}
        clear(source)
        self.song().view.selected_track = source
        browser.load_item(item)

        def wait():
            deadline = self.clock() + LOAD_SECONDS
            while not any(is_called(device, name) for device in chain(source)):
                if self.clock() > deadline:
                    raise Refused(f"{name} didn't appear on {SOURCE}")
                yield
            return {"device": self.describe(self.device())}

        return wait()

    def describe(self, device):
        return {
            "name": device.name,
            "class": getattr(device, "class_display_name", device.name),
            "latency": getattr(device, "latency_in_samples", None),
            "parameters": [describe_parameter(parameter) for parameter in device.parameters],
        }

    def set(self, wanted):
        device = self.device()
        parameters = list(device.parameters)
        by_name = {parameter.name: parameter for parameter in parameters}
        chosen = []
        for name, value in wanted.items():
            parameter = by_name.get(name)
            if parameter is None:
                folded = [p for p in parameters if p.name.lower() == str(name).lower()]
                parameter = folded[0] if len(folded) == 1 else None
            if parameter is None:
                near = difflib.get_close_matches(str(name), list(by_name), n=4, cutoff=0.4)
                hint = f"; did you mean {', '.join(near)}?" if near else f"; it has {', '.join(by_name)}"
                raise Refused(f"{device.name} has no parameter called {name}{hint}")
            try:
                target = values.value_for(parameter, value)
            except values.Unsettable as error:
                raise Refused(str(error))
            chosen.append((name, parameter, target))
        for _, parameter, target in chosen:
            parameter.value = target
        return {
            "set": {name: str(parameter) for name, parameter, _ in chosen},
            "shown": {parameter.name: str(parameter) for parameter in parameters},
            "latency": getattr(device, "latency_in_samples", None),
        }

    # Rendering.

    def render(self, path, seconds):
        """Play the file through the source track and record the sink for `seconds`, then wait for
        Live to finish writing the recording."""
        if not os.path.isabs(path) or not os.path.isfile(path):
            raise Refused(f"there's no file at {path}")
        source, sink = self.tracks()
        song = self.song()
        if song.is_playing:
            song.stop_playing()
            yield
        source_slot, sink_slot = source.clip_slots[0], sink.clip_slots[0]
        for slot in (source_slot, sink_slot):
            if slot.has_clip:
                slot.delete_clip()
        source_slot.create_audio_clip(path)
        played = source_slot.clip
        settle(played, "warping", False)
        settle(played, "looping", False)
        # Live says an empty slot will record only once it's been fired, so the sink is fired and
        # then watched until it is recording, before the source starts.
        sink_slot.fire()
        deadline = self.clock() + ARM_SECONDS
        while not (
            sink_slot.is_recording
            or getattr(sink_slot, "is_triggered", False)
            or sink_slot.will_record_on_start
            or sink_slot.has_clip
        ):
            if self.clock() > deadline:
                song.stop_playing()
                raise Refused(f"{SINK} won't record: {recording_state(sink, sink_slot)}")
            yield
        source_slot.fire()
        started = self.clock()
        while self.clock() < started + seconds:
            yield
        song.stop_playing()
        deadline = self.clock() + WRITE_SECONDS
        size = None
        while True:
            yield
            if self.clock() > deadline:
                raise Refused("Live didn't finish writing the recording")
            if not sink_slot.has_clip or sink_slot.is_recording:
                continue
            recorded = sink_slot.clip.file_path
            if not recorded or not os.path.isfile(recorded):
                continue
            now = os.path.getsize(recorded)
            if now > 0 and now == size:
                break
            size = now
        return {
            "recorded": recorded,
            "rate": getattr(sink_slot.clip, "sample_rate", None),
            "played rate": getattr(played, "sample_rate", None),
            "clip gain": getattr(played, "gain_display_string", None),
            "warping": getattr(played, "warping", None),
        }


def recording_state(track, slot):
    """What Live says about the sink, for when its slot won't record."""
    fields = {
        "arm": getattr(track, "arm", None),
        "can be armed": getattr(track, "can_be_armed", None),
        "frozen": getattr(track, "is_frozen", None),
        "input": getattr(getattr(track, "input_routing_type", None), "display_name", None),
        "channel": getattr(getattr(track, "input_routing_channel", None), "display_name", None),
        "monitoring": getattr(track, "current_monitoring_state", None),
        "slot has a clip": getattr(slot, "has_clip", None),
        "slot will record": getattr(slot, "will_record_on_start", None),
        "slot triggered": getattr(slot, "is_triggered", None),
        "slot recording": getattr(slot, "is_recording", None),
        "playing": getattr(getattr(track, "canonical_parent", None), "is_playing", None),
    }
    return ", ".join(f"{name} {value}" for name, value in fields.items())


def chain(track):
    """A track's devices, leaving out its mixer if Live lists it among them."""
    return [device for device in track.devices if getattr(device, "class_name", "") != "MixerDevice"]


def clear(track, keep=None):
    """Take every device off a track but one called `keep`, last first, so the indices hold."""
    for index in reversed(range(len(track.devices))):
        device = track.devices[index]
        if getattr(device, "class_name", "") == "MixerDevice" or (keep and is_called(device, keep)):
            continue
        track.delete_device(index)


def pick(items, name):
    """The routing whose name is `name`, or the one name that ends with it, as Live may number a
    track's name in its lists."""
    for item in items:
        if item.display_name == name:
            return item
    ending = [item for item in items if item.display_name.lower().endswith(name.lower())]
    return ending[0] if len(ending) == 1 else None


def is_called(device, name):
    names = (getattr(device, "class_display_name", None), device.name)
    return any(found and found.lower() == name.lower() for found in names)


def find_in_browser(item, name, depth=3):
    """A loadable browser item called `name`, looking first at each folder's own items, then
    within them, no deeper than `depth`."""
    if item is None or depth < 0:
        return None
    children = list(item.children)
    for child in children:
        if child.is_loadable and child.name.lower() == name.lower():
            return child
    for child in children:
        if child.is_folder or not child.is_loadable:
            found = find_in_browser(child, name, depth - 1)
            if found is not None:
                return found
    return None


def describe_parameter(parameter):
    low, high = float(parameter.min), float(parameter.max)
    out = {
        "name": parameter.name,
        "value": float(parameter.value),
        "min": low,
        "max": high,
        "shown": str(parameter),
        "from": parameter.str_for_value(low),
        "to": parameter.str_for_value(high),
        "quantized": bool(parameter.is_quantized),
    }
    if parameter.is_quantized and parameter.value_items:
        out["items"] = [str(item) for item in parameter.value_items]
    return out


def settle(clip, name, value):
    """Set a clip's property if Live lets it, as some can't be set on some clips."""
    try:
        if getattr(clip, name) != value:
            setattr(clip, name, value)
    except Exception:
        pass
