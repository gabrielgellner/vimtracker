//! What keys do on the track page.

use ratatui::crossterm::event::KeyCode;
use std::sync::Arc;

use vt_model::accumulator::{Accumulation, Adds, AtLimit, MOST_DEGREES, Moves, Reset};
use vt_model::action::{Action, TrackControl, edit};
use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio, MOST_OCTAVES, retrig_meaning};
use vt_model::constants::{
    BAR_STEPS, MAX_ENVELOPE_MS, MAX_GROUPS, MAX_TRACK_RESET, TRACK_ENVELOPES,
};
use vt_model::envelope::Envelope;
use vt_model::grid::{GRID_COLUMNS, GRID_ROWS};
use vt_model::groove::GrooveId;
use vt_model::insert::Insert;
use vt_model::kit::CHOKE_GROUPS;
use vt_model::lfo::{
    LFOS_PER_TRACK, Lfo, MAX_LFO_DEPTH, Rate, Shape, depth_value, note_rates, phase_value,
};
use vt_model::selection::Motion;
use vt_model::settings::{
    Effect, Kind, LONGEST_GLIDE_MS, SettingId, gate_percent, gate_value, glide_value, voice_entries,
};
use vt_model::song::{FilterType, Machine, Track, TrackKind, reset_named};
use vt_model::speed::Speed;

use crate::app::{App, Entry};
use crate::glyphs;
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};
use crate::input::picker::{Item, Pick, Picker};

/// How far a small and a large step move a phase, in degrees.
const PHASE_NUDGE: i32 = 15;
const PHASE_JUMP: i32 = 90;
/// How far a large step moves `Gate`, in percent.
const GATE_JUMP: i32 = 10;
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

/// One of the track's accumulator settings, which its `ACU`s share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccumulatorField {
    Up,
    Down,
    Moves,
    AtLimit,
    Adds,
    Reset,
}

/// One of the track's arpeggio settings, which its `ARP`s share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArpField {
    Rate,
    Mode,
    Octaves,
    Retrig,
}

/// One setting on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// What makes the sound.
    Machine,
    /// One of the machine's settings, by its place among them.
    Setting(usize),
    /// The file it plays.
    Sample,
    Mute,
    Solo,
    Base(TrackControl),
    Level,
    /// The gain at its input, for gain staging, which the fader then leaves free for the mix.
    Trim,
    /// Its own groove, or the project's.
    Groove,
    /// The choke group it's in, or none.
    Choke,
    /// The group it plays through, or none for the master.
    Group,
    /// How much of its last pulse a note lasts, as a percentage.
    GateLength,
    /// How long its steps are, as a note division.
    Speed,
    /// How many of the song's steps its chain plays before starting again, or off.
    Reset,
    /// How long a tie glides into the next note, in ms, or off.
    Glide,
    FilterType,
    /// How the filter's type responds, which a step can lock too.
    FilterMode,
    Lfo(usize, LfoField),
    Envelope(usize, EnvelopeField),
    Accumulator(AccumulatorField),
    Arpeggio(ArpField),
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
pub static ENVELOPES: [[Control; 7]; TRACK_ENVELOPES] = {
    let mut rows = [envelope_row(0); TRACK_ENVELOPES];
    let mut slot = 0;
    while slot < TRACK_ENVELOPES {
        rows[slot] = envelope_row(slot);
        slot += 1;
    }
    rows
};
/// Each LFO's row.
pub static LFOS: [[Control; 6]; LFOS_PER_TRACK] = {
    let mut rows = [lfo_row(0); LFOS_PER_TRACK];
    let mut slot = 0;
    while slot < LFOS_PER_TRACK {
        rows[slot] = lfo_row(slot);
        slot += 1;
    }
    rows
};
/// The accumulators' row, under the LFOs, as they move notes from pass to pass as the LFOs move
/// settings over time.
pub const ACCUMULATOR: [Control; 6] = [
    Control::Accumulator(AccumulatorField::Up),
    Control::Accumulator(AccumulatorField::Down),
    Control::Accumulator(AccumulatorField::Moves),
    Control::Accumulator(AccumulatorField::AtLimit),
    Control::Accumulator(AccumulatorField::Adds),
    Control::Accumulator(AccumulatorField::Reset),
];
/// The arpeggio's row, under the accumulator's, as both play a row's note on from what's written.
pub const ARPEGGIO: [Control; 4] = [
    Control::Arpeggio(ArpField::Rate),
    Control::Arpeggio(ArpField::Mode),
    Control::Arpeggio(ArpField::Octaves),
    Control::Arpeggio(ArpField::Retrig),
];

/// The mixer's columns.
pub const MIXER: [Control; 7] = [
    Control::Trim,
    Control::Level,
    Control::Base(TrackControl::Pan),
    Control::Base(TrackControl::SendA),
    Control::Base(TrackControl::SendB),
    Control::Mute,
    Control::Solo,
];

/// The mixer's row it opens on, its `Level`, as it's for the mix.
pub const MIXER_START: usize = 1;

/// The mixer's columns, as rows like a page's.
pub const MIXER_ROWS: [&[Control]; 1] = [&MIXER];

/// How many columns of settings a device has, its grid's.
pub const DEVICE_COLUMNS: usize = GRID_COLUMNS;
/// How many rows the cursor goes through in a device: its title, then its grid's.
pub const DEVICE_ROWS: usize = 1 + GRID_ROWS;

/// One device in a page's rack, as the cursor and the page see it: which it is, the controls in
/// its title over the grid's first columns, and its settings grid, each place a control or a hole
/// the cursor steps over. Every device has as many rows, whatever it holds, so moving between
/// devices keeps the cursor's row and column. The FX page's slots are the first devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSpec {
    /// Which device it is, such as a slot's digit, dim at the start of its title.
    pub identity: String,
    /// Whether `b` can switch it out, which gives its title a switch.
    pub switchable: bool,
    pub title: [Option<Control>; DEVICE_COLUMNS],
    pub grid: [[Option<Control>; DEVICE_COLUMNS]; GRID_ROWS],
}

impl DeviceSpec {
    /// A slot on the FX page: its effect over the grid's first column and its type over the
    /// second, then its effect's grid. An empty slot has only its effect.
    pub fn slot(slot: usize, insert: Insert) -> Self {
        let mut title = [None; DEVICE_COLUMNS];
        title[0] = Some(Control::Insert(slot));
        title[1] = insert.type_param().map(|at| Control::InsertParam(slot, at));
        let grid = insert.grid();
        Self {
            identity: (slot + 1).to_string(),
            switchable: insert != Insert::Empty,
            title,
            grid: std::array::from_fn(|row| {
                grid.row(row)
                    .map(|at| at.map(|at| Control::InsertParam(slot, at)))
            }),
        }
    }

    /// Its rows as the cursor goes through them: its title, then its grid's.
    pub fn rows(&self) -> impl Iterator<Item = [Option<Control>; DEVICE_COLUMNS]> + '_ {
        std::iter::once(self.title).chain(self.grid.iter().copied())
    }
}

/// Where the cursor is in a page's rack: in which device, on which of its rows, the title being 0,
/// and in which column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceCursor {
    pub device: usize,
    pub row: usize,
    pub column: usize,
}

const fn base(control: TrackControl) -> Control {
    Control::Base(control)
}

