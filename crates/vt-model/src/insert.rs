//! The effects a track can hold, in order, after its filters.

use serde::{Deserialize, Serialize};

use crate::constants::{MAX_GROUPS, MAX_LEVEL, MAX_SOUND_TRACKS, UNITY_LEVEL};
use crate::grid::{GRID_ROWS, Grid};
use crate::lfo::{DIVISION_NAMES, SWEEP_NAMES};

mod colour;
mod dynamics;
mod filters;
mod modulation;
mod pitch;
mod space;

pub use colour::*;
pub use dynamics::*;
pub use filters::*;
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

impl Setting {
    /// The highest value a step can give it.
    pub const fn highest(self) -> u8 {
        match self {
            Self::Type => DistortType::ALL.len() as u8 - 1,
            Self::ChorusType => ChorusType::ALL.len() as u8 - 1,
            Self::Source => SOURCE_NAMES.len() as u8 - 1,
            Self::AutoGain | Self::PeakClip => 1,
            Self::CompressorType => CompressorType::ALL.len() as u8 - 1,
            Self::ConsoleRatio => CONSOLE_RATIOS.len() as u8 - 1,
            Self::ConsoleAttack => CONSOLE_ATTACKS_MS.len() as u8 - 1,
            Self::ConsoleRelease => CONSOLE_AUTO_RELEASE,
            Self::GlueRatio => GLUE_RATIOS.len() as u8 - 1,
            Self::GlueAttack => GLUE_ATTACK_NAMES.len() as u8 - 1,
            Self::GlueRelease => GLUE_AUTO_RELEASE,
            Self::Bits => MOST_BITS - LEAST_BITS,
            Self::MuLaw | Self::FollowNote => 1,
            Self::Gain => MAX_LEVEL,
            Self::Time => DIVISION_NAMES.len() as u8 - 1,
            Self::SweepRate => SWEEP_NAMES.len() as u8 - 1,
            Self::Stages => STAGE_NAMES.len() as u8 - 1,
            Self::PingPong | Self::Freeze | Self::Knock | Self::Fast | Self::Trig => 1,
            Self::RepeatLength => REPEAT_LENGTH_NAMES.len() as u8 - 1,
            Self::Repeats => REPEAT_NAMES.len() as u8 - 1,
            Self::Retime => RETIMES.len() as u8 - 1,
            Self::Shift => SHIFT_NAMES.len() as u8 - 1,
            Self::Wave => WAVE_NAMES.len() as u8 - 1,
            Self::GateMode => GateMode::ALL.len() as u8 - 1,
            Self::CombMode => CombMode::ALL.len() as u8 - 1,
            Self::CloudsMode => CloudsMode::ALL.len() as u8 - 1,
            Self::Quality => QUALITY_NAMES.len() as u8 - 1,
            _ => 0xFF,
        }
    }

    /// What full modulation moves it by, in its own units.
    pub const fn span(self) -> f32 {
        match self {
            Self::Type => (DistortType::ALL.len() - 1) as f32,
            Self::ChorusType => (ChorusType::ALL.len() - 1) as f32,
            Self::Source => (SOURCE_NAMES.len() - 1) as f32,
            Self::AutoGain | Self::PeakClip => 1.0,
            Self::CompressorType => (CompressorType::ALL.len() - 1) as f32,
            Self::ConsoleRatio => (CONSOLE_RATIOS.len() - 1) as f32,
            Self::ConsoleAttack => (CONSOLE_ATTACKS_MS.len() - 1) as f32,
            Self::ConsoleRelease => CONSOLE_AUTO_RELEASE as f32,
            Self::GlueRatio => (GLUE_RATIOS.len() - 1) as f32,
            Self::GlueAttack => (GLUE_ATTACK_NAMES.len() - 1) as f32,
            Self::GlueRelease => GLUE_AUTO_RELEASE as f32,
            Self::Bits => (MOST_BITS - LEAST_BITS) as f32,
            Self::MuLaw | Self::FollowNote => 1.0,
            Self::Time => (DIVISION_NAMES.len() - 1) as f32,
            Self::SweepRate => (SWEEP_NAMES.len() - 1) as f32,
            Self::Stages => (STAGE_NAMES.len() - 1) as f32,
            Self::PingPong | Self::Freeze | Self::Knock | Self::Fast | Self::Trig => 1.0,
            Self::RepeatLength => (REPEAT_LENGTH_NAMES.len() - 1) as f32,
            Self::Repeats => (REPEAT_NAMES.len() - 1) as f32,
            Self::Retime => (RETIMES.len() - 1) as f32,
            Self::Shift => (SHIFT_NAMES.len() - 1) as f32,
            Self::Wave => (WAVE_NAMES.len() - 1) as f32,
            Self::GateMode => (GateMode::ALL.len() - 1) as f32,
            Self::CombMode => (CombMode::ALL.len() - 1) as f32,
            Self::CloudsMode => (CloudsMode::ALL.len() - 1) as f32,
            Self::Quality => (QUALITY_NAMES.len() - 1) as f32,
            _ => 255.0,
        }
    }
}

