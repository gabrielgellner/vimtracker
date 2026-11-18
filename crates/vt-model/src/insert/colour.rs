//! The effects that colour a sound: a distortion, a tape, an exciter, an analog colour and a reel.

use serde::{Deserialize, Serialize};

use crate::constants::UNITY_LEVEL;

use super::{
    AMOUNT_PARAM, Columns, Entry, GAIN_PARAM, Kind, MIX_PARAM, Mode, OFF_ON, Place, Shown, Units,
    WET_MIX_PARAM, from_middle, hex_param, is_zero, place_of, places_of, unity_gain,
};

/// What a distortion does to the sound. The waveshapers come first; the guitar pedals after them
/// are each modelled on a pedal's circuit and tuned against a reference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistortType {
    #[default]
    Soft,
    Hard,
    Fold,
    Sine,
    Crush,
    Down,
    /// A Tube Screamer's: the mids clipped softly by diodes in an op-amp's feedback and added back
    /// to the clean sound, then its treble cut and lift, which does nothing at `Tone` `80`.
    Overdrive,
    /// An op-amp distortion's, as a RAT's or a SansAmp's: high gain and hard clipping, then a tone
    /// low-pass.
    Distortion,
}

impl DistortType {
    /// Every type, in the order `+` goes through them.
    pub const ALL: [Self; 8] = [
        Self::Soft,
        Self::Hard,
        Self::Fold,
        Self::Sine,
        Self::Crush,
        Self::Down,
        Self::Overdrive,
        Self::Distortion,
    ];

    /// Whether it's one of the guitar pedals, which have a `Tone`.
    pub fn is_pedal(self) -> bool {
        matches!(self, Self::Overdrive | Self::Distortion)
    }

    /// The name on screen.
    pub const fn name(self) -> &'static str {
        DISTORT_TYPE_NAMES[self as usize]
    }
}

crate::listed::listed!(DistortType);

const DISTORT_TYPE_NAMES: [&str; 8] = [
    "Soft",
    "Hard",
    "Fold",
    "Sine",
    "Crush",
    "Down",
    "Overdrive",
    "Distortion",
];
pub(super) const TYPE_PARAM: Entry = Entry {
    meaning: "how it bends the sound: clipping, folding, crushing or a guitar pedal",
    name: "Type",
    key: "type",
    label: Some("TYPE"),
    kind: Kind::Stepped {
        choices: &DISTORT_TYPE_NAMES,
        wraps: true,
    },
    default: 0,
    place: Place::Type,
};

