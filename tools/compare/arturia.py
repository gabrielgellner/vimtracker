"""Starting an Arturia plugin from one of its factory presets, so a render doesn't depend on
whichever preset the plugin last had open.

An Arturia plugin loads with the preset it last had, which can be anything: Acid V here loaded
"Carbon Based", a sequence that plays its own pattern from a held key. A spec's `[reference]` can
name a factory preset, `preset = "Default"`, which is read from Arturia's preset folder and laid
over the plugin's state, along with `state` for the settings that aren't parameters, such as
Acid V's `Sequencer_OnOff = 0`.

The state is the plugin's own saved settings, as a host saves them in a project: a text archive
of named values, which a preset file holds too. This only reads the preset and changes values in
the state by name, as a host restoring a project would. Nothing here looks at the plugin's code.
"""

from __future__ import annotations

import re
import struct
from pathlib import Path

PRESETS = Path("/Library/Arturia/Presets")
# JUCE's base64 alphabet, which pedalboard's VST3 state uses for the plugin's own state.
ALPHABET = ".ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+"
# A named value in the archive: the name's length, the name, and a number.
NAMED = re.compile(rb"(?<= )(\d+) ([A-Za-z][A-Za-z0-9_]*) (-?\d[\d.eE+-]*)(?= )")
# The header pedalboard puts before a VST3 state's XML: a tag and the XML's length.
HEADER = b"VC2!"


def decode(text: str) -> bytes:
    size_text, _, body = text.partition(".")
    size = int(size_text)
    out = bytearray(size)
    position = 0
    for char in body:
        value = ALPHABET.index(char)
        for bit in range(6):
            if value >> bit & 1:
                byte, shift = divmod(position + bit, 8)
                if byte < size:
                    out[byte] |= 1 << shift
        position += 6
    return bytes(out)


def encode(data: bytes) -> str:
    bits = len(data) * 8
    chars = []
    for position in range(0, bits, 6):
        value = 0
        for bit in range(6):
            index = position + bit
            if index < bits and data[index // 8] >> (index % 8) & 1:
                value |= 1 << bit
        chars.append(ALPHABET[value])
    return f"{len(data)}.{''.join(chars)}"


def named_values(archive: bytes) -> dict[str, bytes]:
    """The archive's named values, where a name is as long as its length says."""
    found = {}
    for match in NAMED.finditer(archive):
        if int(match.group(1)) == len(match.group(2)):
            found[match.group(2).decode()] = match.group(3)
    return found


def with_values(archive: bytes, values: dict[str, bytes]) -> bytes:
    def replace(match):
        name = match.group(2).decode()
        if int(match.group(1)) != len(match.group(2)) or name not in values:
            return match.group(0)
        return match.group(1) + b" " + match.group(2) + b" " + values[name]

    return NAMED.sub(replace, archive)


def preset_path(plugin_path: str, name: str) -> Path:
    folder = PRESETS / Path(plugin_path).stem
    found = sorted(path for path in folder.rglob(name) if path.is_file())
    if not found:
        raise SystemExit(f"there's no preset called {name} under {folder}")
    return found[0]


def preset_name(plugin) -> str:
    """The name of the preset the plugin has, from its state."""
    component = _component(plugin.raw_state)
    archive = decode(component)
    match = re.match(rb"\d+ serialization::archive(?: \d+)* (\d+) ", archive)
    if not match:
        return "?"
    length = int(match.group(1))
    return archive[match.end() : match.end() + length].decode("utf-8", "replace")


def _component(state: bytes) -> str:
    xml = state[8:].decode("utf-8")
    return re.search(r"<IComponent>([^<]*)</IComponent>", xml).group(1)


def start_from(plugin, plugin_path: str, preset: str | None, state: dict | None) -> None:
    """Lay a factory preset's values, then `state`'s, over the plugin's state."""
    if not preset and not state:
        return
    raw = plugin.raw_state
    if not raw.startswith(HEADER):
        raise SystemExit(f"{plugin_path}'s state isn't one this can change")
    # The XML's length in the header leaves out the zero that ends it.
    xml = raw[8:].rstrip(b"\x00").decode("utf-8")
    component = _component(raw)
    archive = decode(component)
    values = {}
    if preset:
        values.update(named_values(preset_path(plugin_path, preset).read_bytes()))
    for name, value in (state or {}).items():
        values[name] = str(value).encode()
    known = named_values(archive)
    unknown = [name for name in (state or {}) if name not in known]
    if unknown:
        raise SystemExit(f"{plugin_path}'s state has no {', '.join(unknown)}")
    changed = with_values(archive, values)
    xml = xml.replace(component, encode(changed))
    body = xml.encode("utf-8")
    plugin.raw_state = HEADER + struct.pack("<I", len(body)) + body + b"\x00"
    if preset and preset_name(plugin) == "?":
        raise SystemExit(f"{plugin_path} didn't take its state back")