/// One setting an effect declares. The pages, stepping, `x`, `K` and typing read these, so a new
/// effect needs no new page code.
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
    /// What it chooses between, in order, or nothing for a value in hex.
    pub choices: &'static [&'static str],
    /// Whether stepping past the last choice goes round to the first, or stops. A choice that's a
    /// scale stops, and one that's a list goes round.
    pub wraps: bool,
    pub default: u8,
}

/// The choices of a setting that's off or on, such as a delay's `Ping-pong`.
pub const OFF_ON: [&str; 2] = ["Off", "On"];
/// How a delay goes to a new time: gliding there, which bends the repeats' pitch as tape does, or
/// jumping, crossfading from where it read to where it reads now, so they keep their pitch.
pub const RETIMES: [&str; 2] = ["Glide", "Jump"];

impl Param {
    /// Whether it's off or on and nothing else, which the pages show as a toggle.
    pub fn is_toggle(&self) -> bool {
        self.choices == OFF_ON
    }
}

/// The most settings any effect shows at once, in the type or mode it's in: a grid's twelve and
/// its type or mode in the title (decision 312).
pub const MAX_PARAMS: usize = 13;
/// The most settings any effect keeps across all of its types and modes, of which it shows at
/// most [`MAX_PARAMS`]. A compressor keeps each type's own.
pub const MAX_KEPT_PARAMS: usize = 17;

const MIX_PARAM: Param = Param {
    name: "Mix",
    key: "mix",
    label: Some("MIX"),
    setting: Setting::Mix,
    choices: &[],
    wraps: false,
    default: 0xFF,
};
const GAIN_PARAM: Param = Param {
    name: "Gain",
    key: "gain",
    label: Some("GAIN"),
    setting: Setting::Gain,
    choices: &[],
    wraps: false,
    default: UNITY_LEVEL,
};
const AMOUNT_PARAM: Param = Param {
    name: "Amount",
    key: "amount",
    label: Some("AMNT"),
    setting: Setting::Amount,
    choices: &[],
    wraps: false,
    default: 0x40,
};

