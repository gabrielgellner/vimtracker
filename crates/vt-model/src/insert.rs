//! The effects a track can hold, in order, after its filters.

use serde::{Deserialize, Serialize};

use crate::constants::{MAX_GROUPS, MAX_LEVEL, MAX_SOUND_TRACKS, UNITY_LEVEL};
use crate::grid::{GRID_ROWS, Grid};
use crate::settings::{Entry, Kind, Units};

/// An effect's `read` and `written`, from one list of its fields and the setting each is, so a
/// field is named once here rather than twice. It goes in the effect's `impl`. Each entry is one of:
/// - `field => Setting`, a `u8` read and written as it is;
/// - `field: Choices => Setting`, a type or mode, read as its place in `Choices::ALL`;
/// - `field => Setting via (place, kept)`, kept on a scale of its own: `place` turns what's kept
///   into what a step gives, and `kept` turns that back. Distort's `bits` is the one.
///
/// A setting the effect doesn't have reads as `0` and writes as nothing. [`Insert::write`] only
/// writes one it has, within its range.
macro_rules! effect_settings {
    (
        $($field:ident $(: $choices:ident)? => $setting:ident $(via ($place:path, $kept:path))?),+
        $(,)?
    ) => {
        /// A setting's value, on the scale a step gives. One it doesn't have reads as nothing.
        pub(super) fn read(self, setting: $crate::insert::Setting) -> u8 {
            match setting {
                $($crate::insert::Setting::$setting => {
                    effect_settings!(@read self.$field $(, $choices)? $(, via $place)?)
                })+
                _ => 0,
            }
        }

        /// The same with a setting changed, on the scale `read` gives.
        pub(super) fn written(self, setting: $crate::insert::Setting, value: u8) -> Self {
            let mut settings = self;
            match setting {
                $($crate::insert::Setting::$setting => {
                    settings.$field = effect_settings!(@write value $(, $choices)? $(, via $kept)?);
                })+
                _ => {}
            }
            settings
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

/// One of a track's slots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Insert {
    #[default]
    Empty,
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
}

/// What a slot's setting is, for what moves it to read and write it on its own scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Type,
    Amount,
    Bits,
    /// Whether `Crush` compands as mu-law does, and whether `Down` holds at a multiple of the note.
    MuLaw,
    FollowNote,
    /// How long `Down` holds each sample, or how fast a chorus moves.
    Rate,
    /// A chorus's kind.
    ChorusType,
    /// How far a chorus's copies move.
    Depth,
    /// An EQ's bands: where each sits, how much each boosts or cuts, and how wide the middle is.
    LowFrequency,
    Low,
    MidFrequency,
    Mid,
    MidQ,
    HighFrequency,
    High,
    /// A compressor's: where it starts turning down, by how much, how fast, and what it hears.
    Threshold,
    Ratio,
    Attack,
    Release,
    Source,
    /// Whether a compressor makes up what it takes by itself, and how much it turns up what it's
    /// turned down, in dB, after that.
    AutoGain,
    Makeup,
    /// A compressor's character, and the console and Glue types' switches: each one's ratio,
    /// attack and release, a place among its choices. Glue's `Range` is the most it turns down,
    /// and its `Peak Clip` whether it soft clips what comes out, after everything else.
    CompressorType,
    ConsoleRatio,
    ConsoleAttack,
    ConsoleRelease,
    GlueRatio,
    GlueAttack,
    GlueRelease,
    Range,
    PeakClip,
    /// A limiter's: how hard it's driven, the most that comes out, and how fast it lets go.
    Input,
    Ceiling,
    LimitRelease,
    /// How long a phaser's or flanger's sweep takes, as a place in the list of sweeps.
    SweepRate,
    /// The frequency a phaser sweeps around, and how many all-passes it has.
    Centre,
    Stages,
    /// A flanger's delay at the middle of its sweep, and its feedback, negative below `80`.
    FlangerDelay,
    FlangerFeedback,
    /// A tape's: how hard it's driven, its tilt, its bias, its head bump, and its slow and fast
    /// wobble.
    TapeDrive,
    Tone,
    Bias,
    Bump,
    Wow,
    Flutter,
    /// A plate's: how big it is, scaling its tank's delays, and how far its tank's all-passes
    /// wobble.
    Size,
    Excursion,
    /// A pitch shifter's: how far it shifts, in whole semitones and then in cents, and how long
    /// each of its grains is.
    Shift,
    Fine,
    Grain,
    /// A granular effect's: how far back its grains read, how long they are, their envelope, how
    /// often they start, how wide they spread, whether recording stops, and how much each grain
    /// strays from what's set.
    Position,
    Length,
    Shape,
    Density,
    Spread,
    Freeze,
    Random,
    /// Warps': where its algorithm knob is, what each algorithm's own setting is, how hard the
    /// modulator is driven, and its oscillator's waveform.
    Algorithm,
    Timbre,
    ModulatorLevel,
    Wave,
    /// A low-pass gate's: what it opens, where it rests closed, its filter's resonance, and how
    /// slowly it responds.
    GateMode,
    Base,
    Resonance,
    Response,
    /// A resonator's: how its partials spread, how long its high ones ring, and where it's struck
    /// and heard. Its `Decay` and `Width` are the reverb's and the delay's.
    Material,
    Bright,
    Pickup,
    /// A comb's: whether it's a comb or a flanger. Its `Feedback` is the flanger's, its `Tone`
    /// the resonator's `Bright` and its `Spread` the granular effect's.
    CombMode,
    /// A vowel filter's: where it is among the vowels, how big a voice it makes, and how far its
    /// formants follow the note. Its `Res` is the low-pass gate's.
    Vowel,
    VoiceSize,
    KeyTrack,
    /// A frequency shifter's: how far it shifts, in Hz, how far apart its sides are shifted, and
    /// the delay in its loop. Its `Feedback` is the delay's and its `Track` the vowel filter's.
    ShiftHertz,
    ShiftSpread,
    LoopTime,
    /// A spring reverb's: how long its springs are, how much its early reflections come back,
    /// how much its all-passes smear a sound into a chirp, how much its loop's delay wanders, and
    /// whether it's knocked. Its `Decay` and `Damp` are the reverb's.
    SpringSize,
    Reflections,
    Spin,
    Chaos,
    Knock,
    /// A stutter's: whether it's repeating, how long the piece it repeats is, as a note division
    /// up to a beat, how many times it plays it again, and how much quieter each time.
    Trig,
    RepeatLength,
    Repeats,
    RepeatDecay,
    /// An exciter's: where the highs it excites start, and how fast its level detector rises and
    /// falls. Its `Drive` is the delay's.
    ExciterTone,
    ExciterAttack,
    ExciterRelease,
    /// A rotary speaker's: whether its rotors spin fast or slow, how long they take to change,
    /// how far the horn's pitch wavers and how far its loudness swells. Its `Drive` and `Width`
    /// are the delay's.
    Fast,
    Inertia,
    Doppler,
    Tremolo,
    /// A space's: how big its tank is. Its `Decay` and `Damp` are the reverb's and its `Mod` the
    /// plate's.
    SpaceSize,
    /// Clouds': its mode, where in its recording it reads, how long a piece, how many and how
    /// regularly, their texture, how much of its reverb, and its recording's quality. Its `Pitch`
    /// is the pitch shifter's, its `Blend` a `Mix`, and its `Spread`, `Feedback` and `Freeze` the
    /// granular effect's and the delay's.
    CloudsMode,
    CloudsPosition,
    CloudsSize,
    CloudsDensity,
    Texture,
    ReverbAmount,
    Quality,
    /// Beads': its mode, where in its recording it reads, how often grains start or how long its
    /// delay is, and whether that follows the song's steps. Its `Size` is the granular effect's
    /// `Length`, its `Pitch` the pitch shifter's, its `Quality` Clouds' and the rest the granular
    /// effect's, the delay's and Clouds'.
    BeadsMode,
    BeadsTime,
    BeadsDensity,
    Clock,
    /// A Space Echo's: its mode, how fast its tape runs and whether that follows the song's tempo,
    /// how hard it's driven, its tone and how loud its repeats are, each on its own scale. Its
    /// `Intensity` is the delay's `Feedback`, its `Inertia` the rotary's, its `Wow` the tape's and
    /// its `Reverb` Clouds'.
    EchoMode,
    EchoRate,
    EchoSync,
    EchoInput,
    EchoBass,
    EchoTreble,
    EchoLevel,
    Mix,
    Gain,
    /// A place in the list of note divisions.
    Time,
    Feedback,
    PingPong,
    /// Whether a delay glides to a new time or crossfades to it.
    Retime,
    Width,
    LowCut,
    HighCut,
    Drive,
    Pre,
    Decay,
    Damp,
}

/// One setting an effect declares. The pages, stepping, `x`, `K`, typing and the status line read
/// these, so a new effect needs no new page code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Param {
    pub name: &'static str,
    /// The name songs store it by, which never changes.
    pub key: &'static str,
    /// The grid's short label, 3 or 4 letters, after the slot's digit. Without one, a step can't
    /// lock it.
    pub label: Option<&'static str>,
    /// What it is, on its own scale.
    pub setting: Setting,
    /// Its range and the units it reads in, or its choices. A choice that's a scale stops at its
    /// ends, and one that's a list goes round.
    pub kind: Kind,
    pub default: u8,
    /// What it is to its effect.
    pub place: Place,
}