/// The Sound page of a track playing a sample, top to bottom, with output on the right: its
/// `Sample`, then the sampler's `Slice`, `Start`, `Length` and `Play`, as a synth's settings are,
/// with the filter below them as on a synth with more than four.
pub const SAMPLE_ROWS: [&[Control]; 13] = [
    &[Control::Machine, Control::Trim],
    &[Control::Sample, Control::Level],
    &[Control::Setting(1), base(TrackControl::Pan)],
    &[Control::Setting(2), base(TrackControl::SendA)],
    &[Control::Setting(3), base(TrackControl::SendB)],
    &[Control::Setting(4), Control::Group],
    &[Control::FilterType, base(TrackControl::Pitch)],
    &[Control::FilterMode, Control::Groove],
    &[base(TrackControl::Cutoff), Control::Choke],
    &[base(TrackControl::Resonance), Control::GateLength],
    &[base(TrackControl::Drive), Control::Speed],
    &[base(TrackControl::LowCut), Control::Reset],
    &[base(TrackControl::HighCut), Control::Glide],
];
/// The Sound page of a track playing a synth with four settings: its settings down the left, and
/// output on the right, which is a row longer.
pub const SYNTH_ROWS: [&[Control]; 13] = [
    &[Control::Machine, Control::Trim],
    &[Control::Setting(0), Control::Level],
    &[Control::Setting(1), base(TrackControl::Pan)],
    &[Control::Setting(2), base(TrackControl::SendA)],
    &[Control::Setting(3), base(TrackControl::SendB)],
    &[Control::FilterType, Control::Group],
    &[Control::FilterMode, base(TrackControl::Pitch)],
    &[base(TrackControl::Cutoff), Control::Groove],
    &[base(TrackControl::Resonance), Control::Choke],
    &[base(TrackControl::Drive), Control::GateLength],
    &[base(TrackControl::LowCut), Control::Speed],
    &[base(TrackControl::HighCut), Control::Reset],
    &[Control::Glide],
];
/// The same for a synth with more than four, whose later settings carry on down the left.
pub const SYNTH8_ROWS: [&[Control]; 16] = [
    &[Control::Machine, Control::Trim],
    &[Control::Setting(0), Control::Level],
    &[Control::Setting(1), base(TrackControl::Pan)],
    &[Control::Setting(2), base(TrackControl::SendA)],
    &[Control::Setting(3), base(TrackControl::SendB)],
    &[Control::Setting(4), Control::Group],
    &[Control::Setting(5), base(TrackControl::Pitch)],
    &[Control::Setting(6), Control::Groove],
    &[Control::Setting(7), Control::Choke],
    &[Control::FilterType, Control::GateLength],
    &[Control::FilterMode, Control::Speed],
    &[base(TrackControl::Cutoff), Control::Reset],
    &[base(TrackControl::Resonance), Control::Glide],
    &[base(TrackControl::Drive)],
    &[base(TrackControl::LowCut)],
    &[base(TrackControl::HighCut)],
];
/// The same for a synth with three, whose fifth row has only its `Send B`, on the right.
pub const SYNTH3_ROWS: [&[Control]; 13] = [
    &[Control::Machine, Control::Trim],
    &[Control::Setting(0), Control::Level],
    &[Control::Setting(1), base(TrackControl::Pan)],
    &[Control::Setting(2), base(TrackControl::SendA)],
    &[base(TrackControl::SendB)],
    &[Control::FilterType, Control::Group],
    &[Control::FilterMode, base(TrackControl::Pitch)],
    &[base(TrackControl::Cutoff), Control::Groove],
    &[base(TrackControl::Resonance), Control::Choke],
    &[base(TrackControl::Drive), Control::GateLength],
    &[base(TrackControl::LowCut), Control::Speed],
    &[base(TrackControl::HighCut), Control::Reset],
    &[Control::Glide],
];
/// The Sound page of a return, which has no machine or filter.
pub const RETURN_ROWS: [&[Control]; 3] = [
    &[Control::Trim],
    &[Control::Level],
    &[base(TrackControl::Pan)],
];
/// The Sound page of a group, which sends to the returns as well.
pub const GROUP_ROWS: [&[Control]; 5] = [
    &[Control::Trim],
    &[Control::Level],
    &[base(TrackControl::Pan)],
    &[base(TrackControl::SendA)],
    &[base(TrackControl::SendB)],
];
/// The Sound page of the master.
pub const MASTER_ROWS: [&[Control]; 2] = [&[Control::Trim], &[Control::Level]];
/// The Mod page's rows, top to bottom.
pub static MOD_ROWS: [&[Control]; TRACK_ENVELOPES + LFOS_PER_TRACK + 2] = {
    let mut rows: [&[Control]; TRACK_ENVELOPES + LFOS_PER_TRACK + 2] =
        [&[]; TRACK_ENVELOPES + LFOS_PER_TRACK + 2];
    let mut row = 0;
    while row < FIXED_MOD_ROWS.len() {
        rows[row] = FIXED_MOD_ROWS[row];
        row += 1;
    }
    rows[row] = &ACCUMULATOR;
    rows[row + 1] = &ARPEGGIO;
    rows
};
/// The Mod page of a return or the master, which have no notes for an accumulator or an arpeggio
/// to play: each envelope's row, then each LFO's.
pub static FIXED_MOD_ROWS: [&[Control]; TRACK_ENVELOPES + LFOS_PER_TRACK] = {
    let mut rows: [&[Control]; TRACK_ENVELOPES + LFOS_PER_TRACK] =
        [&[]; TRACK_ENVELOPES + LFOS_PER_TRACK];
    let mut slot = 0;
    while slot < TRACK_ENVELOPES {
        rows[slot] = &ENVELOPES[slot];
        slot += 1;
    }
    let mut slot = 0;
    while slot < LFOS_PER_TRACK {
        rows[TRACK_ENVELOPES + slot] = &LFOS[slot];
        slot += 1;
    }
    rows
};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
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

    /// The page's devices, top to bottom, on a page that's a rack of them: the FX page's slots.
    /// The other pages are rows of settings, for now.
    pub fn devices(self, track: &Track) -> Vec<DeviceSpec> {
        match self {
            Self::Fx => track
                .inserts
                .iter()
                .enumerate()
                .map(|(slot, insert)| DeviceSpec::slot(slot, *insert))
                .collect(),
            Self::Sound | Self::Mod => Vec::new(),
        }
    }

    /// The page's rows, top to bottom, for a track with this machine.
    pub fn rows(self, machine: Machine) -> &'static [&'static [Control]] {
        match (self, machine) {
            (Self::Sound, Machine::Sampler) => &SAMPLE_ROWS,
            (Self::Sound, Machine::Plaits(voice)) if voice_entries(voice).len() > 4 => &SYNTH8_ROWS,
            (Self::Sound, Machine::Plaits(voice)) if voice_entries(voice).len() > 3 => &SYNTH_ROWS,
            (Self::Sound, Machine::Plaits(_)) => &SYNTH3_ROWS,
            (Self::Mod, _) => &MOD_ROWS,
            // It's a rack, whose rows depend on what's in each slot: see `Page::devices`.
            (Self::Fx, _) => &[],
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
            Self::Base(TrackControl::Pitch) => "Pitch",
            Self::Choke => "Choke",
            Self::Group => "Group",
            Self::GateLength => "Gate",
            Self::Speed => "Speed",
            Self::Reset => "Reset",
            Self::Glide => "Glide",
            Self::Machine => "Machine",
            Self::Setting(_) => "Setting",
            Self::Sample => "Sample",
            Self::Mute => "Mute",
            Self::Solo => "Solo",
            Self::FilterType => "Filter",
            Self::FilterMode => "Mode",
            Self::Level => "Level",
            Self::Trim => "Trim",
            Self::Groove => "Groove",
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
            Self::Accumulator(AccumulatorField::Up) => "Up",
            Self::Accumulator(AccumulatorField::Down) => "Down",
            Self::Accumulator(AccumulatorField::Moves) => "Moves",
            Self::Accumulator(AccumulatorField::AtLimit) => "Limit",
            Self::Accumulator(AccumulatorField::Adds) => "Adds",
            Self::Accumulator(AccumulatorField::Reset) => "Reset",
            Self::Arpeggio(ArpField::Rate) => "Rate",
            Self::Arpeggio(ArpField::Mode) => "Mode",
            Self::Arpeggio(ArpField::Octaves) => "Oct",
            Self::Arpeggio(ArpField::Retrig) => "Retrig",
            Self::Insert(_) => "Effect",
            Self::InsertParam(..) => "Setting",
        }
    }

    /// The setting's name on screen, on this track, as a slot's settings depend on what's in it.
    pub fn name_on(self, track: &Track) -> &'static str {
        match self {
            // The status line names it in full, as its column's name is short.
            Self::Accumulator(field) => match field {
                AccumulatorField::Up => "Accumulator up",
                AccumulatorField::Down => "Accumulator down",
                AccumulatorField::Moves => "Accumulator moves",
                AccumulatorField::AtLimit => "Accumulator at its limit",
                AccumulatorField::Adds => "Accumulator adds",
                AccumulatorField::Reset => "Accumulator starts again",
            },
            Self::Arpeggio(field) => match field {
                ArpField::Rate => "Arp rate",
                ArpField::Mode => "Arp mode",
                ArpField::Octaves => "Arp octaves",
                ArpField::Retrig => "Arp retrig",
            },
            Self::InsertParam(slot, param) => track.inserts[slot]
                .params()
                .get(param)
                .map_or_else(|| self.label(), |found| found.name),
            _ => self.label_on(track),
        }
    }

    /// Which column of the Sound page it's in: output on the right, everything else on the left.
    pub fn column(self) -> usize {
        match self {
            Self::Level
            | Self::Trim
            | Self::Groove
            | Self::Choke
            | Self::Group
            | Self::GateLength
            | Self::Speed
            | Self::Reset
            | Self::Glide
            | Self::Base(
                TrackControl::Pan | TrackControl::SendA | TrackControl::SendB | TrackControl::Pitch,
            ) => 1,
            _ => 0,
        }
    }

    /// The setting this is on this track, and its value there, for the status line to show in
    /// its units. The filters' drive and cuts aren't settings yet.
    pub fn value_on(self, track: &Track) -> Option<(SettingId, u16)> {
        let base = |id: SettingId, value: u8| {
            Some((id, u16::from(value) << 8 | u16::from(track.fine(id))))
        };
        match self {
            Self::Setting(_) => self
                .setting_on(track)
                .and_then(|id| base(id, track.setting(id))),
            Self::Lfo(lfo, LfoField::Depth) => {
                Some((SettingId::lfo(lfo, 0)?, depth_value(track.lfos[lfo].depth)))
            }
            Self::Lfo(lfo, LfoField::Phase) => {
                Some((SettingId::lfo(lfo, 1)?, phase_value(track.lfos[lfo].phase)))
            }
            Self::Level => base(SettingId::VOL, track.level),
            Self::Base(TrackControl::Cutoff) => base(SettingId::CUT, track.cutoff),
            Self::Base(TrackControl::Resonance) => base(SettingId::RES, track.resonance),
            Self::Base(TrackControl::Pan) => base(SettingId::PAN, track.pan),
            Self::Base(TrackControl::SendA) => base(SettingId::SEND_A, track.send_a),
            Self::Base(TrackControl::SendB) => base(SettingId::SEND_B, track.send_b),
            Self::Base(TrackControl::Pitch) => base(SettingId::PITCH, track.pitch),
            Self::GateLength => Some((SettingId::GATE_LENGTH, u16::from(track.gate_length()) << 8)),
            Self::Speed => Some((SettingId::SPEED, u16::from(track.speed.index()) << 8)),
            Self::Glide => Some((SettingId::GLIDE, u16::from(track.glide) << 8)),
            // What it plays, which is its type's first where its type doesn't offer the track's.
            Self::FilterMode if track.filter.has_modes() => Some((
                SettingId::FILTER_MODE,
                u16::from(track.filter.plays(track.filter_mode).index()) << 8,
            )),
            Self::InsertParam(slot, param) => {
                let insert = track.inserts[slot];
                let setting = insert.params().get(param)?.setting;
                let effect = Effect::of(insert)?;
                let at = effect
                    .params()
                    .iter()
                    .position(|found| found.setting == setting)?;
                base(SettingId::slot(slot, effect, at)?, insert.value(param))
            }
            _ => None,
        }
    }

    /// The setting whose fine part this shows on the Sound page, if it has one: the track's own
    /// that steps can set too, and its machine's that sweep. An effect's settings are whole, as
    /// effects only hear those.
    pub fn fine_on(self, track: &Track) -> Option<SettingId> {
        let id = match self {
            Self::Base(which) => which.setting()?,
            Self::Setting(_) => self.setting_on(track)?,
            _ => return None,
        };
        let sweeps = matches!(id.entry().kind, Kind::Continuous { .. });
        (sweeps && self.exists_on(track)).then_some(id)
    }

    /// The setting of the track's machine this is, if it's one.
    pub fn setting_on(self, track: &Track) -> Option<SettingId> {
        match self {
            Self::Setting(setting) => track.machine_setting(setting),
            _ => None,
        }
    }

    /// Whether this track has the setting: a slot only has the settings of what's in it, only a
    /// sound track or a group sends, and the master can't be panned, muted or soloed.
    pub fn exists_on(self, track: &Track) -> bool {
        match self {
            Self::InsertParam(slot, param) => param < track.inserts[slot].params().len(),
            Self::Base(TrackControl::SendA | TrackControl::SendB) => {
                track.is_sound() || track.is_group()
            }
            Self::Base(TrackControl::Pitch)
            | Self::Choke
            | Self::Group
            | Self::GateLength
            | Self::Speed
            | Self::Reset
            | Self::Glide
            | Self::Accumulator(_)
            | Self::Arpeggio(_) => track.is_sound(),
            Self::Base(TrackControl::Pan) | Self::Mute | Self::Solo => {
                track.kind != TrackKind::Master
            }
            Self::Setting(_) => self.setting_on(track).is_some(),
            _ => true,
        }
    }

    /// The setting's name on screen, on this track, as each machine names its own.
    pub fn label_on(self, track: &Track) -> &'static str {
        self.setting_on(track)
            .map_or_else(|| self.label(), SettingId::name)
    }

    /// Whether this is two hex digits, entered as soon as the second is typed.
    fn is_hex(self) -> bool {
        matches!(
            self,
            Self::Base(_) | Self::Level | Self::Trim | Self::InsertParam(..) | Self::Setting(_)
        )
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
                | Self::Sample
                | Self::Mute
                | Self::Solo
                | Self::FilterType
                | Self::FilterMode
                | Self::Groove
                | Self::Speed
                | Self::Lfo(
                    _,
                    LfoField::Destination | LfoField::Shape | LfoField::Retrig
                )
                | Self::Envelope(_, EnvelopeField::Destination | EnvelopeField::Reset)
                | Self::Accumulator(
                    AccumulatorField::Moves
                        | AccumulatorField::AtLimit
                        | AccumulatorField::Adds
                        | AccumulatorField::Reset
                )
                | Self::Arpeggio(ArpField::Rate | ArpField::Mode | ArpField::Retrig)
        )
    }
}

