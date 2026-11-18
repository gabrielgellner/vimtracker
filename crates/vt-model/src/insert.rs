//! The effects a track can hold, in order, after its filters.

use serde::{Deserialize, Serialize};

use crate::constants::{MAX_GROUPS, MAX_LEVEL, MAX_SOUND_TRACKS, UNITY_LEVEL};
use crate::grid::{GRID_ROWS, Grid};
use crate::settings::{Entry, Kind, Units};
pub use crate::settings::{OFF_ON, Place};

/// An effect's `read` and `written`, from one list of its fields, so a field is named once here
/// rather than twice. It goes in the effect's `impl`, and lists its fields in the order its
/// `PARAMS` keeps their settings: the first field is the first setting. Each entry is one of:
/// - `field`, a `u8` read and written as it is;
/// - `field: Choices`, a type or mode, read as its place in `Choices::ALL`;
/// - `field via (place, kept)`, kept on a scale of its own: `place` turns what's kept into what a
///   step gives, and `kept` turns that back. Distort's `bits` is the one.
///
/// A place past its settings reads as `0` and writes as nothing. [`Insert::write`] only writes
/// one it has, within its range.
macro_rules! effect_settings {
    (
        $($field:ident $(: $choices:ident)? $(via ($place:path, $kept:path))?),+
        $(,)?
    ) => {
        /// A setting's value, by its place among those it keeps, on the scale a step gives.
        pub(super) fn read(self, at: usize) -> u8 {
            effect_settings!(@places $($field),+);
            match at {
                $(places::$field => {
                    effect_settings!(@read self.$field $(, $choices)? $(, via $place)?)
                })+
                _ => 0,
            }
        }

        /// The same with a setting changed, on the scale `read` gives.
        pub(super) fn written(self, at: usize, value: u8) -> Self {
            effect_settings!(@places $($field),+);
            let mut settings = self;
            match at {
                $(places::$field => {
                    settings.$field = effect_settings!(@write value $(, $choices)? $(, via $kept)?);
                })+
                _ => {}
            }
            settings
        }
    };
    // Each field's place, as a constant named after it, for `read` and `written` to match on.
    (@places $($field:ident),+) => {
        #[allow(non_upper_case_globals)]
        mod places {
            #[allow(non_camel_case_types)]
            enum Field { $($field),+ }
            $(pub(super) const $field: usize = Field::$field as usize;)+
        }
    };
    (@read $stored:expr) => { $stored };
    (@read $stored:expr, via $place:path) => { $place($stored) };
    (@read $stored:expr, $choices:ident) => { $stored as u8 };
    (@write $value:expr) => { $value };
    (@write $value:expr, via $kept:path) => { $kept($value) };
    (@write $value:expr, $choices:ident) => {
        $choices::ALL[usize::from($value).min($choices::ALL.len() - 1)]
    };
}

mod colour;
mod dynamics;
mod filters;
mod kind;
mod modulation;
mod pitch;
mod space;

pub use colour::*;
pub use dynamics::*;
pub use filters::*;
pub use kind::*;
pub use modulation::*;
pub use pitch::*;
pub use space::*;

/// How many a track has.
pub const INSERTS: usize = 4;