/// Where a setting sits in its effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// One of the effect's own, in its grid.
    Own,
    /// What the effect is: its type or mode, in the slot's title rather than its grid.
    Type,
    /// In the title too, after its type, but a played setting: Beads' quality.
    Title,
    /// What comes out, `Mix` and `Gain`: most effects' last column.
    Output,
}

/// The choices of a setting that's off or on, such as a delay's `Ping-pong`.
pub const OFF_ON: [&str; 2] = ["Off", "On"];
/// How a delay goes to a new time: gliding there, which bends the repeats' pitch as tape does, or
/// jumping, crossfading from where it read to where it reads now, so they keep their pitch.
pub const RETIMES: [&str; 2] = ["Glide", "Jump"];

impl Param {
    /// What it chooses between, in order, or nothing for a value in hex.
    pub const fn choices(&self) -> &'static [&'static str] {
        match self.kind {
            Kind::Stepped { choices, .. } => choices,
            Kind::Continuous { .. } => &[],
        }
    }

    /// Whether stepping past its last choice goes round to the first, or stops.
    pub const fn wraps(&self) -> bool {
        matches!(self.kind, Kind::Stepped { wraps: true, .. })
    }

    /// Whether it's off or on and nothing else, which the pages show as a toggle.
    pub fn is_toggle(&self) -> bool {
        self.choices() == OFF_ON
    }

    /// What full modulation moves it by, on the scale a step gives: the whole of a list, and `FF`
    /// for a value, as a depth is in hex steps of what it moves. `Gain` too, though it stops at
    /// the level scale's top, as the faders do.
    pub fn span(&self) -> f32 {
        match self.kind {
            Kind::Stepped { .. } => f32::from(self.kind.top()),
            Kind::Continuous { .. } => 255.0,
        }
    }

    /// It as an entry in the table of every setting.
    pub const fn entry(&self) -> Entry {
        Entry {
            key: self.key,
            name: self.name,
            label: self.label,
            kind: self.kind,
            default: self.default,
        }
    }

    /// The same, reading in `units`.
    const fn units(self, units: Units) -> Self {
        let Kind::Continuous { top, middle, .. } = self.kind else {
            return self;
        };
        Self {
            kind: Kind::Continuous { top, middle, units },
            ..self
        }
    }

    /// The same, with its middle at `80`, either way of which it reads.
    const fn centred(self) -> Self {
        let Kind::Continuous { top, units, .. } = self.kind else {
            return self;
        };
        Self {
            kind: Kind::Continuous {
                top,
                middle: true,
                units,
            },
            ..self
        }
    }
}