/// What the status line says for `Mode` on a type that has none.
fn no_modes(filter: FilterType) -> String {
    format!("{} has no modes", filter.name())
}

/// The place nearest a row and field that holds a setting: in a row with none, the nearest row
/// above with some, and on a hole, the nearest setting along the row, the left one of two as near.
fn place_in(rows: &[Vec<Option<Control>>], row: usize, field: usize) -> (usize, usize) {
    let mut row = row.min(rows.len().saturating_sub(1));
    while row > 0 && rows[row].iter().all(Option::is_none) {
        row -= 1;
    }
    let places = rows.get(row).map_or(&[][..], Vec::as_slice);
    let field = (0..places.len())
        .filter(|at| places[*at].is_some())
        .min_by_key(|at| (at.abs_diff(field), *at > field))
        .unwrap_or(0);
    (row, field)
}

/// The next item in a list, going round.
fn next_in<T: Copy + PartialEq>(items: &[T], current: T, up: bool) -> T {
    let position = items.iter().position(|item| *item == current).unwrap_or(0);
    let step = if up { 1 } else { items.len() - 1 };
    items[(position + step) % items.len()]
}

/// What a key did on the track page, as far as the page alone can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The page did it.
    Done,
    /// It can't be done here, and the status line says why.
    Refused(&'static str),
    /// It isn't one of the page's own keys: an edit, which needs the song, or what it means
    /// anywhere.
    Passed,
}

/// The track page as a component: which of its pages shows, and where its cursor is, kept while
/// other views are shown. What's on a page depends on the cursor's track, so each move is given
/// that track, or none, and the page holds no song of its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackPage {
    /// The page showing: Sound, Mod or FX.
    pub page: Page,
    /// The row and field the cursor is kept at. It can be on a hole or past the end of a shorter
    /// row, as after `}` onto an empty slot, so where it's seen is [`TrackPage::place`].
    pub row: usize,
    pub field: usize,
    /// Where `j` and `k` last left the cursor, as its row and field, and the field it was on before
    /// a shorter row moved it in. Moving up or down from there goes back to that field where a row
    /// has it, as Vim keeps its column.
    pub(crate) wanted: Option<(usize, usize, usize)>,
    /// Whether the cursor is on the fine digits of the setting.
    pub fine: bool,
}

impl TrackPage {
    /// The page's devices, on a page that's a rack of them.
    pub fn devices(&self, track: Option<&Track>) -> Vec<DeviceSpec> {
        track.map_or_else(Vec::new, |track| self.page.devices(track))
    }