/// Writes [`Insert`] and [`EffectKind`] from one list of the effects, `Variant(Settings)`: both
/// enums, both `ALL`s, [`Insert::kind`], and the forwarding of [`Insert::read`] and
/// [`Insert::write`] to each effect's own, which its `effect_settings!` writes. The list is in the
/// order [`SettingId`](crate::settings::SettingId) numbers their settings, which is also
/// [`EFFECTS`]'s, and is never reordered. A new effect is a line here, a row of `EFFECTS` and its
/// `effect_settings!`.
macro_rules! effects {
    ($($effect:ident($settings:ident)),+ $(,)?) => {
        /// One of a track's slots.
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
        pub enum Insert {
            #[default]
            Empty,
            $($effect($settings),)+
        }

        /// An effect a slot can hold, apart from its settings: a dense index for tables.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum EffectKind {
            $($effect,)+
        }

        impl EffectKind {
            /// In the order [`SettingId`](crate::settings::SettingId) numbers their settings,
            /// which is [`Insert::ALL`]'s without `Empty`. Never reordered: a track's saved
            /// `settings` map is in this order.
            pub const ALL: [Self; Self::COUNT] = [$(Self::$effect,)+];
            pub const COUNT: usize = [$(Self::$effect,)+].len();
        }

        impl Insert {
            /// Every effect a slot can hold, as it starts, in the order `+` goes through them.
            pub const ALL: [Self; EffectKind::COUNT + 1] =
                [Self::Empty, $(Self::$effect($settings::DEFAULT),)+];

            /// The effect it holds, if any.
            pub const fn kind(self) -> Option<EffectKind> {
                match self {
                    Self::Empty => None,
                    $(Self::$effect(_) => Some(EffectKind::$effect),)+
                }
            }

            /// Its effect's own `read`, or nothing if it's empty.
            fn read_own(self, at: usize) -> Option<u8> {
                match self {
                    Self::Empty => None,
                    $(Self::$effect(settings) => Some(settings.read(at)),)+
                }
            }

            /// Its effect's own `written`, or itself if it's empty.
            fn written_own(self, at: usize, value: u8) -> Self {
                match self {
                    Self::Empty => self,
                    $(Self::$effect(settings) => Self::$effect(settings.written(at, value)),)+
                }
            }
        }
    };
}

effects! {
    Distort(Distort),
    Delay(Delay),
    Reverb(Reverb),
    Chorus(Chorus),
    Eq(Equaliser),
    Compressor(Compressor),
    Limit(Limiter),
    Phaser(Phaser),
    Flanger(Flanger),
    Tape(Tape),
    Plate(Plate),
    Pitch(Pitch),
    Grains(Grains),
    Warps(Warps),
    Lpg(LowPassGate),
    Reso(Resonator),
    Comb(Comb),
    Vowel(Vowel),
    FreqShift(FreqShift),
    Spring(Spring),
    Space(Space),
    Rotary(Rotary),
    Exciter(Exciter),
    Stutter(Stutter),
    Clouds(Clouds),
    Beads(Beads),
    Echo(Echo),
    Hall(Hall),
}

/// How a delay goes to a new time: gliding there, which bends the repeats' pitch as tape does, or
/// jumping, crossfading from where it read to where it reads now, so they keep their pitch.
pub(crate) const RETIMES: [&str; 2] = ["Glide", "Jump"];

/// The most settings any effect shows at once, in the type or mode it's in: a grid's twelve and
/// its type or mode in the title, and Beads' quality beside its mode there.
pub const MAX_PARAMS: usize = 14;
/// The most settings any effect keeps across all of its types and modes, of which it shows at
/// most [`MAX_PARAMS`]. A compressor keeps each type's own.
pub const MAX_KEPT_PARAMS: usize = 17;

const MIX_PARAM: Entry = Entry {
    meaning: "how much of the effect is heard against the sound going in",
    name: "Mix",
    key: "mix",
    label: Some("MIX"),
    kind: Kind::Continuous {
        top: 0xFF,
        middle: false,
        units: Units::Percent,
    },
    default: 0xFF,
    place: Place::Output,
};
const GAIN_PARAM: Entry = Entry {
    meaning: "turns what comes out of the effect up or down",
    name: "Gain",
    key: "gain",
    label: Some("GAIN"),
    kind: Kind::Continuous {
        top: MAX_LEVEL,
        middle: false,
        units: Units::Decibels,
    },
    default: UNITY_LEVEL,
    place: Place::Output,
};
const AMOUNT_PARAM: Entry = Entry {
    meaning: "how hard it drives the sound",
    name: "Amount",
    key: "amount",
    label: Some("AMNT"),
    kind: Kind::Continuous {
        top: 0xFF,
        middle: false,
        units: Units::Percent,
    },
    default: 0x40,
    place: Place::Own,
};

/// A delay or reverb's mix. It starts half wet in a slot, so the sound stays under it.
const WET_MIX_PARAM: Entry = Entry {
    default: 0x80,
    ..MIX_PARAM
};
/// A setting from `00` to `FF`, as a share of its range.
const fn hex_param(
    name: &'static str,
    key: &'static str,
    label: &'static str,
    default: u8,
) -> Entry {
    Entry {
        meaning: "",
        name,
        key,
        label: Some(label),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Percent,
        },
        default,
        place: Place::Own,
    }
}

