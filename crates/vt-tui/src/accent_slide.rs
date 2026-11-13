//! Accent, slide and a one-shot octave from the Note column in Insert, as a 303's ACCENT, SLIDE,
//! UP and DOWN keys are pressed with a note or after it. `'`, `/`, `k` and `,` tapped after a note
//! switch it on or off on the row just written, and held while notes are played, write them with
//! it. None needs a new notation: an accent is a velocity of [`ACCENT_VELOCITY`] or more, a slide
//! is `HLD` on the gate (decision 46), and an octave is the note itself, so all work on every
//! track.

use ratatui::crossterm::event::KeyCode;
use vt_model::action::Action;
use vt_model::constants::DEFAULT_NOTE_VELOCITY;
use vt_model::selection::Field;
use vt_model::step::{
    ACCENT_VELOCITY, GateKind, MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Note, Step, slid,
};

use crate::app::{App, Cursor};
use crate::input::bindings::{Cmd, Resolved};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};

/// The velocity an accent writes: full, as a 303's accent is all or nothing.
pub const ACCENTED: u8 = MAX_VELOCITY;

/// What a key puts on a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Accent,
    Slide,
    OctaveUp,
    OctaveDown,
}

impl Flag {
    fn name(self) -> &'static str {
        match self {
            Self::Accent => "Accent",
            Self::Slide => "Slide",
            Self::OctaveUp | Self::OctaveDown => "Octave",
        }
    }

    /// The octaves it moves a note by, if it's one of the octave keys.
    fn octaves(self) -> i8 {
        match self {
            Self::OctaveUp => 1,
            Self::OctaveDown => -1,
            Self::Accent | Self::Slide => 0,
        }
    }
}

/// A key held for a flag, and whether a note has been played while it's down, which makes it a
/// hold rather than a tap.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HeldFlag {
    key: KeyCode,
    flag: Flag,
    used: bool,
}

/// Where the last note played in Insert went, and where the cursor was left after it, so a flag
/// tapped next goes on that row however far auto-jump moved on.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Written {
    pub(crate) track: usize,
    pub(crate) phrase: u32,
    pub(crate) step: usize,
    pub(crate) cursor: Cursor,
}

/// A row whose note a tapped octave key moved, how far, and the note it left, so the same key
/// tapped again puts it back. It's forgotten once the row's note is anything else.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Moved {
    pub(crate) row: (usize, u32, usize),
    pub(crate) octaves: i8,
    pub(crate) note: u8,
}

impl App {
    /// The flag a key in Insert's Note column puts on, from the binding table.
    pub(crate) fn flag_key(&self, key: Key) -> Option<Flag> {
        match self.table.resolve(Mode::Insert, &[key]) {
            Resolved::Command(Cmd::Accent) => Some(Flag::Accent),
            Resolved::Command(Cmd::Slide) => Some(Flag::Slide),
            Resolved::Command(Cmd::OctaveUp) => Some(Flag::OctaveUp),
            Resolved::Command(Cmd::OctaveDown) => Some(Flag::OctaveDown),
            _ => None,
        }
    }

    /// Whether this key is holding a flag, so its repeats, and kitty's presses of it, do nothing.
    pub(crate) fn holds_flag(&self, key: Key) -> bool {
        self.held_flags
            .iter()
            .any(|held| held.key == key.physical())
    }

    /// A flag's key went down. What it does waits for what comes next: a note played while it's
    /// down is written with it, and with none, letting go switches it on the row just written.
    pub(crate) fn hold_flag(&mut self, key: Key, flag: Flag) {
        self.held_flags.push(HeldFlag {
            key: key.physical(),
            flag,
            used: false,
        });
        self.repeats_ignored = true;
    }

    /// A key let go. If it held a flag and no note was played meanwhile, it was a tap.
    pub(crate) fn let_go_of_flag(&mut self, key: Key) {
        let Some(at) = self
            .held_flags
            .iter()
            .position(|held| held.key == key.physical())
        else {
            return;
        };
        let held = self.held_flags.remove(at);
        let writing = self.mode == Mode::Insert
            && self.view == View::Phrase
            && self.cursor.field == Field::Note;
        if held.used || !writing {
            return;
        }
        if self.recording.is_some() {
            // Recording writes a note only once its row starts, so there's no row to switch yet.
            self.status = format!(
                "While recording, hold {} as you play a note",
                self.flag_key_name(held.flag)
            );
        } else {
            self.toggle_flag(held.flag);
        }
    }

    /// Let go of every flag, as leaving Insert or losing focus does, doing nothing.
    pub(crate) fn drop_flags(&mut self) {
        self.held_flags.clear();
    }

    /// Whether this flag's key is down.
    fn flag_held(&self, flag: Flag) -> bool {
        self.held_flags.iter().any(|held| held.flag == flag)
    }

    /// The velocity a note played now takes: the accent's while its key is down.
    pub(crate) fn flagged_velocity(&self, velocity: u8) -> u8 {
        if self.flag_held(Flag::Accent) {
            ACCENTED
        } else {
            velocity
        }
    }

    /// Whether a note played now slides into the next.
    pub(crate) fn flagged_slide(&self) -> bool {
        self.flag_held(Flag::Slide)
    }

    /// The note played now goes in: an octave up while `k` is down, and down while `,` is, unless
    /// that's out of range. Both down move it neither way.
    pub(crate) fn flagged_note(&self, midi: u8) -> u8 {
        let octaves = [Flag::OctaveUp, Flag::OctaveDown]
            .into_iter()
            .filter(|flag| self.flag_held(*flag))
            .map(Flag::octaves)
            .sum();
        moved_by(midi, octaves).unwrap_or(midi)
    }

