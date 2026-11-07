//! What keys do on the track page.

use ratatui::crossterm::event::KeyCode;
use std::sync::Arc;

use vt_model::action::{Action, TrackControl, edit};
use vt_model::constants::{MAX_ENVELOPE_MS, MAX_SWING, MIN_SWING, TRACK_ENVELOPES};
use vt_model::envelope::Envelope;
use vt_model::insert::{INSERTS, Insert, MAX_PARAMS};
use vt_model::lfo::{LFOS_PER_TRACK, Lfo, MAX_LFO_DEPTH, Rate, Shape, note_rates};
use vt_model::selection::Motion;
use vt_model::song::{FilterType, Machine, SynthVoice, Track, TrackKind};
use vt_model::target::Target;

use crate::app::{App, Entry};
use crate::input::bindings::Cmd;
use crate::input::command::swing;
use crate::input::mode::{Mode, View};
use crate::input::picker::{Item, Pick, Picker};

/// How far a small and a large step move a phase, in degrees.
const PHASE_NUDGE: i32 = 15;
const PHASE_JUMP: i32 = 90;
/// How far a small and a large step move a swing, in tenths of a percent.
const SWING_NUDGE: i32 = 10;
const SWING_JUMP: i32 = 50;
/// How many small steps a large one is for a value in hex.
const HEX_JUMP: i64 = 0x10;
/// How many small steps a large one is for an envelope time.
const TIME_JUMP: u16 = 10;
/// How far a large step moves a sustain, in percent.
const SUSTAIN_JUMP: i16 = 10;

/// One setting of an LFO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoField {
    Destination,
    Shape,
    Rate,
    Depth,
    Phase,
    Retrig,
}

/// One setting of an envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeField {
    Destination,
    Attack,
    Decay,
    Sustain,
    Release,
    Depth,
    Reset,
}

/// One setting on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// What makes the sound.
    Machine,
    /// Which synth it plays.
    SynthVoice,
    /// The file it plays.
    Sample,
    Mute,
    Solo,
    Base(TrackControl),
    Level,
    Swing,
    FilterType,
    Lfo(usize, LfoField),
    Envelope(usize, EnvelopeField),
    /// What's in one of the track's slots.
    Insert(usize),
    /// One of the settings declared by what's in a slot.
    InsertParam(usize, usize),
}

const fn lfo_row(slot: usize) -> [Control; 6] {
    [
        Control::Lfo(slot, LfoField::Destination),
        Control::Lfo(slot, LfoField::Shape),
        Control::Lfo(slot, LfoField::Rate),
        Control::Lfo(slot, LfoField::Depth),
        Control::Lfo(slot, LfoField::Phase),
        Control::Lfo(slot, LfoField::Retrig),
    ]
}

const fn envelope_row(slot: usize) -> [Control; 7] {
    [
        Control::Envelope(slot, EnvelopeField::Destination),
        Control::Envelope(slot, EnvelopeField::Attack),
        Control::Envelope(slot, EnvelopeField::Decay),
        Control::Envelope(slot, EnvelopeField::Sustain),
        Control::Envelope(slot, EnvelopeField::Release),
        Control::Envelope(slot, EnvelopeField::Depth),
        Control::Envelope(slot, EnvelopeField::Reset),
    ]
}

/// Each envelope's row.
pub const ENVELOPES: [[Control; 7]; TRACK_ENVELOPES] = [envelope_row(0), envelope_row(1)];
/// Each LFO's row.
pub const LFOS: [[Control; 6]; LFOS_PER_TRACK] = [lfo_row(0), lfo_row(1)];

/// The mixer's columns.
pub const MIXER: [Control; 6] = [
    Control::Level,
    Control::Base(TrackControl::Pan),
    Control::Base(TrackControl::SendA),
    Control::Base(TrackControl::SendB),
    Control::Mute,
    Control::Solo,
];

/// The mixer's columns, as rows like a page's.
pub const MIXER_ROWS: [&[Control]; 1] = [&MIXER];

/// The rows of a slot on the FX page, as columns of two settings, one over the other. The effect
/// is in both, as one tall column, so `j` and `k` on it go between slots.
const fn insert_top(slot: usize) -> [Control; FX_COLUMNS + 1] {
    [
        Control::Insert(slot),
        Control::InsertParam(slot, 0),
        Control::InsertParam(slot, 2),
        Control::InsertParam(slot, 4),
        Control::InsertParam(slot, 6),
        Control::InsertParam(slot, 8),
    ]
}

const fn insert_bottom(slot: usize) -> [Control; FX_COLUMNS] {
    [
        Control::Insert(slot),
        Control::InsertParam(slot, 1),
        Control::InsertParam(slot, 3),
        Control::InsertParam(slot, 5),
        Control::InsertParam(slot, 7),
    ]
}

const INSERT_TOPS: [[Control; FX_COLUMNS + 1]; INSERTS] =
    [insert_top(0), insert_top(1), insert_top(2)];
const INSERT_BOTTOMS: [[Control; FX_COLUMNS]; INSERTS] =
    [insert_bottom(0), insert_bottom(1), insert_bottom(2)];
/// How many columns of settings a slot has.
pub const FX_COLUMNS: usize = MAX_PARAMS.div_ceil(2);
/// How many rows a slot takes.
pub const SLOT_ROWS: usize = 2;
/// The FX page's rows, top to bottom. The cursor stops at the last setting an effect uses.
pub const FX_ROWS: [&[Control]; INSERTS * SLOT_ROWS] = [
    &INSERT_TOPS[0],
    &INSERT_BOTTOMS[0],
    &INSERT_TOPS[1],
    &INSERT_BOTTOMS[1],
    &INSERT_TOPS[2],
    &INSERT_BOTTOMS[2],
];

const fn base(control: TrackControl) -> Control {
    Control::Base(control)
}

