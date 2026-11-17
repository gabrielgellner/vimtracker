//! The settings on the track page's pages and the mixer, as the cursor stops on them: their names,
//! their values on a track, and which can be listed or typed.

use vt_model::action::TrackControl;
use vt_model::constants::FX_SLOTS;
use vt_model::lfo::phase_value;
use vt_model::macros::{MACROS, Macro};
use vt_model::modulation::ModField;
use vt_model::modulation::depth_value;
use vt_model::settings::SettingId;
use vt_model::shape::GenParam;
use vt_model::song::{FilterType, Machine, Track, TrackKind};

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

/// One of an FX column's lane's settings, on the Gen page, which move every phrase's lane on that
/// column from its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneField {
    Length,
    Rate,
    Start,
}

impl LaneField {
    /// Its place among the lane's settings in the registry, and the track's [`LaneMove`].
    ///
    /// [`LaneMove`]: vt_model::lane::LaneMove
    pub const fn at(self) -> usize {
        match self {
            Self::Length => 0,
            Self::Rate => 1,
            Self::Start => 2,
        }
    }
}

/// What the status line calls each FX column's lane's settings.
const LANE_NAMES: [[&str; 3]; FX_SLOTS] = [
    ["FX1 Length", "FX1 Rate", "FX1 Start"],
    ["FX2 Length", "FX2 Rate", "FX2 Start"],
    ["FX3 Length", "FX3 Rate", "FX3 Start"],
];

/// One setting on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// What makes the sound.
    Machine,
    /// One of the machine's settings, by its place among them.
    Setting(usize),
    /// The file it plays.
    Sample,
    /// The sample's own tune, in semitones, or the slice's it plays where that has its own, as
    /// the sample page sets them (see [`super::tuning`]).
    Tune,
    /// The sample's own cents, on top of its tune and any slice's.
    Cents,
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
    /// What it's for, which its colour shows, or `Auto` to go by its machine.
    Role,
    /// How much of its last pulse a note lasts, as a percentage.
    GateLength,
    /// How long its steps are, as a note division.
    Speed,
    /// How many of the song's steps its chain plays before starting again, or off.
    Reset,
    /// How long a tie glides into the next note, in ms, or off.
    Glide,
    /// Whether a synth's oscillators run on from note to note or start again on each.
    Phase,
    /// How its notes join: each struck, or tied into one whose gate is still open.
    VoiceMode,
    /// Which notes `Glide` slides into: only ties, or every note from the one sounding.
    GlideMode,
    /// The song's limiter, on or off, after the master's `Level`: the master's alone.
    Limiter,
    FilterType,
    /// How the filter's type responds, which a step can lock too.
    FilterMode,
    /// Whether the filter's `Res` stops short of ringing by itself, on or off.
    Safe,
    /// Whether the filter's cutoff follows the note's pitch, on or off, which a step can lock.
    KeyTrack,
    /// One of a mod slot's settings, by the slot's place.
    Mod(usize, ModField),
    Accumulator(AccumulatorField),
    Arpeggio(ArpField),
    /// What's in one of the track's slots.
    Insert(usize),
    /// One of the settings of a phrase's live generator, by its place in the phrase's list, on
    /// the Gen page.
    Gen {
        phrase: u32,
        generator: usize,
        param: GenParam,
    },
    /// The Gen page's `Empty`, where `K` makes another column of the phrase live.
    NewGenerator(u32),
    /// One of the settings declared by what's in a slot.
    InsertParam(usize, usize),
    /// One of the track's macros, by its place, from 0 to 7.
    Macro(usize),
    /// One of an FX column's lane's settings, by the column's place, on the Gen page.
    Lane(usize, LaneField),
}

pub(crate) const fn base(control: TrackControl) -> Control {
    Control::Base(control)
}