/// The most settings any effect shows at once, in the type or mode it's in: a grid's twelve and
/// its type or mode in the title (decision 312), and Beads' quality beside its mode there
/// (decision 320).
pub const MAX_PARAMS: usize = 14;
/// The most settings any effect keeps across all of its types and modes, of which it shows at
/// most [`MAX_PARAMS`]. A compressor keeps each type's own.
pub const MAX_KEPT_PARAMS: usize = 17;

const MIX_PARAM: Param = Param {
    name: "Mix",
    key: "mix",
    label: Some("MIX"),
    setting: Setting::Mix,
    kind: Kind::Continuous {
        top: 0xFF,
        middle: false,
        units: Units::Percent,
    },
    default: 0xFF,
    place: Place::Output,
};
const GAIN_PARAM: Param = Param {
    name: "Gain",
    key: "gain",
    label: Some("GAIN"),
    setting: Setting::Gain,
    kind: Kind::Continuous {
        top: MAX_LEVEL,
        middle: false,
        units: Units::Decibels,
    },
    default: UNITY_LEVEL,
    place: Place::Output,
};
const AMOUNT_PARAM: Param = Param {
    name: "Amount",
    key: "amount",
    label: Some("AMNT"),
    setting: Setting::Amount,
    kind: Kind::Continuous {
        top: 0xFF,
        middle: false,
        units: Units::Percent,
    },
    default: 0x40,
    place: Place::Own,
};