/// The Sound page of a track playing a sample, top to bottom, with output on the right.
pub const SAMPLE_ROWS: [&[Control]; 8] = [
    &[Control::Machine, Control::Level],
    &[Control::Sample, base(TrackControl::Pan)],
    &[Control::FilterType, base(TrackControl::SendA)],
    &[base(TrackControl::Cutoff), base(TrackControl::SendB)],
    &[base(TrackControl::Resonance), Control::Swing],
    &[base(TrackControl::Drive)],
    &[base(TrackControl::LowCut)],
    &[base(TrackControl::HighCut)],
];
/// The Sound page of a track playing a synth.
pub const SYNTH_ROWS: [&[Control]; 11] = [
    &[Control::Machine, Control::Level],
    &[Control::SynthVoice, base(TrackControl::Pan)],
    &[base(TrackControl::Harmonics), base(TrackControl::SendA)],
    &[base(TrackControl::Timbre), base(TrackControl::SendB)],
    &[base(TrackControl::Morph), Control::Swing],
    &[Control::FilterType],
    &[base(TrackControl::Cutoff)],
    &[base(TrackControl::Resonance)],
    &[base(TrackControl::Drive)],
    &[base(TrackControl::LowCut)],
    &[base(TrackControl::HighCut)],
];
/// The Sound page of a return, which has no machine or filter.
pub const RETURN_ROWS: [&[Control]; 2] = [&[Control::Level], &[base(TrackControl::Pan)]];
/// The Sound page of the master.
pub const MASTER_ROWS: [&[Control]; 1] = [&[Control::Level]];
/// The Mod page's rows, top to bottom.
pub const MOD_ROWS: [&[Control]; TRACK_ENVELOPES + LFOS_PER_TRACK] =
    [&ENVELOPES[0], &ENVELOPES[1], &LFOS[0], &LFOS[1]];

/// An effect taken out of a slot, waiting to be put in another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TakenInsert {
    pub insert: Insert,
    pub track: usize,
    pub slot: usize,
    /// How far undo went when it was taken, to tell if `p` comes straight after.
    pub undo_depth: usize,
}

/// A part of the track page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Sound,
    Mod,
    Fx,
}

impl Page {
    /// Every page, in order.
    pub const ALL: [Self; 3] = [Self::Sound, Self::Mod, Self::Fx];

    /// The page's name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sound => "Sound",
            Self::Mod => "Mod",
            Self::Fx => "FX",
        }
    }

    /// The page next to this one, or this one at either end.
    fn beside(self, next: bool) -> Self {
        let at = Self::ALL.iter().position(|page| *page == self).unwrap_or(0);
        let to = if next { at + 1 } else { at.saturating_sub(1) };
        Self::ALL[to.min(Self::ALL.len() - 1)]
    }

    /// The page's rows, top to bottom, for a track with this machine.
    pub fn rows(self, machine: Machine) -> &'static [&'static [Control]] {
        match (self, machine) {
            (Self::Sound, Machine::Sample) => &SAMPLE_ROWS,
            (Self::Sound, Machine::Plaits) => &SYNTH_ROWS,
            (Self::Mod, _) => &MOD_ROWS,
            (Self::Fx, _) => &FX_ROWS,
        }
    }
}

impl Control {
    /// The setting's name on screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::Base(TrackControl::Cutoff) => "Cut",
            Self::Base(TrackControl::Resonance) => "Res",
            Self::Base(TrackControl::Pan) => "Pan",
            Self::Base(TrackControl::SendA) => "Send A",
            Self::Base(TrackControl::SendB) => "Send B",
            Self::Base(TrackControl::Drive) => "Drive",
            Self::Base(TrackControl::LowCut) => "Low cut",
            Self::Base(TrackControl::HighCut) => "High cut",
            Self::Machine => "Machine",
            Self::SynthVoice => "Voice",
            Self::Base(TrackControl::Harmonics) => "Harm",
            Self::Base(TrackControl::Timbre) => "Timbre",
            Self::Base(TrackControl::Morph) => "Morph",
            Self::Sample => "File",
            Self::Mute => "Mute",
            Self::Solo => "Solo",
            Self::FilterType => "Filter",
            Self::Level => "Level",
            Self::Swing => "Swing",
            Self::Lfo(_, LfoField::Destination) => "Dest",
            Self::Lfo(_, LfoField::Shape) => "Shape",
            Self::Lfo(_, LfoField::Rate) => "Rate",
            Self::Lfo(_, LfoField::Depth) => "Depth",
            Self::Lfo(_, LfoField::Phase) => "Phase",
            Self::Lfo(_, LfoField::Retrig) => "Retrig",
            Self::Envelope(_, EnvelopeField::Destination) => "Dest",
            Self::Envelope(_, EnvelopeField::Attack) => "Attack",
            Self::Envelope(_, EnvelopeField::Decay) => "Decay",
            Self::Envelope(_, EnvelopeField::Sustain) => "Sustain",
            Self::Envelope(_, EnvelopeField::Release) => "Release",
            Self::Envelope(_, EnvelopeField::Depth) => "Depth",
            Self::Envelope(_, EnvelopeField::Reset) => "Reset",
            Self::Insert(_) => "Effect",
            Self::InsertParam(..) => "Setting",
        }
    }

    /// The setting's name on screen, on this track, as a slot's settings depend on what's in it.
    pub fn name_on(self, track: &Track) -> &'static str {
        match self {
            Self::InsertParam(slot, param) => track.inserts[slot]
                .params()
                .get(param)
                .map_or_else(|| self.label(), |found| found.name),
            _ => self.label_on(track.synth_voice),
        }
    }

    /// Whether this track has the setting: a slot only has the settings of what's in it, a fixed
    /// track sends nowhere, and the master can't be panned, muted or soloed.
    pub fn exists_on(self, track: &Track) -> bool {
        match self {
            Self::InsertParam(slot, param) => param < track.inserts[slot].params().len(),
            Self::Base(TrackControl::SendA | TrackControl::SendB) => track.is_sound(),
            Self::Base(TrackControl::Pan) | Self::Mute | Self::Solo => {
                track.kind != TrackKind::Master
            }
            _ => true,
        }
    }

    /// The setting's name on screen, for a track playing this voice. A synth's three settings do
    /// different things on each.
    pub fn label_on(self, voice: SynthVoice) -> &'static str {
        let [first, second, third] = voice.settings();
        match self {
            Self::Base(TrackControl::Harmonics) => first,
            Self::Base(TrackControl::Timbre) => second,
            Self::Base(TrackControl::Morph) => third,
            _ => self.label(),
        }
    }

    /// Whether this is two hex digits, entered as soon as the second is typed.
    fn is_hex(self) -> bool {
        matches!(self, Self::Base(_) | Self::Level | Self::InsertParam(..))
    }

    /// Whether this is picked from a list rather than typed, on this track.
    fn is_choice(self, track: &Track) -> bool {
        if let Self::InsertParam(slot, param) = self {
            return track.inserts[slot]
                .params()
                .get(param)
                .is_some_and(|found| !found.choices.is_empty());
        }
        matches!(
            self,
            Self::Insert(_)
                | Self::Machine
                | Self::SynthVoice
                | Self::Sample
                | Self::Mute
                | Self::Solo
                | Self::FilterType
                | Self::Lfo(
                    _,
                    LfoField::Destination | LfoField::Shape | LfoField::Retrig
                )
                | Self::Envelope(_, EnvelopeField::Destination | EnvelopeField::Reset)
        )
    }
}