/// A delay or reverb's mix. It starts half wet in a slot, so the sound stays under it.
const WET_MIX_PARAM: Param = Param {
    default: 0x80,
    ..MIX_PARAM
};
/// A setting from `00` to `FF`.
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
        choices: &[],
        wraps: false,
        default,
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
    pub const ALL: [Self; 26] = [
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
        match self {
            Self::Empty => "---",
            Self::Distort(_) => "Distort",
            Self::Delay(_) => "Delay",
            Self::Reverb(_) => "Reverb",
            Self::Chorus(_) => "Chorus",
            Self::Eq(_) => "EQ3",
            Self::Compressor(_) => "Comp",
            Self::Limit(_) => "Limit",
            Self::Phaser(_) => "Phaser",
            Self::Flanger(_) => "Flanger",
            Self::Tape(_) => "Tape",
            Self::Plate(_) => "Plate",
            Self::Pitch(_) => "Pitch",
            Self::Grains(_) => "Grains",
            Self::Warps(_) => "Warps",
            Self::Lpg(_) => "LPG",
            Self::Reso(_) => "Reso",
            Self::Comb(_) => "Comb",
            Self::Vowel(_) => "Vowel",
            Self::FreqShift(_) => "Freq Shift",
            Self::Spring(_) => "Spring",
            Self::Stutter(_) => "Stutter",
            Self::Exciter(_) => "Exciter",
            Self::Rotary(_) => "Rotary",
            Self::Space(_) => "Space",
            Self::Clouds(_) => "Clouds",
        }
    }

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Empty => "Nothing here",
            Self::Distort(_) => "Clips, folds or crushes the sound, or drives it as a pedal",
            Self::Delay(_) => "Repeats the sound, bouncing side to side",
            Self::Reverb(_) => "Puts the sound in a room",
            Self::Chorus(_) => "Thickens it with drifting copies, an ensemble or a Juno's",
            Self::Eq(_) => "Shapes its tone: a low and a high shelf, and a middle band",
            Self::Compressor(_) => "Evens out its level, or ducks it under another track",
            Self::Limit(_) => "Keeps its peaks under a ceiling, looking 2 ms ahead",
            Self::Phaser(_) => "Sweeps notches through it with all-passes",
            Self::Flanger(_) => "Sweeps a comb through it with a short delay",
            Self::Tape(_) => "Saturates, warms and wobbles it as tape does",
            Self::Plate(_) => "Puts the sound on a bright, dense plate",
            Self::Pitch(_) => "Shifts its pitch up or down, keeping its time",
            Self::Grains(_) => "Records it and plays it back as a cloud of grains",
            Self::Warps(_) => "Ring mods, folds or vocodes it with another track",
            Self::Lpg(_) => "Gates or filters it by its level, or another track's",
            Self::Reso(_) => "Rings it like a bell or bar tuned to the note",
            Self::Comb(_) => "Rings it through a comb or flanger tuned to the note",
            Self::Vowel(_) => "Makes it say a vowel, morphing from A to U",
            Self::FreqShift(_) => "Moves every frequency by the same Hz: bells and spirals",
            Self::Spring(_) => {
                "Puts the sound in a spring tank, as a guitar amp has: drips and boings"
            }
            Self::Stutter(_) => "Repeats a piece of the sound when a step says: rolls and glitches",
            Self::Exciter(_) => "Adds air: harmonics made from the highs, more as they get louder",
            Self::Rotary(_) => "Spins it in a Leslie's cabinet, slow or fast, for organs and more",
            Self::Space(_) => "Washes the sound into a huge, nearly endless pad",
            Self::Clouds(_) => {
                "Mutable's Clouds: grains, stretching or a looping delay, with reverb"
            }
        }
    }

    /// The settings it has, in the mode it's in.
    pub fn params(self) -> &'static [Param] {
        match self {
            Self::Empty => &[],
            Self::Distort(distort) => match distort.kind {
                DistortType::Crush => &CRUSH_PARAMS,
                DistortType::Down => &DOWN_PARAMS,
                DistortType::Overdrive | DistortType::Distortion => &PEDAL_PARAMS,
                _ => &SHAPING_PARAMS,
            },
            Self::Delay(_) => &DELAY_PARAMS,
            Self::Reverb(_) => &REVERB_PARAMS,
            Self::Chorus(_) => &CHORUS_PARAMS,
            Self::Eq(_) => &EQ_PARAMS,
            Self::Compressor(compressor) => match compressor.kind {
                CompressorType::Clean => &CLEAN_PARAMS,
                CompressorType::Console => &CONSOLE_PARAMS,
                CompressorType::Glue => &GLUE_PARAMS,
            },
            Self::Limit(_) => &LIMIT_PARAMS,
            Self::Phaser(_) => &PHASER_PARAMS,
            Self::Flanger(_) => &FLANGER_PARAMS,
            Self::Tape(_) => &TAPE_PARAMS,
            Self::Plate(_) => &PLATE_PARAMS,
            Self::Pitch(_) => &PITCH_PARAMS,
            Self::Grains(_) => &GRAINS_PARAMS,
            Self::Warps(_) => &WARPS_PARAMS,
            Self::Lpg(_) => &LPG_PARAMS,
            Self::Reso(_) => &RESO_PARAMS,
            Self::Comb(_) => &COMB_PARAMS,
            Self::Vowel(_) => &VOWEL_PARAMS,
            Self::FreqShift(_) => &FREQ_SHIFT_PARAMS,
            Self::Spring(_) => &SPRING_PARAMS,
            Self::Space(_) => &SPACE_PARAMS,
            Self::Rotary(_) => &ROTARY_PARAMS,
            Self::Exciter(_) => &EXCITER_PARAMS,
            Self::Stutter(_) => &STUTTER_PARAMS,
            Self::Clouds(_) => &CLOUDS_PARAMS,
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

    /// A setting as a step gives it: a place for `Type` and `Bits`. Nothing if there's no effect
    /// here that has it. A distortion keeps every mode's, and a compressor every type's, so one not
    /// shown now still reads.
    pub fn read(self, setting: Setting) -> Option<u8> {
        let distort = match self {
            Self::Distort(distort) => distort,
            Self::Delay(delay) => {
                return DELAY_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| delay.read(setting));
            }
            Self::Reverb(reverb) => {
                return REVERB_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| reverb.read(setting));
            }
            Self::Chorus(chorus) => {
                return CHORUS_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| chorus.read(setting));
            }
            Self::Eq(eq) => {
                return EQ_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| eq.read(setting));
            }
            Self::Compressor(compressor) => {
                return COMPRESSOR_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| compressor.read(setting));
            }
            Self::Limit(limiter) => {
                return LIMIT_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| limiter.read(setting));
            }
            Self::Phaser(phaser) => {
                return PHASER_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| phaser.read(setting));
            }
            Self::Flanger(flanger) => {
                return FLANGER_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| flanger.read(setting));
            }
            Self::Tape(tape) => {
                return TAPE_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| tape.read(setting));
            }
            Self::Plate(plate) => {
                return PLATE_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| plate.read(setting));
            }
            Self::Pitch(pitch) => {
                return PITCH_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| pitch.read(setting));
            }
            Self::Grains(grains) => {
                return GRAINS_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| grains.read(setting));
            }
            Self::Warps(warps) => {
                return WARPS_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| warps.read(setting));
            }
            Self::Lpg(gate) => {
                return LPG_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| gate.read(setting));
            }
            Self::Reso(resonator) => {
                return RESO_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| resonator.read(setting));
            }
            Self::Comb(comb) => {
                return COMB_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| comb.read(setting));
            }
            Self::Vowel(vowel) => {
                return VOWEL_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| vowel.read(setting));
            }
            Self::FreqShift(shifter) => {
                return FREQ_SHIFT_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| shifter.read(setting));
            }
            Self::Spring(spring) => {
                return SPRING_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| spring.read(setting));
            }
            Self::Stutter(stutter) => {
                return STUTTER_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| stutter.read(setting));
            }
            Self::Exciter(exciter) => {
                return EXCITER_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| exciter.read(setting));
            }
            Self::Rotary(rotary) => {
                return ROTARY_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| rotary.read(setting));
            }
            Self::Space(space) => {
                return SPACE_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| space.read(setting));
            }
            Self::Clouds(clouds) => {
                return CLOUDS_PARAMS
                    .iter()
                    .any(|found| found.setting == setting)
                    .then(|| clouds.read(setting));
            }
            Self::Empty => return None,
        };
        Some(match setting {
            Setting::Type => distort.kind as u8,
            Setting::Amount => distort.amount,
            Setting::Bits => distort.bits - LEAST_BITS,
            Setting::Rate => distort.rate,
            Setting::MuLaw => distort.mu_law,
            Setting::FollowNote => distort.track,
            Setting::Mix => distort.mix,
            Setting::Gain => distort.gain,
            Setting::Tone => distort.tone,
            _ => return None,
        })
    }

    /// The same with a setting moved, on the scale `read` gives, landing on a whole value within its
    /// range. It's how a stepped setting ends up stepping, whatever moved it.
    #[must_use]
    pub fn write(self, setting: Setting, value: f32) -> Self {
        let whole = value.round().clamp(0.0, f32::from(setting.highest())) as u8;
        let distort = match self {
            Self::Distort(distort) => distort,
            Self::Delay(delay) => return Self::Delay(delay.written(setting, whole)),
            Self::Reverb(reverb) => return Self::Reverb(reverb.written(setting, whole)),
            Self::Chorus(chorus) => return Self::Chorus(chorus.written(setting, whole)),
            Self::Eq(eq) => return Self::Eq(eq.written(setting, whole)),
            Self::Compressor(compressor) => {
                return Self::Compressor(compressor.written(setting, whole));
            }
            Self::Limit(limiter) => return Self::Limit(limiter.written(setting, whole)),
            Self::Phaser(phaser) => return Self::Phaser(phaser.written(setting, whole)),
            Self::Flanger(flanger) => return Self::Flanger(flanger.written(setting, whole)),
            Self::Tape(tape) => return Self::Tape(tape.written(setting, whole)),
            Self::Plate(plate) => return Self::Plate(plate.written(setting, whole)),
            Self::Pitch(pitch) => return Self::Pitch(pitch.written(setting, whole)),
            Self::Grains(grains) => return Self::Grains(grains.written(setting, whole)),
            Self::Warps(warps) => return Self::Warps(warps.written(setting, whole)),
            Self::Lpg(gate) => return Self::Lpg(gate.written(setting, whole)),
            Self::Reso(resonator) => return Self::Reso(resonator.written(setting, whole)),
            Self::Comb(comb) => return Self::Comb(comb.written(setting, whole)),
            Self::Vowel(vowel) => return Self::Vowel(vowel.written(setting, whole)),
            Self::FreqShift(shifter) => return Self::FreqShift(shifter.written(setting, whole)),
            Self::Spring(spring) => return Self::Spring(spring.written(setting, whole)),
            Self::Stutter(stutter) => return Self::Stutter(stutter.written(setting, whole)),
            Self::Exciter(exciter) => return Self::Exciter(exciter.written(setting, whole)),
            Self::Rotary(rotary) => return Self::Rotary(rotary.written(setting, whole)),
            Self::Space(space) => return Self::Space(space.written(setting, whole)),
            Self::Clouds(clouds) => return Self::Clouds(clouds.written(setting, whole)),
            Self::Empty => return self,
        };
        let mut distort = distort;
        match setting {
            Setting::Type => distort.kind = DistortType::ALL[usize::from(whole)],
            Setting::Amount => distort.amount = whole,
            Setting::Bits => distort.bits = whole + LEAST_BITS,
            Setting::Rate => distort.rate = whole,
            Setting::MuLaw => distort.mu_law = whole,
            Setting::FollowNote => distort.track = whole,
            Setting::Mix => distort.mix = whole,
            Setting::Gain => distort.gain = whole,
            Setting::Tone => distort.tone = whole,
            _ => {}
        }
        Self::Distort(distort)
    }

    /// The setting that changes the effect's character, by its place among those it has now: the
    /// `Type` of a Distort, a Chorus or a Comp, and the `Mode` of an LPG or a Comb. The FX page
    /// shows it in the slot's title, beside the effect, rather than in its grid.
    pub fn type_param(self) -> Option<usize> {
        self.params().iter().position(|found| {
            matches!(
                found.setting,
                Setting::Type
                    | Setting::ChorusType
                    | Setting::CompressorType
                    | Setting::GateMode
                    | Setting::CombMode
                    | Setting::CloudsMode
            )
        })
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
        let output = |at: &usize| matches!(params[*at].setting, Setting::Mix | Setting::Gain);
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

fn unity_gain() -> u8 {
    UNITY_LEVEL
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(value: &u8) -> bool {
    *value == 0
}

#[cfg(test)]
mod tests;