/// A delay or reverb's mix. It starts half wet in a slot, so the sound stays under it.
const WET_MIX_PARAM: Param = Param {
    default: 0x80,
    ..MIX_PARAM
};
/// A setting from `00` to `FF`, as a share of its range.
const fn hex_param(
    name: &'static str,
    key: &'static str,
    label: &'static str,
    setting: Setting,
    default: u8,
) -> Param {
    Param {
        name,
        key,
        label: Some(label),
        setting,
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
    /// Every effect a slot can hold, as it starts, in the order `+` goes through them.
    pub const ALL: [Self; 28] = [
        Self::Empty,
        Self::Distort(Distort::DEFAULT),
        Self::Delay(Delay::DEFAULT),
        Self::Reverb(Reverb::DEFAULT),
        Self::Chorus(Chorus::DEFAULT),
        Self::Eq(Equaliser::DEFAULT),
        Self::Compressor(Compressor::DEFAULT),
        Self::Limit(Limiter::DEFAULT),
        Self::Phaser(Phaser::DEFAULT),
        Self::Flanger(Flanger::DEFAULT),
        Self::Tape(Tape::DEFAULT),
        Self::Plate(Plate::DEFAULT),
        Self::Pitch(Pitch::DEFAULT),
        Self::Grains(Grains::DEFAULT),
        Self::Warps(Warps::DEFAULT),
        Self::Lpg(LowPassGate::DEFAULT),
        Self::Reso(Resonator::DEFAULT),
        Self::Comb(Comb::DEFAULT),
        Self::Vowel(Vowel::DEFAULT),
        Self::FreqShift(FreqShift::DEFAULT),
        Self::Spring(Spring::DEFAULT),
        Self::Space(Space::DEFAULT),
        Self::Rotary(Rotary::DEFAULT),
        Self::Exciter(Exciter::DEFAULT),
        Self::Stutter(Stutter::DEFAULT),
        Self::Clouds(Clouds::DEFAULT),
        Self::Beads(Beads::DEFAULT),
        Self::Echo(Echo::DEFAULT),
    ];

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
    pub fn meaning(self) -> &'static str {
        self.kind()
            .map_or("Nothing here", |kind| kind.spec().meaning)
    }

    /// The settings it has, in the mode it's in.
    pub fn params(self) -> &'static [Param] {
        match self {
            Self::Distort(distort) => match distort.kind {
                DistortType::Crush => &CRUSH_PARAMS,
                DistortType::Down => &DOWN_PARAMS,
                DistortType::Overdrive | DistortType::Distortion => &PEDAL_PARAMS,
                _ => &SHAPING_PARAMS,
            },
            Self::Compressor(compressor) => match compressor.kind {
                CompressorType::Clean => &CLEAN_PARAMS,
                CompressorType::Console => &CONSOLE_PARAMS,
                CompressorType::Glue => &GLUE_PARAMS,
            },
            _ => self.kind().map_or(&[], EffectKind::params),
        }
    }

    /// A setting's value, by its place among those it has now: a place in its choices, or the
    /// value itself.
    pub fn value(self, param: usize) -> u8 {
        self.params()
            .get(param)
            .and_then(|found| self.read(found.setting))
            .unwrap_or(0)
    }

    /// The same with a setting changed, by its place among those it has now.
    #[must_use]
    pub fn with_value(self, param: usize, value: u8) -> Self {
        self.params()
            .get(param)
            .map_or(self, |found| self.write(found.setting, f32::from(value)))
    }

    /// The setting its effect keeps, in any of its types and modes, if it has it.
    fn kept(self, setting: Setting) -> Option<&'static Param> {
        let kind = self.kind()?;
        kind.params().iter().find(|found| found.setting == setting)
    }

    /// A setting as a step gives it: a place for `Type` and `Bits`. Nothing if there's no effect
    /// here that has it. A distortion keeps every mode's, and a compressor every type's, so one not
    /// shown now still reads.
    pub fn read(self, setting: Setting) -> Option<u8> {
        self.kept(setting)?;
        Some(match self {
            Self::Empty => return None,
            Self::Distort(distort) => distort.read(setting),
            Self::Delay(delay) => delay.read(setting),
            Self::Reverb(reverb) => reverb.read(setting),
            Self::Chorus(chorus) => chorus.read(setting),
            Self::Eq(eq) => eq.read(setting),
            Self::Compressor(compressor) => compressor.read(setting),
            Self::Limit(limiter) => limiter.read(setting),
            Self::Phaser(phaser) => phaser.read(setting),
            Self::Flanger(flanger) => flanger.read(setting),
            Self::Tape(tape) => tape.read(setting),
            Self::Plate(plate) => plate.read(setting),
            Self::Pitch(pitch) => pitch.read(setting),
            Self::Grains(grains) => grains.read(setting),
            Self::Warps(warps) => warps.read(setting),
            Self::Lpg(gate) => gate.read(setting),
            Self::Reso(resonator) => resonator.read(setting),
            Self::Comb(comb) => comb.read(setting),
            Self::Vowel(vowel) => vowel.read(setting),
            Self::FreqShift(shifter) => shifter.read(setting),
            Self::Spring(spring) => spring.read(setting),
            Self::Space(space) => space.read(setting),
            Self::Rotary(rotary) => rotary.read(setting),
            Self::Exciter(exciter) => exciter.read(setting),
            Self::Stutter(stutter) => stutter.read(setting),
            Self::Clouds(clouds) => clouds.read(setting),
            Self::Beads(beads) => beads.read(setting),
            Self::Echo(echo) => echo.read(setting),
        })
    }

    /// The same with a setting moved, on the scale `read` gives, landing on a whole value within its
    /// range. It's how a stepped setting ends up stepping, whatever moved it. One there's no effect
    /// here to have stays as it was.
    #[must_use]
    pub fn write(self, setting: Setting, value: f32) -> Self {
        let Some(param) = self.kept(setting) else {
            return self;
        };
        let whole = value.round().clamp(0.0, f32::from(param.kind.top())) as u8;
        match self {
            Self::Empty => self,
            Self::Distort(distort) => Self::Distort(distort.written(setting, whole)),
            Self::Delay(delay) => Self::Delay(delay.written(setting, whole)),
            Self::Reverb(reverb) => Self::Reverb(reverb.written(setting, whole)),
            Self::Chorus(chorus) => Self::Chorus(chorus.written(setting, whole)),
            Self::Eq(eq) => Self::Eq(eq.written(setting, whole)),
            Self::Compressor(compressor) => Self::Compressor(compressor.written(setting, whole)),
            Self::Limit(limiter) => Self::Limit(limiter.written(setting, whole)),
            Self::Phaser(phaser) => Self::Phaser(phaser.written(setting, whole)),
            Self::Flanger(flanger) => Self::Flanger(flanger.written(setting, whole)),
            Self::Tape(tape) => Self::Tape(tape.written(setting, whole)),
            Self::Plate(plate) => Self::Plate(plate.written(setting, whole)),
            Self::Pitch(pitch) => Self::Pitch(pitch.written(setting, whole)),
            Self::Grains(grains) => Self::Grains(grains.written(setting, whole)),
            Self::Warps(warps) => Self::Warps(warps.written(setting, whole)),
            Self::Lpg(gate) => Self::Lpg(gate.written(setting, whole)),
            Self::Reso(resonator) => Self::Reso(resonator.written(setting, whole)),
            Self::Comb(comb) => Self::Comb(comb.written(setting, whole)),
            Self::Vowel(vowel) => Self::Vowel(vowel.written(setting, whole)),
            Self::FreqShift(shifter) => Self::FreqShift(shifter.written(setting, whole)),
            Self::Spring(spring) => Self::Spring(spring.written(setting, whole)),
            Self::Stutter(stutter) => Self::Stutter(stutter.written(setting, whole)),
            Self::Exciter(exciter) => Self::Exciter(exciter.written(setting, whole)),
            Self::Rotary(rotary) => Self::Rotary(rotary.written(setting, whole)),
            Self::Space(space) => Self::Space(space.written(setting, whole)),
            Self::Clouds(clouds) => Self::Clouds(clouds.written(setting, whole)),
            Self::Beads(beads) => Self::Beads(beads.written(setting, whole)),
            Self::Echo(echo) => Self::Echo(echo.written(setting, whole)),
        }
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
    /// what they mean: a compressor's detector, times and output each have a column, an EQ's
    /// bands are its columns, and a delay's repeats, their tone and what comes out are its.
    pub fn grid(self) -> Grid {
        use Setting as S;
        let placed: &[&[Setting]] = match self {
            Self::Compressor(compressor) => match compressor.kind {
                CompressorType::Clean => &[
                    &[S::Threshold, S::Ratio],
                    &[S::Attack, S::Release, S::Source],
                    &[S::Makeup, S::AutoGain],
                    &[S::Mix],
                ],
                CompressorType::Console => &[
                    &[S::Threshold, S::ConsoleRatio],
                    &[S::ConsoleAttack, S::ConsoleRelease, S::Source],
                    &[S::Makeup, S::AutoGain],
                    &[S::Mix],
                ],
                CompressorType::Glue => &[
                    &[S::Threshold, S::GlueRatio, S::Range],
                    &[S::GlueAttack, S::GlueRelease, S::Source],
                    &[S::Makeup, S::AutoGain],
                    &[S::Mix, S::PeakClip],
                ],
            },
            Self::Eq(_) => &[
                &[S::LowFrequency, S::Low],
                &[S::MidFrequency, S::Mid, S::MidQ],
                &[S::HighFrequency, S::High],
                &[S::Gain],
            ],
            Self::Delay(_) => &[
                &[S::Time, S::Feedback, S::PingPong],
                &[S::LowCut, S::HighCut, S::Drive],
                &[S::Retime, S::Wow],
                &[S::Width, S::Mix, S::Gain],
            ],
            Self::Spring(_) => &[
                &[S::Decay, S::SpringSize, S::Damp],
                &[S::Spin, S::Reflections],
                &[S::Chaos, S::Knock],
                &[S::Mix, S::Gain],
            ],
            Self::Stutter(_) => &[
                &[S::Trig],
                &[S::RepeatLength, S::Repeats, S::RepeatDecay],
                &[S::Mix, S::Gain],
            ],
            Self::Exciter(_) => &[
                &[S::Drive, S::ExciterTone],
                &[S::ExciterAttack, S::ExciterRelease],
                &[S::Mix, S::Gain],
            ],
            Self::Rotary(_) => &[
                &[S::Fast, S::Inertia],
                &[S::Doppler, S::Tremolo],
                &[S::Drive, S::Width],
                &[S::Mix, S::Gain],
            ],
            // The module's big knobs, then its small ones with its freeze button, then what its
            // `Blend` knob also does, then its quality over what comes out.
            Self::Clouds(_) => &[
                &[S::CloudsPosition, S::CloudsSize, S::Shift],
                &[S::CloudsDensity, S::Texture, S::Freeze],
                &[S::Spread, S::Feedback, S::ReverbAmount],
                &[S::Quality, S::Mix, S::Gain],
            ],
            // The module's big knobs; its pitch, its attenuverters' randomness and its freeze
            // button; its density, the clock it follows and its feedback; then its reverb over
            // what comes out. Its mode and quality are in the title.
            Self::Beads(_) => &[
                &[S::BeadsTime, S::Length, S::Shape],
                &[S::Shift, S::Random, S::Freeze],
                &[S::BeadsDensity, S::Clock, S::Feedback],
                &[S::ReverbAmount, S::Mix, S::Gain],
            ],
            // The tape's speed and how it gets there; the loop; the tone and the reverb; then the
            // repeats' level over what comes out. Its mode is in the title.
            Self::Echo(_) => &[
                &[S::EchoRate, S::EchoSync, S::Inertia],
                &[S::Feedback, S::EchoInput, S::Wow],
                &[S::EchoBass, S::EchoTreble, S::ReverbAmount],
                &[S::EchoLevel, S::Mix, S::Gain],
            ],
            _ => return self.default_grid(),
        };
        let params = self.params();
        let index = |setting: &Setting| params.iter().position(|found| found.setting == *setting);
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
    pub fn stepped(self, up: bool) -> Self {
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

#[cfg(test)]
mod tests;
