//! The song and everything it refers to by ID.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::accumulator::Accumulation;
use crate::arpeggio::Arpeggio;
use crate::bank::{Bank, BankEntry, ContentHash};
use crate::constants::{
    BAR_STEPS, BEAT_STEPS, DB_PER_LEVEL, DB_PER_TRIM, DEFAULT_MILLIBPM, DEFAULT_PHRASE_STEPS,
    FIXED_TRACKS, FORMAT_VERSION, FX_SLOTS, MAX_GROUPS, MAX_LEVEL, MAX_PHRASE_STEPS,
    MAX_TRACK_RESET, UNITY_LEVEL, UNITY_TRIM,
};
use crate::generator::{Generator, GeneratorFile, Span};
use crate::grid::{GRID_ROWS, Grid};
use crate::groove::{Groove, GrooveId};
use crate::insert::{INSERTS, Insert};
use crate::modulation::{MOD_SLOTS, ModSlot, default_mods};
use crate::order::{Deal, Dealer, Order, Round};
use crate::scale::Scale;
use crate::selection::{Extent, StepRange};
use crate::settings::{Owner, SAMPLER_SETTINGS, SettingId, voice_entries};
use crate::shape::{Shape, ShapeColumn};
use crate::speed::Speed;
use crate::step::{FxCommand, FxSlot, Step};

/// The gain of a level value, on an even scale in dB like a fader's. `00` is silent, nothing goes
/// past `MAX_LEVEL`, and the value can be between steps, as an LFO moves it.
pub fn level_gain(level: f32) -> f32 {
    if level <= 0.0 {
        return 0.0;
    }
    let level = level.min(f32::from(MAX_LEVEL));
    10.0_f32.powf((level - f32::from(UNITY_LEVEL)) * DB_PER_LEVEL / 20.0)
}

/// What a trim value does in dB, a quarter of a dB a step either side of `80`, or `None` for `00`,
/// which is off.
pub fn trim_db(trim: u8) -> Option<f32> {
    (trim > 0).then(|| (f32::from(trim) - f32::from(UNITY_TRIM)) * DB_PER_TRIM)
}

/// The gain of a trim value: exactly 1 at `80`, and 0 at `00`.
pub fn trim_gain(trim: u8) -> f32 {
    trim_db(trim).map_or(0.0, |db| 10.0_f32.powf(db / 20.0))
}

/// A trim value as the status line shows it: `+12.0 dB`, `-0.25 dB`, or `-inf dB` for off. Its
/// quarters are shown, as each step is one.
pub fn trim_text(trim: u8) -> String {
    match trim_db(trim) {
        None => "-inf dB".to_owned(),
        Some(db) if (db * 2.0).fract() == 0.0 => format!("{db:+.1} dB"),
        Some(db) => format!("{db:+.2} dB"),
    }
}

/// A track's filter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterType {
    #[default]
    LowPass,
    HighPass,
    BandPass,
    Notch,
    /// A saturating four-pole ladder, which self-oscillates and costs more.
    Ladder,
    /// A saturating diode ladder, the kind in Roland's TB-303, which costs more too.
    Diode,
    /// The Korg 35 low-pass in Korg's MS-20, which screams as it self-oscillates and costs more.
    Ms20LowPass,
    /// The Korg 35's high-pass, from the same circuit.
    Ms20HighPass,
    /// Vital's `Dirty` low-pass, whose resonance loop saturates, which costs more too.
    Dirty,
    /// The OB-X's two-pole state-variable filter, after OB-Xd's, whose damping falls as it gets
    /// quieter, so it sings by itself near the top of `Res`, and costs more too.
    ObX,
}

impl FilterType {
    /// Every type, in the order a list shows them.
    pub const ALL: [Self; 10] = [
        Self::LowPass,
        Self::HighPass,
        Self::BandPass,
        Self::Notch,
        Self::Ladder,
        Self::Diode,
        Self::Ms20LowPass,
        Self::Ms20HighPass,
        Self::Dirty,
        Self::ObX,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::LowPass => "LP",
            Self::HighPass => "HP",
            Self::BandPass => "BP",
            Self::Notch => "Notch",
            Self::Ladder => "Ladder",
            Self::Diode => "Diode",
            Self::Ms20LowPass => "MS20 LP",
            Self::Ms20HighPass => "MS20 HP",
            Self::Dirty => "Dirty",
            Self::ObX => "OB-X",
        }
    }

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::LowPass => "Low-pass, clean, 12 or 24 dB",
            Self::HighPass => "High-pass, clean, 12 or 24 dB",
            Self::BandPass => "Band-pass, clean",
            Self::Notch => "Notch, clean",
            Self::Ladder => "Saturating ladder, low-pass and more, self-oscillates",
            Self::Diode => "Diode ladder low-pass, 303 style, self-oscillates",
            Self::Ms20LowPass => "Korg 35 low-pass, MS-20 style, 12 dB, screams",
            Self::Ms20HighPass => "Korg 35 high-pass, MS-20 style, 6 dB, screams",
            Self::Dirty => "Vital's low-pass and more, saturating resonance",
            Self::ObX => "OB-X state-variable, 12 dB, multimode, sings",
        }
    }

    /// Whether it always saturates, at twice the rate, rather than only when driven.
    pub fn saturates(self) -> bool {
        matches!(
            self,
            Self::Ladder
                | Self::Diode
                | Self::Ms20LowPass
                | Self::Ms20HighPass
                | Self::Dirty
                | Self::ObX
        )
    }

    /// The modes it offers, the one it starts in first. The MS-20's offer none.
    pub fn modes(self) -> &'static [FilterMode] {
        use FilterMode::{
            BandPass, BandPass12, Full, HighPass6, HighPass12, HighPass12LowPass6, HighPass18,
            HighPass18LowPass6, HighPass24, LowCut1, LowCut2, LowCut3, LowPass6, LowPass12,
            LowPass18, LowPass24, Notch, Notch12, Notch12LowPass6, Phaser, PhaserLowPass6, Slope12,
            Slope24,
        };
        match self {
            Self::LowPass | Self::HighPass | Self::BandPass | Self::Notch => &[Slope12, Slope24],
            // The Oberheim Xpander's fifteen, and Vital's notch.
            Self::Ladder => &[
                LowPass24,
                LowPass18,
                LowPass12,
                LowPass6,
                BandPass,
                BandPass12,
                HighPass6,
                HighPass12,
                HighPass18,
                HighPass24,
                Notch,
                Notch12,
                Phaser,
                HighPass12LowPass6,
                HighPass18LowPass6,
                Notch12LowPass6,
                PhaserLowPass6,
            ],
            Self::Dirty => &[
                LowPass24, LowPass12, BandPass, HighPass12, HighPass24, Notch,
            ],
            Self::Diode => &[Full, LowCut3, LowCut2, LowCut1],
            // OB-Xd's two-pole morphs from low-pass through band-pass or notch to high-pass.
            Self::ObX => &[LowPass12, BandPass12, HighPass12, Notch12],
            Self::Ms20LowPass | Self::Ms20HighPass => &[],
        }
    }

    /// Whether it has more than one mode, so `Mode` means something on it.
    pub fn has_modes(self) -> bool {
        !self.modes().is_empty()
    }

    /// The mode it plays for a track set to this one: the same, if it offers it, or its first. A
    /// type with no modes plays as it always does, whatever this is.
    pub fn plays(self, mode: FilterMode) -> FilterMode {
        let modes = self.modes();
        if modes.contains(&mode) {
            mode
        } else {
            modes.first().copied().unwrap_or(mode)
        }
    }
}

/// How a track's filter type responds: how steep the clean types are, which of their responses the
/// ladder, `Dirty` and `OB-X` give out, or where the diode's low cut sits. Each type offers some of these.
/// A track keeps its mode when its type changes, and a type that doesn't offer it plays its own
/// first, so going to another type and back finds the mode again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterMode {
    /// The clean types as they are, and with a second filter after them, for twice the slope.
    #[default]
    Slope12,
    Slope24,
    /// The ladder's and `Dirty`'s responses, mixed from the stages they already have.
    LowPass24,
    LowPass12,
    BandPass,
    HighPass12,
    HighPass24,
    Notch,
    /// Where the diode's input high-pass sits: far under, as `Acid` has it, or 3, 2 or 1 octaves
    /// under `Cut`, so less bass reaches the ladder.
    Full,
    LowCut3,
    LowCut2,
    LowCut1,
    /// The rest of the Oberheim Xpander's mixes of the ladder's stages, which only the ladder
    /// offers, but for `BP12` and `N12`, which `OB-X` has too. They come after the others, so a
    /// step's value for those stays as it was.
    LowPass18,
    LowPass6,
    BandPass12,
    HighPass6,
    HighPass18,
    Notch12,
    /// Three stages' phase shift against what goes in: a dip above `Cut`.
    Phaser,
    /// A high-pass, notch or phaser and a 6 dB low-pass at once.
    HighPass12LowPass6,
    HighPass18LowPass6,
    Notch12LowPass6,
    PhaserLowPass6,
}

impl FilterMode {
    /// Every mode, in the order a step's value counts them.
    pub const ALL: [Self; 23] = [
        Self::Slope12,
        Self::Slope24,
        Self::LowPass24,
        Self::LowPass12,
        Self::BandPass,
        Self::HighPass12,
        Self::HighPass24,
        Self::Notch,
        Self::Full,
        Self::LowCut3,
        Self::LowCut2,
        Self::LowCut1,
        Self::LowPass18,
        Self::LowPass6,
        Self::BandPass12,
        Self::HighPass6,
        Self::HighPass18,
        Self::Notch12,
        Self::Phaser,
        Self::HighPass12LowPass6,
        Self::HighPass18LowPass6,
        Self::Notch12LowPass6,
        Self::PhaserLowPass6,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        FILTER_MODE_NAMES[usize::from(self.index())]
    }

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Slope12 => "12 dB an octave, one filter",
            Self::Slope24 => "24 dB an octave, a second filter after the first",
            Self::LowPass24 => "Low-pass, 24 dB",
            Self::LowPass18 => "Low-pass, 18 dB",
            Self::LowPass12 => "Low-pass, 12 dB",
            Self::LowPass6 => "Low-pass, 6 dB",
            Self::BandPass => "Band-pass, 12 dB either side",
            Self::BandPass12 => "Band-pass, 6 dB either side, wider",
            Self::HighPass6 => "High-pass, 6 dB",
            Self::HighPass12 => "High-pass, 12 dB",
            Self::HighPass18 => "High-pass, 18 dB",
            Self::HighPass24 => "High-pass, 24 dB",
            Self::Notch => "Notch, 24 dB",
            Self::Notch12 => "Notch, 12 dB, narrower",
            Self::Phaser => "Phaser: a dip above Cut, the bass thinned",
            Self::HighPass12LowPass6 => "High-pass 12 dB and low-pass 6 dB, a lopsided band",
            Self::HighPass18LowPass6 => "High-pass 18 dB and low-pass 6 dB, a lopsided band",
            Self::Notch12LowPass6 => "Notch 12 dB and low-pass 6 dB",
            Self::PhaserLowPass6 => "Phaser and low-pass 6 dB",
            Self::Full => "All the bass goes in, as in Acid",
            Self::LowCut3 => "Low cut 3 octaves under Cut",
            Self::LowCut2 => "Low cut 2 octaves under Cut",
            Self::LowCut1 => "Low cut 1 octave under Cut",
        }
    }

    /// Its place in the list, which is a step's value for it.
    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0) as u8
    }

    /// The mode at a place in the list, if there's one there.
    pub fn at(index: u8) -> Option<Self> {
        Self::ALL.get(usize::from(index)).copied()
    }
}

/// Each mode's name, in the list's order, for a step's value to show.
pub const FILTER_MODE_NAMES: [&str; 23] = [
    "12 dB", "24 dB", "LP24", "LP12", "BP24", "HP12", "HP24", "N24", "Full", "-3 oct", "-2 oct",
    "-1 oct", "LP18", "LP6", "BP12", "HP6", "HP18", "N12", "PH18", "HP12+LP6", "HP18+LP6",
    "N12+LP6", "PH18+LP6",
];

/// What a track is. Only `Sound` tracks have a machine, and the rest are fixed, after them, in this
/// order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    #[default]
    Sound,
    /// A group, from 1 to `MAX_GROUPS`, which sums the sound tracks put in it and plays the sum
    /// through its own slots, level and pan to the master, as Ableton's group tracks do. It sends
    /// to the returns as a sound track does.
    Group(u8),
    /// A return, which other tracks send to.
    ReturnA,
    ReturnB,
    /// What everything ends up in.
    Master,
}

impl TrackKind {
    /// The fixed tracks, in order: the groups, the returns and the master.
    pub const FIXED: [Self; FIXED_TRACKS] = {
        let mut kinds = [Self::Master; FIXED_TRACKS];
        let mut group = 0;
        while group < MAX_GROUPS {
            kinds[group] = Self::Group(group as u8 + 1);
            group += 1;
        }
        kinds[MAX_GROUPS] = Self::ReturnA;
        kinds[MAX_GROUPS + 1] = Self::ReturnB;
        kinds
    };

    /// The short name of a fixed track.
    pub fn letter(self) -> String {
        match self {
            Self::Sound => String::new(),
            Self::Group(group) => format!("G{group}"),
            Self::ReturnA => "A".to_owned(),
            Self::ReturnB => "B".to_owned(),
            Self::Master => "M".to_owned(),
        }
    }

