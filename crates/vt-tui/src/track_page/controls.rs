//! The settings on the track page's pages and the mixer, as the cursor stops on them: their names,
//! their values on a track, and which can be listed or typed.

use vt_model::action::TrackControl;
use vt_model::lfo::phase_value;
use vt_model::modulation::{ModKind, depth_value};
use vt_model::settings::{Effect, SettingId};
use vt_model::song::{FilterType, Track, TrackKind};

/// One setting of a mod slot: its type, its mode, where it goes and how far, which every type has
/// in its own way, then each type's own, which mean what they do under the slot's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModField {
    Type,
    /// The type's mode, as an envelope's curve or an LFO's `Free`, `Trig`, `Hold`, `One` or
    /// `Half`, for a type that has one.
    Mode,
    /// Whether it moves what it reaches one way or both (#434).
    Polarity,
    Destination,
    Depth,
    /// An envelope's: an ADSR's and an AHD's attack, decay and reset, an ADSR's sustain and
    /// release, and an AHD's hold.
    Attack,
    Decay,
    Sustain,
    Release,
    Reset,
    Hold,
    /// A drum envelope's: how long its peak takes, the level it falls to, and how many times it
    /// strikes. Its `Decay` is the body's.
    Peak,
    Body,
    Ratchet,
    /// An LFO's.
    Shape,
    Rate,
    Phase,
    Deform,
    Fade,
}

impl ModField {
    /// Whether a slot of this type has it.
    pub fn on(self, kind: ModKind) -> bool {
        match self {
            Self::Type | Self::Polarity | Self::Destination | Self::Depth => true,
            Self::Mode => kind.has_mode(),
            Self::Decay => kind.is_envelope(),
            // A drum always strikes from full, so it has no attack to reset.
            Self::Attack | Self::Reset => matches!(kind, ModKind::Adsr | ModKind::Ahd),
            Self::Peak | Self::Body | Self::Ratchet => kind == ModKind::Drum,
            Self::Sustain | Self::Release => kind == ModKind::Adsr,
            Self::Hold => kind == ModKind::Ahd,
            Self::Shape | Self::Rate | Self::Phase | Self::Deform | Self::Fade => {
                kind == ModKind::Lfo
            }
        }
    }
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
    /// The song's limiter, on or off, after the master's `Level`: the master's alone.
    Limiter,
    FilterType,
    /// How the filter's type responds, which a step can lock too.
    FilterMode,
    /// One of a mod slot's settings, by the slot's place.
    Mod(usize, ModField),
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
            Self::Base(TrackControl::VelocityDelta) => "Velocity",
            Self::Choke => "Choke",
            Self::Group => "Group",
            Self::GateLength => "Gate",
            Self::Speed => "Speed",
            Self::Reset => "Reset",
            Self::Glide => "Glide",
            Self::Limiter => "Limiter",
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
            Self::Mod(_, field) => match field {
                ModField::Type => "Type",
                ModField::Mode => "Mode",
                ModField::Polarity => "Polarity",
                ModField::Destination => "Dest",
                ModField::Depth => "Depth",
                ModField::Attack => "Attack",
                ModField::Decay => "Decay",
                ModField::Sustain => "Sustain",
                ModField::Release => "Release",
                ModField::Reset => "Reset",
                ModField::Hold => "Hold",
                ModField::Peak => "Peak",
                ModField::Body => "Body",
                ModField::Ratchet => "Ratchet",
                ModField::Shape => "Shape",
                ModField::Rate => "Rate",
                ModField::Phase => "Phase",
                ModField::Deform => "Deform",
                ModField::Fade => "Fade",
            },
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
            Self::Mod(slot, ModField::Depth) => Some((
                SettingId::modulator(slot, 0)?,
                depth_value(track.mods[slot].depth),
            )),
            Self::Mod(slot, ModField::Phase) => Some((
                SettingId::modulator(slot, 1)?,
                phase_value(track.mods[slot].lfo.phase),
            )),
            Self::Mod(slot, ModField::Deform) => Some((
                SettingId::modulator(slot, 3)?,
                depth_value(track.mods[slot].lfo.deform),
            )),
            Self::Mod(slot, ModField::Polarity) => Some((
                SettingId::modulator(slot, 2)?,
                u16::from(track.mods[slot].polarity().index()) << 8,
            )),
            Self::Level => base(SettingId::LEVEL, track.level),
            Self::Base(TrackControl::Cutoff) => base(SettingId::CUT, track.cutoff),
            Self::Base(TrackControl::Resonance) => base(SettingId::RES, track.resonance),
            Self::Base(TrackControl::Pan) => base(SettingId::PAN, track.pan),
            Self::Base(TrackControl::SendA) => base(SettingId::SEND_A, track.send_a),
            Self::Base(TrackControl::SendB) => base(SettingId::SEND_B, track.send_b),
            Self::Base(TrackControl::Pitch) => base(SettingId::PITCH, track.pitch),
            Self::Base(TrackControl::VelocityDelta) => {
                base(SettingId::VELOCITY_DELTA, track.velocity_delta)
            }
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
    /// that steps can set too, and its machine's that sweep, but not those on the level scale. An
    /// effect's settings are whole, as effects only hear those.
    pub fn fine_on(self, track: &Track) -> Option<SettingId> {
        let id = match self {
            Self::Base(which) => which.setting()?,
            Self::Setting(_) => self.setting_on(track)?,
            _ => return None,
        };
        (id.entry().kind.has_fine() && self.exists_on(track)).then_some(id)
    }

    /// The setting of the track's machine this is, if it's one.
    pub fn setting_on(self, track: &Track) -> Option<SettingId> {
        match self {
            Self::Setting(setting) => track.machine_setting(setting),
            _ => None,
        }
    }

    /// Whether this track has the setting: a slot only has the settings of what's in it, only a
    /// sound track or a group sends, the master can't be panned, muted or soloed, and only the
    /// master has the limiter.
    pub fn exists_on(self, track: &Track) -> bool {
        match self {
            Self::Limiter => track.kind == TrackKind::Master,
            Self::InsertParam(slot, param) => param < track.inserts[slot].params().len(),
            Self::Base(TrackControl::SendA | TrackControl::SendB) => {
                track.is_sound() || track.is_group()
            }
            Self::Base(TrackControl::Pitch | TrackControl::VelocityDelta)
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
        if let Self::Mod(slot, ModField::Mode) = self {
            return track.mods[slot].kind.mode_label();
        }
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
                | Self::Limiter
                | Self::FilterType
                | Self::FilterMode
                | Self::Groove
                | Self::Speed
                | Self::Mod(
                    _,
                    ModField::Type
                        | ModField::Mode
                        | ModField::Polarity
                        | ModField::Destination
                        | ModField::Shape
                        | ModField::Reset
                )
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
