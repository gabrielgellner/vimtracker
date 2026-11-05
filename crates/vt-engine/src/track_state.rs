//! Fixed-size per-track playback state, made ahead of time so the callback never allocates.

use crate::voice::{NoteOn, Voice};

/// One track's voices. A new note moves the old one to `tail`, where it fades.
#[derive(Debug, Clone, Copy, Default)]
pub struct TrackState {
    pub voice: Voice,
    pub tail: Voice,
    /// The instrument a note with no instrument keeps playing.
    pub instrument: Option<u32>,
    /// Live controls, kept here rather than in the song so undo can't change them.
    pub mute: bool,
    pub solo: bool,
}

impl TrackState {
    /// Replace the playing note with a new one.
    pub fn start(&mut self, note: NoteOn, rate: u32) {
        self.cut_to_tail();
        self.voice = Voice::start(note, rate);
    }

    /// Fade the playing note.
    pub fn cut_to_tail(&mut self) {
        if self.voice.active() {
            self.tail = self.voice;
            self.tail.fade();
            self.voice = Voice::default();
        }
    }

    /// Stop sounding, when the transport halts.
    pub fn stop(&mut self) {
        self.cut_to_tail();
    }
}