    /// What a fixed track is called in full.
    pub fn title(self) -> String {
        match self {
            Self::Sound => String::new(),
            Self::Group(group) => format!("Group {group}"),
            Self::ReturnA => "Return A".to_owned(),
            Self::ReturnB => "Return B".to_owned(),
            Self::Master => "Master".to_owned(),
        }
    }

    /// Where it is among the fixed tracks, counted from the first after the sound tracks.
    pub fn fixed_place(self) -> Option<usize> {
        Self::FIXED.iter().position(|kind| *kind == self)
    }
}

/// One voice's lane through the song.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    /// Whether it plays sounds, or is one of the fixed tracks at the end.
    #[serde(default)]
    pub kind: TrackKind,
    /// Two characters, or empty for none.
    #[serde(default)]
    pub name: String,
    /// What makes its sound.
    #[serde(default)]
    pub machine: Machine,
    /// The machines' settings it has changed from their defaults, by setting. A track keeps every
    /// voice's, so switching voice and back finds the first as it was.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<SettingId, u8>,
    /// The fine parts of its own settings and its machines', where they aren't `00`, so `Cut` at
    /// `40` with `80` here is half a step above `40`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fine: BTreeMap<SettingId, u8>,
    pub mute: bool,
    pub solo: bool,
    /// This track's own groove, instead of the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groove: Option<GrooveId>,
    /// The track's level, set outside its patterns. It also scales what the track sends to the
    /// returns.
    #[serde(default = "unity_level")]
    pub level: u8,
    /// What comes into it is turned up or down by, before anything else it does: a sound track's
    /// voice before its filter, and a fixed track's input before its slots. It's for gain staging,
    /// set outside its patterns and never modulated, `80` for 0 dB as [`trim_db`] has it, and
    /// `00` silences it.
    #[serde(default = "unity_trim", skip_serializing_if = "is_unity_trim")]
    pub trim: u8,
    /// What the track's held `CUT` starts from, and what its LFOs move around until a step sets it.
    #[serde(default = "open_cutoff")]
    pub cutoff: u8,
    /// Where held `RES` starts.
    #[serde(default)]
    pub resonance: u8,
    #[serde(default)]
    pub filter: FilterType,
    /// Where held `MODE` starts: how its filter type responds.
    #[serde(default)]
    pub filter_mode: FilterMode,
    /// How hard the filter is driven.
    #[serde(default)]
    pub drive: u8,
    /// The utility filter after it: a high-pass, which `00` turns off.
    #[serde(default)]
    pub low_cut: u8,
    /// And a low-pass, which `FF` turns off.
    #[serde(default = "open_cutoff")]
    pub high_cut: u8,
    /// The effects after the filters, in order.
    #[serde(default)]
    pub inserts: [Insert; INSERTS],
    /// The slots switched out, to hear the track without them. They keep running underneath.
    #[serde(default, skip_serializing_if = "none_bypassed")]
    pub bypassed: [bool; INSERTS],
    /// Where held `PAN` starts.
    #[serde(default = "centre")]
    pub pan: u8,
    /// How much it sends to return `A`, and where held `SNA` starts.
    #[serde(default)]
    pub send_a: u8,
    /// How much it sends to return `B`, and where held `SNB` starts.
    #[serde(default)]
    pub send_b: u8,
    /// Where held `PTCH` starts, in semitones from `80`, as a kit sound's tune sets it.
    #[serde(default = "centre")]
    pub pitch: u8,
    /// Where held `VDLT` starts: how far its notes' velocities are moved up or down from their
    /// own, by [`velocity_delta`](crate::settings::velocity_delta), with `80` as none.
    #[serde(default = "centre", skip_serializing_if = "is_centre")]
    pub velocity_delta: u8,
    /// The choke group it's in, if any, from 1 to `CHOKE_GROUPS`: a note on it cuts off what's
    /// sounding on the other tracks in the group, as a closed hat cuts an open one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choke: Option<u8>,
    /// The group it plays through, from 1 to `MAX_GROUPS`, or `None` to go straight to the master.
    /// Only a sound track can be in one, and its sends still go from the track itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u8>,
    /// Where held `LEN` starts, its `Gate`: how much of its last pulse a note lasts, or `None`
    /// for its machine's own, which [`Track::gate_length`] gives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_length: Option<u8>,
    /// How long each of its steps is, and where held `RATE` starts: a sixteenth unless changed.
    #[serde(default, skip_serializing_if = "is_sixteenth")]
    pub speed: Speed,
    /// How many of the song's steps its chain plays before starting again from its top, counted
    /// from where its chain last started, or `None` to play on, so a short part can be kept to a
    /// bar while the others drift. From 1 to [`MAX_TRACK_RESET`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset: Option<u16>,
    /// How long a tie glides into the next note, and where held `GLD` starts, on the `00` to `FF`
    /// scale of [`glide_ms`](crate::settings::glide_ms): `00`, unless changed, jumps, or on `Acid`
    /// slides by its own `Slide`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub glide: u8,
    /// How its `ACU`s move its notes: how far, what happens at the limits, and when they start
    /// again.
    #[serde(default, skip_serializing_if = "is_default_accumulation")]
    pub accumulation: Accumulation,
    /// How its `ARP`s play: how fast, in what order, over how many octaves, and whether each note
    /// starts again.
    #[serde(default, skip_serializing_if = "is_default_arpeggio")]
    pub arpeggio: Arpeggio,
    /// Its mod slots, worked out in order, each able to move only those after it.
    #[serde(default = "default_mods")]
    pub mods: [ModSlot; MOD_SLOTS],
    /// The chains this track can play, numbered within it.
    #[serde(default)]
    pub chains: BTreeMap<u32, Arc<Chain>>,
    /// The phrases its chains can play, numbered within it.
    #[serde(default)]
    pub phrases: BTreeMap<u32, Arc<Phrase>>,
}

fn is_centre(value: &u8) -> bool {
    *value == centre()
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

fn is_sixteenth(speed: &Speed) -> bool {
    *speed == Speed::Sixteenth
}

fn is_default_accumulation(accumulation: &Accumulation) -> bool {
    *accumulation == Accumulation::default()
}

fn is_default_arpeggio(arpeggio: &Arpeggio) -> bool {
    *arpeggio == Arpeggio::default()
}

fn none_bypassed(bypassed: &[bool; INSERTS]) -> bool {
    !bypassed.contains(&true)
}

impl Track {
    /// A fixed track: named for what it is, and with no chain until one is given to it, so it
    /// doesn't change how long a section is.
    fn fixed(kind: TrackKind) -> Self {
        let mut inserts = [Insert::Empty; INSERTS];
        inserts[0] = match kind {
            TrackKind::ReturnA => Insert::RETURN_DELAY,
            TrackKind::ReturnB => Insert::RETURN_REVERB,
            TrackKind::Sound | TrackKind::Group(_) | TrackKind::Master => Insert::Empty,
        };
        Self {
            kind,
            name: kind.letter(),
            inserts,
            ..Self::default()
        }
    }

    /// Whether this track plays sounds.
    pub fn is_sound(&self) -> bool {
        self.kind == TrackKind::Sound
    }

    /// Whether this is one of the group tracks.
    pub fn is_group(&self) -> bool {
        matches!(self.kind, TrackKind::Group(_))
    }

    /// The synth voice it plays, if it plays one.
    pub fn voice(&self) -> Option<SynthVoice> {
        match self.machine {
            Machine::Plaits(voice) if self.is_sound() => Some(voice),
            _ => None,
        }
    }

    /// How much of its last pulse a note lasts, on the `00` to `FF` scale: its own `Gate`, or its
    /// machine's if it hasn't one.
    pub fn gate_length(&self) -> u8 {
        self.gate_length
            .unwrap_or_else(|| self.machine.gate_length())
    }

    /// The bank entry its sampler plays, before any step chooses another: `00` for none.
    pub fn sample(&self) -> u8 {
        self.setting(SettingId::SAMPLE)
    }

    /// One of its machine's settings, by its place among them: a synth's, or the sampler's, whose
    /// first is `Sample`.
    pub fn machine_setting(&self, setting: usize) -> Option<SettingId> {
        match self.machine {
            Machine::Sampler if self.is_sound() => SettingId::sampler(setting),
            _ => SettingId::voice(self.voice()?, setting),
        }
    }

    /// The value of one of its machine's settings, by its place among them, for `HAR`, `TIM`,
    /// `MOR` and `ACC` to start from. A machine without that setting gives 0, which nothing reads.
    pub fn machine_value(&self, setting: usize) -> u8 {
        self.machine_setting(setting)
            .map_or(0, |id| self.setting(id))
    }

    /// Set one of its machine's settings, by its place among them, if the machine has it.
    pub fn set_machine_value(&mut self, setting: usize, value: u8) {
        if let Some(id) = self.machine_setting(setting) {
            self.set_setting(id, value);
        }
    }

    /// A setting's value on this track: what it's set to, or its default.
    pub fn setting(&self, id: SettingId) -> u8 {
        self.settings
            .get(&id)
            .copied()
            .unwrap_or_else(|| id.entry().default)
    }

    /// Set a setting, within what it takes. A setting back at its default isn't kept.
    pub fn set_setting(&mut self, id: SettingId, value: u8) {
        let entry = id.entry();
        let value = value.min(entry.kind.top());
        if value == entry.default {
            self.settings.remove(&id);
        } else {
            self.settings.insert(id, value);
        }
    }

    /// A setting's fine part on this track, `00` unless it's been set.
    pub fn fine(&self, id: SettingId) -> u8 {
        self.fine.get(&id).copied().unwrap_or(0)
    }

    /// Set a setting's fine part. `00` isn't kept, and there's none at the setting's top, nor on
    /// one with no fine part, as the level scale's sends have none (decision 175).
    pub fn set_fine(&mut self, id: SettingId, fine: u8) {
        if fine == 0
            || !id.entry().kind.has_fine()
            || self.coarse(id) == Some(id.entry().kind.top())
        {
            self.fine.remove(&id);
        } else {
            self.fine.insert(id, fine);
        }
    }

    /// The coarse value of one of its own settings or its machines', which a fine part goes with.
    pub fn coarse(&self, id: SettingId) -> Option<u8> {
        match id.owner().0 {
            Owner::Voice(_) | Owner::Sampler => Some(self.setting(id)),
            Owner::Track => match id {
                SettingId::CUT => Some(self.cutoff),
                SettingId::RES => Some(self.resonance),
                SettingId::PAN => Some(self.pan),
                SettingId::SEND_A => Some(self.send_a),
                SettingId::SEND_B => Some(self.send_b),
                _ => None,
            },
            Owner::Slot { .. } | Owner::Mod(_) | Owner::Bypass(_) => None,
        }
    }

    /// The total steps a chain lasts: the pulses of every row in its phrases' loops. A missing or
    /// empty chain has none, and so does one whose rows are all skipped.
    pub fn chain_steps(&self, chain: u32) -> usize {
        self.chains.get(&chain).map_or(0, |chain| {
            chain
                .entries
                .iter()
                .filter_map(|entry| self.phrases.get(&entry.phrase))
                .map(|phrase| phrase.time())
                .sum()
        })
    }

    /// Find a step within a chain, counted from where it started, looping the chain if the
    /// position is past its end. Which time round it is says how far each phrase's order has
    /// walked, as each pass of a phrase carries its walk on from the last, and deals any phrase in
    /// it whose order is dealt, from the track's dealer.
    pub fn chain_position(&self, chain: u32, step: usize, dealer: Dealer) -> Option<ChainPosition> {
        let total = self.chain_steps(chain);
        if total == 0 {
            return None;
        }
        let lap = step / total;
        let mut remaining = step % total;
        let chain = self.chains.get(&chain)?;
        for (slot, entry) in chain.entries.iter().enumerate() {
            let Some(phrase) = self.phrases.get(&entry.phrase) else {
                continue;
            };
            let time = phrase.time();
            if remaining < time {
                let deal = dealer.deal(lap as i64, slot);
                let visit = phrase.walk_at(lap, remaining, deal)?;
                return Some(ChainPosition {
                    slot,
                    phrase: entry.phrase,
                    step: visit.row,
                    pulse: visit.pulse,
                    transpose: entry.transpose,
                    time: remaining,
                    visit: visit.visit,
                    deal,
                });
            }
            remaining -= time;
        }
        None
    }

    /// Where a slot and a row of its phrase's loop fall in a chain, if the chain still has them,
    /// on a time round it: where that pass of the phrase's walk plays the row at this pulse,
    /// nearest to `near` steps into the pass, or `near` itself if the pass doesn't play the row.
    pub fn chain_place(
        &self,
        chain: u32,
        at: RowPlace,
        lap: usize,
        dealer: Dealer,
    ) -> Option<usize> {
        let entries = &self.chains.get(&chain)?.entries;
        let phrase = self.phrases.get(&entries.get(at.slot)?.phrase)?;
        if at.row >= phrase.len() {
            return None;
        }
        let before: usize = entries[..at.slot]
            .iter()
            .filter_map(|entry| self.phrases.get(&entry.phrase))
            .map(|phrase| phrase.time())
            .sum();
        let deal = dealer.deal(lap as i64, at.slot);
        let place = phrase
            .place_in_pass(at.row, at.pulse, lap, at.near, deal)
            .unwrap_or_else(|| at.near.min(phrase.time().saturating_sub(1)));
        Some(before + place)
    }

    /// The step at a position, once a chain position has been found.
    pub fn step_at(&self, position: ChainPosition) -> Option<&Step> {
        self.phrases
            .get(&position.phrase)?
            .loop_steps()
            .get(position.step)
    }

    /// Make a chain with this ID and one new phrase.
    pub fn add_chain(&mut self, chain: u32) {
        let phrase = lowest_unused(&self.phrases);
        self.phrases
            .insert(phrase, Arc::new(Phrase::new(DEFAULT_PHRASE_STEPS)));
        self.chains.insert(
            chain,
            Arc::new(Chain {
                entries: vec![ChainEntry {
                    phrase,
                    transpose: 0,
                }],
                label: String::new(),
            }),
        );
    }
}

impl Default for Track {
    fn default() -> Self {
        Self {
            kind: TrackKind::default(),
            name: String::new(),
            machine: Machine::default(),
            settings: BTreeMap::new(),
            fine: BTreeMap::new(),
            mute: false,
            solo: false,
            groove: None,
            level: unity_level(),
            trim: unity_trim(),
            cutoff: open_cutoff(),
            resonance: 0,
            filter: FilterType::default(),
            filter_mode: FilterMode::default(),
            drive: 0,
            low_cut: 0,
            high_cut: open_cutoff(),
            inserts: Default::default(),
            bypassed: [false; INSERTS],
            pan: centre(),
            send_a: 0,
            send_b: 0,
            pitch: centre(),
            velocity_delta: centre(),
            choke: None,
            group: None,
            gate_length: None,
            speed: Speed::Sixteenth,
            reset: None,
            glide: 0,
            accumulation: Accumulation::default(),
            arpeggio: Arpeggio::default(),
            mods: default_mods(),
            chains: BTreeMap::new(),
            phrases: BTreeMap::new(),
        }
    }
}

fn unity_level() -> u8 {
    UNITY_LEVEL
}

fn unity_trim() -> u8 {
    UNITY_TRIM
}

fn is_unity_trim(trim: &u8) -> bool {
    *trim == UNITY_TRIM
}

fn open_cutoff() -> u8 {
    0xFF
}

fn centre() -> u8 {
    0x80
}

/// A synth's accent before any is set: Plaits' 0.8, as it strikes with nothing patched to its
/// level.
pub const DEFAULT_ACCENT: u8 = 0xCC;

/// A `Gate` that holds each note to the next, as every machine but `Acid` starts with.
pub const FULL_GATE: u8 = 0xFF;
/// `Acid`'s `Gate`, 50%, as a TB-303's gate closes about halfway through each step.
pub const ACID_GATE: u8 = 0x80;

/// What a section does to the tracks when it starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SectionStart {
    /// A track given a different chain starts it from its top, and one given the same keeps going.
    #[default]
    New,
    /// Every track goes back to its start, each time the section's length comes round.
    Reset,
}