/// `Crush`'s bit depth, as a place among the depths it chooses between.
const BITS_PARAM: Entry = Entry {
    meaning: "how many bits each sample keeps: fewer is grittier and gates quiet sounds",
    name: "Bits",
    key: "bits",
    label: Some("BITS"),
    kind: Kind::Stepped {
        choices: &BIT_NAMES,
        wraps: false,
    },
    // 9 bits.
    default: 7,
    place: Place::Own,
};
const RATE_PARAM: Entry = Entry {
    meaning: "how long each sample is held, lowering the sample rate",
    name: "Rate",
    key: "rate",
    label: Some("RATE"),
    kind: Kind::Continuous {
        top: 0xFF,
        middle: false,
        units: Units::Percent,
    },
    default: 0x40,
    place: Place::Own,
};
/// Whether `Crush` squeezes the sound by its log before it rounds and opens it out after, so
/// quiet sounds keep finer steps, as an 8-bit sampler's mu-law does.
const MU_LAW_PARAM: Entry = Entry {
    meaning: "squeezes the sound before rounding it, so quiet sounds keep finer steps",
    name: "Mu-law",
    key: "mulaw",
    label: Some("MULW"),
    kind: Kind::Stepped {
        choices: &OFF_ON,
        wraps: true,
    },
    default: 0,
    place: Place::Own,
};
/// Whether `Down` holds at a whole number of times the note's frequency, so what it folds back
/// lands on the note's harmonics.
const FOLLOW_PARAM: Entry = Entry {
    meaning: "holds in step with the note, so what folds back stays in tune",
    name: "Track",
    key: "track",
    label: Some("TRCK"),
    kind: Kind::Stepped {
        choices: &OFF_ON,
        wraps: true,
    },
    default: 0,
    place: Place::Own,
};
/// A pedal's tone control: from dark at `00` to bright at `FF`.
const PEDAL_TONE_PARAM: Entry = Entry {
    meaning: "the pedal's tone control, from dark to bright",
    name: "Tone",
    key: "tone",
    label: Some("TONE"),
    kind: Kind::Continuous {
        top: 0xFF,
        middle: true,
        units: Units::Percent,
    },
    default: 0x80,
    place: Place::Own,
};
/// Every setting a distortion has in any of its modes, in the order songs number them. Each mode
/// shows only its own.
pub(crate) const DISTORT_PARAMS: [Entry; 9] = [
    TYPE_PARAM,
    AMOUNT_PARAM,
    BITS_PARAM,
    RATE_PARAM,
    MIX_PARAM,
    GAIN_PARAM,
    PEDAL_TONE_PARAM,
    MU_LAW_PARAM,
    FOLLOW_PARAM,
];
/// What the shaping modes have: how hard it drives.
const SHAPING: Mode = Mode {
    shown: &places_of(&DISTORT_PARAMS, ["type", "amount", "mix", "gain"]),
    grid: None,
};
/// The pedals have a tone control as well.
const PEDAL: Mode = Mode {
    shown: &places_of(&DISTORT_PARAMS, ["type", "amount", "tone", "mix", "gain"]),
    grid: None,
};
/// `Crush` has whole bits in place of an `Amount`.
const CRUSH: Mode = Mode {
    shown: &places_of(&DISTORT_PARAMS, ["type", "bits", "mulaw", "mix", "gain"]),
    grid: None,
};
/// `Down` has how long it holds each sample.
const DOWN: Mode = Mode {
    shown: &places_of(&DISTORT_PARAMS, ["type", "rate", "track", "mix", "gain"]),
    grid: None,
};
/// Each type's mode, in the order of [`DistortType::ALL`].
pub(super) const DISTORT_SHOWN: Shown = Shown::ByType {
    type_at: place_of(&DISTORT_PARAMS, "type"),
    modes: &[
        SHAPING, SHAPING, SHAPING, SHAPING, CRUSH, DOWN, PEDAL, PEDAL,
    ],
};

/// The bit depths `Crush` chooses between. Above these it can't be heard.
pub(super) const LEAST_BITS: u8 = 2;
pub(super) const MOST_BITS: u8 = 12;
const BIT_NAMES: [&str; (MOST_BITS - LEAST_BITS + 1) as usize] =
    ["2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"];

/// A bit depth kept, from 2 to 12, as its place among `BIT_NAMES`, which a step gives.
const fn bits_place(bits: u8) -> u8 {
    bits - LEAST_BITS
}

/// A place among `BIT_NAMES` as the bit depth kept.
const fn bits_kept(place: u8) -> u8 {
    place + LEAST_BITS
}

/// A distortion's settings. It keeps every mode's own, so changing mode and back finds them as
/// they were.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Distort {
    pub kind: DistortType,
    /// How hard the shaping modes drive.
    pub amount: u8,
    /// `Crush`'s bit depth, from 2 to 12.
    #[serde(default = "default_bits")]
    pub bits: u8,
    /// How long `Down` holds each sample.
    #[serde(default = "default_rate")]
    pub rate: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    /// What comes out, on the level scale, so `E0` leaves it as it is. It's right there to turn
    /// down as `Amount` makes it louder.
    #[serde(default = "unity_gain")]
    pub gain: u8,
    /// The pedals' tone control, from dark at `00` to bright at `FF`.
    #[serde(default = "default_tone")]
    pub tone: u8,
    /// Whether `Crush` compands as mu-law does: `1`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub mu_law: u8,
    /// Whether `Down` holds at a whole number of times the note's frequency: `1`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub track: u8,
}

impl Distort {
    /// As it starts.
    pub const DEFAULT: Self = Self {
        kind: DistortType::Soft,
        amount: AMOUNT_PARAM.default,
        bits: BITS_PARAM.default + LEAST_BITS,
        rate: RATE_PARAM.default,
        mix: MIX_PARAM.default,
        gain: GAIN_PARAM.default,
        tone: PEDAL_TONE_PARAM.default,
        mu_law: 0,
        track: 0,
    };