    /// The rows of the page showing, which depend on the track's kind and machine.
    pub fn page_rows(&self, track: Option<&Track>) -> &'static [&'static [Control]] {
        let kind = track.map_or(TrackKind::Sound, |track| track.kind);
        match (self.page, kind) {
            (Page::Sound, TrackKind::Group(_)) => &GROUP_ROWS,
            (Page::Sound, TrackKind::ReturnA | TrackKind::ReturnB) => &RETURN_ROWS,
            (Page::Sound, TrackKind::Master) => &MASTER_ROWS,
            (Page::Mod, kind) if kind != TrackKind::Sound => &FIXED_MOD_ROWS,
            _ => self
                .page
                .rows(track.map_or(Machine::Sampler, |track| track.machine)),
        }
    }

    /// The page's rows as the cursor goes through them, each a list of places that can be holes it
    /// steps over. On a rack they're each device's in turn. Elsewhere they're each row's settings
    /// up to the first this track doesn't have.
    pub fn rows(&self, track: Option<&Track>) -> Vec<Vec<Option<Control>>> {
        let devices = self.devices(track);
        if !devices.is_empty() {
            return devices
                .iter()
                .flat_map(DeviceSpec::rows)
                .map(Vec::from)
                .collect();
        }
        self.page_rows(track)
            .iter()
            .map(|row| {
                row[..fields_in(row, track)]
                    .iter()
                    .copied()
                    .map(Some)
                    .collect()
            })
            .collect()
    }

    /// Where the cursor is, as a row and a place in it that hold a setting. In a row with none,
    /// such as an empty slot's grid, it's the nearest row above with some, which in a slot is its
    /// title at least. On a hole, it's the nearest setting along the row, the left one of two as
    /// near.
    pub fn place(&self, track: Option<&Track>) -> (usize, usize) {
        place_in(&self.rows(track), self.row, self.field)
    }

    /// What the cursor is on.
    pub fn control(&self, track: Option<&Track>) -> Control {
        let (row, field) = self.place(track);
        let rows = self.rows(track);
        let place = rows
            .get(row)
            .and_then(|row| row.get(field).copied().flatten());
        place.unwrap_or(Control::Machine)
    }

    /// Where the cursor is in the page's rack, on a page that's one.
    pub fn device_cursor(&self, track: Option<&Track>) -> Option<DeviceCursor> {
        if self.devices(track).is_empty() {
            return None;
        }
        let (row, column) = self.place(track);
        Some(DeviceCursor {
            device: row / DEVICE_ROWS,
            row: row % DEVICE_ROWS,
            column,
        })
    }

    /// Put the cursor at the place nearest a row and field that holds a setting, off any fine
    /// digits.
    pub fn move_to(&mut self, track: Option<&Track>, row: usize, field: usize) {
        (self.row, self.field) = place_in(&self.rows(track), row, field);
        self.fine = false;
    }

    /// Run one of the page's own keys: the moves, `{` and `}` across a rack's devices, and the
    /// keys to the pages either side. `times` is its count, at least 1.
    pub fn key(&mut self, cmd: Cmd, times: usize, track: Option<&Track>) -> Outcome {
        let field = self.field;
        match cmd {
            Cmd::Move(Motion::Down) => (0..times).for_each(|_| self.step_row(1, track)),
            Cmd::Move(Motion::Up) => (0..times).for_each(|_| self.step_row(-1, track)),
            Cmd::Move(Motion::NextBeat | Motion::PreviousBeat)
                if self.devices(track).is_empty() =>
            {
                return Outcome::Refused("{ and } go between slots on the FX page");
            }
            Cmd::Move(Motion::NextBeat) => self.move_device(times as isize, track),
            Cmd::Move(Motion::PreviousBeat) => self.move_device(-(times as isize), track),
            Cmd::Move(Motion::First) => self.move_row_keeping_field(0, track),
            Cmd::Move(Motion::Last) => self.move_row_keeping_field(usize::MAX, track),
            Cmd::Move(Motion::Right) => (0..times).for_each(|_| self.step_right(track)),
            Cmd::Move(Motion::Left) => (0..times).for_each(|_| self.step_left(track)),
            Cmd::Move(Motion::RowStart) => self.row_end(false, track),
            Cmd::Move(Motion::RowEnd) => self.row_end(true, track),
            Cmd::PreviousPage | Cmd::NextPage => {
                self.page = self.page.beside(cmd == Cmd::NextPage);
                self.move_to(track, 0, field);
            }
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// Put the cursor where it's seen, keeping the column `j` and `k` aim for. Left elsewhere,
    /// as after `}` onto an empty slot, an edit could bring back the setting it was kept for.
    pub fn settle(&mut self, track: Option<&Track>) {
        let wanted = self.wanted_field();
        let place = self.place(track);
        if place != (self.row, self.field) {
            (self.row, self.field) = place;
            self.wanted = Some((place.0, place.1, wanted));
        }
    }

    /// Whether the setting under the cursor has fine digits to stop on.
    fn has_fine_here(&self, track: Option<&Track>) -> bool {
        let control = self.control(track);
        match track {
            Some(track) => control.fine_on(track).is_some(),
            None => control.fine_on(&Track::default()).is_some(),
        }
    }

    /// `l`: onto the fine digits of a setting that has them, and then on to the next, over any
    /// holes.
    fn step_right(&mut self, track: Option<&Track>) {
        if !self.fine && self.has_fine_here(track) {
            self.fine = true;
            return;
        }
        let (row, field) = self.place(track);
        let next = self.rows(track)[row]
            .iter()
            .enumerate()
            .skip(field + 1)
            .find_map(|(at, place)| place.and(Some(at)));
        self.move_to(track, row, next.unwrap_or(field));
    }

    /// `h`: back off the fine digits, and onto the previous setting's if it has them.
    fn step_left(&mut self, track: Option<&Track>) {
        if self.fine {
            self.fine = false;
            return;
        }
        let (row, field) = self.place(track);
        let before = self.rows(track)[row][..field]
            .iter()
            .rposition(Option::is_some);
        self.move_to(track, row, before.unwrap_or(field));
        self.fine = before.is_some() && self.has_fine_here(track);
    }

    /// Move a row up or down, past any with nothing in them, as an empty slot's grid is, aiming
    /// for the field `j` and `k` aim for.
    fn step_row(&mut self, by: isize, track: Option<&Track>) {
        let wanted = self.wanted_field();
        let rows = self.rows(track);
        let mut row = self.place(track).0;
        while let Some(next) = row.checked_add_signed(by).filter(|next| *next < rows.len()) {
            row = next;
            if rows[row].iter().any(Option::is_some) {
                self.move_to(track, row, wanted);
                break;
            }
        }
        self.wanted = Some((self.row, self.field, wanted));
    }

    /// The field `j` and `k` aim for: the one before a shorter row moved the cursor in, if it hasn't
    /// moved since.
    fn wanted_field(&self) -> usize {
        match self.wanted {
            Some((row, field, wanted)) if (row, field) == (self.row, self.field) => wanted,
            _ => self.field,
        }
    }

    /// Up or down to a row, aiming for the field `j` and `k` aim for.
    fn move_row_keeping_field(&mut self, row: usize, track: Option<&Track>) {
        let wanted = self.wanted_field();
        self.move_to(track, row, wanted);
        self.wanted = Some((self.row, self.field, wanted));
    }

    /// `0` and `$`: the first or the last setting on the cursor's row, over any holes.
    fn row_end(&mut self, last: bool, track: Option<&Track>) {
        let row = self.place(track).0;
        let field = if last { usize::MAX } else { 0 };
        self.move_to(track, row, field);
    }

    /// Go to a device some way from the cursor's, on the same row and column, stopping at either
    /// end of the rack. The row and column are kept as they were, so they come back after a
    /// device that hasn't got them, such as an empty slot.
    fn move_device(&mut self, by: isize, track: Option<&Track>) {
        let count = self.devices(track).len();
        let device = (self.row / DEVICE_ROWS)
            .saturating_add_signed(by)
            .min(count.saturating_sub(1));
        self.row = device * DEVICE_ROWS + self.row % DEVICE_ROWS;
    }
}

/// How many settings a row of a page other than FX has on this track.
fn fields_in(row: &[Control], track: Option<&Track>) -> usize {
    track.map_or(row.len(), |track| {
        row.iter()
            .take_while(|control| control.exists_on(track))
            .count()
            .max(1)
    })
}

impl App {
    /// What the cursor is on, on the track page or the mixer.
    pub fn track_control(&self) -> Control {
        if self.view == View::Mixer {
            return MIXER[self.mixer_row_here()];
        }
        self.track_page.control(self.track_here())
    }

    /// Put the track page's cursor at the place nearest a row and field that holds a setting.
    pub(crate) fn move_control(&mut self, row: usize, field: usize) {
        let song = Arc::clone(self.song());
        self.track_page
            .move_to(song.tracks.get(self.cursor.track), row, field);
    }

    /// The mixer's rows this track has: a fixed track has no sends, and the master no pan, mute or
    /// solo.
    fn mixer_rows_here(&self) -> Vec<usize> {
        let track = self.song().tracks.get(self.cursor.track);
        (0..MIXER.len())
            .filter(|row| track.is_none_or(|track| MIXER[*row].exists_on(track)))
            .collect()
    }

    /// The row the cursor is on. On a track without the row it was on, it's the nearest above it,
    /// and the row it was on is kept, so going back to a track that has it finds it again.
    fn mixer_row_here(&self) -> usize {
        let rows = self.mixer_rows_here();
        let wanted = self.mixer_row.min(MIXER.len() - 1);
        rows.iter()
            .rev()
            .find(|row| **row <= wanted)
            .or(rows.first())
            .copied()
            .unwrap_or(0)
    }

    /// Go to the first or last track that has the row the cursor is on, staying on that row.
    fn mixer_row_end(&mut self, last: bool) {
        let row = self.mixer_row_here();
        let has = |track: &Track| MIXER[row].exists_on(track);
        let tracks = &self.song().tracks;
        // The first or last strip shown, as a folded one is stepped over.
        let found = {
            let mut shown = self
                .folds
                .shown(self.song())
                .filter(|&track| has(&tracks[track]));
            if last { shown.last() } else { shown.next() }
        };
        if let Some(to) = found {
            self.go_to_track_at(to);
            self.mixer_row = row;
        }
    }

    /// Move up or down the mixer's rows, going only through those this track has.
    fn move_mixer_row(&mut self, by: isize) {
        let rows = self.mixer_rows_here();
        let here = self.mixer_row_here();
        let at = rows.iter().position(|row| *row == here).unwrap_or(0);
        let to = (at as isize)
            .saturating_add(by)
            .clamp(0, rows.len() as isize - 1);
        self.mixer_row = rows.get(to as usize).copied().unwrap_or(0);
    }

    fn page_track(&self) -> Track {
        self.song()
            .tracks
            .get(self.cursor.track)
            .cloned()
            .unwrap_or_default()
    }

    /// Put the track in a group, or take it out of any.
    fn set_group_here(&mut self, group: Option<u8>) {
        self.apply(&Action::SetTrackGroup {
            first: self.cursor.track,
            last: self.cursor.track,
            group,
        });
    }

    fn set_lfo(&mut self, slot: usize, lfo: Lfo) {
        self.apply(&Action::SetLfo {
            track: self.cursor.track,
            slot,
            lfo,
        });
    }

    /// Set how many of the song's steps the track's chain plays before starting again, or none.
    fn set_reset(&mut self, reset: Option<u16>) {
        self.apply(&Action::SetTrackReset {
            track: self.cursor.track,
            reset,
        });
    }

    /// Set how long the track's ties glide into the next note.
    fn set_glide(&mut self, glide: u8) {
        self.apply(&Action::SetTrackGlide {
            track: self.cursor.track,
            glide,
        });
    }

    fn set_hex(&mut self, control: Control, value: u8) {
        let track = self.cursor.track;
        // On the mixer, the cursor can be on a setting this track doesn't have.
        let tracks = &self.song().tracks;
        if tracks.get(track).is_some_and(|t| !control.exists_on(t)) {
            return;
        }
        if let Some(setting) = tracks.get(track).and_then(|t| control.setting_on(t)) {
            self.apply(&Action::SetTrackSetting {
                track,
                setting,
                value,
            });
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
            Control::Trim => self.apply(&Action::SetTrackTrim { track, trim: value }),
            _ => {}
        }
    }

    fn adjust_control(&mut self, up: bool, count: usize, large: bool) {
        let control = self.track_control();
        let track = self.page_track();
        // A machine's setting stops at its own top, as a list stops at its last choice.
        let top = control
            .setting_on(&track)
            .map_or(0xFF, |id| id.entry().kind.top());
        let change = |value: u8| {
            let size = if large { HEX_JUMP } else { 1 };
            let moved = i64::from(value.min(top)) + if up { size } else { -size } * count as i64;
            moved.clamp(0, i64::from(top)) as u8
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
            Control::Setting(_) => {
                if let Some(id) = control.setting_on(&track) {
                    self.set_hex(control, change(track.setting(id)));
                }
            }
            Control::Machine => {
                let machine = next_in(&Machine::ALL, track.machine, up);
                self.apply(&Action::SetTrackMachine {
                    track: self.cursor.track,
                    machine,
                });
            }
            Control::Sample => {
                let size = if large { HEX_JUMP as usize } else { 1 };
                let sample = self.next_sample(track.sample(), up, size * count);
                self.apply(&Action::SetTrackSample {
                    track: self.cursor.track,
                    sample,
                });
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
                    TrackControl::Pitch => track.pitch,
                };
                self.set_hex(control, change(value));
            }
            Control::Level => self.set_hex(control, change(track.level)),
            Control::Trim => self.set_hex(control, change(track.trim)),
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
            Control::FilterMode => {
                let modes = track.filter.modes();
                if modes.is_empty() {
                    self.status = no_modes(track.filter);
                    return;
                }
                let mode = next_in(modes, track.filter.plays(track.filter_mode), up);
                self.apply(&Action::SetFilterMode {
                    track: self.cursor.track,
                    mode,
                });
            }
            Control::Choke => {
                let from = i64::from(track.choke.unwrap_or(0));
                let moved = (from + if up { 1 } else { -1 } * count.min(0x100) as i64)
                    .clamp(0, i64::from(CHOKE_GROUPS)) as u8;
                self.apply(&Action::SetTrackChoke {
                    track: self.cursor.track,
                    choke: (moved > 0).then_some(moved),
                });
            }
            Control::Group => {
                let from = i64::from(track.group.unwrap_or(0));
                let moved = (from + if up { 1 } else { -1 } * count.min(0x100) as i64)
                    .clamp(0, MAX_GROUPS as i64) as u8;
                self.set_group_here((moved > 0).then_some(moved));
            }
            Control::GateLength => {
                let size = if large { GATE_JUMP } else { 1 };
                let by = size * count.min(100) as i32;
                let from = i32::from(gate_percent(track.gate_length()));
                let moved = from + if up { by } else { -by };
                self.apply(&Action::SetTrackGateLength {
                    track: self.cursor.track,
                    gate_length: Some(gate_value(moved.clamp(1, 100) as u8)),
                });
            }
            Control::Glide => {
                let size = if large { HEX_JUMP } else { 1 };
                let by = size * count.min(0x100) as i64;
                let moved = i64::from(track.glide) + if up { by } else { -by };
                self.set_glide(moved.clamp(0, 0xFF) as u8);
            }
            Control::Reset => {
                // A step at a time, or a bar to the next whole bar, with off below 1.
                let from = u32::from(track.reset.unwrap_or(0));
                let (bar, by) = (
                    BAR_STEPS as u32,
                    count.min(usize::from(MAX_TRACK_RESET)) as u32,
                );
                let moved = match (large, up) {
                    (false, true) => from + by,
                    (false, false) => from.saturating_sub(by),
                    (true, true) => (from / bar + by) * bar,
                    (true, false) => from.div_ceil(bar).saturating_sub(by) * bar,
                };
                let moved = moved.min(u32::from(MAX_TRACK_RESET)) as u16;
                self.set_reset((moved > 0).then_some(moved));
            }
            Control::Speed => {
                // One way at a time, stopping at either end, as the list runs short to long.
                let at = usize::from(track.speed.index());
                let speed = Speed::from_index(if up { at + 1 } else { at.saturating_sub(1) });
                self.apply(&Action::SetTrackSpeed {
                    track: self.cursor.track,
                    speed,
                });
            }
            Control::Groove => {
                // One at a time, from what it plays, as a large step would skip through the list.
                let from = track.groove.unwrap_or(self.song().groove).index();
                let last = GrooveId::all().count() - 1;
                let to = if up {
                    (from + 1).min(last)
                } else {
                    from.saturating_sub(1)
                };
                if track.groove.is_none() || to != from {
                    self.set_track_groove(GrooveId::from_index(to));
                }
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
            Control::Accumulator(field) => {
                let mut accumulation = track.accumulation;
                for _ in 0..count.min(0x100) {
                    adjust_accumulation(&mut accumulation, field, up, large);
                }
                self.set_accumulation(accumulation);
            }
            Control::Arpeggio(field) => {
                let mut arpeggio = track.arpeggio;
                for _ in 0..count.min(0x10) {
                    adjust_arpeggio(&mut arpeggio, field, up);
                }
                self.set_arpeggio(arpeggio);
            }
        }
    }

    fn set_accumulation(&mut self, accumulation: Accumulation) {
        if accumulation != self.page_track().accumulation {
            self.apply(&Action::SetAccumulation {
                track: self.cursor.track,
                accumulation,
            });
        }
    }

    fn set_arpeggio(&mut self, arpeggio: Arpeggio) {
        if arpeggio != self.page_track().arpeggio {
            self.apply(&Action::SetArpeggio {
                track: self.cursor.track,
                arpeggio,
            });
        }
    }

    /// The slot the cursor is in, on the FX page.
    fn fx_slot(&self) -> usize {
        self.track_page
            .device_cursor(self.track_here())
            .map_or(0, |cursor| cursor.device)
    }

    /// `b`: switch this slot out to hear the track without it, or back in.
    fn toggle_bypass(&mut self) {
        let slot = self.fx_slot();
        let track = self.page_track();
        let insert = track.inserts[slot];
        if insert == Insert::Empty {
            self.status = "There's nothing in this slot to bypass".to_owned();
            return;
        }
        let bypassed = !track.bypassed[slot];
        self.apply(&Action::SetBypass {
            track: self.cursor.track,
            slot,
            bypassed,
        });
        self.status = if bypassed {
            format!("Bypassed {}: b brings it back", insert.name())
        } else {
            format!("{} is back in", insert.name())
        };
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
        self.move_control(slot * DEVICE_ROWS, 0);
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

    /// Move the fine part of the setting under the cursor, carrying into its value past `FF` or
    /// below `00`, as one undo step.
    fn adjust_fine(&mut self, up: bool, count: usize, large: bool) {
        let control = self.track_control();
        let track = self.page_track();
        let Some(id) = control.fine_on(&track) else {
            return;
        };
        let Some(coarse) = track.coarse(id) else {
            return;
        };
        let size = if large { HEX_JUMP } else { 1 } * count as i64;
        let value = i64::from(coarse) << 8 | i64::from(track.fine(id));
        let top = i64::from(id.entry().kind.top()) << 8;
        let moved = (value + if up { size } else { -size }).clamp(0, top);
        let (coarse_now, fine) = ((moved >> 8) as u8, (moved & 0xFF) as u8);
        let index = self.cursor.track;
        let mut actions = Vec::new();
        if coarse_now != coarse {
            actions.push(match control {
                Control::Base(control) => Action::SetTrackBase {
                    track: index,
                    control,
                    value: coarse_now,
                },
                _ => Action::SetTrackSetting {
                    track: index,
                    setting: id,
                    value: coarse_now,
                },
            });
        }
        actions.push(Action::SetTrackFine {
            track: index,
            setting: id,
            fine,
        });
        self.apply(&Action::Batch(actions));
    }

    /// Set the fine part of the setting under the cursor.
    fn set_fine_here(&mut self, fine: u8) {
        if let Some(setting) = self.track_control().fine_on(&self.page_track()) {
            self.apply(&Action::SetTrackFine {
                track: self.cursor.track,
                setting,
                fine,
            });
        }
    }

    /// Run a command on the mixer. `h` and `l` go between its strips, and `j` and `k` up and down
    /// them. Everything else means what it does on the track page.
    pub(crate) fn run_mixer(&mut self, cmd: Cmd) -> bool {
        let times = self.count.unwrap_or(1).max(1) as isize;
        match cmd {
            Cmd::Move(Motion::Right) => self.move_track(times),
            Cmd::Move(Motion::Left) => self.move_track(-times),
            Cmd::Move(Motion::RowStart) => self.mixer_row_end(false),
            Cmd::Move(Motion::RowEnd) => self.mixer_row_end(true),
            Cmd::Move(Motion::Down) => self.move_mixer_row(times),
            Cmd::Move(Motion::Up) => self.move_mixer_row(-times),
            Cmd::Move(Motion::First) => self.move_mixer_row(isize::MIN),
            Cmd::Move(Motion::Last) => self.move_mixer_row(isize::MAX),
            _ => return self.run_track_page(cmd),
        }
        self.count = None;
        true
    }

    /// Run a command on the track page. Returns false if it means the same as anywhere.
    pub(crate) fn run_track_page(&mut self, cmd: Cmd) -> bool {
        let song = Arc::clone(self.song());
        let track = song.tracks.get(self.cursor.track);
        // Only `{` and `}` go on from where the cursor was kept, so they come back to it.
        let kept = self.track_page.clone();
        let braces = matches!(cmd, Cmd::Move(Motion::NextBeat | Motion::PreviousBeat));
        if !braces && self.view != View::Mixer {
            self.track_page.settle(track);
        }
        let count = self.count.take();
        let times = count.unwrap_or(1).max(1);
        match self.track_page.key(cmd, times, track) {
            Outcome::Done => return true,
            Outcome::Refused(why) => {
                self.status = why.to_owned();
                return true;
            }
            Outcome::Passed => {}
        }
        let fine = self.track_page.fine;
        match cmd {
            Cmd::FieldJump => self.open_field_jump(),
            Cmd::Audition => {
                self.mode = Mode::Keys;
                self.status = "a w s e d f play, < > change octave, ; stops".to_owned();
            }
            Cmd::RemoveRow | Cmd::Paste if self.track_page.page != Page::Fx => {
                self.status = "dd and p move effects, on the FX page".to_owned();
            }
            Cmd::Bypass if self.track_page.page != Page::Fx => {
                self.status = "b bypasses an effect, on the FX page".to_owned();
            }
            Cmd::Bypass => self.toggle_bypass(),
            Cmd::RemoveRow => self.take_insert(),
            Cmd::Paste => self.put_insert(),
            Cmd::Increase if fine => self.adjust_fine(true, times, false),
            Cmd::Decrease if fine => self.adjust_fine(false, times, false),
            Cmd::IncreaseLarge if fine => self.adjust_fine(true, times, true),
            Cmd::DecreaseLarge if fine => self.adjust_fine(false, times, true),
            Cmd::ClearField if fine => self.set_fine_here(0),
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
                // What it means elsewhere, such as going to another track, keeps the cursor
                // where it was kept.
                self.track_page = kept;
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
                    TrackControl::Pitch => track.pitch,
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
            Control::Setting(_) => {
                if let Some(id) = control.setting_on(&track)
                    && track.setting(id) != id.entry().default
                {
                    self.set_hex(control, id.entry().default);
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
            Control::Sample if track.sample() != 0 => {
                self.apply(&Action::SetTrackSample {
                    track: self.cursor.track,
                    sample: 0,
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
            Control::Trim if track.trim != fresh.trim => self.set_hex(control, fresh.trim),
            Control::Trim => {}
            Control::Groove if track.groove.is_some() => self.set_track_groove(None),
            Control::Groove => {}
            Control::Choke if track.choke.is_some() => {
                self.apply(&Action::SetTrackChoke {
                    track: self.cursor.track,
                    choke: None,
                });
            }
            Control::Choke => {}
            Control::Group if track.group.is_some() => self.set_group_here(None),
            Control::Group => {}
            Control::GateLength if track.gate_length.is_some() => {
                self.apply(&Action::SetTrackGateLength {
                    track: self.cursor.track,
                    gate_length: None,
                });
            }
            Control::GateLength => {}
            Control::Glide if track.glide != fresh.glide => self.set_glide(fresh.glide),
            Control::Glide => {}
            Control::Speed if track.speed != fresh.speed => {
                self.apply(&Action::SetTrackSpeed {
                    track: self.cursor.track,
                    speed: fresh.speed,
                });
            }
            Control::Speed => {}
            Control::Reset if track.reset.is_some() => self.set_reset(None),
            Control::Reset => {}
            Control::FilterType => {
                if track.filter != fresh.filter {
                    self.apply(&Action::SetFilterType {
                        track: self.cursor.track,
                        filter: fresh.filter,
                    });
                }
            }
            Control::FilterMode => {
                if track.filter_mode != fresh.filter_mode {
                    self.apply(&Action::SetFilterMode {
                        track: self.cursor.track,
                        mode: fresh.filter_mode,
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
            Control::Accumulator(field) => {
                let (now, default) = (track.accumulation, Accumulation::default());
                let reset = match field {
                    AccumulatorField::Up => Accumulation {
                        up: default.up,
                        ..now
                    },
                    AccumulatorField::Down => Accumulation {
                        down: default.down,
                        ..now
                    },
                    AccumulatorField::Moves => Accumulation {
                        moves: default.moves,
                        ..now
                    },
                    AccumulatorField::AtLimit => Accumulation {
                        at_limit: default.at_limit,
                        ..now
                    },
                    AccumulatorField::Adds => Accumulation {
                        adds: default.adds,
                        ..now
                    },
                    AccumulatorField::Reset => Accumulation {
                        reset: default.reset,
                        ..now
                    },
                };
                self.set_accumulation(reset);
            }
            Control::Arpeggio(field) => {
                let (now, default) = (track.arpeggio, Arpeggio::default());
                let reset = match field {
                    ArpField::Rate => Arpeggio {
                        rate: default.rate,
                        ..now
                    },
                    ArpField::Mode => Arpeggio {
                        mode: default.mode,
                        ..now
                    },
                    ArpField::Octaves => Arpeggio {
                        octaves: default.octaves,
                        ..now
                    },
                    ArpField::Retrig => Arpeggio {
                        retrig: default.retrig,
                        ..now
                    },
                };
                self.set_arpeggio(reset);
            }
        }
    }

    /// List what an LFO's setting can be, with the current one highlighted.
    /// Switch this track's mute or solo.
    /// The bank entry so many on from this one, up or down, through those the bank has, with
    /// `Off` below the first. It stops at either end.
    fn next_sample(&self, from: u8, up: bool, count: usize) -> u8 {
        let numbers = std::iter::once(0).chain(self.song().bank.iter().map(|(number, _)| number));
        let found = if up {
            numbers.filter(|number| *number > from).take(count).last()
        } else {
            let below: Vec<u8> = numbers.filter(|number| *number < from).collect();
            below.get(below.len().saturating_sub(count)).copied()
        };
        found.unwrap_or(from)
    }

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
            self.open_bank_picker();
            return;
        }
        if self.track_control().setting_on(&self.page_track()) == Some(SettingId::SLICE) {
            self.open_slice_picker();
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
        if self.track_control() == Control::FilterMode {
            self.open_filter_mode_picker();
            return;
        }
        if self.track_control() == Control::Speed {
            self.open_speed_picker();
            return;
        }
        if self.track_control() == Control::Machine {
            self.open_machine_picker();
            return;
        }
        if self.track_control() == Control::Groove {
            self.open_groove_picker(true);
            return;
        }
        if let Some(id) = self.track_control().setting_on(&self.page_track())
            && let Kind::Stepped { choices, .. } = id.entry().kind
        {
            self.open_setting_picker(id, choices);
            return;
        }
        if let Control::Envelope(slot, field) = self.track_control() {
            self.open_envelope_picker(slot, field);
            return;
        }
        if let Control::Accumulator(field) = self.track_control() {
            self.open_accumulator_picker(field);
            return;
        }
        if let Control::Arpeggio(field) = self.track_control() {
            self.open_arpeggio_picker(field);
            return;
        }
        let Control::Lfo(slot, field) = self.track_control() else {
            self.status =
                "K chooses from a list on Machine, Voice, Patch, File, Type, Groove, Speed, Dest, Shape, Rate, Retrig and Reset"
                    .to_owned();
            return;
        };
        let track = self.cursor.track;
        let page = self.page_track();
        let lfo = page.lfos[slot];
        let pick = |lfo| Pick::Lfo { track, slot, lfo };
        let (title, items): (&'static str, Vec<Item>) = match field {
            LfoField::Destination => {
                let off = Item::new(
                    "---",
                    pick(Lfo {
                        destination: None,
                        ..lfo
                    }),
                )
                .meaning("Off");
                let on = SettingId::for_lfo(&page, slot)
                    .into_iter()
                    .map(|destination| {
                        Item::new(
                            destination.name_on(&page),
                            pick(Lfo {
                                destination: Some(destination),
                                ..lfo
                            }),
                        )
                        .meaning(destination.meaning())
                        .category(destination.group())
                    });
                ("LFO destination", std::iter::once(off).chain(on).collect())
            }
            LfoField::Shape => {
                let shapes =
                    Shape::ALL.map(|shape| Item::new(shape.name(), pick(Lfo { shape, ..lfo })));
                ("LFO shape", shapes.to_vec())
            }
            LfoField::Rate => {
                let rates = note_rates()
                    .into_iter()
                    .map(|rate| Item::new(rate.to_string(), pick(Lfo { rate, ..lfo })))
                    .collect();
                ("LFO rate. i types any other", rates)
            }
            LfoField::Retrig => {
                let choices = [false, true].map(|retrig| {
                    let name = if retrig { "on" } else { "off" };
                    Item::new(name, pick(Lfo { retrig, ..lfo }))
                });
                ("LFO retrig", choices.to_vec())
            }
            LfoField::Depth | LfoField::Phase => {
                self.status =
                    "K chooses from a list on Type, Dest, Shape, Rate, Retrig and Reset".to_owned();
                return;
            }
        };
        let position = items.iter().position(|item| item.pick == pick(lfo));
        self.show_picker(Picker::new(title, items).opened_on(position));
    }

    /// List the filter types, with what each does and the current one highlighted.
    /// List the effects a slot can hold, with the current one highlighted.
    fn open_insert_picker(&mut self, slot: usize) {
        let track = self.cursor.track;
        let current = self.page_track().inserts[slot];
        let items = Insert::ALL
            .map(|insert| {
                Item::new(
                    insert.name(),
                    Pick::Insert {
                        track,
                        slot,
                        // Choosing what's already there keeps its settings.
                        insert: if insert.name() == current.name() {
                            current
                        } else {
                            insert
                        },
                    },
                )
                .meaning(insert.meaning())
            })
            .to_vec();
        let position = Insert::ALL
            .iter()
            .position(|insert| insert.name() == current.name());
        self.show_picker(Picker::new("Effect", items).opened_on(position));
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
            .map(|(index, choice)| {
                Item::new(
                    *choice,
                    Pick::Insert {
                        track,
                        slot,
                        insert: current.with_value(param, index as u8),
                    },
                )
            })
            .collect();
        let position = usize::from(current.value(param));
        self.show_picker(Picker::new(found.name, items).opened_on(Some(position)));
    }

    fn open_filter_picker(&mut self) {
        let track = self.cursor.track;
        let current = self.page_track().filter;
        let items = FilterType::ALL
            .map(|filter| {
                Item::new(filter.name(), Pick::FilterType { track, filter })
                    .meaning(filter.meaning())
            })
            .to_vec();
        let position = FilterType::ALL.iter().position(|filter| *filter == current);
        self.show_picker(Picker::new("Filter type", items).opened_on(position));
    }

    /// List what one of the accumulator's settings can be, with what each does and the one it has
    /// highlighted. Its limits are numbers, typed or moved instead.
    fn open_accumulator_picker(&mut self, field: AccumulatorField) {
        let track = self.cursor.track;
        let now = self.page_track().accumulation;
        let pick = |accumulation| Pick::Accumulation {
            track,
            accumulation,
        };
        let (title, items): (&str, Vec<Item>) = match field {
            AccumulatorField::Up | AccumulatorField::Down => {
                self.status = format!(
                    "A limit is a number of degrees from 1 to {MOST_DEGREES}: i types it, = and - move it"
                );
                return;
            }
            AccumulatorField::Moves => (
                "Accumulator moves",
                Moves::ALL
                    .map(|moves| {
                        Item::new(moves.name(), pick(Accumulation { moves, ..now }))
                            .meaning(moves.meaning())
                    })
                    .to_vec(),
            ),
            AccumulatorField::AtLimit => (
                "Accumulator at its limit",
                AtLimit::ALL
                    .map(|at_limit| {
                        Item::new(at_limit.name(), pick(Accumulation { at_limit, ..now }))
                            .meaning(at_limit.meaning())
                    })
                    .to_vec(),
            ),
            AccumulatorField::Adds => (
                "Accumulator adds",
                Adds::ALL
                    .map(|adds| {
                        Item::new(adds.name(), pick(Accumulation { adds, ..now }))
                            .meaning(adds.meaning())
                    })
                    .to_vec(),
            ),
            AccumulatorField::Reset => (
                "Accumulator starts again",
                Reset::all()
                    .map(|reset| {
                        Item::new(reset.name(), pick(Accumulation { reset, ..now }))
                            .meaning(reset.meaning())
                    })
                    .collect(),
            ),
        };
        let position = items.iter().position(|item| item.pick == pick(now));
        self.show_picker(Picker::new(title, items).opened_on(position));
    }

    /// List what one of the arpeggio's settings can be, with what each does and the one it has
    /// highlighted. Its octaves are a number, typed or moved instead.
    fn open_arpeggio_picker(&mut self, field: ArpField) {
        let track = self.cursor.track;
        let now = self.page_track().arpeggio;
        let pick = |arpeggio| Pick::Arpeggio { track, arpeggio };
        let (title, items): (&str, Vec<Item>) = match field {
            ArpField::Octaves => {
                self.status = format!(
                    "Octaves is a number from 1 to {MOST_OCTAVES}: i types it, = and - move it"
                );
                return;
            }
            ArpField::Rate => (
                "Arp rate",
                ArpRate::ALL
                    .map(|rate| {
                        Item::new(rate.name(), pick(Arpeggio { rate, ..now }))
                            .meaning(rate.meaning())
                    })
                    .to_vec(),
            ),
            ArpField::Mode => (
                "Arp mode",
                ArpMode::ALL
                    .map(|mode| {
                        Item::new(mode.name(), pick(Arpeggio { mode, ..now }))
                            .meaning(mode.meaning())
                    })
                    .to_vec(),
            ),
            ArpField::Retrig => (
                "Arp retrig",
                [false, true]
                    .map(|retrig| {
                        let name = if retrig { "on" } else { "off" };
                        Item::new(name, pick(Arpeggio { retrig, ..now }))
                            .meaning(retrig_meaning(retrig))
                    })
                    .to_vec(),
            ),
        };
        let position = items.iter().position(|item| item.pick == pick(now));
        self.show_picker(Picker::new(title, items).opened_on(position));
    }

    /// List the speeds a track's steps can go at, with the one it plays highlighted.
    fn open_speed_picker(&mut self) {
        let track = self.cursor.track;
        let current = self.page_track().speed;
        let items = Speed::ALL
            .map(|speed| {
                Item::new(speed.name(), Pick::Speed { track, speed }).meaning(speed.meaning())
            })
            .to_vec();
        let position = Speed::ALL.iter().position(|speed| *speed == current);
        self.show_picker(Picker::new("Speed", items).opened_on(position));
    }

    /// List the modes the track's filter type offers, with what each does and the one it plays
    /// highlighted.
    fn open_filter_mode_picker(&mut self) {
        let track = self.cursor.track;
        let page = self.page_track();
        let modes = page.filter.modes();
        if modes.is_empty() {
            self.status = no_modes(page.filter);
            return;
        }
        let items = modes
            .iter()
            .map(|mode| {
                Item::new(mode.name(), Pick::FilterMode { track, mode: *mode })
                    .meaning(mode.meaning())
            })
            .collect();
        let playing = page.filter.plays(page.filter_mode);
        let position = modes.iter().position(|mode| *mode == playing);
        self.show_picker(Picker::new("Filter mode", items).opened_on(position));
    }

    /// List the machines, each with its glyph, a sampler, a synth or a drum, and what it does, and
    /// the current one highlighted.
    fn open_machine_picker(&mut self) {
        let track = self.cursor.track;
        let current = self.page_track().machine;
        let items = Machine::ALL
            .map(|machine| {
                let label = format!("{} {}", glyphs::machine(machine), machine.name());
                Item::new(label, Pick::Machine { track, machine }).meaning(machine.meaning())
            })
            .to_vec();
        let position = Machine::ALL.iter().position(|machine| *machine == current);
        self.show_picker(Picker::new("Machine", items).opened_on(position));
    }

    /// List what a machine's setting can be, such as 6-op's patches, with the current one
    /// highlighted.
    fn open_setting_picker(&mut self, setting: SettingId, choices: &'static [&'static str]) {
        let track = self.cursor.track;
        let current = self.page_track().setting(setting);
        let items = (0..choices.len().min(0x100))
            .map(|at| {
                Item::new(
                    format!("{at:02X}  {}", choices[at]),
                    Pick::Setting {
                        track,
                        setting,
                        value: at as u8,
                    },
                )
            })
            .collect();
        self.show_picker(Picker::new(setting.name(), items).opened_on(Some(usize::from(current))));
    }

    /// List what an envelope's setting can be, with the current one highlighted.
    fn open_envelope_picker(&mut self, slot: usize, field: EnvelopeField) {
        let track = self.cursor.track;
        let page = self.page_track();
        let envelope = self.page_track().envelopes[slot];
        let pick = |envelope| Pick::Envelope {
            track,
            slot,
            envelope,
        };
        let (title, items): (&'static str, Vec<Item>) = match field {
            EnvelopeField::Destination => {
                let off = Item::new(
                    "---",
                    pick(Envelope {
                        destination: None,
                        ..envelope
                    }),
                )
                .meaning("Off");
                let on = SettingId::for_envelope(&page)
                    .into_iter()
                    .map(|destination| {
                        Item::new(
                            destination.name_on(&page),
                            pick(Envelope {
                                destination: Some(destination),
                                ..envelope
                            }),
                        )
                        .meaning(destination.meaning())
                        .category(destination.group())
                    });
                (
                    "Envelope destination",
                    std::iter::once(off).chain(on).collect(),
                )
            }
            EnvelopeField::Reset => {
                let choices = [true, false].map(|reset| {
                    let (name, meaning) = if reset {
                        ("on", "Each note starts it from zero")
                    } else {
                        ("off", "Each note goes on from where it is")
                    };
                    Item::new(name, pick(Envelope { reset, ..envelope })).meaning(meaning)
                });
                ("Envelope reset", choices.to_vec())
            }
            _ => {
                self.status = "K chooses from a list on Dest and Reset".to_owned();
                return;
            }
        };
        let position = items.iter().position(|item| item.pick == pick(envelope));
        self.show_picker(Picker::new(title, items).opened_on(position));
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
                    if self.track_page.fine {
                        if let Ok(fine) = u8::from_str_radix(&text, 16) {
                            self.set_fine_here(fine);
                        }
                    } else {
                        self.commit_control(control, &text);
                    }
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
            Control::Base(_) | Control::Level | Control::Trim | Control::Setting(_) => {
                if let Ok(value) = u8::from_str_radix(text, 16) {
                    self.set_hex(control, value);
                }
            }
            Control::Choke if text.eq_ignore_ascii_case("off") => {
                self.apply(&Action::SetTrackChoke {
                    track: self.cursor.track,
                    choke: None,
                });
            }
            Control::Choke => match text.parse::<u8>() {
                Ok(group) if (1..=CHOKE_GROUPS).contains(&group) => {
                    self.apply(&Action::SetTrackChoke {
                        track: self.cursor.track,
                        choke: Some(group),
                    });
                }
                _ => self.status = format!("Choke is a group from 1 to {CHOKE_GROUPS}, or off"),
            },
            Control::Group if text.eq_ignore_ascii_case("off") => self.set_group_here(None),
            Control::Group => {
                let number = text.trim_start_matches(['G', 'g']).parse::<u8>();
                match number {
                    Ok(group) if (1..=MAX_GROUPS).contains(&usize::from(group)) => {
                        self.set_group_here(Some(group));
                    }
                    _ => self.status = format!("Group is a group from 1 to {MAX_GROUPS}, or off"),
                }
            }
            Control::GateLength => match text.trim_end_matches('%').trim().parse::<u8>() {
                Ok(percent) if (1..=100).contains(&percent) => {
                    self.apply(&Action::SetTrackGateLength {
                        track: self.cursor.track,
                        gate_length: Some(gate_value(percent)),
                    });
                }
                _ => self.status = "Gate is a percent from 1 to 100".to_owned(),
            },
            Control::Glide if text.eq_ignore_ascii_case("off") => self.set_glide(0),
            Control::Glide => match text
                .trim_end_matches("ms")
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|ms| (0.0..=LONGEST_GLIDE_MS).contains(ms))
            {
                Some(ms) => self.set_glide(glide_value(ms)),
                None => {
                    self.status = format!("Glide is a time in ms up to {LONGEST_GLIDE_MS}, or off");
                }
            },
            Control::Reset => match reset_named(text) {
                Some(reset) => self.set_reset(reset),
                None => {
                    self.status = format!(
                        "Reset is a number of steps from 1 to {MAX_TRACK_RESET}, of bars like 2bar, or off"
                    );
                }
            },
            Control::Groove | Control::Speed => {}
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
            Control::Lfo(..) | Control::FilterType | Control::FilterMode | Control::Machine => {}
            Control::Accumulator(field @ (AccumulatorField::Up | AccumulatorField::Down)) => {
                match text.trim().parse::<u8>() {
                    Ok(limit) if (1..=MOST_DEGREES).contains(&limit) => {
                        let now = track.accumulation;
                        self.set_accumulation(if field == AccumulatorField::Up {
                            Accumulation { up: limit, ..now }
                        } else {
                            Accumulation { down: limit, ..now }
                        });
                    }
                    _ => {
                        self.status =
                            format!("A limit is a number of degrees from 1 to {MOST_DEGREES}");
                    }
                }
            }
            Control::Accumulator(_) => {}
            Control::Arpeggio(ArpField::Octaves) => match text.trim().parse::<u8>() {
                Ok(octaves) if (1..=MOST_OCTAVES).contains(&octaves) => {
                    self.set_arpeggio(Arpeggio {
                        octaves,
                        ..track.arpeggio
                    });
                }
                _ => self.status = format!("Octaves is a number from 1 to {MOST_OCTAVES}"),
            },
            Control::Arpeggio(_) => {}
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
            choices.extend(SettingId::for_envelope(track).into_iter().map(Some));
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
            choices.extend(SettingId::for_lfo(track, slot).into_iter().map(Some));
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

/// How far a large step moves an accumulator's limit: an octave of a seven-note scale.
const LIMIT_JUMP: u8 = 7;

/// Move one of the accumulator's settings one notch. The limits stop at either end, as does the
/// list of resets, and the short lists go round.
fn adjust_accumulation(
    accumulation: &mut Accumulation,
    field: AccumulatorField,
    up: bool,
    large: bool,
) {
    let limit = |value: u8| {
        let size = if large { LIMIT_JUMP } else { 1 };
        let moved = if up {
            value.saturating_add(size)
        } else {
            value.saturating_sub(size)
        };
        moved.clamp(1, MOST_DEGREES)
    };
    match field {
        AccumulatorField::Up => accumulation.up = limit(accumulation.up),
        AccumulatorField::Down => accumulation.down = limit(accumulation.down),
        AccumulatorField::Moves => {
            accumulation.moves = next_in(&Moves::ALL, accumulation.moves, up)
        }
        AccumulatorField::AtLimit => {
            accumulation.at_limit = next_in(&AtLimit::ALL, accumulation.at_limit, up);
        }
        AccumulatorField::Adds => accumulation.adds = next_in(&Adds::ALL, accumulation.adds, up),
        AccumulatorField::Reset => {
            let resets: Vec<Reset> = Reset::all().collect();
            let at = resets
                .iter()
                .position(|reset| *reset == accumulation.reset)
                .unwrap_or(0);
            let to = if up {
                (at + 1).min(resets.len() - 1)
            } else {
                at.saturating_sub(1)
            };
            accumulation.reset = resets[to];
        }
    }
}

/// Move one of the arpeggio's settings one notch. The rates stop at either end, as they run from
/// the shortest note to the longest, and so do the octaves, while the modes go round.
fn adjust_arpeggio(arpeggio: &mut Arpeggio, field: ArpField, up: bool) {
    match field {
        ArpField::Rate => {
            let rates = ArpRate::ALL;
            let at = rates
                .iter()
                .position(|rate| *rate == arpeggio.rate)
                .unwrap_or(0);
            let to = if up {
                (at + 1).min(rates.len() - 1)
            } else {
                at.saturating_sub(1)
            };
            arpeggio.rate = rates[to];
        }
        ArpField::Mode => arpeggio.mode = next_in(&ArpMode::ALL, arpeggio.mode, up),
        ArpField::Octaves => {
            let moved = if up {
                arpeggio.octaves.saturating_add(1)
            } else {
                arpeggio.octaves.saturating_sub(1)
            };
            arpeggio.octaves = moved.clamp(1, MOST_OCTAVES);
        }
        ArpField::Retrig => arpeggio.retrig = !arpeggio.retrig,
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