/// What makes a track's sound.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Machine {
    /// Plays one of the bank's samples, once, at the note's speed.
    #[default]
    Sampler,
    /// One of the synth voices, after Mutable Instruments' Plaits, each with its own settings.
    Plaits(SynthVoice),
}

impl Machine {
    /// How much of its last pulse a note lasts until a track sets its own `Gate`.
    pub fn gate_length(self) -> u8 {
        match self {
            Self::Plaits(SynthVoice::Acid) => ACID_GATE,
            _ => FULL_GATE,
        }
    }

    /// Every machine, in the order a list shows them: Sampler, then each Plaits voice.
    pub const ALL: [Self; 1 + SynthVoice::ALL.len()] = {
        let mut all = [Self::Sampler; 1 + SynthVoice::ALL.len()];
        let mut at = 0;
        while at < SynthVoice::ALL.len() {
            all[at + 1] = Self::Plaits(SynthVoice::ALL[at]);
            at += 1;
        }
        all
    };

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sampler => "Sampler",
            Self::Plaits(voice) => voice.name(),
        }
    }

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Sampler => "A sample from the bank, at the speed of its note",
            Self::Plaits(voice) => voice.meaning(),
        }
    }

    /// Its settings, by their places among them: the sampler's own, or a synth's.
    pub fn settings(self) -> impl Iterator<Item = SettingId> {
        let count = match self {
            Self::Sampler => SAMPLER_SETTINGS,
            Self::Plaits(voice) => voice_entries(voice).len(),
        };
        (0..count).filter_map(move |at| match self {
            Self::Sampler => SettingId::sampler(at),
            Self::Plaits(voice) => SettingId::voice(voice, at),
        })
    }

    /// The place of a setting it has, by the key its file saves it under.
    fn setting_at(self, key: &str) -> Option<usize> {
        self.settings().position(|id| id.entry().key == key)
    }

    /// The setting that gives it its character, which its device's title shows after it, as an
    /// effect's type is: the sample the sampler plays, `Acid`'s wave, 6-op's patch, and the chord
    /// of `Chord` and `Chiptune`. Other synths have none.
    pub fn type_param(self) -> Option<usize> {
        let key = match self {
            Self::Sampler => "sample",
            Self::Plaits(SynthVoice::Acid) => "wave",
            Self::Plaits(SynthVoice::SixOp) => "patch",
            Self::Plaits(SynthVoice::Chord | SynthVoice::Chiptune) => "chord",
            Self::Plaits(_) => return None,
        };
        self.setting_at(key)
    }

    /// Where its settings sit on the Sound page, by their places among them, its type being in the
    /// title instead. Most have theirs down each column in the order they're listed. The sampler's
    /// say which part of the sample plays and how down its first column, and where that part starts
    /// and how long it is down its second, as the sample page has them; `Acid`'s are in pairs as a
    /// TB-303's knobs are, the filter's, its envelope's, then accent and slide, with the Devil
    /// Fish's drive and `Drift` after them.
    pub fn grid(self) -> Grid {
        let placed: &[&[&str]] = match self {
            Self::Sampler => &[&["slice", "play"], &["start", "length"]],
            Self::Plaits(SynthVoice::Acid) => &[
                &["cutoff", "resonance"],
                &["envmod", "decay"],
                &["accent", "slide"],
                &["drive", "drift"],
            ],
            Self::Plaits(_) => {
                let own: Vec<usize> = (0..self.settings().count())
                    .filter(|at| Some(*at) != self.type_param())
                    .collect();
                let columns: Vec<Vec<Option<usize>>> = own
                    .chunks(GRID_ROWS)
                    .map(|column| column.iter().copied().map(Some).collect())
                    .collect();
                return Grid::from_columns(&columns);
            }
        };
        let columns: Vec<Vec<Option<usize>>> = placed
            .iter()
            .map(|column| column.iter().map(|key| self.setting_at(key)).collect())
            .collect();
        Grid::from_columns(&columns)
    }
}

/// How many patches 6-op has, which its `Patch` counts from `00`.
pub const SIX_OP_PATCHES: u8 = 96;

/// Which synth the Plaits machine plays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SynthVoice {
    /// A square and a saw.
    #[default]
    VirtualAnalog,
    /// A saw or a square, with a sub, through a resonant filter.
    VirtualAnalogVcf,
    /// A saw or a square through a diode ladder, with accent and slide, after Roland's TB-303.
    Acid,
    /// Two-operator FM.
    Fm,
    /// A sine bent by a second wave's phase.
    PhaseDistortion,
    /// A slope through a waveshaper and a wavefolder.
    Waveshaping,
    /// Two formants, made of grains.
    Formant,
    /// 24 harmonics, in bumps along the spectrum.
    Additive,
    /// A grid of waves in four banks.
    Wavetable,
    /// A path traced over a surface of waves.
    WaveTerrain,
    /// A chord from one note, on organ, strings or waves.
    Chord,
    /// A chord of square waves, as on old game consoles.
    Chiptune,
    /// Six-operator FM, playing DX7 patches.
    SixOp,
    /// A cloud of eight saws in grains.
    Swarm,
    /// A plucked string.
    String,
    /// A struck resonator.
    Modal,
    /// Random impulses through resonant filters.
    Dust,
    /// Clocked noise through a resonant filter.
    Noise,
    /// An 808-style bass drum.
    Kick,
    /// An 808-style snare drum.
    Snare,
    /// An 808-style hi-hat.
    Hat,
}

impl SynthVoice {
    /// Every voice, in the order a list shows them.
    pub const ALL: [Self; 21] = [
        Self::VirtualAnalog,
        Self::VirtualAnalogVcf,
        Self::Acid,
        Self::Fm,
        Self::PhaseDistortion,
        Self::Waveshaping,
        Self::Formant,
        Self::Additive,
        Self::Wavetable,
        Self::WaveTerrain,
        Self::SixOp,
        Self::Chord,
        Self::Chiptune,
        Self::Swarm,
        Self::String,
        Self::Modal,
        Self::Dust,
        Self::Noise,
        Self::Kick,
        Self::Snare,
        Self::Hat,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::VirtualAnalog => "VA",
            Self::VirtualAnalogVcf => "VA VCF",
            Self::Acid => "Acid",
            Self::Fm => "FM",
            Self::PhaseDistortion => "PD",
            Self::Waveshaping => "Fold",
            Self::Formant => "Formant",
            Self::Additive => "Additive",
            Self::Swarm => "Swarm",
            Self::WaveTerrain => "Terrain",
            Self::Chord => "Chord",
            Self::Chiptune => "Chip",
            Self::Wavetable => "Waves",
            Self::SixOp => "6-op",
            Self::String => "String",
            Self::Modal => "Modal",
            Self::Dust => "Dust",
            Self::Noise => "Noise",
            Self::Kick => "Kick",
            Self::Snare => "Snare",
            Self::Hat => "Hat",
        }
    }

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::VirtualAnalog => "A square and a saw, for bass and leads",
            Self::VirtualAnalogVcf => "A saw or square with a sub, through a filter",
            Self::Acid => "A 303-style bass, with accent and slide",
            Self::PhaseDistortion => "A sine bent out of shape, as on Casio's CZ",
            Self::Formant => "Two formants of grains, for vowels and leads",
            Self::Additive => "24 harmonics in bumps, for organs and glass",
            Self::Swarm => "Eight saws in grains, for pads and drones",
            Self::WaveTerrain => "A path over a surface of waves, for evolving tones",
            Self::Chord => "A chord from one note, on organ, strings or waves",
            Self::Chiptune => "A chord of squares, as on old game consoles",
            Self::Fm => "Two-operator FM, for bass, bells and keys",
            Self::Waveshaping => "A wave folded into harmonics, for bass and leads",
            Self::Wavetable => "Waves in four banks to sweep, for pads and leads",
            Self::SixOp => "Six-op FM with 96 DX7 patches, for keys and bells",
            Self::String => "A plucked string, from a sitar's buzz to metal",
            Self::Modal => "A struck resonator, for bells, bars and wood",
            Self::Dust => "Random clicks, filtered, for crackle and rain",
            Self::Noise => "Clocked noise through a filter, for hiss and wind",
            Self::Kick => "An 808-style bass drum, tuned by its note",
            Self::Snare => "An 808-style snare drum, tuned by its note",
            Self::Hat => "An 808-style hi-hat, tuned by its note",
        }
    }

    /// Whether a note keeps sounding until something ends it.
    pub fn holds(self) -> bool {
        matches!(
            self,
            Self::VirtualAnalog
                | Self::VirtualAnalogVcf
                | Self::Acid
                | Self::Fm
                | Self::PhaseDistortion
                | Self::Waveshaping
                | Self::Formant
                | Self::Additive
                | Self::Wavetable
                | Self::WaveTerrain
                | Self::SixOp
                | Self::Chord
                | Self::Chiptune
                | Self::Swarm
                | Self::Dust
                | Self::Noise
        )
    }
}

/// When a launched chain starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Launch {
    /// When the track's own chain ends.
    ChainEnd,
    /// On the next step this many into the row, so every row's start is one.
    Every(NonZeroU32),
}

/// The lengths `:launch` has names for.
const LAUNCH_NAMES: [(&str, u32); 7] = [
    ("step", 1),
    ("1/4", BEAT_STEPS as u32),
    ("1/2", BEAT_STEPS as u32 * 2),
    ("1bar", BAR_STEPS as u32),
    ("2bar", BAR_STEPS as u32 * 2),
    ("4bar", BAR_STEPS as u32 * 4),
    ("8bar", BAR_STEPS as u32 * 8),
];

/// What `:launch` takes for launching when a chain ends.
const LAUNCH_CHAIN: &str = "chain";

/// Every name `:launch` takes, in order, which the command line completes from.
pub const LAUNCH_WORDS: [&str; LAUNCH_NAMES.len() + 1] = {
    let mut words = [LAUNCH_CHAIN; LAUNCH_NAMES.len() + 1];
    let mut at = 0;
    while at < LAUNCH_NAMES.len() {
        words[at] = LAUNCH_NAMES[at].0;
        at += 1;
    }
    words
};

impl Launch {
    /// Launch on a grid of this many steps, if it's 1 or more.
    pub fn every(steps: usize) -> Option<Self> {
        let steps = u32::try_from(steps).ok()?;
        NonZeroU32::new(steps).map(Self::Every)
    }

    /// The setting with this name in `:launch`.
    pub fn named(name: &str) -> Option<Self> {
        if name == LAUNCH_CHAIN {
            return Some(Self::ChainEnd);
        }
        let (_, steps) = LAUNCH_NAMES.iter().find(|(known, _)| *known == name)?;
        NonZeroU32::new(*steps).map(Self::Every)
    }

    /// When a launch starts, to finish a sentence on the status line.
    pub fn when(self) -> String {
        match self {
            Self::ChainEnd => "when its chain ends".to_owned(),
            Self::Every(_) => format!("on the {self} grid"),
        }
    }
}

impl Default for Launch {
    fn default() -> Self {
        Self::every(BAR_STEPS).unwrap_or(Self::ChainEnd)
    }
}

impl fmt::Display for Launch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self::Every(steps) = self else {
            return f.write_str("chain");
        };
        match LAUNCH_NAMES.iter().find(|(_, known)| *known == steps.get()) {
            Some((name, _)) => f.write_str(name),
            None => write!(f, "{steps}-step"),
        }
    }
}