    effect_settings! {
        kind: DistortType,
        amount,
        bits via (bits_place, bits_kept),
        rate,
        mix,
        gain,
        tone,
        mu_law,
        track,
    }
}

impl Default for Distort {
    fn default() -> Self {
        Self::DEFAULT
    }
}

fn default_bits() -> u8 {
    BITS_PARAM.default + LEAST_BITS
}

fn default_rate() -> u8 {
    RATE_PARAM.default
}

fn default_tone() -> u8 {
    PEDAL_TONE_PARAM.default
}

/// How far a tape's `Drive` turns what goes in up or down, at either end, in dB.
pub(crate) const MOST_TAPE_DRIVE_DB: f32 = 18.0;

/// A tape's drive in dB, for a value on the `00` to `FF` scale: from -18 dB, through 0 dB at `80`,
/// to +18 dB.
pub fn tape_drive_db(value: f32) -> f32 {
    from_middle(value) * MOST_TAPE_DRIVE_DB
}

/// A tape's, in the order the page shows them.
pub(crate) const TAPE_PARAMS: [Entry; 8] = [
    hex_param("Drive", "drive", "DRIV", Tape::DEFAULT.drive)
        .units(Units::TapeDrive)
        .centred()
        .means("how hard it hits the tape, which warms and thickens what's loud"),
    hex_param("Tone", "tone", "TONE", Tape::DEFAULT.tone)
        .centred()
        .means("tilts the tone: brighter turned up, darker turned down"),
    hex_param("Bias", "bias", "BIAS", Tape::DEFAULT.bias)
        .centred()
        .means("turned up it softens fast edges; turned down, quiet detail crackles"),
    hex_param("Bump", "bump", "BUMP", Tape::DEFAULT.bump)
        .means("how much the heads lift the lows, around 69 Hz"),
    hex_param("Wow", "wow", "WOW", Tape::DEFAULT.wow)
        .means("a slow wobble in pitch, as worn tape has"),
    hex_param("Flutter", "flutter", "FLUT", Tape::DEFAULT.flutter)
        .means("a faster, steady wobble in pitch, about twice a second"),
    Entry {
        default: 0xFF,
        meaning: "how much goes through the tape; below FF it blends in a faint chorus",
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// A tape's settings, after Airwindows' ToTape. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tape {
    /// How hard it's driven into the tape, from -18 dB to +18 dB, with none at `80`.
    pub drive: u8,
    /// Its tilt: darker below `80` and brighter above.
    pub tone: u8,
    /// None at `80`. Above, it's overbiased, which softens what moves fast; below, underbiased,
    /// which makes quiet detail stick and crackle.
    pub bias: u8,
    /// How much the heads boost the lows, around 69 Hz.
    pub bump: u8,
    /// The slow wobble in pitch, and the fast one.
    pub wow: u8,
    pub flutter: u8,
    /// From `00`, all dry, to `FF`, all through the tape.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Tape {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Tape {
    /// As it starts: driven at 0 dB with no tilt and no bias, and half the head bump, as ToTape's
    /// controls all start half way, with a little wow and flutter, and all wet.
    pub const DEFAULT: Self = Self {
        drive: 0x80,
        tone: 0x80,
        bias: 0x80,
        bump: 0x80,
        wow: 0x40,
        flutter: 0x40,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        drive,
        tone,
        bias,
        bump,
        wow,
        flutter,
        mix,
        gain,
    }
}

/// Where an exciter's high-pass is at the bottom and top of `Tone`, in Hz, as Surge's.
pub(crate) const EXCITER_TONE_HZ: (f32, f32) = (500.0, 10_000.0);
/// How fast its level detector rises and falls at the ends of `Attack` and `Release`, in ms, as
/// Surge's knobs go.
pub(crate) const EXCITER_ATTACK_MS: (f32, f32) = (5.0, 20.0);
pub(crate) const EXCITER_RELEASE_MS: (f32, f32) = (50.0, 200.0);

/// Between two ends on a log scale, for a value on the `00` to `FF` scale.
fn exciter_scale(value: f32, (low, high): (f32, f32)) -> f32 {
    low * (high / low).powf((value / 255.0).clamp(0.0, 1.0))
}

/// Where an exciter's high-pass is, in Hz: 500 Hz at `00` to 10 kHz at `FF`, evenly in pitch, so
/// `80` is about 2.2 kHz.
pub fn exciter_tone_hz(value: f32) -> f32 {
    exciter_scale(value, EXCITER_TONE_HZ)
}

/// How fast an exciter's level detector rises, in ms: 5 at `00` to 20 at `FF`, 10 at `80`.
pub fn exciter_attack_ms(value: f32) -> f32 {
    exciter_scale(value, EXCITER_ATTACK_MS)
}

/// How fast it falls, in ms: 50 at `00` to 200 at `FF`, 100 at `80`.
pub fn exciter_release_ms(value: f32) -> f32 {
    exciter_scale(value, EXCITER_RELEASE_MS)
}

/// An exciter's, in the order the page shows them, as columns: how hard it drives and where the
/// highs it drives start; then how its detector rises and falls; then how much is added, and the
/// gain.
pub(crate) const EXCITER_PARAMS: [Entry; 6] = [
    hex_param("Drive", "drive", "DRIV", Exciter::DEFAULT.drive)
        .means("how hard the highs are driven, so how much it makes"),
    hex_param("Tone", "tone", "TONE", Exciter::DEFAULT.tone)
        .units(Units::ExciterTone)
        .means("where the highs it excites start"),
    hex_param("Attack", "attack", "ATK", Exciter::DEFAULT.attack)
        .units(Units::ExciterAttack)
        .means("how fast its level detector rises as the highs get louder"),
    hex_param("Release", "release", "REL", Exciter::DEFAULT.release)
        .units(Units::ExciterRelease)
        .means("how fast its level detector falls as the highs fade"),
    WET_MIX_PARAM.means("how much of what it makes is added to the sound"),
    GAIN_PARAM,
];

/// Where its settings sit on the FX page.
pub(super) const EXCITER_GRID: Columns =
    &[&["drive", "tone"], &["attack", "release"], &["mix", "gain"]];

/// An exciter's settings: Chowdhury's model of the Aphex Aural Exciter, as Surge ships it. One a
/// file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Exciter {
    /// How hard the highs are driven into its shaper.
    pub drive: u8,
    /// Where the highs it excites start, from 500 Hz to 10 kHz.
    pub tone: u8,
    /// How fast its level detector rises and falls, from 5 to 20 ms and 50 to 200 ms.
    pub attack: u8,
    pub release: u8,
    /// How much of what it makes is added to the sound, from none at `00`.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Exciter {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Exciter {
    /// As it starts: Surge's, half way on everything.
    pub const DEFAULT: Self = Self {
        drive: 0x80,
        tone: 0x80,
        attack: 0x80,
        release: 0x80,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        drive,
        tone,
        attack,
        release,
        mix,
        gain,
    }
}

