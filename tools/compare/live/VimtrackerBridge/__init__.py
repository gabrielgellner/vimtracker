"""vimtracker's bridge to Ableton Live, a control surface script that lets the comparison harness
in `tools/compare` load Live's built-in devices, set them and render through them (#364).

Live loads it from `Remote Scripts/VimtrackerBridge` in its User Library, which
`tools/compare/live/install.sh` links here, and runs it once it's chosen as a control surface in
Settings, Link, Tempo & MIDI. The surface lives in `surface.py`, which needs Live's own modules;
`bridge.py`, which does the work, doesn't, so it can be tested without Live.
"""


def create_instance(c_instance):
    """What Live calls to start a control surface script."""
    from .surface import Surface

    return Surface(c_instance)