/// A track's own `Reset` as the track page shows it: `Off`, whole bars as `:launch` names them,
/// such as `1bar`, and anything else in steps, such as `5-step`.
pub fn reset_name(reset: Option<u16>) -> String {
    match reset {
        None => "Off".to_owned(),
        Some(steps) if usize::from(steps).is_multiple_of(BAR_STEPS) => {
            format!("{}bar", usize::from(steps) / BAR_STEPS)
        }
        Some(steps) => format!("{steps}-step"),
    }
}

/// A track's own `Reset` from what's typed: `off`, a number of steps, in hex after `0x`, or of
/// bars, such as `2bar`, from 1 step to [`MAX_TRACK_RESET`]. The outer `None` is for text that
/// isn't one.
pub fn reset_named(text: &str) -> Option<Option<u16>> {
    let text = text.trim().to_ascii_lowercase();
    if text == "off" {
        return Some(None);
    }
    let steps = if let Some(bars) = text.strip_suffix("bars").or(text.strip_suffix("bar")) {
        bars.trim()
            .parse::<u32>()
            .ok()?
            .checked_mul(BAR_STEPS as u32)?
    } else if let Some(digits) = text.strip_prefix("0x") {
        u32::from_str_radix(digits, 16).ok()?
    } else {
        text.trim_end_matches("-step")
            .trim_end_matches("steps")
            .trim_end_matches("step")
            .trim()
            .parse::<u32>()
            .ok()?
    };
    let steps = u16::try_from(steps).ok()?;
    (1..=MAX_TRACK_RESET)
        .contains(&steps)
        .then_some(Some(steps))
}

/// How many times a section plays before moving on. Saved songs can't hold 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Repeat(NonZeroU16);

impl Repeat {
    /// As many times as this, from 1 to `u16::MAX`.
    pub fn new(times: i64) -> Self {
        let times = times.clamp(1, i64::from(u16::MAX)) as u16;
        Self(NonZeroU16::new(times).unwrap_or(NonZeroU16::MIN))
    }

    /// How many times it is.
    pub fn get(self) -> u16 {
        self.0.get()
    }
}

impl Default for Repeat {
    fn default() -> Self {
        Self(NonZeroU16::MIN)
    }
}

/// One row of the song, holding one chain per track.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub chains: Vec<Option<u32>>,
    /// How many steps it plays. If not set, the length of its longest chain.
    #[serde(default)]
    pub length: Option<usize>,
    /// How many times it plays before moving on.
    #[serde(default)]
    pub repeat: Repeat,
    /// What it does to the tracks when it starts.
    #[serde(default)]
    pub start: SectionStart,
    /// Send every track back to its start after this many steps. Without it, each loops on its own.
    #[serde(default)]
    pub reset: Option<usize>,
}

/// One phrase in a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainEntry {
    pub phrase: u32,
    pub transpose: i8,
}

/// A list of phrases, each with a transpose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chain {
    pub entries: Vec<ChainEntry>,
    /// What to call it on screen, or empty for just its number.
    #[serde(default)]
    pub label: String,
}

/// Where an FX column loops on its own, among a phrase's stored steps. It drifts against the
/// notes, which follow the phrase's loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lane {
    pub start: usize,
    pub length: usize,
}

impl Lane {
    /// Whether a stored step is in it.
    pub fn contains(self, step: usize) -> bool {
        (self.start..self.start + self.length).contains(&step)
    }

    /// The stored step it reads at this count, which can be below 0 after playing from a step.
    pub fn step_at(self, count: i64) -> usize {
        self.start + count.rem_euclid(self.length.max(1) as i64) as usize
    }
}

/// One track's sequence of steps, reusable by ID.
///
/// It stores more than it plays, like a clip: a loop of `length` steps starting at `start`, with
/// anything either side of it kept but silent. Rows on screen count from the loop's start, so the
/// loop's first step is always beat 1. Each FX column can have a lane of its own. Its loop's rows
/// play in its order, while its lanes keep to their own loops, forward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PhraseFile", into = "PhraseFile")]
pub struct Phrase {
    steps: Vec<Step>,
    start: usize,
    length: usize,
    lanes: [Option<Lane>; FX_SLOTS],
    order: Order,
    generators: Vec<Generator>,
}

#[derive(Serialize, Deserialize)]
struct PhraseFile {
    steps: Vec<Step>,
    #[serde(default)]
    start: usize,
    #[serde(default)]
    length: Option<usize>,
    #[serde(default)]
    lanes: [Option<Lane>; FX_SLOTS],
    #[serde(default, skip_serializing_if = "Order::is_forward")]
    order: Order,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    generators: Vec<GeneratorFile>,
}

/// A lane that fits in this many stored steps, or none if it can't.
fn fitted(lane: Option<Lane>, stored: usize) -> Option<Lane> {
    let lane = lane?;
    (lane.start < stored).then(|| Lane {
        start: lane.start,
        length: lane.length.clamp(1, stored - lane.start),
    })
}

impl From<PhraseFile> for Phrase {
    fn from(file: PhraseFile) -> Self {
        let stored = file.steps.len();
        let start = file.start.min(stored.saturating_sub(1));
        let length = file
            .length
            .unwrap_or(stored)
            .min(stored - start.min(stored));
        let generators = file
            .generators
            .into_iter()
            .filter_map(|generator| generator.generator(&file.steps))
            .collect();
        Self {
            lanes: file.lanes.map(|lane| fitted(lane, stored)),
            steps: file.steps,
            start,
            length,
            order: file.order,
            generators,
        }
    }
}

impl From<Phrase> for PhraseFile {
    fn from(phrase: Phrase) -> Self {
        Self {
            steps: phrase.steps,
            start: phrase.start,
            length: Some(phrase.length),
            lanes: phrase.lanes,
            order: phrase.order,
            generators: phrase.generators.iter().map(GeneratorFile::of).collect(),
        }
    }
}

impl Phrase {
    /// A phrase of empty steps. A phrase always has at least one.
    pub fn new(length: usize) -> Self {
        let length = length.max(1);
        Self {
            steps: vec![Step::default(); length],
            start: 0,
            length,
            lanes: [None; FX_SLOTS],
            order: Order::Forward,
            generators: Vec::new(),
        }
    }

    /// A phrase holding these steps, looping over all of them.
    pub fn from_steps(steps: Vec<Step>) -> Self {
        let length = steps.len();
        Self {
            steps,
            start: 0,
            length,
            lanes: [None; FX_SLOTS],
            order: Order::Forward,
            generators: Vec::new(),
        }
    }

    /// Every stored step, in or out of the loop.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The steps that play.
    pub fn loop_steps(&self) -> &[Step] {
        &self.steps[self.start..self.start + self.length]
    }

    /// The order its loop's rows play in.
    pub fn order(&self) -> Order {
        self.order
    }

    /// Play its loop's rows in another order.
    pub fn set_order(&mut self, order: Order) {
        self.order = order;
    }

    /// The rows of its loop one round of its order visits, in turn, with this deal if it's dealt.
    pub fn round(&self, deal: Deal) -> Round<'_> {
        Round::new(self.loop_steps(), self.order, deal)
    }

    /// The next value with this command in a slot, for a glide: looking on from a visit of a
    /// round, in the order the rows play and wrapping round the round, with how many steps there
    /// are from the start of that visit's row to the row it's on. The row itself doesn't count.
    pub fn glide_to(
        &self,
        visit: usize,
        deal: Deal,
        slot: usize,
        command: FxCommand,
    ) -> Option<(usize, FxSlot)> {
        let steps = self.loop_steps();
        let visits = self.round(deal).count();
        let mut time = 0;
        for (distance, row) in self
            .round(deal)
            .cycle()
            .skip(visit)
            .take(visits)
            .enumerate()
        {
            if distance > 0 {
                let fx = steps[row].fx.get(slot)?;
                if fx.command == Some(command) {
                    return Some((time, *fx));
                }
            }
            time += steps[row].pulses();
        }
        None
    }

    /// Its live generators, each on a column of its own rows or of a lane.
    pub fn generators(&self) -> &[Generator] {
        &self.generators
    }

    /// The rows a generator would cover now: its own, as many as are stored, or its FX column's
    /// lane, or none if that column has no lane.
    pub fn span_rows(&self, generator: &Generator) -> Option<StepRange> {
        let stored = self.steps.len();
        match generator.span {
            Span::Rows { first, last } => {
                (first < stored).then(|| StepRange::between(first, last.min(stored - 1)))
            }
            Span::Lane => match generator.column {
                ShapeColumn::Fx { slot, .. } => self.lane(slot).map(|lane| {
                    StepRange::between(lane.start, lane.start + lane.length.max(1) - 1)
                }),
                _ => None,
            },
        }
    }

    /// Make a generator live, from the values on its rows now. One on the same column whose rows
    /// overlap is applied first, so this one starts from the values it left. Returns false if it
    /// has no rows here.
    pub fn add_generator(&mut self, mut generator: Generator) -> bool {
        let Some(range) = self.span_rows(&generator) else {
            return false;
        };
        self.generators.retain(|other| {
            !other.shares(generator.column)
                || other
                    .rows()
                    .is_none_or(|rows| rows.last < range.first || rows.first > range.last)
        });
        generator.start(&mut self.steps, range);
        self.generators.push(generator);
        true
    }

    /// Give a live generator a new shape, and write it. Returns false if there's no such one.
    pub fn reshape_generator(&mut self, index: usize, shape: Shape) -> bool {
        let Some(generator) = self.generators.get_mut(index) else {
            return false;
        };
        generator.shape = shape;
        generator.render(&mut self.steps);
        true
    }

    /// Apply a live generator: its values stay, as plain ones.
    pub fn apply_generator(&mut self, index: usize) -> Option<Generator> {
        (index < self.generators.len()).then(|| self.generators.remove(index))
    }

    /// Follow an edit to the phrase: a live generator whose cells the edit changed by hand is
    /// applied, and the rest rerender where the edit changed what they read or moved their lane.
    pub fn follow_generators(&mut self) {
        let ranges: Vec<Option<StepRange>> = self
            .generators
            .iter()
            .map(|generator| self.span_rows(generator))
            .collect();
        let steps = &mut self.steps;
        let mut ranges = ranges.into_iter();
        self.generators
            .retain_mut(|generator| generator.follow(steps, ranges.next().flatten()));
    }

    /// An FX column's own lane, if it has one. One without follows the phrase's loop.
    pub fn lane(&self, slot: usize) -> Option<Lane> {
        self.lanes.get(slot).copied().flatten()
    }

    /// Give an FX column a lane of its own, or none to follow the phrase's loop again. Steps are
    /// stored for all of it, up to the most a phrase keeps.
    pub fn set_lane(&mut self, slot: usize, lane: Option<Lane>) {
        if slot >= FX_SLOTS {
            return;
        }
        if let Some(lane) = lane {
            self.store_up_to((lane.start + lane.length.max(1)).min(MAX_PHRASE_STEPS));
        }
        self.lanes[slot] = fitted(lane, self.steps.len());
    }

    /// The next value with this command in a lane, looking forward from one of its steps and
    /// wrapping within it, with how many steps away it is.
    pub fn next_in_lane(
        &self,
        slot: usize,
        step: usize,
        command: FxCommand,
    ) -> Option<(usize, FxSlot)> {
        let lane = self.lane(slot)?;
        let at = step.checked_sub(lane.start)?;
        (1..lane.length).find_map(|distance| {
            let index = lane.start + (at + distance) % lane.length;
            let fx = self.steps.get(index)?.fx.get(slot)?;
            (fx.command == Some(command)).then_some((distance, *fx))
        })
    }

    /// How many steps a pass of its loop lasts, whatever its order: each row's pulses, so a skipped
    /// row takes none.
    pub fn time(&self) -> usize {
        self.loop_steps().iter().map(Step::pulses).sum()
    }

    /// How many steps into a pass a row of the loop starts, as written, so where it plays forward.
    pub fn time_to(&self, row: usize) -> usize {
        self.loop_steps().iter().take(row).map(Step::pulses).sum()
    }

    /// How many steps a round of its order lasts before its walk repeats: a pass for an order that
    /// visits each row once, and for a dealt one, whose round is a pass, and longer for one that
    /// visits some rows twice.
    pub fn round_time(&self) -> usize {
        if self.order.is_dealt() {
            return self.time();
        }
        let steps = self.loop_steps();
        self.round(Deal::default())
            .map(|row| steps[row].pulses())
            .sum()
    }

    /// Where a pass starts in the round of its order, for a pass this many since its chain started.
    fn pass_start(&self, pass: usize, round: usize) -> usize {
        pass % round * (self.time() % round) % round
    }

    /// The visit of its order's walk playing this many steps into a pass, this many passes since
    /// its chain started, with the pass's deal if the order is dealt: its row, which of the row's
    /// pulses, and which visit of the round it is. The walk carries on from one pass to the next,
    /// so a row whose pulses run past the end of a pass plays the rest of them at the start of the
    /// next. Skipped rows are passed over, and a loop that takes no time has none.
    pub fn walk_at(&self, pass: usize, time: usize, deal: Deal) -> Option<Visit> {
        let round = self.round_time();
        if time >= self.time() || round == 0 {
            return None;
        }
        let steps = self.loop_steps();
        let mut left = (self.pass_start(pass, round) + time) % round;
        for (visit, row) in self.round(deal).enumerate() {
            let pulses = steps[row].pulses();
            if left < pulses {
                return Some(Visit {
                    row,
                    pulse: left,
                    visit,
                });
            }
            left -= pulses;
        }
        None
    }

    /// Where in a pass, this many since its chain started, the walk plays a row at one of its
    /// pulses, or its last if it has fewer: the place nearest to `near`, if the pass plays the row
    /// at all.
    pub fn place_in_pass(
        &self,
        row: usize,
        pulse: usize,
        pass: usize,
        near: usize,
        deal: Deal,
    ) -> Option<usize> {
        let (length, round) = (self.time() as i64, self.round_time() as i64);
        let steps = self.loop_steps();
        let pulses = steps.get(row)?.pulses();
        if pulses == 0 || round == 0 {
            return None;
        }
        let pulse = pulse.min(pulses - 1) as i64;
        let mut best: Option<i64> = None;
        // Where each round that plays in the pass starts, from the pass's start.
        let mut started = -(self.pass_start(pass, round as usize) as i64);
        while started < length {
            let mut time = started;
            for visited in self.round(deal) {
                let place = time + pulse;
                let closer = |best: i64| place.abs_diff(near as i64) < best.abs_diff(near as i64);
                if visited == row && (0..length).contains(&place) && best.is_none_or(closer) {
                    best = Some(place);
                }
                time += steps[visited].pulses() as i64;
            }
            started += round;
        }
        best.map(|place| place as usize)
    }

    /// The number of rows that play.
    pub fn len(&self) -> usize {
        self.length
    }

    /// Always false, since a phrase has at least one step.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Where the loop starts among the stored steps.
    pub fn start(&self) -> usize {
        self.start
    }

    /// Where the loop sits, for working out rows and beats.
    pub fn extent(&self) -> Extent {
        Extent {
            start: self.start,
            length: self.length,
            stored: self.steps.len(),
        }
    }

    /// One stored step, to change.
    pub fn step_mut(&mut self, index: usize) -> Option<&mut Step> {
        self.steps.get_mut(index)
    }

    /// Set the loop's length, keeping every step. A length of 0 becomes 1.
    pub fn set_len(&mut self, length: usize) {
        self.length = length.max(1);
        self.store_up_to(self.start + self.length);
    }

    /// Store at least this many steps, up to the most a phrase keeps. The loop stays where it is.
    pub fn grow_to(&mut self, stored: usize) {
        self.store_up_to(stored.min(MAX_PHRASE_STEPS));
    }

    fn store_up_to(&mut self, stored: usize) {
        if stored > self.steps.len() {
            self.steps.resize(stored, Step::default());
        }
    }

    /// Move the loop without changing its length, adding empty steps if it runs off either end.
    pub fn slide(&mut self, by: isize) {
        let room = MAX_PHRASE_STEPS.saturating_sub(self.steps.len());
        let to = (self.start as isize).saturating_add(by);
        // Where the loop started, counted in the steps stored after any are added in front.
        let mut was = self.start;
        if to < 0 {
            let added = to.unsigned_abs().min(room);
            self.steps
                .splice(0..0, std::iter::repeat_n(Step::default(), added));
            self.start = 0;
            was += added;
            for lane in self.lanes.iter_mut().flatten() {
                lane.start += added;
            }
            for generator in &mut self.generators {
                generator.shift(added);
            }
        } else {
            let highest = MAX_PHRASE_STEPS
                .max(self.steps.len())
                .saturating_sub(self.length);
            self.start = (to as usize).min(highest);
        }
        self.store_up_to(self.start + self.length);
        // Lanes move with the loop, as they're set where it is. One that would go above the first
        // step stops there.
        let moved = self.start as isize - was as isize;
        let mut lanes = self.lanes;
        for lane in lanes.iter_mut().flatten() {
            lane.start = lane.start.saturating_add_signed(moved);
        }
        for (slot, lane) in lanes.into_iter().enumerate() {
            self.set_lane(slot, lane);
        }
    }

    /// Move the loop's steps later, wrapping round within it. Negative moves them earlier, and steps
    /// outside the loop stay where they are.
    pub fn rotate(&mut self, by: isize) {
        let (start, length) = (self.start, self.length);
        // A live generator on the rows it moves is applied, as its values move with them.
        let end = start + length;
        self.generators.retain(|generator| {
            generator
                .rows()
                .is_none_or(|rows| rows.last < start || rows.first >= end)
        });
        if length > 0 {
            let by = by.rem_euclid(length as isize) as usize;
            self.steps[start..start + length].rotate_right(by);
        }
    }

    /// Double the loop by repeating it. Steps after it move down rather than being overwritten.
    pub fn double(&mut self) {
        // Its live generators are applied, as the rows after the loop move.
        self.generators.clear();
        let copy = self.loop_steps().to_vec();
        let end = self.start + self.length;
        let added = copy.len();
        self.steps.splice(end..end, copy);
        self.length *= 2;
        // Lanes after the loop move down with their steps.
        for lane in self.lanes.iter_mut().flatten() {
            if lane.start >= end {
                lane.start += added;
            }
        }
    }
}

