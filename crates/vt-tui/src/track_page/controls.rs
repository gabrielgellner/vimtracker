//! The settings on the track page's pages and the mixer, as the cursor stops on them: their names,
//! their values on a track, and which can be listed or typed.

use vt_model::action::TrackControl;
use vt_model::lfo::{depth_value, phase_value};
use vt_model::settings::{Effect, Kind, SettingId};
use vt_model::song::{FilterType, Track, TrackKind};

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

pub(super) const fn base(control: TrackControl) -> Control {
    Control::Base(control)
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
    pub(super) fn is_hex(self) -> bool {
        matches!(
            self,
            Self::Base(_) | Self::Level | Self::Trim | Self::InsertParam(..) | Self::Setting(_)
        )
    }

    /// Whether this is picked from a list rather than typed, on this track.
    pub(super) fn is_choice(self, track: &Track) -> bool {
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
pub(super) fn no_modes(filter: FilterType) -> String {
    format!("{} has no modes", filter.name())
}
