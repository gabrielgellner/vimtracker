"""Changing a plugin's saved settings by name before it renders, for settings that aren't
parameters, such as Acid V's sequencer switch or ABL3's MIDI mode.

A VST3 plugin's state, as pedalboard gives it, is a host's XML around the plugin's own state,
which is the plugin's to lay out. Two layouts are known here:

- Arturia's, a text archive of named values, which `arturia.py` reads and changes, and which can
  start from a factory preset.
- JUCE's, an XML element whose attributes are the settings, as AudioRealism's ABL3 keeps them
  (`MidiMode="0"` and so on). A spec's `state` sets the attributes of its outermost element by
  name.

Both are only the settings a host would save in a project, changed by name as a host restoring a
project would. Nothing here looks at a plugin's code.
"""

from __future__ import annotations

import re
import struct

import arturia

# The tag pedalboard and JUCE put before an XML state, then its length as four bytes.
HEADER = b"VC2!"


def _wrapped(body: bytes) -> bytes:
    """XML put back as a state: the tag, the length, the XML and the zero that ends it."""
    return HEADER + struct.pack("<I", len(body)) + body + b"\x00"


def _unwrapped(state: bytes) -> str:
    return state[8:].rstrip(b"\x00").decode("utf-8")


def _juce_settings(plugin) -> tuple[str, str] | None:
    """The host's XML and the plugin's own XML inside it, if the plugin keeps its state as JUCE's
    XML."""
    raw = plugin.raw_state
    if not raw.startswith(HEADER):
        return None
    host = _unwrapped(raw)
    found = re.search(r"<IComponent>([^<]*)</IComponent>", host)
    if not found:
        return None
    inner = arturia.decode(found.group(1))
    if not inner.startswith(HEADER) or b"<?xml" not in inner[:64]:
        return None
    return host, _unwrapped(inner)


def set_juce(plugin, path: str, values: dict) -> None:
    """Set attributes of a JUCE plugin's outermost element by name."""
    host, inner = _juce_settings(plugin)
    root = re.search(r"<(?!\?)[A-Za-z0-9_]+(?:\s+[A-Za-z0-9_]+=\"[^\"]*\")*\s*/?>", inner)
    element = root.group(0)
    changed = element
    unknown = []
    for name, value in values.items():
        text = str(int(value)) if isinstance(value, bool) else str(value)
        changed, count = re.subn(rf'(\s{re.escape(name)}=")[^"]*(")', rf"\g<1>{text}\g<2>", changed, count=1)
        if count == 0:
            unknown.append(name)
    if unknown:
        raise SystemExit(f"{path}'s state has no {', '.join(unknown)}")
    inner = inner[: root.start()] + changed + inner[root.end() :]
    component = re.search(r"<IComponent>([^<]*)</IComponent>", host).group(1)
    host = host.replace(component, arturia.encode(_wrapped(inner.encode("utf-8"))))
    plugin.raw_state = _wrapped(host.encode("utf-8"))


def start_from(plugin, path: str, preset: str | None, state: dict | None) -> None:
    """Lay a factory preset's values, then `state`'s, over the plugin's state, in whichever layout
    it keeps."""
    if not preset and not state:
        return
    if _juce_settings(plugin) is not None:
        if preset:
            raise SystemExit(f"{path} keeps its state as XML, which has no factory presets to start from")
        set_juce(plugin, path, state or {})
        return
    arturia.start_from(plugin, path, preset, state)