/// The next item in a list, going round.
fn next_in<T: Copy + PartialEq>(items: &[T], current: T, up: bool) -> T {
    let position = items.iter().position(|item| *item == current).unwrap_or(0);
    let step = if up { 1 } else { items.len() - 1 };
    items[(position + step) % items.len()]
}

impl App {
    /// What the cursor is on.
    pub fn track_control(&self) -> Control {
        if self.view == View::Mixer {
            return MIXER[self.mixer_row.min(MIXER.len() - 1)];
        }
        let rows = self.page_rows();
        let row = rows[self.track_row.min(rows.len() - 1)];
        row[self.track_field.min(self.fields_in(row) - 1)]
    }

    /// How many settings a row has on this track, which on the FX page depends on the effect.
    fn fields_in(&self, row: &[Control]) -> usize {
        let track = self.song().tracks.get(self.cursor.track);
        track.map_or(row.len(), |track| {
            row.iter()
                .take_while(|control| control.exists_on(track))
                .count()
                .max(1)
        })
    }

    /// The rows of the page showing, which depend on the track's kind and machine.
    pub(crate) fn page_rows(&self) -> &'static [&'static [Control]] {
        let track = self.song().tracks.get(self.cursor.track);
        let kind = track.map_or(TrackKind::Sound, |track| track.kind);
        match (self.track_page, kind) {
            (Page::Sound, TrackKind::ReturnA | TrackKind::ReturnB) => &RETURN_ROWS,
            (Page::Sound, TrackKind::Master) => &MASTER_ROWS,
            _ => self
                .track_page
                .rows(track.map_or(Machine::Sample, |track| track.machine)),
        }
    }

    fn page_track(&self) -> Track {
        self.song()
            .tracks
            .get(self.cursor.track)
            .cloned()
            .unwrap_or_default()
    }

    fn set_lfo(&mut self, slot: usize, lfo: Lfo) {
        self.apply(&Action::SetLfo {
            track: self.cursor.track,
            slot,
            lfo,
        });
    }

    fn set_hex(&mut self, control: Control, value: u8) {
        let track = self.cursor.track;
        // On the mixer, the cursor can be on a setting this track doesn't have.
        let tracks = &self.song().tracks;
        if tracks.get(track).is_some_and(|t| !control.exists_on(t)) {
            return;
        }
        match control {
            Control::Base(control) => self.apply(&Action::SetTrackBase {
                track,
                control,
                value,
            }),
            Control::Level => self.apply(&Action::SetTrackLevel {
                track,
                level: value,
            }),
            _ => {}
        }
    }

    fn adjust_control(&mut self, up: bool, count: usize, large: bool) {
        let control = self.track_control();
        let track = self.page_track();
        let change = |value: u8| {
            let size = if large { HEX_JUMP } else { 1 };
            let moved = i64::from(value) + if up { size } else { -size } * count as i64;
            moved.clamp(0, 0xFF) as u8
        };
        match control {
            Control::Mute | Control::Solo => {
                let live = self
                    .levels
                    .get(self.cursor.track)
                    .copied()
                    .unwrap_or_default();
                let on = if control == Control::Mute {
                    live.mute
                } else {
                    live.solo
                };
                if on != up {
                    self.toggle_control(control);
                }
            }
            Control::SynthVoice => {
                let voice = next_in(&SynthVoice::ALL, track.synth_voice, up);
                self.apply(&Action::SetTrackSynthVoice {
                    track: self.cursor.track,
                    voice,
                });
            }
            Control::Machine => {
                let machine = next_in(&Machine::ALL, track.machine, up);
                self.apply(&Action::SetTrackMachine {
                    track: self.cursor.track,
                    machine,
                });
            }
            Control::Sample => {
                self.status = "K chooses a sample".to_owned();
            }
            Control::Base(which) => {
                let value = match which {
                    TrackControl::Cutoff => track.cutoff,
                    TrackControl::Resonance => track.resonance,
                    TrackControl::Pan => track.pan,
                    TrackControl::SendA => track.send_a,
                    TrackControl::SendB => track.send_b,
                    TrackControl::Drive => track.drive,
                    TrackControl::LowCut => track.low_cut,
                    TrackControl::HighCut => track.high_cut,
                    TrackControl::Harmonics => track.harmonics,
                    TrackControl::Timbre => track.timbre,
                    TrackControl::Morph => track.morph,
                };
                self.set_hex(control, change(value));
            }
            Control::Level => self.set_hex(control, change(track.level)),
            Control::Insert(slot) => self.set_insert(slot, track.inserts[slot].stepped(up)),
            Control::InsertParam(slot, param) => {
                let insert = track.inserts[slot];
                let Some(found) = insert.params().get(param) else {
                    return;
                };
                let value = insert.value(param);
                let count = found.choices.len() as u8;
                let moved = if found.choices.is_empty() {
                    change(value)
                } else if found.wraps {
                    (value + if up { 1 } else { count - 1 }) % count
                } else if up {
                    (value + 1).min(count - 1)
                } else {
                    value.saturating_sub(1)
                };
                self.set_insert(slot, insert.with_value(param, moved));
            }
            Control::FilterType => {
                // One way at a time, as a large step would skip through a short list.
                let filter = next_in(&FilterType::ALL, track.filter, up);
                self.apply(&Action::SetFilterType {
                    track: self.cursor.track,
                    filter,
                });
            }
            Control::Swing => {
                let from = track.swing.unwrap_or(self.song().swing);
                let size = if large { SWING_JUMP } else { SWING_NUDGE };
                let by = size * count.min(usize::from(MAX_SWING)) as i32;
                let moved = i32::from(from) + if up { by } else { -by };
                self.apply(&Action::SetTrackSwing {
                    track: self.cursor.track,
                    swing: Some(moved.clamp(i32::from(MIN_SWING), i32::from(MAX_SWING)) as u16),
                });
            }
            Control::Lfo(slot, field) => {
                let mut lfo = track.lfos[slot];
                for _ in 0..count.min(0x200) {
                    adjust_lfo(&mut lfo, slot, field, up, large, &track);
                }
                self.set_lfo(slot, lfo);
            }
            Control::Envelope(slot, field) => {
                let mut envelope = track.envelopes[slot];
                for _ in 0..count.min(0x200) {
                    adjust_envelope(&mut envelope, field, up, large, &track);
                }
                self.set_envelope(slot, envelope);
            }
        }
    }

    /// The slot the cursor is in, on the FX page.
    fn fx_slot(&self) -> usize {
        self.track_row / SLOT_ROWS
    }

    /// `dd`: take the effect out of this slot, for `p` to put somewhere.
    fn take_insert(&mut self) {
        let slot = self.fx_slot();
        let insert = self.page_track().inserts[slot];
        if insert == Insert::Empty {
            self.status = "There's nothing in this slot to take".to_owned();
            return;
        }
        self.set_insert(slot, Insert::Empty);
        self.taken_insert = Some(TakenInsert {
            insert,
            track: self.cursor.track,
            slot,
            undo_depth: self.history.undo_depth(),
        });
        self.move_control(self.track_row, 0);
        self.status = format!("Took {}: p puts it in a slot", insert.name());
    }

    /// `p`: put what was taken into this slot. What's there goes where it came from, if that's still
    /// empty, so two slots swap. Straight after `dd`, the whole move is one undo step.
    fn put_insert(&mut self) {
        let Some(taken) = self.taken_insert.take() else {
            self.status = "dd takes an effect out of a slot first".to_owned();
            return;
        };
        let (track, slot) = (self.cursor.track, self.fx_slot());
        let displaced = self.page_track().inserts[slot];
        let mut actions = vec![Action::SetInsert {
            track,
            slot,
            insert: taken.insert,
        }];
        let source_empty = (taken.track, taken.slot) != (track, slot)
            && self
                .song()
                .tracks
                .get(taken.track)
                .is_some_and(|source| source.inserts[taken.slot] == Insert::Empty);
        if displaced != Insert::Empty {
            if source_empty {
                actions.push(Action::SetInsert {
                    track: taken.track,
                    slot: taken.slot,
                    insert: displaced,
                });
            } else {
                // There's nowhere for it to go, so it's the next one to put.
                self.taken_insert = Some(TakenInsert {
                    insert: displaced,
                    track,
                    slot,
                    undo_depth: 0,
                });
            }
        }
        let mut next = Arc::clone(self.song());
        for action in &actions {
            if let Ok(edited) = edit(&next, action) {
                next = edited;
            }
        }
        if taken.undo_depth == self.history.undo_depth() {
            self.history.replace_current(next);
        } else {
            self.history.commit(next);
        }
        self.dirty = true;
        self.publish();
        self.status = format!("Put {} in slot {}", taken.insert.name(), slot + 1);
    }

    fn set_insert(&mut self, slot: usize, insert: Insert) {
        self.apply(&Action::SetInsert {
            track: self.cursor.track,
            slot,
            insert,
        });
    }

    fn set_envelope(&mut self, slot: usize, envelope: Envelope) {
        self.apply(&Action::SetEnvelope {
            track: self.cursor.track,
            slot,
            envelope,
        });
    }

    pub(crate) fn move_control(&mut self, row: usize, field: usize) {
        let rows = self.page_rows();
        self.track_row = row.min(rows.len() - 1);
        self.track_field = field.min(self.fields_in(rows[self.track_row]) - 1);
    }

    /// Move a row up or down, past any that would leave the cursor on the same setting, as an empty
    /// slot's effect is in both its rows.
    fn step_row(&mut self, by: isize) {
        let before = self.track_control();
        let last = self.page_rows().len() - 1;
        let mut row = self.track_row;
        while let Some(next) = row.checked_add_signed(by).filter(|next| *next <= last) {
            row = next;
            self.move_control(row, self.track_field);
            if self.track_control() != before {
                return;
            }
        }
    }

    /// Run a command on the mixer. `h` and `l` go between its strips, and `j` and `k` up and down
    /// them. Everything else means what it does on the track page.
    pub(crate) fn run_mixer(&mut self, cmd: Cmd) -> bool {
        let times = self.count.unwrap_or(1).max(1) as isize;
        let track = self.cursor.track as isize;
        match cmd {
            Cmd::Move(Motion::Right) => self.move_track(times),
            Cmd::Move(Motion::Left) => self.move_track(-times),
            Cmd::Move(Motion::RowStart) => self.move_track(-track),
            Cmd::Move(Motion::RowEnd) => {
                self.move_track(self.song().tracks.len() as isize - 1 - track)
            }
            Cmd::Move(Motion::Down) => {
                let moved = self.mixer_row.saturating_add(times as usize);
                self.mixer_row = moved.min(MIXER.len() - 1);
            }
            Cmd::Move(Motion::Up) => {
                self.mixer_row = self.mixer_row.saturating_sub(times as usize);
            }
            Cmd::Move(Motion::First) => self.mixer_row = 0,
            Cmd::Move(Motion::Last) => self.mixer_row = MIXER.len() - 1,
            _ => return self.run_track_page(cmd),
        }
        self.count = None;
        true
    }

    /// Run a command on the track page. Returns false if it means the same as anywhere.
    pub(crate) fn run_track_page(&mut self, cmd: Cmd) -> bool {
        let count = self.count.take();
        let times = count.unwrap_or(1).max(1);
        let (row, field) = (self.track_row, self.track_field);
        match cmd {
            Cmd::Move(Motion::Down) => (0..times).for_each(|_| self.step_row(1)),
            Cmd::Move(Motion::Up) => (0..times).for_each(|_| self.step_row(-1)),
            Cmd::Move(Motion::NextBeat | Motion::PreviousBeat) if self.track_page != Page::Fx => {
                self.status = "{ and } go between slots on the FX page".to_owned();
            }
            // Only the slot changes, so a setting goes to the same setting in the next. The field
            // is left as it is, so it comes back after an empty slot.
            Cmd::Move(Motion::NextBeat) => {
                let slot = (row / SLOT_ROWS).saturating_add(times).min(INSERTS - 1);
                self.track_row = slot * SLOT_ROWS + row % SLOT_ROWS;
            }
            Cmd::Move(Motion::PreviousBeat) => {
                let slot = (row / SLOT_ROWS).saturating_sub(times);
                self.track_row = slot * SLOT_ROWS + row % SLOT_ROWS;
            }
            Cmd::Move(Motion::First) => self.move_control(0, field),
            Cmd::Move(Motion::Last) => self.move_control(usize::MAX, field),
            Cmd::Move(Motion::Right) => self.move_control(row, field.saturating_add(times)),
            Cmd::Move(Motion::Left) => self.move_control(row, field.saturating_sub(times)),
            Cmd::Move(Motion::RowStart) => self.move_control(row, 0),
            Cmd::Move(Motion::RowEnd) => self.move_control(row, usize::MAX),
            Cmd::PreviousPage | Cmd::NextPage => {
                self.track_page = self.track_page.beside(cmd == Cmd::NextPage);
                self.move_control(0, field);
            }
            Cmd::FieldJump => self.open_field_jump(),
            Cmd::RemoveRow | Cmd::Paste if self.track_page != Page::Fx => {
                self.status = "dd and p move effects, on the FX page".to_owned();
            }
            Cmd::RemoveRow => self.take_insert(),
            Cmd::Paste => self.put_insert(),
            Cmd::Increase => self.adjust_control(true, times, false),
            Cmd::Decrease => self.adjust_control(false, times, false),
            Cmd::IncreaseLarge => self.adjust_control(true, times, true),
            Cmd::DecreaseLarge => self.adjust_control(false, times, true),
            Cmd::ClearField => self.reset_control(),
            Cmd::PickValue => self.open_control_picker(),
            Cmd::EnterInsert if self.track_control().is_choice(&self.page_track()) => {
                self.open_control_picker();
            }
            Cmd::EnterInsert => {
                self.entry = Entry::default();
                self.mode = Mode::Insert;
            }
            _ => {
                self.count = count;
                return false;
            }
        }
        true
    }

    /// Put the setting under the cursor back to its default. If it's there already, nothing changes
    /// and no undo step is left.
    fn reset_control(&mut self) {
        let control = self.track_control();
        let track = self.page_track();
        let fresh = Track::default();
        match control {
            Control::Base(which) => {
                let value = |track: &Track| match which {
                    TrackControl::Cutoff => track.cutoff,
                    TrackControl::Resonance => track.resonance,
                    TrackControl::Pan => track.pan,
                    TrackControl::SendA => track.send_a,
                    TrackControl::SendB => track.send_b,
                    TrackControl::Drive => track.drive,
                    TrackControl::LowCut => track.low_cut,
                    TrackControl::HighCut => track.high_cut,
                    TrackControl::Harmonics => track.harmonics,
                    TrackControl::Timbre => track.timbre,
                    TrackControl::Morph => track.morph,
                };
                if value(&track) != value(&fresh) {
                    self.set_hex(control, value(&fresh));
                }
            }
            Control::Mute | Control::Solo => {
                let live = self
                    .levels
                    .get(self.cursor.track)
                    .copied()
                    .unwrap_or_default();
                if (control == Control::Mute && live.mute)
                    || (control == Control::Solo && live.solo)
                {
                    self.toggle_control(control);
                }
            }
            Control::SynthVoice => {
                if track.synth_voice != fresh.synth_voice {
                    self.apply(&Action::SetTrackSynthVoice {
                        track: self.cursor.track,
                        voice: fresh.synth_voice,
                    });
                }
            }
            Control::Machine => {
                if track.machine != fresh.machine {
                    self.apply(&Action::SetTrackMachine {
                        track: self.cursor.track,
                        machine: fresh.machine,
                    });
                }
            }
            Control::Sample if track.sample.is_some() => {
                self.apply(&Action::SetTrackSample {
                    track: self.cursor.track,
                    sample: None,
                });
            }
            Control::Sample => {}
            Control::Insert(slot) => {
                if track.inserts[slot] != Insert::Empty {
                    self.set_insert(slot, Insert::Empty);
                }
            }
            Control::InsertParam(slot, param) => {
                let insert = track.inserts[slot];
                if let Some(found) = insert.params().get(param)
                    && insert.value(param) != found.default
                {
                    self.set_insert(slot, insert.with_value(param, found.default));
                }
            }
            Control::Level if track.level != fresh.level => self.set_hex(control, fresh.level),
            Control::Level => {}
            Control::Swing if track.swing.is_some() => {
                self.apply(&Action::SetTrackSwing {
                    track: self.cursor.track,
                    swing: None,
                });
            }
            Control::Swing => {}
            Control::FilterType => {
                if track.filter != fresh.filter {
                    self.apply(&Action::SetFilterType {
                        track: self.cursor.track,
                        filter: fresh.filter,
                    });
                }
            }
            Control::Lfo(slot, field) => {
                let (lfo, default) = (track.lfos[slot], Lfo::default());
                let reset = match field {
                    LfoField::Destination => Lfo {
                        destination: default.destination,
                        ..lfo
                    },
                    LfoField::Shape => Lfo {
                        shape: default.shape,
                        ..lfo
                    },
                    LfoField::Rate => Lfo {
                        rate: default.rate,
                        ..lfo
                    },
                    LfoField::Depth => Lfo {
                        depth: default.depth,
                        ..lfo
                    },
                    LfoField::Phase => Lfo {
                        phase: default.phase,
                        ..lfo
                    },
                    LfoField::Retrig => Lfo {
                        retrig: default.retrig,
                        ..lfo
                    },
                };
                if reset != lfo {
                    self.set_lfo(slot, reset);
                }
            }
            Control::Envelope(slot, field) => {
                let (envelope, default) = (track.envelopes[slot], fresh.envelopes[slot]);
                let reset = match field {
                    EnvelopeField::Destination => Envelope {
                        destination: default.destination,
                        ..envelope
                    },
                    EnvelopeField::Attack => Envelope {
                        attack_ms: default.attack_ms,
                        ..envelope
                    },
                    EnvelopeField::Decay => Envelope {
                        decay_ms: default.decay_ms,
                        ..envelope
                    },
                    EnvelopeField::Sustain => Envelope {
                        sustain: default.sustain,
                        ..envelope
                    },
                    EnvelopeField::Release => Envelope {
                        release_ms: default.release_ms,
                        ..envelope
                    },
                    EnvelopeField::Depth => Envelope {
                        depth: default.depth,
                        ..envelope
                    },
                    EnvelopeField::Reset => Envelope {
                        reset: default.reset,
                        ..envelope
                    },
                };
                if reset != envelope {
                    self.set_envelope(slot, reset);
                }
            }
        }
    }

    /// List what an LFO's setting can be, with the current one highlighted.
    /// Switch this track's mute or solo.
    fn toggle_control(&mut self, control: Control) {
        if control == Control::Mute {
            self.toggle_mute();
        } else {
            self.toggle_solo();
        }
    }

    fn open_control_picker(&mut self) {
        let control = self.track_control();
        if matches!(control, Control::Mute | Control::Solo) {
            self.toggle_control(control);
            return;
        }
        if self.track_control() == Control::Sample {
            self.open_default_sample_picker();
            return;
        }
        if let Control::Insert(slot) = control {
            self.open_insert_picker(slot);
            return;
        }
        if let Control::InsertParam(slot, param) = control {
            self.open_insert_param_picker(slot, param);
            return;
        }
        if self.track_control() == Control::FilterType {
            self.open_filter_picker();
            return;
        }
        if self.track_control() == Control::Machine {
            self.open_machine_picker();
            return;
        }
        if self.track_control() == Control::SynthVoice {
            self.open_voice_picker();
            return;
        }
        if let Control::Envelope(slot, field) = self.track_control() {
            self.open_envelope_picker(slot, field);
            return;
        }
        let Control::Lfo(slot, field) = self.track_control() else {
            self.status =
                "K chooses from a list on Machine, Voice, File, Type, Dest, Shape, Rate, Retrig and Reset"
                    .to_owned();
            return;
        };
        let track = self.cursor.track;
        let page = self.page_track();
        let lfo = page.lfos[slot];
        let (title, choices): (&'static str, Vec<(String, Lfo)>) = match field {
            LfoField::Destination => {
                let off = (
                    "---  Off".to_owned(),
                    Lfo {
                        destination: None,
                        ..lfo
                    },
                );
                let on = Target::for_lfo(&page, slot).into_iter().map(|destination| {
                    (
                        format!(
                            "{}  {}",
                            destination.name_on(&page),
                            destination.meaning_on(&page)
                        ),
                        Lfo {
                            destination: Some(destination),
                            ..lfo
                        },
                    )
                });
                ("LFO destination", std::iter::once(off).chain(on).collect())
            }
            LfoField::Shape => {
                let shapes =
                    Shape::ALL.map(|shape| (shape.name().to_owned(), Lfo { shape, ..lfo }));
                ("LFO shape", shapes.to_vec())
            }
            LfoField::Rate => {
                let rates = note_rates()
                    .into_iter()
                    .map(|rate| (rate.to_string(), Lfo { rate, ..lfo }))
                    .collect();
                ("LFO rate. i types any other", rates)
            }
            LfoField::Retrig => {
                let choices = [false, true].map(|retrig| {
                    let name = if retrig { "on" } else { "off" };
                    (name.to_owned(), Lfo { retrig, ..lfo })
                });
                ("LFO retrig", choices.to_vec())
            }
            LfoField::Depth | LfoField::Phase => {
                self.status =
                    "K chooses from a list on Type, Dest, Shape, Rate, Retrig and Reset".to_owned();
                return;
            }
        };
        let position = choices.iter().position(|(_, choice)| *choice == lfo);
        let items = choices
            .into_iter()
            .map(|(label, lfo)| Item {
                label,
                pick: Pick::Lfo { track, slot, lfo },
            })
            .collect();
        let picker = if field == LfoField::Destination {
            let groups = std::iter::once("---".to_owned())
                .chain(Target::for_lfo(&page, slot).into_iter().map(Target::group))
                .collect();
            Picker::grouped(title, items, groups, position.unwrap_or(0))
        } else {
            let mut picker = Picker::new(title, items);
            picker.move_highlight(position.unwrap_or(0) as isize);
            picker
        };
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// List the filter types, with what each does and the current one highlighted.
    /// List the effects a slot can hold, with the current one highlighted.
    fn open_insert_picker(&mut self, slot: usize) {
        let track = self.cursor.track;
        let current = self.page_track().inserts[slot];
        let items = Insert::ALL
            .map(|insert| Item {
                label: format!("{}  {}", insert.name(), insert.meaning()),
                pick: Pick::Insert {
                    track,
                    slot,
                    // Choosing what's already there keeps its settings.
                    insert: if insert.name() == current.name() {
                        current
                    } else {
                        insert
                    },
                },
            })
            .to_vec();
        let position = Insert::ALL
            .iter()
            .position(|insert| insert.name() == current.name());
        let mut picker = Picker::new("Effect", items);
        picker.move_highlight(position.unwrap_or(0) as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// List the choices of a slot's setting, with the current one highlighted.
    fn open_insert_param_picker(&mut self, slot: usize, param: usize) {
        let track = self.cursor.track;
        let current = self.page_track().inserts[slot];
        let Some(found) = current
            .params()
            .get(param)
            .filter(|p| !p.choices.is_empty())
        else {
            self.status = "This is a value: = and - change it, and i types it".to_owned();
            return;
        };
        let items = found
            .choices
            .iter()
            .enumerate()
            .map(|(index, choice)| Item {
                label: (*choice).to_owned(),
                pick: Pick::Insert {
                    track,
                    slot,
                    insert: current.with_value(param, index as u8),
                },
            })
            .collect();
        let mut picker = Picker::new(found.name, items);
        picker.move_highlight(isize::from(current.value(param) as i8));
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    fn open_filter_picker(&mut self) {
        let track = self.cursor.track;
        let current = self.page_track().filter;
        let items = FilterType::ALL
            .map(|filter| Item {
                label: format!("{}  {}", filter.name(), filter.meaning()),
                pick: Pick::FilterType { track, filter },
            })
            .to_vec();
        let position = FilterType::ALL.iter().position(|filter| *filter == current);
        let mut picker = Picker::new("Filter type", items);
        picker.move_highlight(position.unwrap_or(0) as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// List the machines, with what each does and the current one highlighted.
    fn open_machine_picker(&mut self) {
        let track = self.cursor.track;
        let current = self.page_track().machine;
        let items = Machine::ALL
            .map(|machine| Item {
                label: format!("{:<8}{}", machine.name(), machine.meaning()),
                pick: Pick::Machine { track, machine },
            })
            .to_vec();
        let position = Machine::ALL.iter().position(|machine| *machine == current);
        let mut picker = Picker::new("Machine", items);
        picker.move_highlight(position.unwrap_or(0) as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// List the synth voices, with what each does and the current one highlighted.
    fn open_voice_picker(&mut self) {
        let track = self.cursor.track;
        let current = self.page_track().synth_voice;
        let items = SynthVoice::ALL
            .map(|voice| Item {
                label: format!("{:<8}{}", voice.name(), voice.meaning()),
                pick: Pick::SynthVoice { track, voice },
            })
            .to_vec();
        let position = SynthVoice::ALL.iter().position(|voice| *voice == current);
        let mut picker = Picker::new("Voice", items);
        picker.move_highlight(position.unwrap_or(0) as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// List what an envelope's setting can be, with the current one highlighted.
    fn open_envelope_picker(&mut self, slot: usize, field: EnvelopeField) {
        let track = self.cursor.track;
        let page = self.page_track();
        let envelope = self.page_track().envelopes[slot];
        let (title, choices): (&'static str, Vec<(String, Envelope)>) = match field {
            EnvelopeField::Destination => {
                let off = (
                    "---  Off".to_owned(),
                    Envelope {
                        destination: None,
                        ..envelope
                    },
                );
                let on = Target::for_envelope(&page).into_iter().map(|destination| {
                    (
                        format!(
                            "{}  {}",
                            destination.name_on(&page),
                            destination.meaning_on(&page)
                        ),
                        Envelope {
                            destination: Some(destination),
                            ..envelope
                        },
                    )
                });
                (
                    "Envelope destination",
                    std::iter::once(off).chain(on).collect(),
                )
            }
            EnvelopeField::Reset => {
                let choices = [true, false].map(|reset| {
                    let name = if reset {
                        "on  Each note starts it from zero"
                    } else {
                        "off  Each note goes on from where it is"
                    };
                    (name.to_owned(), Envelope { reset, ..envelope })
                });
                ("Envelope reset", choices.to_vec())
            }
            _ => {
                self.status = "K chooses from a list on Dest and Reset".to_owned();
                return;
            }
        };
        let position = choices.iter().position(|(_, choice)| *choice == envelope);
        let items = choices
            .into_iter()
            .map(|(label, envelope)| Item {
                label,
                pick: Pick::Envelope {
                    track,
                    slot,
                    envelope,
                },
            })
            .collect();
        let picker = if field == EnvelopeField::Destination {
            let groups = std::iter::once("---".to_owned())
                .chain(Target::for_envelope(&page).into_iter().map(Target::group))
                .collect();
            Picker::grouped(title, items, groups, position.unwrap_or(0))
        } else {
            let mut picker = Picker::new(title, items);
            picker.move_highlight(position.unwrap_or(0) as isize);
            picker
        };
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    /// Type a value: two hex digits for levels, and anything else then `Enter`.
    pub(crate) fn insert_track_page(&mut self, key: KeyCode) {
        let control = self.track_control();
        match key {
            KeyCode::Esc => {
                self.entry = Entry::default();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.entry.text.pop();
            }
            KeyCode::Enter if !control.is_hex() => {
                let text = std::mem::take(&mut self.entry.text);
                self.mode = Mode::Normal;
                if !text.is_empty() {
                    self.commit_control(control, &text);
                }
            }
            KeyCode::Char(c) if control.is_hex() && c.is_ascii_hexdigit() => {
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 2 {
                    let text = std::mem::take(&mut self.entry.text);
                    self.mode = Mode::Normal;
                    self.commit_control(control, &text);
                }
            }
            KeyCode::Char(c) if !control.is_hex() && self.entry.text.len() < 12 => {
                self.entry.text.push(c);
            }
            _ => {}
        }
    }

    fn commit_control(&mut self, control: Control, text: &str) {
        let track = self.page_track();
        match control {
            Control::Mute | Control::Solo => {}
            Control::Sample => {}
            Control::Insert(_) => {}
            Control::InsertParam(slot, param) => {
                if let Ok(value) = u8::from_str_radix(text, 16) {
                    let insert = track.inserts[slot];
                    self.set_insert(slot, insert.with_value(param, value));
                }
            }
            Control::Base(_) | Control::Level => {
                if let Ok(value) = u8::from_str_radix(text, 16) {
                    self.set_hex(control, value);
                }
            }
            Control::Swing if text.eq_ignore_ascii_case("off") => {
                self.apply(&Action::SetTrackSwing {
                    track: self.cursor.track,
                    swing: None,
                });
            }
            Control::Swing => match swing(text) {
                Some(value) => self.apply(&Action::SetTrackSwing {
                    track: self.cursor.track,
                    swing: Some(value),
                }),
                None => self.status = "Swing is a percent from 50 to 75, or off".to_owned(),
            },
            Control::Lfo(slot, LfoField::Rate) => match text.parse::<Rate>() {
                Ok(rate) => {
                    let lfo = Lfo {
                        rate,
                        ..track.lfos[slot]
                    };
                    self.set_lfo(slot, lfo);
                }
                Err(error) => self.status = error.to_string(),
            },
            Control::Lfo(slot, LfoField::Depth) => match parse_depth(text) {
                Some(depth) => {
                    let lfo = Lfo {
                        depth,
                        ..track.lfos[slot]
                    };
                    self.set_lfo(slot, lfo);
                }
                None => self.status = "Depth is hex from -FF to +FF".to_owned(),
            },
            Control::Lfo(slot, LfoField::Phase) => match text.parse::<u16>() {
                Ok(phase) if phase < 360 => {
                    let lfo = Lfo {
                        phase,
                        ..track.lfos[slot]
                    };
                    self.set_lfo(slot, lfo);
                }
                _ => self.status = "Phase is degrees from 0 to 359".to_owned(),
            },
            Control::Lfo(..) | Control::FilterType | Control::Machine | Control::SynthVoice => {}
            Control::Envelope(slot, field) => {
                let current = track.envelopes[slot];
                let ms = text.parse::<u16>().ok().filter(|ms| *ms <= MAX_ENVELOPE_MS);
                let changed = match field {
                    EnvelopeField::Attack => ms.map(|attack_ms| Envelope {
                        attack_ms,
                        ..current
                    }),
                    EnvelopeField::Decay => ms.map(|decay_ms| Envelope {
                        decay_ms,
                        ..current
                    }),
                    EnvelopeField::Release => ms.map(|release_ms| Envelope {
                        release_ms,
                        ..current
                    }),
                    EnvelopeField::Sustain => text
                        .trim_end_matches('%')
                        .parse::<u8>()
                        .ok()
                        .filter(|sustain| *sustain <= 100)
                        .map(|sustain| Envelope { sustain, ..current }),
                    EnvelopeField::Depth => {
                        parse_depth(text).map(|depth| Envelope { depth, ..current })
                    }
                    EnvelopeField::Destination | EnvelopeField::Reset => None,
                };
                match changed {
                    Some(envelope) => self.set_envelope(slot, envelope),
                    None => {
                        self.status = match field {
                            EnvelopeField::Sustain => "Sustain is a percent from 0 to 100",
                            EnvelopeField::Depth => "Depth is hex from -FF to +FF",
                            _ => "Times are milliseconds from 0 to 8000",
                        }
                        .to_owned();
                    }
                }
            }
        }
    }
}

/// How far a small step moves an envelope time. Short times need finer steps than long ones.
fn time_step(value: u16) -> u16 {
    match value {
        0..=9 => 1,
        10..=99 => 5,
        _ => 25,
    }
}

/// Move one setting of an envelope one notch.
fn adjust_envelope(
    envelope: &mut Envelope,
    field: EnvelopeField,
    up: bool,
    large: bool,
    track: &Track,
) {
    let time = |value: u16| {
        let size = if large { TIME_JUMP } else { 1 };
        if up {
            (value + time_step(value) * size).min(MAX_ENVELOPE_MS)
        } else {
            value.saturating_sub(time_step(value.saturating_sub(1)) * size)
        }
    };
    match field {
        EnvelopeField::Destination => {
            let mut choices = vec![None];
            choices.extend(Target::for_envelope(track).into_iter().map(Some));
            envelope.destination = next_in(&choices, envelope.destination, up);
        }
        EnvelopeField::Attack => envelope.attack_ms = time(envelope.attack_ms),
        EnvelopeField::Decay => envelope.decay_ms = time(envelope.decay_ms),
        EnvelopeField::Release => envelope.release_ms = time(envelope.release_ms),
        EnvelopeField::Sustain => {
            let size = if large { SUSTAIN_JUMP } else { 1 };
            let by = if up { size } else { -size };
            envelope.sustain = (i16::from(envelope.sustain) + by).clamp(0, 100) as u8;
        }
        EnvelopeField::Depth => {
            let size = if large { HEX_JUMP as i16 } else { 1 };
            let moved = envelope.depth + if up { size } else { -size };
            envelope.depth = moved.clamp(-MAX_LFO_DEPTH, MAX_LFO_DEPTH);
        }
        EnvelopeField::Reset => envelope.reset = !envelope.reset,
    }
}

/// Move one setting of an LFO one notch.
fn adjust_lfo(lfo: &mut Lfo, slot: usize, field: LfoField, up: bool, large: bool, track: &Track) {
    match field {
        LfoField::Destination => {
            let mut choices = vec![None];
            choices.extend(Target::for_lfo(track, slot).into_iter().map(Some));
            lfo.destination = next_in(&choices, lfo.destination, up);
        }
        LfoField::Shape => lfo.shape = next_in(&Shape::ALL, lfo.shape, up),
        LfoField::Rate if large => lfo.rate = lfo.rate.jumped(up),
        LfoField::Rate => lfo.rate = lfo.rate.nudged(up),
        LfoField::Depth => {
            let size = if large { HEX_JUMP as i16 } else { 1 };
            let moved = lfo.depth + if up { size } else { -size };
            lfo.depth = moved.clamp(-MAX_LFO_DEPTH, MAX_LFO_DEPTH);
        }
        LfoField::Phase => {
            let size = if large { PHASE_JUMP } else { PHASE_NUDGE };
            let by = if up { size } else { -size };
            lfo.phase = (i32::from(lfo.phase) + by).rem_euclid(360) as u16;
        }
        LfoField::Retrig => lfo.retrig = !lfo.retrig,
    }
}

/// Read a depth, with an optional sign, in hex.
fn parse_depth(text: &str) -> Option<i16> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => (-1, digits),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let depth = i16::from_str_radix(digits, 16).ok()?;
    (0..=MAX_LFO_DEPTH).contains(&depth).then_some(sign * depth)
}