    /// A note was played with these flags' keys down, so each counts as held rather than tapped,
    /// and the status line says what the note got.
    pub(crate) fn use_flags(&mut self) {
        if self.held_flags.is_empty() {
            return;
        }
        for held in &mut self.held_flags {
            held.used = true;
        }
        let flags = match (self.flag_held(Flag::Accent), self.flag_held(Flag::Slide)) {
            (true, true) => Some("Accent and slide on"),
            (true, false) => Some("Accent on"),
            (false, true) => Some("Slide on"),
            (false, false) => None,
        };
        let octave = match (
            self.flag_held(Flag::OctaveUp),
            self.flag_held(Flag::OctaveDown),
        ) {
            (true, false) => Some("up"),
            (false, true) => Some("down"),
            _ => None,
        };
        self.status = match (flags, octave) {
            (Some(flags), Some(way)) => format!("{flags}, octave {way}"),
            (Some(flags), None) => flags.to_owned(),
            (None, Some(way)) => format!("Octave {way}"),
            (None, None) => "Octave as played".to_owned(),
        };
    }

    /// The row a tapped flag goes on: the one the last note went on, if the cursor is still where
    /// that left it, and otherwise the cursor's.
    fn flag_target(&self) -> Option<(usize, u32, usize)> {
        match self.last_written {
            Some(written) if written.cursor == self.cursor => {
                Some((written.track, written.phrase, written.step))
            }
            _ => Some((self.cursor.track, self.phrase_id()?, self.cursor.step)),
        }
    }

    /// Switch a flag on its row's note, or off, as one change to undo.
    fn toggle_flag(&mut self, flag: Flag) {
        let Some((track, phrase, at)) = self.flag_target() else {
            self.status = "This track has no phrase here".to_owned();
            return;
        };
        let Some(step) = self.song().tracks[track]
            .phrases
            .get(&phrase)
            .and_then(|phrase| phrase.steps().get(at))
            .cloned()
        else {
            return;
        };
        let Note::On(note) = step.note else {
            let verb = match flag {
                Flag::OctaveUp | Flag::OctaveDown => "move",
                Flag::Accent => "accent",
                Flag::Slide => "slide",
            };
            self.status = format!("No note here to {verb}");
            return;
        };
        if flag.octaves() != 0 {
            self.toggle_octave(flag.octaves(), (track, phrase, at), step, note);
            return;
        }
        let (value, on) = match flag {
            Flag::Accent => {
                let on = !accented(&step);
                let velocity = if on { ACCENTED } else { self.unaccented() };
                (
                    Step {
                        velocity: Some(velocity),
                        ..step
                    },
                    on,
                )
            }
            Flag::Slide => {
                let on = step.gate_kind() != GateKind::Hold;
                let gate = slid(step.gate, on);
                (Step { gate, ..step }, on)
            }
            Flag::OctaveUp | Flag::OctaveDown => return,
        };
        self.apply(&Action::SetStep {
            track,
            phrase,
            step: at,
            value,
        });
        self.status = format!("{} {}", flag.name(), if on { "on" } else { "off" });
    }

    /// Move a row's note an octave up or down, as one change to undo, unless this key moved it
    /// last, when it goes back to where it was played.
    fn toggle_octave(&mut self, by: i8, row: (usize, u32, usize), step: Step, note: u8) {
        let octaves = self
            .octave_moved
            .filter(|moved| moved.row == row && moved.note == note)
            .map_or(0, |moved| moved.octaves);
        let to = if octaves == by { 0 } else { octaves + by };
        let Some(moved) = moved_by(note, to - octaves) else {
            let way = if by > 0 { "above" } else { "below" };
            self.status = format!("No octave {way} this note");
            return;
        };
        let (track, phrase, at) = row;
        self.apply(&Action::SetStep {
            track,
            phrase,
            step: at,
            value: Step {
                note: Note::On(moved),
                ..step
            },
        });
        self.octave_moved = Some(Moved {
            row,
            octaves: to,
            note: moved,
        });
        self.status = match to.signum() {
            1 => "Octave up",
            -1 => "Octave down",
            _ => "Octave as played",
        }
        .to_owned();
    }

    /// The key a flag is on, as the binding table has it.
    fn flag_key_name(&self, flag: Flag) -> &'static str {
        let cmd = match flag {
            Flag::Accent => Cmd::Accent,
            Flag::Slide => Cmd::Slide,
            Flag::OctaveUp => Cmd::OctaveUp,
            Flag::OctaveDown => Cmd::OctaveDown,
        };
        self.table
            .rows()
            .find(|binding| binding.mode == Mode::Insert && binding.cmd == cmd)
            .map_or("its key", |binding| binding.keys)
    }

    /// The velocity taking an accent off leaves: the one notes are written with, unless that's
    /// an accent itself, and then the one a new song writes with.
    fn unaccented(&self) -> u8 {
        if self.note_velocity < ACCENT_VELOCITY {
            self.note_velocity
        } else {
            DEFAULT_NOTE_VELOCITY
        }
    }
}

/// Whether a row's note is accented. An empty velocity plays at full, so it is.
fn accented(step: &Step) -> bool {
    step.velocity
        .is_none_or(|velocity| velocity >= ACCENT_VELOCITY)
}

/// A note some octaves up or down, if that's still in range.
fn moved_by(note: u8, octaves: i8) -> Option<u8> {
    let moved = i16::from(note) + 12 * i16::from(octaves);
    (i16::from(MIN_NOTE)..=i16::from(MAX_NOTE))
        .contains(&moved)
        .then_some(moved as u8)
}