impl Control {
    /// The setting's name on screen.
    #[allow(
        clippy::match_same_arms,
        reason = "a name for each control, in the page's order"
    )]
    pub fn label(self) -> &'static str {
        match self {
            Self::Base(TrackControl::Cutoff) => "Cut",
            Self::Base(TrackControl::Resonance) => "Res",
            Self::Base(TrackControl::Pan) => "Pan",
            Self::Base(TrackControl::SendA) => "Send A",
            Self::Base(TrackControl::SendB) => "Send B",
            Self::Base(TrackControl::Drive) => "Drive",
            Self::Base(TrackControl::Punch) => "Punch",
            Self::Base(TrackControl::LowCut) => "Low cut",
            Self::Base(TrackControl::HighCut) => "High cut",
            Self::Base(TrackControl::Pitch) => "Pitch",
            Self::Base(TrackControl::VelocityDelta) => "Velocity",
            Self::Base(TrackControl::Quantise) => "Quantise",
            Self::Base(TrackControl::Jump) => "Jump",
            Self::Base(TrackControl::Order) => "Order",
            Self::Base(TrackControl::Reverse) => "Reverse",
            Self::Base(TrackControl::OrderX) => "Order X",
            Self::Base(TrackControl::OrderY) => "Order Y",
            Self::Base(TrackControl::PhraseLength) => "Length",
            Self::Base(TrackControl::PhraseStart) => "Start",
            Self::Base(TrackControl::ChanceDelta) => "Chance",
            Self::Base(TrackControl::ChanceTarget) => "Target",
            Self::Choke => "Choke",
            Self::Group => "Group",
            Self::Role => "Role",
            Self::GateLength => "Gate",
            Self::Speed => "Speed",
            Self::Reset => "Reset",
            Self::Glide => "Glide",
            Self::Phase => "Phase",
            Self::VoiceMode => "Mode",
            Self::GlideMode => "Glides",
            Self::Limiter => "Limiter",
            Self::Machine => "Machine",
            Self::Setting(_) => "Setting",
            Self::Sample => "Sample",
            Self::Tune => "Tune",
            Self::Cents => "Cents",
            Self::Mute => "Mute",
            Self::Solo => "Solo",
            Self::FilterType => "Filter",
            Self::FilterMode => "Mode",
            Self::Safe => "Safe",
            Self::KeyTrack => "Key track",
            Self::Level => "Level",
            Self::Trim => "Trim",
            Self::Groove => "Groove",
            Self::Mod(_, field) => field.name(),
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
            // As the registry names it until it's named.
            Self::Macro(at) => {
                SettingId::macro_at(at.min(MACROS - 1)).map_or("Macro", SettingId::name)
            }
            Self::Gen { param, .. } => param.name(),
            Self::NewGenerator(_) => "Empty",
            Self::Lane(_, LaneField::Length) => "Length",
            Self::Lane(_, LaneField::Rate) => "Rate",
            Self::Lane(_, LaneField::Start) => "Start",
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
            Self::Lane(slot, field) => LANE_NAMES[slot.min(FX_SLOTS - 1)][field.at()],
            // The sample's, which the status line says, as the track's `Pitch` is another.
            Self::Tune => "Sample tune",
            Self::Cents => "Sample cents",
            // The phrase's, as a lane's are `Length` and `Start` too.
            Self::Base(TrackControl::PhraseLength) => "Phrase length",
            Self::Base(TrackControl::PhraseStart) => "Phrase start",
            // In full, as the Play device's column has room only for `Glides`.
            Self::GlideMode => "Glide mode",
            _ => self.label_on(track),
        }
    }

    /// The setting this is on this track, and its value there, for the status line to show in
    /// its units.
    pub(crate) fn value_on(self, track: &Track) -> Option<(SettingId, u16)> {
        let base = |id: SettingId, value: u8| {
            Some((id, u16::from(value) << 8 | u16::from(track.fine(id))))
        };
        match self {
            Self::Setting(_) => self
                .setting_on(track)
                .and_then(|id| base(id, track.setting(id))),
            // The slot's settings that steps can lock, whatever its type.
            Self::Mod(slot, field) if field.locked().is_some() => {
                let modulator = &track.mods[slot];
                let value = match field {
                    ModField::Depth => depth_value(modulator.depth),
                    ModField::Phase => phase_value(modulator.lfo.phase),
                    ModField::Deform => depth_value(modulator.lfo.deform),
                    _ => u16::from(modulator.polarity().index()) << 8,
                };
                Some((field.setting(slot, modulator.kind)?, value))
            }
            Self::Level => base(SettingId::LEVEL, track.level),
            Self::Base(which) => base(which.setting()?, track.control(which)),
            Self::GateLength => Some((SettingId::GATE_LENGTH, u16::from(track.gate_length()) << 8)),
            Self::Speed => Some((SettingId::SPEED, u16::from(track.speed.index()) << 8)),
            Self::Glide => Some((SettingId::GLIDE, u16::from(track.glide) << 8)),
            Self::Phase => Some((SettingId::PHASE, u16::from(track.phase.index()) << 8)),
            Self::VoiceMode => Some((
                SettingId::VOICE_MODE,
                u16::from(track.voice_mode.index()) << 8,
            )),
            Self::KeyTrack => Some((SettingId::KEY_TRACK, u16::from(track.key_track) << 8)),
            Self::Safe => Some((SettingId::SAFE, u16::from(track.safe) << 8)),
            Self::FilterType => {
                Some((SettingId::FILTER_TYPE, u16::from(track.filter.index()) << 8))
            }
            Self::GlideMode => Some((
                SettingId::GLIDE_MODE,
                u16::from(track.glide_mode.index()) << 8,
            )),
            // What it plays, which is its type's first where its type doesn't offer the track's.
            Self::FilterMode if track.filter.has_modes() => Some((
                SettingId::FILTER_MODE,
                u16::from(track.filter.plays(track.filter_mode).index()) << 8,
            )),
            Self::Macro(at) => Some((SettingId::macro_at(at)?, track.macros[at].value)),
            Self::Lane(slot, field) => Some((
                SettingId::lane(slot, field.at())?,
                u16::from(track.lanes.get(slot)?.value(field.at())) << 8,
            )),
            Self::InsertParam(slot, param) => {
                let insert = track.inserts[slot];
                let (effect, at) = (insert.kind()?, insert.kept_at(param)?);
                base(SettingId::slot(slot, effect, at)?, insert.value(param))
            }
            _ => None,
        }
    }

    /// The setting of the registry this is on this track, for a macro to hold: what
    /// [`Control::value_on`] gives, and a mod slot's type's own, the accumulator's and the
    /// arpeggio's that macros and modulation move, while the slot's type has it.
    pub(crate) fn setting_for_macro(self, track: &Track) -> Option<SettingId> {
        if let Some((id, _)) = self.value_on(track) {
            return Some(id);
        }
        match self {
            Self::Mod(slot, field) => {
                let id = SettingId::mod_own(slot, field.own(track.mods[slot].kind)?)?;
                id.exists_on(track).then_some(id)
            }
            Self::Accumulator(field) => {
                let at = match field {
                    AccumulatorField::Up => 0,
                    AccumulatorField::Down => 1,
                    _ => return None,
                };
                let id = SettingId::accumulation(at)?;
                Some(id)
            }
            Self::Arpeggio(field) => {
                let at = match field {
                    ArpField::Rate => 0,
                    ArpField::Octaves => 1,
                    _ => return None,
                };
                let id = SettingId::arpeggio(at)?;
                Some(id)
            }
            _ => None,
        }
    }

    /// The setting whose fine part this shows on the Sound page, if it has one: the track's own
    /// that steps can set too, and its machine's that sweep, but not those on the level scale. An
    /// effect's settings are whole, as effects only hear those.
    pub(crate) fn fine_on(self, track: &Track) -> Option<SettingId> {
        let id = match self {
            Self::Base(which) => which.setting()?,
            Self::Setting(_) => self.setting_on(track)?,
            _ => return None,
        };
        (id.entry().kind.has_fine() && self.exists_on(track)).then_some(id)
    }

    /// The setting of the track's machine this is, if it's one.
    pub(crate) fn setting_on(self, track: &Track) -> Option<SettingId> {
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
            Self::Base(
                TrackControl::Pitch
                | TrackControl::VelocityDelta
                | TrackControl::Quantise
                | TrackControl::Jump
                | TrackControl::Order
                | TrackControl::Reverse
                | TrackControl::OrderX
                | TrackControl::OrderY
                | TrackControl::PhraseLength
                | TrackControl::PhraseStart
                | TrackControl::ChanceDelta
                | TrackControl::ChanceTarget,
            )
            | Self::Choke
            | Self::Group
            | Self::Role
            | Self::GateLength
            | Self::Speed
            | Self::Reset
            | Self::Glide
            | Self::VoiceMode
            | Self::KeyTrack
            | Self::GlideMode
            | Self::Accumulator(_)
            | Self::Arpeggio(_)
            | Self::Lane(..) => track.is_sound(),
            Self::Base(TrackControl::Pan) | Self::Mute | Self::Solo => {
                track.kind != TrackKind::Master
            }
            Self::Setting(_) => self.setting_on(track).is_some(),
            // A sampler starts its sample again on each note anyway.
            Self::Phase => track.voice().is_some(),
            Self::Tune | Self::Cents => track.is_sound() && track.machine == Machine::Sampler,
            _ => true,
        }
    }

    /// The setting's name on screen, on this track, as each machine names its own.
    pub fn label_on(self, track: &Track) -> &'static str {
        if let Self::Mod(slot, ModField::Mode) = self {
            return track.mods[slot].kind.mode_label();
        }
        // An attractor has no cycle to start part way through, so its phase is the axis heard.
        if let Self::Mod(slot, ModField::Phase) = self
            && track.mods[slot].lfo.shape.is_attractor()
        {
            return "Axis";
        }
        self.setting_on(track)
            .map_or_else(|| self.label(), SettingId::name)
    }

    /// Whether this is two hex digits, entered as soon as the second is typed.
    pub(crate) fn is_hex(self) -> bool {
        // `Phrase`'s are places, a switch and lists (decision 400).
        if let Self::Base(
            TrackControl::Order
            | TrackControl::Reverse
            | TrackControl::OrderX
            | TrackControl::OrderY
            | TrackControl::PhraseLength
            | TrackControl::PhraseStart
            | TrackControl::ChanceTarget,
        ) = self
        {
            return false;
        }
        matches!(
            self,
            Self::Base(_)
                | Self::Level
                | Self::Trim
                | Self::InsertParam(..)
                | Self::Setting(_)
                | Self::Macro(_)
        )
    }

    /// Whether this is picked from a list rather than typed, on this track.
    pub(crate) fn is_choice(self, track: &Track) -> bool {
        if let Self::InsertParam(slot, param) = self {
            return track.inserts[slot]
                .params()
                .get(param)
                .is_some_and(|found| !found.choices().is_empty());
        }
        matches!(
            self,
            Self::Insert(_)
                | Self::Machine
                | Self::Sample
                | Self::Mute
                | Self::Solo
                | Self::Limiter
                | Self::Safe
                | Self::KeyTrack
                | Self::FilterType
                | Self::FilterMode
                | Self::Groove
                | Self::Speed
                | Self::Role
                | Self::Mod(
                    _,
                    ModField::Type
                        | ModField::Mode
                        | ModField::Polarity
                        | ModField::Destination
                        | ModField::Shape
                        | ModField::Reset
                        | ModField::Loop
                )
                | Self::Accumulator(
                    AccumulatorField::Moves
                        | AccumulatorField::AtLimit
                        | AccumulatorField::Adds
                        | AccumulatorField::Reset
                )
                | Self::Arpeggio(ArpField::Rate | ArpField::Mode | ArpField::Retrig)
                | Self::Lane(_, LaneField::Rate)
                | Self::Base(
                    TrackControl::Reverse
                        | TrackControl::OrderX
                        | TrackControl::OrderY
                        | TrackControl::ChanceTarget
                )
        )
    }
}

/// What the status line says for `Mode` on a type that has none.
pub(crate) fn no_modes(filter: FilterType) -> String {
    format!("{} has no modes", filter.name())
}

/// A macro's name as the band and the lists show it: its own or its first mapping's, or
/// `Macro 3` while it holds nothing.
pub(crate) fn macro_name(held: &Macro, at: usize) -> String {
    held.shown_name()
        .unwrap_or_else(|| format!("Macro {}", at + 1))
}
