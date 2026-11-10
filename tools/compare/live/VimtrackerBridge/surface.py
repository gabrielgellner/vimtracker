"""The control surface Live runs: it starts the bridge, and polls it each time Live updates its
surfaces' displays, about ten times a second, on Live's main thread, where the Live Object Model
may be used. It has no controls and needs no MIDI ports.
"""

import Live
from _Framework.ControlSurface import ControlSurface

from .bridge import MONITOR_OFF, NO_QUANTIZATION, Bridge


def constant(find, fallback):
    """One of Live's own constants, or the number it stands for if this Live names it otherwise."""
    try:
        return find()
    except Exception:
        return fallback


class Surface(ControlSurface):
    def __init__(self, c_instance):
        ControlSurface.__init__(self, c_instance)
        self._bridge = Bridge(
            song=self.song,
            application=Live.Application.get_application(),
            log=self.log_message,
            monitor_off=constant(lambda: Live.Track.Track.monitoring_states.OFF, MONITOR_OFF),
            no_quantization=constant(lambda: Live.Song.Quantization.q_no_q, NO_QUANTIZATION),
        )
        try:
            self._bridge.start()
        except OSError as error:
            self.log_message(f"vimtracker bridge couldn't listen: {error}")
            self.show_message(f"vimtracker bridge couldn't listen: {error}")
        else:
            self.show_message(f"vimtracker bridge listening on 127.0.0.1:{self._bridge.port}")

    def update_display(self):
        ControlSurface.update_display(self)
        try:
            self._bridge.poll()
        except Exception as error:
            self.log_message(f"vimtracker bridge: {type(error).__name__}: {error}")

    def disconnect(self):
        self._bridge.stop()
        ControlSurface.disconnect(self)