/// The curve an analog colour bends the sound with, each a circuit's (#262).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColourType {
    /// `tanh`, a differential pair's: smooth and symmetric.
    #[default]
    Warm,
    /// A 12AX7 triode stage, by Koren's equations: soft, with a strong second harmonic.
    Tube,
    /// A pair of silicon diodes back to back: soft, symmetric, and never quite flat.
    Diode,
    /// A transistor stage: cutting off softly one way and saturating hard the other.
    Transistor,
}

impl ColourType {
    /// Every type, in the order `+` goes through them.
    pub const ALL: [Self; 4] = [Self::Warm, Self::Tube, Self::Diode, Self::Transistor];

    /// The name on screen.
    pub const fn name(self) -> &'static str {
        COLOUR_TYPE_NAMES[self as usize]
    }
}

crate::listed::listed!(ColourType);

const COLOUR_TYPE_NAMES: [&str; 4] = ["Warm", "Tube", "Diode", "Transistor"];

/// How far a colour's `Drive` turns up what reaches its curve at `FF`, in dB, and how far its
/// `Emph` lifts or cuts the highs before the curve at either end.
pub const MOST_COLOUR_DRIVE_DB: f32 = 24.0;
pub const MOST_COLOUR_EMPHASIS_DB: f32 = 12.0;

/// A colour's drive in dB, for a value on the `00` to `FF` scale: none at `00`.
pub fn colour_drive_db(value: f32) -> f32 {
    (value / 255.0).clamp(0.0, 1.0) * MOST_COLOUR_DRIVE_DB
}