/// What a compressor can hear: its own input, a track by its number, a return, or a group. The
/// groups come after the returns, so the returns keep the numbers songs saved them by.
pub const SOURCE_NAMES: [&str; MAX_SOUND_TRACKS + 3 + MAX_GROUPS] = [
    "Self", "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "13", "14",
    "15", "16", "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30",
    "31", "32", "33", "34", "35", "36", "37", "38", "39", "40", "41", "42", "43", "44", "45", "46",
    "47", "48", "49", "50", "51", "52", "53", "54", "55", "56", "57", "58", "59", "60", "61", "62",
    "63", "64", "A", "B", "G1", "G2", "G3", "G4",
];

/// How far a value on the `00` to `FF` scale is from `80`, from -1 to 1. The sides have their own
/// scales, as there are 128 values below the middle and 127 above.
pub fn from_middle(value: f32) -> f32 {
    let from_middle = value - 128.0;
    from_middle / if from_middle < 0.0 { 128.0 } else { 127.0 }
}

/// The track a `Source` hears, by its place among the tracks, if not the slot's own input or
/// oscillator: `0` is that, then the sound tracks by number, then the two returns, which come after
/// the sound tracks and the groups, and then the groups.
fn source_track(source: u8, sounds: usize) -> Option<usize> {
    const RETURNS: usize = MAX_SOUND_TRACKS + 2;
    match usize::from(source) {
        0 => None,
        track if track <= MAX_SOUND_TRACKS => Some(track - 1),
        other if other <= RETURNS => Some(sounds + MAX_GROUPS + other - MAX_SOUND_TRACKS - 1),
        group => Some(sounds + group - RETURNS - 1),
    }
}

impl Insert {
    /// What return `A` starts with: only what comes back, as what's sent still plays on its track.
    pub const RETURN_DELAY: Self = Self::Delay(Delay {
        mix: 0xFF,
        ..Delay::DEFAULT
    });

    /// What return `B` starts with.
    pub const RETURN_REVERB: Self = Self::Reverb(Reverb {
        mix: 0xFF,
        ..Reverb::DEFAULT
    });