/// Where a place in a pass of a phrase lands, on its order's walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visit {
    /// The row of the phrase's loop.
    pub row: usize,
    /// Which of the row's pulses, from 0 for its first.
    pub pulse: usize,
    /// Which of the round's visits, which is the row in a phrase played forward.
    pub visit: usize,
}

/// A row of a chain's slot to find again after a change, with the pulse it was on and how many
/// steps into its pass it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowPlace {
    pub slot: usize,
    pub row: usize,
    pub pulse: usize,
    pub near: usize,
}

/// Where a position in a chain lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainPosition {
    pub slot: usize,
    pub phrase: u32,
    /// The row of the phrase's loop.
    pub step: usize,
    /// Which of the row's pulses, from 0 for its first.
    pub pulse: usize,
    pub transpose: i8,
    /// How many steps into the phrase's pass it is.
    pub time: usize,
    /// Which visit of the round of the phrase's order it is, and the pass's deal, for looking on
    /// in its order.
    pub visit: usize,
    pub deal: Deal,
}

impl ChainPosition {
    /// Its row, to find again after a change.
    pub fn row_place(&self) -> RowPlace {
        RowPlace {
            slot: self.slot,
            row: self.step,
            pulse: self.pulse,
            near: self.time,
        }
    }
}

fn default_limiter() -> bool {
    true
}

fn default_skip() -> u32 {
    BEAT_STEPS as u32
}

/// Everything saved in `song.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Song {
    pub format_version: u32,
    /// The project's name, which saving it to a new folder names the folder after. Empty in a song
    /// that hasn't been given one, which takes its folder's.
    #[serde(default)]
    pub name: String,
    /// Tempo, in thousandths of a BPM.
    pub millibpm: u32,
    pub seed: u64,
    /// Whether entering something in Insert moves on by the grid, `skip`. Kept with the song and
    /// not used to play it.
    #[serde(default)]
    pub auto_jump: bool,
    /// The grid: how far the left and right arrows move in Insert, and auto-jump with them.
    #[serde(default = "default_skip")]
    pub skip: u32,
    /// The scale for entering and moving notes, kept with the song. Playing reads it only for what an
    /// `ACU` moves notes through, which the app sends the engine as it changes.
    #[serde(default)]
    pub scale: Scale,
    /// Whether the main output clips at full level. Off, a render keeps levels above it.
    #[serde(default = "default_limiter")]
    pub limiter: bool,
    /// The groove every track plays with unless it has its own.
    #[serde(default)]
    pub groove: GrooveId,
    /// The project's own grooves, `Custom 1` first. One not made yet plays on the grid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grooves: Vec<Groove>,
    /// When launched chains start.
    #[serde(default)]
    pub launch: Launch,
    /// The project's samples, which a sampler track's `Sample` picks from by number.
    #[serde(default, skip_serializing_if = "Bank::is_empty")]
    pub bank: Bank,
    pub tracks: Vec<Track>,
    pub sections: Vec<Section>,
}

impl Song {
    /// A new project: one section, one chain and one phrase per track.
    pub fn new(tracks: usize) -> Self {
        let mut song = Self {
            format_version: FORMAT_VERSION,
            name: String::new(),
            millibpm: DEFAULT_MILLIBPM,
            seed: 0,
            auto_jump: false,
            skip: default_skip(),
            scale: Scale::default(),
            limiter: default_limiter(),
            groove: GrooveId::STRAIGHT,
            grooves: Vec::new(),
            launch: Launch::default(),
            bank: Bank::default(),
            tracks: Vec::new(),
            sections: vec![Section::default()],
        };
        song.ensure_fixed_tracks();
        for _ in 0..tracks.max(1) {
            song.add_track();
        }
        song
    }

    /// Give a song any fixed tracks it lacks, each in its place after the sound tracks: all of them
    /// for a song built without, or the groups past those it was saved with once `MAX_GROUPS` is
    /// raised.
    pub fn ensure_fixed_tracks(&mut self) {
        let first = self
            .tracks
            .iter()
            .position(|track| !track.is_sound())
            .unwrap_or(self.tracks.len());
        for (place, kind) in TrackKind::FIXED.into_iter().enumerate() {
            let at = first + place;
            if self.tracks.get(at).is_some_and(|track| track.kind == kind) {
                continue;
            }
            self.tracks.insert(at, Track::fixed(kind));
            for section in &mut self.sections {
                if at <= section.chains.len() {
                    section.chains.insert(at, None);
                }
            }
        }
        for section in &mut self.sections {
            section.chains.resize(self.tracks.len(), None);
        }
    }

    /// Put a file in the bank, unless it's in it already, and have a track's sampler play it,
    /// giving the entry's number. Its hash is left unknown, for songs built in code, such as tests',
    /// whose samples are put in the pool by name. A full bank, or no such track, changes nothing.
    pub fn sample_on(&mut self, track: usize, path: &str) -> Option<u8> {
        let number = self
            .bank
            .add(BankEntry::of_file(path, ContentHash::UNKNOWN))?;
        let track = self.tracks.get_mut(track)?;
        track.machine = Machine::Sampler;
        track.set_setting(SettingId::SAMPLE, number);
        Some(number)
    }

    /// The bank entries something names: a track's own `Sample`, or a step's `SMP`. An LFO or
    /// an envelope that moves `Sample` can reach others, which aren't counted.
    pub fn samples_in_use(&self) -> BTreeSet<u8> {
        let mut used = BTreeSet::new();
        for track in &self.tracks {
            used.insert(track.sample());
            let locks = track
                .phrases
                .values()
                .flat_map(|phrase| phrase.steps())
                .flat_map(|step| step.fx.iter())
                .filter(|slot| slot.command == Some(FxCommand::Set(SettingId::SAMPLE)));
            used.extend(locks.map(|slot| slot.coarse()));
        }
        used.remove(&0);
        used
    }

    /// Have every step that locks a slice of an entry by number lock the one it's now numbered,
    /// from what each old number became, so patterns keep their chops as the entry is sliced
    /// again. A step plays the entry its own `SMP` locks, or else its track's `Sample`. A number
    /// past the last slice played the last, so it follows the last. An LFO's depth on `SLI` is a
    /// distance rather than a slice, so it's left as it is.
    pub fn renumber_slices(&mut self, number: u8, renumbered: &[u8]) {
        let Some(last) = renumbered.len().checked_sub(1) else {
            return;
        };
        let renumber = |slot: &mut FxSlot| {
            let old = usize::from(slot.coarse()).min(last);
            slot.value = (u16::from(renumbered[old]) << 8) | u16::from(slot.fine());
        };
        // A lock by a letter follows its slice already, through the entry's marks.
        let is_slice = |slot: &FxSlot| {
            slot.command == Some(FxCommand::Set(SettingId::SLICE)) && slot.slice_mark().is_none()
        };
        for track in &mut self.tracks {
            if track.machine != Machine::Sampler || !track.is_sound() {
                continue;
            }
            let own = track.sample();
            for phrase in track.phrases.values_mut() {
                let plays = |step: &Step| {
                    let locked = step
                        .fx
                        .iter()
                        .rev()
                        .find(|slot| slot.command == Some(FxCommand::Set(SettingId::SAMPLE)));
                    locked.map_or(own, |slot| slot.coarse()) == number
                };
                let touched = phrase
                    .steps()
                    .iter()
                    .any(|step| plays(step) && step.fx.iter().any(is_slice));
                if !touched {
                    continue;
                }
                let phrase = Arc::make_mut(phrase);
                for at in 0..phrase.steps().len() {
                    let Some(step) = phrase.step_mut(at) else {
                        continue;
                    };
                    if plays(step) {
                        step.fx
                            .iter_mut()
                            .filter(|slot| is_slice(slot))
                            .for_each(&renumber);
                    }
                }
            }
        }
    }

    /// How many tracks play sounds. They come first, and then the fixed tracks.
    pub fn sound_tracks(&self) -> usize {
        self.tracks.len().saturating_sub(FIXED_TRACKS)
    }

    /// Where a fixed track is among all the tracks.
    pub fn fixed_track(&self, kind: TrackKind) -> Option<usize> {
        Some(self.sound_tracks() + kind.fixed_place()?)
    }

    /// The group track a sound track plays through, if it's in a group.
    pub fn group_track_of(&self, track: usize) -> Option<usize> {
        let group = self.tracks.get(track).filter(|t| t.is_sound())?.group?;
        self.fixed_track(TrackKind::Group(group))
    }