/// How far a colour's emphasis lifts the highs before its curve, in dB, cutting them after by as
/// much: none at `80`, and cut before below it.
pub fn colour_emphasis_db(value: f32) -> f32 {
    from_middle(value) * MOST_COLOUR_EMPHASIS_DB
}

/// A colour's, in the order songs number them.
pub(crate) const COLOUR_PARAMS: [Entry; 8] = [
    Entry {
        meaning: "the circuit whose curve it bends the sound with: warm, a tube, diodes or a transistor",
        name: "Type",
        key: "type",
        label: Some("TYPE"),
        kind: Kind::Stepped {
            choices: &COLOUR_TYPE_NAMES,
            wraps: true,
        },
        default: 0,
        place: Place::Type,
    },
    hex_param("Drive", "drive", "DRIV", Colour::DEFAULT.drive)
        .units(Units::ColourDrive)
        .means("how hard it drives the curve: clean at 00 but for what's loud, 24 dB at FF"),
    hex_param("Even", "even", "EVEN", Colour::DEFAULT.even)
        .means("tips the curve to one side, for even harmonics: a second that thickens"),
    hex_param("Hard", "hard", "HARD", Colour::DEFAULT.hard)
        .means("sharpens the curve's knee toward a hard clip, for brighter odd harmonics"),
    hex_param("Emph", "emph", "EMPH", Colour::DEFAULT.emph)
        .units(Units::ColourEmphasis)
        .centred()
        .means("above 80 the highs saturate first, as tape and transformers do; below, last"),
    hex_param("Iron", "iron", "IRON", Colour::DEFAULT.iron)
        .means("a transformer's lows: loud bass saturates, with a small lift around 60 Hz"),
    MIX_PARAM,
    GAIN_PARAM,
];

/// Where its settings sit on the FX page.
pub(super) const COLOUR_GRID: Columns = &[
    &["drive"],
    &["even", "hard"],
    &["emph", "iron"],
    &["mix", "gain"],
];