    /// The effect's name on screen.
    pub fn name(self) -> &'static str {
        self.kind().map_or("---", EffectKind::name)
    }

    /// What it does, for a list to choose from.
    pub(crate) fn meaning(self) -> &'static str {
        self.kind()
            .map_or("Nothing here", |kind| kind.spec().meaning)
    }

    /// The heading a list shows it under, or none for an empty slot, at the top.
    pub fn category(self) -> &'static str {
        self.kind().map_or("", EffectKind::category)
    }

    /// The places in [`Insert::ALL`] in the order a list shows them: `Empty` first, then each
    /// effect by [`EffectKind::LISTED`]. A list item still sets its place in `ALL`.
    pub fn listed_order() -> impl Iterator<Item = usize> {
        std::iter::once(0).chain(EffectKind::LISTED.iter().map(|kind| *kind as usize + 1))
    }

    /// The mode its type is in, for an effect that shows each type's own settings.
    fn mode(self) -> Option<&'static Mode> {
        let Shown::ByType { type_at, modes } = self.kind()?.spec().shown else {
            return None;
        };
        modes.get(usize::from(self.read(usize::from(type_at))?))
    }

    /// The settings it has, in the mode it's in.
    pub fn params(self) -> Params {
        let Some(kind) = self.kind() else {
            return Params::NONE;
        };
        match self.mode() {
            Some(mode) => Params::of(kind.params(), mode),
            None => Params::all(kind.params()),
        }
    }

    /// Where a setting is among those its effect keeps, in any of its types and modes, by its
    /// place among those it has now.
    pub fn kept_at(self, param: usize) -> Option<usize> {
        self.params().kept_at(param)
    }

    /// A setting's value, by its place among those it has now: a place in its choices, or the
    /// value itself.
    pub fn value(self, param: usize) -> u8 {
        self.kept_at(param)
            .and_then(|at| self.read(at))
            .unwrap_or(0)
    }

    /// The same with a setting changed, by its place among those it has now.
    #[must_use]
    pub fn with_value(self, param: usize, value: u8) -> Self {
        self.kept_at(param)
            .map_or(self, |at| self.write(at, f32::from(value)))
    }

    /// A setting as a step gives it, by its place among those its effect keeps: a place for `Type`
    /// and `Bits`. Nothing if there's no effect here that has it. A distortion keeps every mode's,
    /// and a compressor every type's, so one not shown now still reads.
    pub fn read(self, at: usize) -> Option<u8> {
        self.kind()?.params().get(at)?;
        self.read_own(at)
    }

    /// The same with a setting moved, on the scale `read` gives, landing on a whole value within its
    /// range. It's how a stepped setting ends up stepping, whatever moved it. One there's no effect
    /// here to have stays as it was.
    #[must_use]
    pub fn write(self, at: usize, value: f32) -> Self {
        let Some(param) = self.kind().and_then(|kind| kind.params().get(at)) else {
            return self;
        };
        let whole = value.round().clamp(0.0, f32::from(param.kind.top())) as u8;
        self.written_own(at, whole)
    }

    /// The setting that changes the effect's character, by its place among those it has now: the
    /// `Type` of a Distort, a Chorus or a Comp, and the `Mode` of an LPG or a Comb. The FX page
    /// shows it in the slot's title, beside the effect, rather than in its grid.
    pub fn type_param(self) -> Option<usize> {
        self.params()
            .iter()
            .position(|found| found.place == Place::Type)
    }

    /// A second setting the FX page shows in the slot's title, after its type: Beads' quality,
    /// which, as its mode does, chooses what it is rather than how it's played.
    pub fn title_param(self) -> Option<usize> {
        self.params()
            .iter()
            .position(|found| found.place == Place::Title)
    }

    /// Where its settings sit on the FX page, by their places among those it has now. Its type is
    /// in the title instead. Most effects have their own settings column by column in the order
    /// they're listed, then a column of what comes out, `Mix` and `Gain`, last. A few are placed by
    /// what they mean, as their descriptions' grids say: a compressor's detector, times and output
    /// each have a column, an EQ's bands are its columns, and a delay's repeats, their tone and
    /// what comes out are its.
    pub fn grid(self) -> Grid {
        let placed = match self.mode() {
            Some(mode) => mode.grid,
            None => self.kind().and_then(|kind| kind.spec().grid),
        };
        let Some(placed) = placed else {
            return self.default_grid();
        };
        let params = self.params();
        let index = |key: &&str| params.iter().position(|found| found.key == *key);
        let columns: Vec<Vec<Option<usize>>> = placed
            .iter()
            .map(|column| column.iter().map(index).collect())
            .collect();
        Grid::from_columns(&columns)
    }

    /// The grid most effects have: their own settings down each column in order, then what comes
    /// out in a column of its own.
    fn default_grid(self) -> Grid {
        let params = self.params();
        let output = |at: &usize| params[*at].place == Place::Output;
        let own: Vec<usize> = (0..params.len())
            .filter(|at| Some(*at) != self.type_param() && !output(at))
            .collect();
        let mut columns: Vec<Vec<Option<usize>>> = own
            .chunks(GRID_ROWS)
            .map(|column| column.iter().copied().map(Some).collect())
            .collect();
        columns.push((0..params.len()).filter(output).map(Some).collect());
        Grid::from_columns(&columns)
    }

    /// The next or previous effect, as it starts, going round. It's the same kind of slot if the
    /// settings differ, so stepping always changes the effect.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn stepped(self, up: bool) -> Self {
        let at = Self::ALL
            .iter()
            .position(|effect| std::mem::discriminant(effect) == std::mem::discriminant(&self))
            .unwrap_or(0);
        let by = if up { 1 } else { Self::ALL.len() - 1 };
        Self::ALL[(at + by) % Self::ALL.len()]
    }
}

crate::listed::listed!(Insert: meaning);

fn unity_gain() -> u8 {
    UNITY_LEVEL
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(value: &u8) -> bool {
    *value == 0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_full(value: &u8) -> bool {
    *value == 0xFF
}

#[cfg(test)]
mod tests;