    /// The sound tracks a group track sums, in order, or none for any other track.
    pub fn group_members(&self, group: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.sound_tracks()).filter(move |&track| self.group_track_of(track) == Some(group))
    }

    /// Whether a track is heard, by the mutes and solos saved with the song: a sound track by its
    /// own and its group's, and a group while any of its tracks is. Soloing a track keeps its
    /// group heard, and soloing a group plays all its tracks. The returns and the master are
    /// always counted heard.
    pub fn heard(&self, track: usize) -> bool {
        self.heard_by(track, |at| (self.tracks[at].mute, self.tracks[at].solo))
    }

    /// Whether a track is heard as [`Song::heard`] has it, by mutes and solos given for each
    /// track as `(mute, solo)`, such as the ones the keys set while playing.
    pub fn heard_by(&self, track: usize, silenced: impl Fn(usize) -> (bool, bool) + Copy) -> bool {
        let Some(this) = self.tracks.get(track) else {
            return false;
        };
        if this.is_group() {
            return self
                .group_members(track)
                .any(|member| self.heard_by(member, silenced));
        }
        if !this.is_sound() {
            return true;
        }
        let (mute, solo) = silenced(track);
        let group = self.group_track_of(track).map(silenced);
        let any_solo =
            (0..self.tracks.len().min(self.sound_tracks() + MAX_GROUPS)).any(|at| silenced(at).1);
        !mute
            && !group.is_some_and(|(mute, _)| mute)
            && (!any_solo || solo || group.is_some_and(|(_, solo)| solo))
    }

    /// A groove's pushes: one that comes with the program, or the project's own, which is a bar
    /// on the grid until it's made.
    pub fn groove(&self, id: GrooveId) -> Groove {
        match id.custom_number() {
            Some(number) => self.grooves.get(number).copied().unwrap_or_default(),
            None => id.preset().unwrap_or_default(),
        }
    }

    /// The groove a track plays with when no step changes it: its own, or the project's.
    pub fn track_groove(&self, track: usize) -> GrooveId {
        self.tracks
            .get(track)
            .and_then(|track| track.groove)
            .unwrap_or(self.groove)
    }

    /// Add a track after the last that plays sounds, with a new chain and phrase in the first
    /// section and none in the rest.
    pub fn add_track(&mut self) {
        let mut track = Track::default();
        track.add_chain(0);
        let at = self.sound_tracks();
        self.tracks.insert(at, track);
        for (index, section) in self.sections.iter_mut().enumerate() {
            section
                .chains
                .insert(at.min(section.chains.len()), (index == 0).then_some(0));
        }
    }

    /// Remove the last track that plays sounds.
    pub fn remove_last_sound_track(&mut self) {
        let Some(at) = self.sound_tracks().checked_sub(1) else {
            return;
        };
        self.tracks.remove(at);
        for section in &mut self.sections {
            if at < section.chains.len() {
                section.chains.remove(at);
            }
        }
    }

    /// How many steps a section plays before the next one.
    pub fn section_length(&self, section: &Section) -> usize {
        section
            .length
            .unwrap_or_else(|| self.section_steps(section))
    }

    /// How many steps a section plays before moving on, with its repeats.
    pub fn section_plays(&self, section: &Section) -> usize {
        self.section_length(section) * usize::from(section.repeat.get())
    }

    /// How many steps the whole song plays, once through.
    pub fn song_steps(&self) -> usize {
        self.sections.iter().map(|s| self.section_plays(s)).sum()
    }

    /// Add a section after this position, with no chains.
    pub fn insert_section(&mut self, at: usize) {
        let section = Section {
            chains: vec![None; self.tracks.len()],
            ..Section::default()
        };
        self.sections.insert(at.min(self.sections.len()), section);
    }

    /// A section's length: its longest chain, in the song's steps, so a chain at half speed lasts
    /// twice its rows. A triplet chain that doesn't end on one of the song's steps is rounded up.
    pub fn section_steps(&self, section: &Section) -> usize {
        self.tracks
            .iter()
            .zip(&section.chains)
            .filter_map(|(track, chain)| {
                chain.map(|chain| track.speed.song_steps(track.chain_steps(chain)))
            })
            .max()
            .unwrap_or(0)
    }
}