/// An analog colour's settings: a circuit's curve between a pre- and a de-emphasis, and a
/// transformer's lows. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colour {
    pub kind: ColourType,
    /// How hard the curve is driven, from none at `00` to 24 dB at `FF`.
    pub drive: u8,
    /// How far the curve's working point moves toward its nearer end, for even harmonics.
    pub even: u8,
    /// How far the curve is blended toward a hard clip at its own ends.
    pub hard: u8,
    /// How far the highs are lifted before the curve and cut after, so they saturate first: none
    /// at `80`, and the other way below.
    pub emph: u8,
    /// How much a transformer's core saturates the lows, and lifts them around 60 Hz.
    pub iron: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Colour {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Colour {
    /// As it starts: warm, driven a little, tipped a little, with a little iron.
    pub const DEFAULT: Self = Self {
        kind: ColourType::Warm,
        drive: 0x40,
        even: 0x40,
        hard: 0x00,
        emph: 0x80,
        iron: 0x40,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        kind: ColourType,
        drive,
        even,
        hard,
        emph,
        iron,
        mix,
        gain,
    }
}

/// The speeds a reel runs at, in inches a second, and their names.
pub const REEL_SPEEDS_IPS: [f32; 4] = [3.75, 7.5, 15.0, 30.0];
const REEL_SPEED_NAMES: [&str; 4] = ["3.75 ips", "7.5 ips", "15 ips", "30 ips"];

/// The most a reel's playback gap, the space between its head and the tape and the depth its
/// coating is read from can be, in µm, and the least of the gap.
pub const LEAST_REEL_GAP_UM: f32 = 0.5;
pub const MOST_REEL_GAP_UM: f32 = 20.0;
pub const MOST_REEL_SPACING_UM: f32 = 20.0;
pub const MOST_REEL_THICKNESS_UM: f32 = 50.0;

/// A reel's speed in inches a second, for its place in the list.
pub fn reel_speed_ips(place: u8) -> f32 {
    REEL_SPEEDS_IPS[usize::from(place).min(REEL_SPEEDS_IPS.len() - 1)]
}

/// A reel's playback gap in µm, for a value on the `00` to `FF` scale: from 0.5 µm to 20 µm,
/// evenly in ratio.
pub fn reel_gap_um(value: f32) -> f32 {
    LEAST_REEL_GAP_UM * (MOST_REEL_GAP_UM / LEAST_REEL_GAP_UM).powf((value / 255.0).clamp(0.0, 1.0))
}

/// How far a reel's tape is from its head, in µm: none at `00`, 20 µm at `FF`, squared so the
/// first half is fine.
pub fn reel_spacing_um(value: f32) -> f32 {
    (value / 255.0).clamp(0.0, 1.0).powi(2) * MOST_REEL_SPACING_UM
}

/// How deep in its coating a reel's tape is read from, in µm: none at `00`, 50 µm at `FF`,
/// squared as the spacing is.
pub fn reel_thickness_um(value: f32) -> f32 {
    (value / 255.0).clamp(0.0, 1.0).powi(2) * MOST_REEL_THICKNESS_UM
}

/// A reel's, in the order songs number them.
pub(crate) const REEL_PARAMS: [Entry; 11] = [
    hex_param("Drive", "drive", "DRIV", Reel::DEFAULT.drive)
        .units(Units::TapeDrive)
        .centred()
        .means("how hard the head's field drives the tape round its loop, so how far it saturates"),
    hex_param("Sat", "sat", "SAT", Reel::DEFAULT.sat)
        .means("how low the tape saturates: turned up, it squashes sooner and comes out thicker"),
    hex_param("Bias", "bias", "BIAS", Reel::DEFAULT.bias)
        .means("turned up the loop thins and it's cleaner; turned down it lags and smears"),
    Entry {
        meaning: "how fast the tape runs: slower is darker, with its head bump lower",
        name: "Speed",
        key: "speed",
        label: Some("SPD"),
        kind: Kind::Stepped {
            choices: &REEL_SPEED_NAMES,
            wraps: false,
        },
        default: Reel::DEFAULT.speed,
        place: Place::Own,
    },
    hex_param("Gap", "gap", "GAP", Reel::DEFAULT.gap)
        .units(Units::ReelGap)
        .means("the playback head's gap: wider loses the highs, with notches as tape speed slows"),
    hex_param("Space", "space", "SPAC", Reel::DEFAULT.space)
        .units(Units::ReelSpacing)
        .means("how far the tape rides off the head, which takes the highs away fast"),
    hex_param("Thick", "thick", "THK", Reel::DEFAULT.thick)
        .units(Units::ReelThickness)
        .means("how deep the coating is read from, which softens the highs gently"),
    hex_param("Wow", "wow", "WOW", Reel::DEFAULT.wow)
        .means("a slow wobble in pitch, as worn tape has"),
    hex_param("Flutter", "flutter", "FLUT", Reel::DEFAULT.flutter)
        .means("a faster, steady wobble in pitch, about twice a second"),
    MIX_PARAM,
    GAIN_PARAM,
];

/// Where its settings sit on the FX page.
pub(super) const REEL_GRID: Columns = &[
    &["drive", "sat", "bias"],
    &["speed", "wow", "flutter"],
    &["gap", "space", "thick"],
    &["mix", "gain"],
];

/// A tape machine's settings, after ChowTape: the tape's magnetic hysteresis, then the playback
/// head's losses and the transport's wow and flutter (#259). One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reel {
    /// How hard the head's field drives the tape, how low it saturates and how much it's biased,
    /// each from none to all.
    pub drive: u8,
    pub sat: u8,
    pub bias: u8,
    /// Its speed's place in [`REEL_SPEEDS_IPS`].
    pub speed: u8,
    /// The playback head's gap, the tape's spacing from it and the depth it's read from.
    pub gap: u8,
    pub space: u8,
    pub thick: u8,
    /// The slow wobble in pitch, and the fast one, as `Tape`'s.
    pub wow: u8,
    pub flutter: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Reel {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Reel {
    /// As it starts: ChowTape's drive, saturation and bias, half way, on a machine at 15 ips
    /// with a fine gap, a little spacing and a thin coating, and `Tape`'s little wow and flutter.
    pub const DEFAULT: Self = Self {
        drive: 0x80,
        sat: 0x80,
        bias: 0x80,
        speed: 2,
        gap: 0x40,
        space: 0x30,
        thick: 0x30,
        wow: 0x40,
        flutter: 0x40,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        drive,
        sat,
        bias,
        speed,
        gap,
        space,
        thick,
        wow,
        flutter,
        mix,
        gain,
    }
}