/// The lowest ID not in use.
pub fn lowest_unused<T>(items: &BTreeMap<u32, T>) -> u32 {
    let mut candidate = 0;
    for id in items.keys() {
        if *id != candidate {
            break;
        }
        candidate += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insert::{Distort, DistortType};

    #[test]
    fn every_machines_grid_places_each_of_its_settings_once_with_its_type_in_the_title() {
        use crate::grid::GRID_COLUMNS;
        use crate::settings::Kind;
        for machine in Machine::ALL {
            let grid = machine.grid();
            let count = machine.settings().count();
            assert!(
                grid.places_each_once(count, machine.type_param()),
                "{machine:?}: {grid:?}"
            );
            // A type is a choice from a list, as an effect's is.
            if let Some(at) = machine.type_param() {
                let id = machine.settings().nth(at).unwrap();
                assert!(
                    matches!(id.entry().kind, Kind::Stepped { .. }),
                    "{machine:?}"
                );
            }
            // No column is left empty before one that's used.
            let used: Vec<usize> = (0..GRID_COLUMNS)
                .filter(|column| (0..GRID_ROWS).any(|row| grid.at(row, *column).is_some()))
                .collect();
            assert_eq!(used, (0..used.len()).collect::<Vec<_>>(), "{machine:?}");
        }
    }

    #[test]
    fn the_sampler_and_acid_place_their_settings_by_what_they_mean() {
        let names = |machine: Machine, row: usize| {
            let ids: Vec<SettingId> = machine.settings().collect();
            machine
                .grid()
                .row(row)
                .map(|at| at.map_or("", |at| ids[at].name()))
        };
        assert_eq!(Machine::Sampler.type_param(), Some(0), "the sample");
        assert_eq!(names(Machine::Sampler, 0), ["Slice", "Start", "", ""]);
        assert_eq!(names(Machine::Sampler, 1), ["Play", "Length", "", ""]);
        let acid = Machine::Plaits(SynthVoice::Acid);
        assert_eq!(acid.type_param(), Some(0), "the wave");
        assert_eq!(names(acid, 0), ["Cutoff", "Env mod", "Accent", "Drive"]);
        assert_eq!(names(acid, 1), ["Resonance", "Decay", "Slide", "Drift"]);
        assert_eq!(names(acid, 2), ["", "", "", ""]);
        let kick = Machine::Plaits(SynthVoice::Kick);
        assert_eq!(kick.type_param(), None);
        assert_eq!(names(kick, 0), ["Punch", "Accent", "", ""]);
        assert_eq!(names(kick, 2), ["Decay", "", "", ""]);
        let six_op = Machine::Plaits(SynthVoice::SixOp);
        assert_eq!(names(six_op, 0), ["Bright", "", "", ""]);
        assert_eq!(names(six_op, 2), ["Accent", "", "", ""]);
        let chord = Machine::Plaits(SynthVoice::Chord);
        assert_eq!(chord.type_param(), Some(0), "the chord");
        assert_eq!(names(chord, 1), ["Wave", "", "", ""]);
    }

    #[test]
    fn a_tracks_reset_is_named_in_bars_or_steps() {
        assert_eq!(reset_name(None), "Off");
        assert_eq!(reset_name(Some(5)), "5-step");
        assert_eq!(reset_name(Some(16)), "1bar");
        assert_eq!(reset_name(Some(48)), "3bar");
        assert_eq!(reset_named("off"), Some(None));
        assert_eq!(reset_named("Off"), Some(None));
        assert_eq!(reset_named("5"), Some(Some(5)));
        assert_eq!(reset_named("5-step"), Some(Some(5)));
        assert_eq!(reset_named("0x10"), Some(Some(16)));
        assert_eq!(reset_named("2bar"), Some(Some(32)));
        assert_eq!(reset_named("3 bars"), Some(Some(48)));
        assert_eq!(reset_named("1024"), Some(Some(MAX_TRACK_RESET)));
        for wrong in ["0", "0bar", "1025", "65bar", "", "x", "-1", "0x"] {
            assert_eq!(reset_named(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn a_tracks_reset_is_from_one_step_to_the_most() {
        let song = Arc::new(Song::new(1));
        let set = |reset| crate::action::Action::SetTrackReset { track: 0, reset };
        let edited = crate::action::edit(&song, &set(Some(16))).unwrap();
        assert_eq!(edited.tracks[0].reset, Some(16));
        assert!(crate::action::edit(&song, &set(Some(0))).is_err());
        assert!(crate::action::edit(&song, &set(Some(MAX_TRACK_RESET + 1))).is_err());
        let off = crate::action::edit(&edited, &set(None)).unwrap();
        assert_eq!(off.tracks[0].reset, None);
    }

    #[test]
    fn a_lane_saves_fits_its_steps_and_follows_them() {
        let mut phrase = Phrase::new(16);
        phrase.set_lane(
            1,
            Some(Lane {
                start: 4,
                length: 3,
            }),
        );
        assert_eq!(
            phrase.lane(1),
            Some(Lane {
                start: 4,
                length: 3
            })
        );
        assert_eq!(phrase.lane(0), None, "others follow the phrase");
        let text = ron::to_string(&phrase).unwrap();
        assert_eq!(ron::from_str::<Phrase>(&text).unwrap(), phrase);

        // A lane past the end stores the steps it needs.
        phrase.set_lane(
            2,
            Some(Lane {
                start: 14,
                length: 6,
            }),
        );
        assert_eq!(phrase.steps().len(), 20);
        // Sliding the loop moves it with the loop, whether steps are added in front or not.
        phrase.slide(-2);
        assert_eq!(
            phrase.lane(1).unwrap().start,
            4,
            "on the same step of the loop"
        );
        assert_eq!(phrase.start(), 0);
        phrase.slide(3);
        assert_eq!(phrase.lane(1).unwrap().start, 7);
        // Doubling the loop moves one after it down with its steps.
        let mut doubled = Phrase::new(4);
        doubled.grow_to(8);
        doubled.set_lane(
            0,
            Some(Lane {
                start: 5,
                length: 2,
            }),
        );
        doubled.double();
        assert_eq!(doubled.lane(0).unwrap().start, 9);
    }

    #[test]
    fn a_saved_phrase_without_lanes_has_none_and_a_bad_one_is_fitted() {
        let mut old = ron::to_string(&Phrase::new(2)).unwrap();
        old = old.replace(",lanes:(None,None,None)", "");
        assert!(!old.contains("lanes"), "{old}");
        let phrase: Phrase = ron::from_str(&old).unwrap();
        assert_eq!(phrase.lane(0), None);
        let file = PhraseFile {
            steps: vec![Step::default(); 2],
            start: 0,
            length: Some(2),
            order: Order::Forward,
            generators: Vec::new(),
            lanes: [
                Some(Lane {
                    start: 1,
                    length: 9,
                }),
                Some(Lane {
                    start: 5,
                    length: 1,
                }),
                None,
            ],
        };
        let phrase = Phrase::from(file);
        assert_eq!(
            phrase.lane(0),
            Some(Lane {
                start: 1,
                length: 1
            })
        );
        assert_eq!(phrase.lane(1), None, "it starts past the steps");
    }

    #[test]
    fn a_lane_reads_by_count_and_a_glide_looks_within_it() {
        let lane = Lane {
            start: 2,
            length: 3,
        };
        let read: Vec<usize> = (0..7).map(|count| lane.step_at(count)).collect();
        assert_eq!(read, [2, 3, 4, 2, 3, 4, 2]);
        assert_eq!(lane.step_at(-1), 4);
        let steps = [
            "--- -- cut:10 --- ---",
            "--- -- --- --- ---",
            "--- -- cut:20 --- ---",
            "--- -- --- --- ---",
            "--- -- cut:30 --- ---",
        ];
        let mut phrase = Phrase::from_steps(steps.iter().map(|t| t.parse().unwrap()).collect());
        phrase.set_lane(0, Some(lane));
        let next = |step| {
            phrase
                .next_in_lane(0, step, FxCommand::CUT)
                .map(|(distance, fx)| (distance, fx.coarse()))
        };
        assert_eq!(
            next(4),
            Some((1, 0x20)),
            "wraps to the lane's start, not row 0"
        );
        assert_eq!(next(2), Some((2, 0x30)));
        assert_eq!(next(0), None, "not in the lane");
    }

    #[test]
    fn next_value_looks_forward_in_the_slot_and_wraps() {
        let steps = [
            "--- -- cut:10 ---",
            "--- -- res:20 ---",
            "--- -- cut:30 cut:90",
            "--- -- --- ---",
        ];
        let steps = steps.iter().map(|text| text.parse().unwrap()).collect();
        let phrase = Phrase::from_steps(steps);
        let found = |step, slot, command| {
            phrase
                .glide_to(step, Deal::default(), slot, command)
                .map(|(distance, fx)| (distance, fx.coarse()))
        };
        assert_eq!(found(0, 0, FxCommand::CUT), Some((2, 0x30)));
        assert_eq!(found(2, 0, FxCommand::CUT), Some((2, 0x10)));
        assert_eq!(found(1, 0, FxCommand::RES), None, "not itself");
        assert_eq!(found(0, 1, FxCommand::CUT), Some((2, 0x90)));
        assert_eq!(found(0, 0, FxCommand::VOL), None);
    }
    use crate::constants::DEFAULT_TRACKS;

    #[test]
    fn a_track_saved_without_a_level_loads_at_0_db() {
        let track: Track = ron::from_str("(mute: false, solo: true)").unwrap();
        assert_eq!(track.level, UNITY_LEVEL);
        assert!(track.solo);
    }

    #[test]
    fn a_tracks_trim_saves_and_loads_and_0_db_is_left_out() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert_eq!(track.trim, UNITY_TRIM);
        assert!(!ron::to_string(&track).unwrap().contains("trim"));
        for trim in [0x00, 0x40, 0xFF] {
            let track = Track {
                trim,
                ..Track::default()
            };
            let text = ron::to_string(&track).unwrap();
            assert!(text.contains(&format!("trim:{trim}")), "{text}");
            let loaded: Track = ron::from_str(&text).unwrap();
            assert_eq!(loaded.trim, trim);
        }
    }

    #[test]
    fn a_trim_reads_in_db_to_its_quarter() {
        assert_eq!(trim_text(0x80), "+0.0 dB");
        assert_eq!(trim_text(0x81), "+0.25 dB");
        assert_eq!(trim_text(0x82), "+0.5 dB");
        assert_eq!(trim_text(0x7D), "-0.75 dB");
        assert_eq!(trim_text(0xC0), "+16.0 dB");
        assert_eq!(trim_text(0xFF), "+31.75 dB");
        assert_eq!(trim_text(0x01), "-31.75 dB");
        assert_eq!(trim_text(0x00), "-inf dB");
    }

    #[test]
    fn a_new_song_has_a_chain_and_phrase_per_track() {
        let song = Song::new(DEFAULT_TRACKS);
        assert_eq!(song.sound_tracks(), DEFAULT_TRACKS);
        assert_eq!(song.sections.len(), 1);
        assert_eq!(song.sections[0].chains.len(), song.tracks.len());
        for track in &song.tracks[..DEFAULT_TRACKS] {
            assert_eq!((track.chains.len(), track.phrases.len()), (1, 1));
        }
        assert_eq!(song.section_steps(&song.sections[0]), DEFAULT_PHRASE_STEPS);
    }

    #[test]
    fn the_fixed_tracks_stay_last_and_dont_set_a_sections_length() {
        let mut song = Song::new(2);
        let kinds = |song: &Song| song.tracks.iter().map(|t| t.kind).collect::<Vec<_>>();
        use TrackKind::{Group, Master, ReturnA, ReturnB, Sound};
        let fixed = [
            Group(1),
            Group(2),
            Group(3),
            Group(4),
            ReturnA,
            ReturnB,
            Master,
        ];
        assert_eq!(kinds(&song)[..2], [Sound, Sound]);
        assert_eq!(kinds(&song)[2..], fixed);
        let names: Vec<&str> = song.tracks[2..].iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["G1", "G2", "G3", "G4", "A", "B", "M"]);
        assert!(song.sections[0].chains[2..].iter().all(Option::is_none));

        song.add_track();
        assert_eq!(kinds(&song)[..3], [Sound, Sound, Sound]);
        assert_eq!(kinds(&song)[3..], fixed);
        assert_eq!(song.sections[0].chains[2], Some(0), "the new one's chain");
        assert_eq!(song.sections[0].chains[3], None, "G1's stays G1's");

        song.remove_last_sound_track();
        assert_eq!(kinds(&song)[2..], fixed);
        assert_eq!(song.sections[0].chains.len(), 2 + FIXED_TRACKS);
    }

    #[test]
    fn a_song_from_before_the_fixed_tracks_gets_them_once() {
        let mut song = Song::new(2);
        song.tracks.truncate(2);
        song.sections[0].chains.truncate(2);
        song.ensure_fixed_tracks();
        assert_eq!(song.tracks.len(), 2 + FIXED_TRACKS);
        assert_eq!(song.sound_tracks(), 2);
        assert_eq!(song.sections[0].chains.len(), 2 + FIXED_TRACKS);
        song.ensure_fixed_tracks();
        assert_eq!(song.tracks.len(), 2 + FIXED_TRACKS);
    }

    #[test]
    fn a_song_saved_with_fewer_groups_gets_the_rest_before_the_returns() {
        let mut song = Song::new(2);
        song.sections[0].chains[2 + MAX_GROUPS] = Some(0);
        // As a song saved when there were only two groups.
        song.tracks.drain(4..2 + MAX_GROUPS);
        song.sections[0].chains.drain(4..2 + MAX_GROUPS);
        song.ensure_fixed_tracks();
        assert_eq!(song.tracks.len(), 2 + FIXED_TRACKS);
        assert_eq!(song.sound_tracks(), 2);
        let kinds: Vec<_> = song.tracks[2..].iter().map(|t| t.kind).collect();
        assert_eq!(kinds, TrackKind::FIXED);
        assert_eq!(song.fixed_track(TrackKind::ReturnA), Some(2 + MAX_GROUPS));
        assert_eq!(
            song.sections[0].chains[2 + MAX_GROUPS],
            Some(0),
            "A's chain stays A's"
        );
    }

    #[test]
    fn a_track_in_a_group_plays_through_its_track_and_saves_it() {
        let mut song = Song::new(3);
        song.tracks[0].group = Some(1);
        song.tracks[1].group = Some(2);
        assert_eq!(song.group_track_of(0), Some(3));
        assert_eq!(song.group_track_of(1), Some(4));
        assert_eq!(song.group_track_of(2), None);
        let text = ron::to_string(&song.tracks[0]).unwrap();
        assert!(text.contains("group:Some(1)"), "{text}");
        let loaded: Track = ron::from_str(&text).unwrap();
        assert_eq!(loaded.group, Some(1));
        let alone = ron::to_string(&song.tracks[2]).unwrap();
        assert!(!alone.contains("group"), "none isn't written: {alone}");
    }

    #[test]
    fn muting_a_group_mutes_its_tracks_and_soloing_one_keeps_it() {
        let mut song = Song::new(3);
        song.tracks[0].group = Some(1);
        song.tracks[1].group = Some(1);
        let group = song.group_track_of(0).unwrap();
        let heard = |song: &Song| (0..3).map(|t| song.heard(t)).collect::<Vec<_>>();
        assert_eq!(heard(&song), [true, true, true]);
        assert!(song.heard(group));

        song.tracks[group].mute = true;
        assert_eq!(
            heard(&song),
            [false, false, true],
            "a muted group mutes its tracks"
        );
        assert!(!song.heard(group));
        song.tracks[group].mute = false;

        song.tracks[0].solo = true;
        assert_eq!(heard(&song), [true, false, false]);
        assert!(song.heard(group), "a soloed track keeps its group heard");
        song.tracks[0].solo = false;

        song.tracks[group].solo = true;
        assert_eq!(
            heard(&song),
            [true, true, false],
            "a soloed group plays its tracks"
        );
        song.tracks[1].mute = true;
        assert_eq!(
            heard(&song),
            [true, false, false],
            "but not one muted itself"
        );
    }

    fn gated(gates: &[&str]) -> Phrase {
        Phrase::from_steps(
            gates
                .iter()
                .map(|gate| format!("C-3 -- {gate} --- --- ---").parse().unwrap())
                .collect(),
        )
    }

    #[test]
    fn a_row_lasts_its_pulses_and_a_skipped_one_none() {
        let phrase = gated(&["SGL1", "HLD3", "MLT0", "RST2"]);
        assert_eq!(phrase.time(), 6);
        let deal = Deal::default();
        let rows: Vec<(usize, usize)> = (0..6)
            .filter_map(|t| phrase.walk_at(0, t, deal))
            .map(|visit| (visit.row, visit.pulse))
            .collect();
        assert_eq!(rows, [(0, 0), (1, 0), (1, 1), (1, 2), (3, 0), (3, 1)]);
        assert_eq!(phrase.walk_at(0, 6, deal), None);
        assert_eq!(phrase.time_to(3), 4, "the skipped row takes no time");
        let skipped = gated(&["SGL0", "HLD0"]);
        assert_eq!(skipped.time(), 0);
        assert_eq!(
            skipped.walk_at(0, 0, deal),
            None,
            "nothing to play, and no hang"
        );
    }

    /// The rows a phrase's walk plays, a step at a time, over this many of its passes.
    fn walked(phrase: &Phrase, passes: usize) -> Vec<usize> {
        (0..passes)
            .flat_map(|pass| {
                (0..phrase.time()).map(move |t| phrase.walk_at(pass, t, Deal::default()).unwrap())
            })
            .map(|visit| visit.row + 1)
            .collect()
    }

    #[test]
    fn an_order_is_a_walk_and_a_pass_keeps_the_phrases_length() {
        // The author's examples: six steps of a phrase of four.
        let mut phrase = Phrase::new(4);
        let six = |phrase: &Phrase| walked(phrase, 2)[..6].to_vec();
        assert_eq!(six(&phrase), [1, 2, 3, 4, 1, 2]);
        phrase.set_order(Order::UpDown);
        assert_eq!(phrase.time(), 4);
        assert_eq!(six(&phrase), [1, 2, 3, 4, 3, 2]);
        phrase.set_order(Order::UpDownInc);
        assert_eq!(six(&phrase), [1, 2, 3, 4, 4, 3]);
        phrase.set_order(Order::Pedal);
        assert_eq!(six(&phrase), [1, 2, 1, 3, 1, 4]);
        // And on, each pass the phrase's length, the walk carrying on across them.
        phrase.set_order(Order::UpDown);
        assert_eq!(walked(&phrase, 3), [1, 2, 3, 4, 3, 2, 1, 2, 3, 4, 3, 2]);
        phrase.set_order(Order::DownUp);
        assert_eq!(walked(&phrase, 3), [4, 3, 2, 1, 2, 3, 4, 3, 2, 1, 2, 3]);
        phrase.set_order(Order::DownUpInc);
        assert_eq!(walked(&phrase, 3), [4, 3, 2, 1, 1, 2, 3, 4, 4, 3, 2, 1]);
        phrase.set_order(Order::Pedal);
        assert_eq!(walked(&phrase, 3), [1, 2, 1, 3, 1, 4, 1, 2, 1, 3, 1, 4]);
        // An order that visits each row once keeps to its pass.
        phrase.set_order(Order::Converge);
        assert_eq!(walked(&phrase, 2), [1, 4, 2, 3, 1, 4, 2, 3]);
    }

    #[test]
    fn a_rows_pulses_stay_together_and_run_on_into_the_next_pass() {
        let mut phrase = gated(&["SGL1", "MLT2", "SGL1"]);
        let deal = Deal::default();
        let rows = |phrase: &Phrase, pass| -> Vec<(usize, usize)> {
            (0..phrase.time())
                .filter_map(|t| phrase.walk_at(pass, t, deal))
                .map(|visit| (visit.row, visit.pulse))
                .collect()
        };
        phrase.set_order(Order::Reverse);
        assert_eq!(phrase.time(), 4);
        assert_eq!(rows(&phrase, 0), [(2, 0), (1, 0), (1, 1), (0, 0)]);
        phrase.set_order(Order::UpDown);
        assert_eq!(phrase.time(), 4, "the pass keeps the rows' length");
        assert_eq!(
            phrase.round_time(),
            6,
            "while a round plays the middle row twice"
        );
        assert_eq!(rows(&phrase, 0), [(0, 0), (1, 0), (1, 1), (2, 0)]);
        // The middle row on the way back, then the first, and the middle row's first pulse ends
        // the pass, so its second starts the next.
        assert_eq!(rows(&phrase, 1), [(1, 0), (1, 1), (0, 0), (1, 0)]);
        assert_eq!(rows(&phrase, 2), [(1, 1), (2, 0), (1, 0), (1, 1)]);
        assert_eq!(
            rows(&phrase, 3),
            rows(&phrase, 0),
            "three passes are two rounds"
        );
    }

    #[test]
    fn a_row_is_found_where_its_pass_plays_it_nearest() {
        let mut phrase = Phrase::new(4);
        phrase.set_order(Order::UpDown);
        let deal = Deal::default();
        // The second pass plays 3 2 1 2: row 2 is at 1 and 3, and row 4 isn't in it.
        assert_eq!(phrase.place_in_pass(1, 0, 1, 0, deal), Some(1));
        assert_eq!(phrase.place_in_pass(1, 0, 1, 3, deal), Some(3));
        assert_eq!(phrase.place_in_pass(3, 0, 1, 0, deal), None);
        // A pulse it no longer has is its last.
        let mut pulses = gated(&["SGL1", "SGL2"]);
        pulses.set_order(Order::Reverse);
        assert_eq!(pulses.place_in_pass(1, 5, 0, 0, deal), Some(1));
    }

    #[test]
    fn a_glide_looks_on_in_the_order_the_rows_play() {
        let steps = [
            "--- -- cut:10 ---",
            "--- -- --- ---",
            "--- -- cut:30 ---",
            "--- -- --- ---",
        ];
        let mut phrase = Phrase::from_steps(steps.iter().map(|t| t.parse().unwrap()).collect());
        phrase.set_order(Order::Reverse);
        let found = |phrase: &Phrase, visit| {
            phrase
                .glide_to(visit, Deal::default(), 0, FxCommand::CUT)
                .map(|(time, fx)| (time, fx.coarse()))
        };
        // Reverse plays rows 3 2 1 0, so from row 2, its visit 1, row 0 is two rows on.
        assert_eq!(found(&phrase, 1), Some((2, 0x10)));
        assert_eq!(found(&phrase, 3), Some((2, 0x30)), "round to row 2 again");
        phrase.set_order(Order::UpDown);
        // Up-down goes round 0 1 2 3 2 1, so from row 2 on the way up, row 2 again is two on.
        assert_eq!(found(&phrase, 2), Some((2, 0x30)));
    }

    #[test]
    fn a_phrase_saves_its_order_only_when_it_has_one() {
        let mut phrase = Phrase::new(2);
        assert!(!ron::to_string(&phrase).unwrap().contains("order"));
        phrase.set_order(Order::DownUpInc);
        let saved = ron::to_string(&phrase).unwrap();
        assert!(saved.contains("order:DownUpInc"), "{saved}");
        let loaded: Phrase = ron::from_str(&saved).unwrap();
        assert_eq!(loaded.order(), Order::DownUpInc);
    }

    #[test]
    fn a_dealt_order_is_dealt_by_the_time_round_the_chain_and_the_slot() {
        let mut song = Song::new(1);
        let mut phrase = Phrase::new(8);
        phrase.set_order(Order::Shuffle);
        song.tracks[0].phrases.insert(0, Arc::new(phrase));
        let track = &song.tracks[0];
        let dealer = Dealer::new(5, 0);
        let lap = |lap: usize| -> Vec<usize> {
            (0..8)
                .map(|step| {
                    track
                        .chain_position(0, lap * 8 + step, dealer)
                        .unwrap()
                        .step
                })
                .collect()
        };
        assert_ne!(lap(0), lap(1));
        assert_eq!(lap(1), lap(1));
        let row = lap(1)[3];
        let place = RowPlace {
            slot: 0,
            row,
            pulse: 0,
            near: 0,
        };
        assert_eq!(
            track.chain_place(0, place, 1, dealer),
            Some(3),
            "where the deal plays it"
        );
    }

    #[test]
    fn a_chain_lasts_its_rows_pulses_and_finds_them() {
        let mut song = Song::new(1);
        song.tracks[0]
            .phrases
            .insert(0, Arc::new(gated(&["HLD3", "SGL1"])));
        song.tracks[0].phrases.insert(7, Arc::new(gated(&["MLT2"])));
        let entry = |phrase| ChainEntry {
            phrase,
            transpose: 0,
        };
        song.tracks[0].chains.insert(
            0,
            Arc::new(Chain {
                entries: vec![entry(0), entry(7)],
                label: String::new(),
            }),
        );
        let track = &song.tracks[0];
        assert_eq!(track.chain_steps(0), 6);
        let at = |step| {
            let found = track.chain_position(0, step, Dealer::default()).unwrap();
            (found.slot, found.step, found.pulse)
        };
        assert_eq!(at(2), (0, 0, 2));
        assert_eq!(at(3), (0, 1, 0));
        assert_eq!(at(5), (1, 0, 1));
        assert_eq!(at(6), (0, 0, 0), "and round again");
        let place = |slot, row| RowPlace {
            slot,
            row,
            pulse: 0,
            near: 0,
        };
        assert_eq!(
            track.chain_place(0, place(1, 0), 0, Dealer::default()),
            Some(4)
        );
        assert_eq!(
            track.chain_place(0, place(0, 1), 0, Dealer::default()),
            Some(3)
        );
    }

    #[test]
    fn a_slot_and_row_find_their_place_in_a_chain_if_they_are_still_there() {
        let mut song = Song::new(1);
        song.tracks[0].phrases.insert(7, Arc::new(Phrase::new(4)));
        let entry = |phrase| ChainEntry {
            phrase,
            transpose: 0,
        };
        song.tracks[0].chains.insert(
            0,
            Arc::new(Chain {
                entries: vec![entry(0), entry(7)],
                label: String::new(),
            }),
        );
        let track = &song.tracks[0];
        let found = |chain, slot, row| {
            let place = RowPlace {
                slot,
                row,
                pulse: 0,
                near: 0,
            };
            track.chain_place(chain, place, 0, Dealer::default())
        };
        assert_eq!(found(0, 1, 2), Some(16 + 2));
        assert_eq!(found(0, 1, 4), None, "past its loop");
        assert_eq!(found(0, 2, 0), None, "no third slot");
        assert_eq!(found(9, 0, 0), None, "no such chain");
    }

    #[test]
    fn chain_positions_cross_phrases_and_loop() {
        let mut song = Song::new(1);
        song.tracks[0].phrases.insert(7, Arc::new(Phrase::new(4)));
        song.tracks[0].chains.insert(
            0,
            Arc::new(Chain {
                entries: vec![
                    ChainEntry {
                        phrase: 0,
                        transpose: 0,
                    },
                    ChainEntry {
                        phrase: 7,
                        transpose: 3,
                    },
                ],
                label: String::new(),
            }),
        );
        assert_eq!(song.tracks[0].chain_steps(0), 20);
        let position = song.tracks[0]
            .chain_position(0, 17, Dealer::default())
            .unwrap();
        assert_eq!((position.slot, position.phrase, position.step), (1, 7, 1));
        assert_eq!(position.transpose, 3);
        assert_eq!(
            song.tracks[0]
                .chain_position(0, 20, Dealer::default())
                .unwrap()
                .step,
            0
        );
        assert_eq!(
            song.tracks[0].chain_position(99, 0, Dealer::default()),
            None
        );
    }

    /// A phrase whose steps can be told apart by velocity, which is the step's stored position.
    fn numbered(length: usize) -> Phrase {
        let steps = (0..length)
            .map(|index| Step {
                velocity: Some(index as u8),
                ..Step::default()
            })
            .collect();
        Phrase::from_steps(steps)
    }

    fn played(phrase: &Phrase) -> Vec<u8> {
        phrase
            .loop_steps()
            .iter()
            .map(|step| step.velocity.unwrap())
            .collect()
    }

    #[test]
    fn steps_outside_the_loop_stay_stored_and_saved() {
        let mut phrase = numbered(4);
        phrase.set_len(2);
        phrase.slide(1);
        assert_eq!(played(&phrase), [1, 2]);
        assert_eq!(phrase.steps().len(), 4);
        let saved = ron::to_string(&phrase).unwrap();
        let loaded: Phrase = ron::from_str(&saved).unwrap();
        assert_eq!(loaded, phrase);
        phrase.set_len(0);
        assert_eq!(phrase.len(), 1);
    }

    #[test]
    fn a_file_from_before_loops_plays_all_its_steps() {
        let file = PhraseFile {
            steps: vec![Step::default(); 3],
            start: 0,
            lanes: [None; FX_SLOTS],
            length: None,
            order: Order::Forward,
            generators: Vec::new(),
        };
        let phrase = Phrase::from(file);
        assert_eq!((phrase.start(), phrase.len()), (0, 3));
    }

    #[test]
    fn a_loop_past_the_end_of_a_file_is_pulled_back() {
        let file = PhraseFile {
            steps: vec![Step::default(); 3],
            start: 9,
            lanes: [None; FX_SLOTS],
            length: Some(9),
            order: Order::Forward,
            generators: Vec::new(),
        };
        let phrase = Phrase::from(file);
        assert_eq!((phrase.start(), phrase.len()), (2, 1));
        assert_eq!(phrase.loop_steps().len(), 1);
    }

    #[test]
    fn sliding_keeps_the_length_and_adds_steps_at_either_end() {
        let mut phrase = numbered(4);
        phrase.set_len(2);
        phrase.slide(3);
        assert_eq!(phrase.start(), 3);
        assert_eq!(phrase.steps().len(), 5, "one more step, so the loop fits");
        phrase.slide(-5);
        assert_eq!(phrase.start(), 0);
        assert_eq!(phrase.len(), 2);
        assert_eq!(phrase.steps().len(), 7, "two more in front");
        assert_eq!(phrase.steps()[2].velocity, Some(0), "what was first moved");
    }

    #[test]
    fn a_huge_slide_stops_at_the_most_a_phrase_keeps() {
        let mut phrase = numbered(4);
        phrase.slide(isize::MAX);
        assert_eq!(phrase.steps().len(), MAX_PHRASE_STEPS);
        assert_eq!(phrase.loop_steps().len(), 4);
        let mut phrase = numbered(4);
        phrase.slide(isize::MIN);
        assert_eq!(phrase.steps().len(), MAX_PHRASE_STEPS);
        assert_eq!(phrase.start(), 0);
    }

    #[test]
    fn doubling_repeats_the_loop_and_keeps_what_is_after_it() {
        let mut phrase = numbered(4);
        phrase.set_len(2);
        phrase.slide(1);
        phrase.double();
        assert_eq!(played(&phrase), [1, 2, 1, 2]);
        assert_eq!(
            phrase.steps()[5].velocity,
            Some(3),
            "moved, not overwritten"
        );
    }

    #[test]
    fn each_filter_type_plays_a_mode_it_offers_and_keeps_its_own() {
        for kind in FilterType::ALL {
            for mode in FilterMode::ALL {
                let played = kind.plays(mode);
                match kind.modes() {
                    [] => assert_eq!(played, mode, "{kind:?} has none to fall back on"),
                    modes if modes.contains(&mode) => assert_eq!(played, mode),
                    modes => assert_eq!(played, modes[0], "{kind:?} {mode:?}"),
                }
            }
        }
        for mode in FilterMode::ALL {
            assert_eq!(FilterMode::at(mode.index()), Some(mode));
        }
        let mut names = FILTER_MODE_NAMES.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), FilterMode::ALL.len(), "each name says which");
    }

    #[test]
    fn a_tracks_filter_mode_saves_and_loads() {
        let track = Track {
            filter: FilterType::Ladder,
            filter_mode: FilterMode::HighPass24,
            ..Track::default()
        };
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("filter_mode:HighPass24"), "{text}");
        assert_eq!(ron::from_str::<Track>(&text).unwrap(), track);
    }

    #[test]
    fn a_track_saved_before_its_page_loads_with_the_old_sound() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert_eq!(track, Track::default());
        assert_eq!((track.cutoff, track.resonance, track.pan), (0xFF, 0, 0x80));
        assert_eq!(track.mods, default_mods());
    }

    #[test]
    fn a_tracks_fine_parts_save_by_key_and_go_at_the_top() {
        let mut track = Track {
            cutoff: 0x40,
            ..Track::default()
        };
        track.set_fine(SettingId::CUT, 0x80);
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("fine:{\"cut\":128}"), "{text}");
        let loaded: Track = ron::from_str(&text).unwrap();
        assert_eq!(loaded.fine(SettingId::CUT), 0x80);
        track.cutoff = 0xFF;
        track.set_fine(SettingId::CUT, 0x80);
        assert_eq!(track.fine(SettingId::CUT), 0);
        assert!(!ron::to_string(&Track::default()).unwrap().contains("fine"));
    }

    #[test]
    fn a_tracks_inserts_save_and_load_and_start_empty() {
        let mut track = Track::default();
        assert_eq!(track.inserts, [Insert::Empty; INSERTS]);
        track.inserts[1] = Insert::Distort(Distort {
            kind: DistortType::Fold,
            amount: 0x80,
            mix: 0x40,
            gain: 0xD0,
            ..Distort::DEFAULT
        });
        let text = ron::to_string(&track).unwrap();
        let loaded: Track = ron::from_str(&text).unwrap();
        assert_eq!(loaded, track);
    }

    #[test]
    fn level_values_are_even_steps_in_db() {
        let db = |level: f32| 20.0 * level_gain(level).log10();
        let close = |a: f32, b: f32| assert!((a - b).abs() < 1e-3, "{a} and {b}");
        assert_eq!(level_gain(0xE0 as f32), 1.0);
        close(db(0xF0 as f32), 4.0);
        close(db(0xF8 as f32), 6.0);
        close(db(0xFF as f32), 6.0);
        close(db(0xD0 as f32), -4.0);
        close(db(0xC0 as f32), -8.0);
        close(db(0x80 as f32), -24.0);
        close(db(0x40 as f32), -40.0);
        close(db(0x01 as f32), -55.75);
        assert_eq!(level_gain(0.0), 0.0);
        assert_eq!(level_gain(-3.0), 0.0);
    }

    #[test]
    fn a_section_saved_without_a_reset_loads_with_none() {
        let section: Section = ron::from_str("(chains: [Some(0)])").unwrap();
        assert_eq!(section.reset, None);
        let saved = ron::to_string(&Section {
            reset: Some(64),
            ..section
        })
        .unwrap();
        assert_eq!(ron::from_str::<Section>(&saved).unwrap().reset, Some(64));
    }

    #[test]
    fn rotating_stays_inside_the_loop() {
        use crate::step::Note;

        let note = |midi| Step {
            note: Note::On(midi),
            ..Step::default()
        };
        let mut phrase = Phrase::from_steps((60..66).map(note).collect());
        phrase.set_len(3);
        phrase.slide(1);
        let notes =
            |phrase: &Phrase| -> Vec<Note> { phrase.steps().iter().map(|s| s.note).collect() };

        phrase.rotate(1);
        let on = |midi: [u8; 6]| midi.map(Note::On).to_vec();
        assert_eq!(notes(&phrase), on([60, 63, 61, 62, 64, 65]));
        phrase.rotate(-1);
        assert_eq!(notes(&phrase), on([60, 61, 62, 63, 64, 65]));
        phrase.rotate(4);
        assert_eq!(notes(&phrase), on([60, 63, 61, 62, 64, 65]));
        assert_eq!((phrase.start(), phrase.len()), (1, 3));
    }

    #[test]
    fn step_at_reads_within_the_loop() {
        let mut song = Song::new(1);
        let mut phrase = numbered(8);
        phrase.set_len(4);
        phrase.slide(2);
        song.tracks[0].phrases.insert(0, Arc::new(phrase));
        assert_eq!(song.tracks[0].chain_steps(0), 4);
        let position = song.tracks[0]
            .chain_position(0, 5, Dealer::default())
            .unwrap();
        assert_eq!(position.step, 1, "the chain loops over the loop");
        assert_eq!(song.tracks[0].step_at(position).unwrap().velocity, Some(3));
    }

    #[test]
    fn the_lowest_unused_id_fills_gaps() {
        let items = BTreeMap::from([(0, ()), (1, ()), (3, ())]);
        assert_eq!(lowest_unused(&items), 2);
        assert_eq!(lowest_unused(&BTreeMap::<u32, ()>::new()), 0);
    }
}
