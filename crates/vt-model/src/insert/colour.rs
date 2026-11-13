//! The effects that colour a sound: a distortion, a tape and an exciter.

use serde::{Deserialize, Serialize};

use crate::constants::UNITY_LEVEL;

use super::{
    AMOUNT_PARAM, GAIN_PARAM, MIX_PARAM, OFF_ON, Param, Setting, WET_MIX_PARAM, from_middle,
    hex_param, is_zero, unity_gain,
};

/// What a distortion does to the sound. The waveshapers come first; the guitar pedals after them
/// are each modelled on a pedal's circuit and tuned against a reference (#361).
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
    pub fn name(self) -> &'static str {
        DISTORT_TYPE_NAMES[self as usize]
    }
}

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
pub(super) const TYPE_PARAM: Param = Param {
    name: "Type",
    key: "type",
    // Choosing a mode chooses a sound, so steps don't lock it.
    label: None,
    setting: Setting::Type,
    choices: &DISTORT_TYPE_NAMES,
    wraps: true,
    default: 0,
};

/// `Crush`'s bit depth, as a place among the depths it chooses between.
const BITS_PARAM: Param = Param {
    name: "Bits",
    key: "bits",
    label: Some("BITS"),
    setting: Setting::Bits,
    choices: &BIT_NAMES,
    wraps: false,
    // 9 bits.
    default: 7,
};
const RATE_PARAM: Param = Param {
    name: "Rate",
    key: "rate",
    label: Some("RATE"),
    setting: Setting::Rate,
    choices: &[],
    wraps: false,
    default: 0x40,
};
/// Whether `Crush` squeezes the sound by its log before it rounds and opens it out after, so
/// quiet sounds keep finer steps, as an 8-bit sampler's mu-law does.
const MU_LAW_PARAM: Param = Param {
    name: "Mu-law",
    key: "mulaw",
    label: Some("MULW"),
    setting: Setting::MuLaw,
    choices: &OFF_ON,
    wraps: true,
    default: 0,
};
/// Whether `Down` holds at a whole number of times the note's frequency, so what it folds back
/// lands on the note's harmonics.
const FOLLOW_PARAM: Param = Param {
    name: "Track",
    key: "track",
    label: Some("TRCK"),
    setting: Setting::FollowNote,
    choices: &OFF_ON,
    wraps: true,
    default: 0,
};
/// A pedal's tone control: from dark at `00` to bright at `FF`.
const PEDAL_TONE_PARAM: Param = Param {
    name: "Tone",
    key: "tone",
    label: Some("TONE"),
    setting: Setting::Tone,
    choices: &[],
    wraps: false,
    default: 0x80,
};
/// Every setting a distortion has in any of its modes, in the order songs number them. Each mode
/// shows only its own.
pub(crate) const DISTORT_PARAMS: [Param; 9] = [
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
pub(super) const SHAPING_PARAMS: [Param; 4] = [TYPE_PARAM, AMOUNT_PARAM, MIX_PARAM, GAIN_PARAM];
/// The pedals have a tone control as well.
pub(super) const PEDAL_PARAMS: [Param; 5] = [
    TYPE_PARAM,
    AMOUNT_PARAM,
    PEDAL_TONE_PARAM,
    MIX_PARAM,
    GAIN_PARAM,
];
/// `Crush` has whole bits in place of an `Amount`.
pub(super) const CRUSH_PARAMS: [Param; 5] =
    [TYPE_PARAM, BITS_PARAM, MU_LAW_PARAM, MIX_PARAM, GAIN_PARAM];
/// `Down` has how long it holds each sample.
pub(super) const DOWN_PARAMS: [Param; 5] =
    [TYPE_PARAM, RATE_PARAM, FOLLOW_PARAM, MIX_PARAM, GAIN_PARAM];

/// The bit depths `Crush` chooses between. Above these it can't be heard.
pub(super) const LEAST_BITS: u8 = 2;
pub(super) const MOST_BITS: u8 = 12;
const BIT_NAMES: [&str; (MOST_BITS - LEAST_BITS + 1) as usize] =
    ["2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"];

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
pub const MOST_TAPE_DRIVE_DB: f32 = 18.0;

/// A tape's drive in dB, for a value on the `00` to `FF` scale: from -18 dB, through 0 dB at `80`,
/// to +18 dB.
pub fn tape_drive_db(value: f32) -> f32 {
    from_middle(value) * MOST_TAPE_DRIVE_DB
}

/// A tape's, in the order the page shows them.
pub(crate) const TAPE_PARAMS: [Param; 8] = [
    hex_param(
        "Drive",
        "drive",
        "DRIV",
        Setting::TapeDrive,
        Tape::DEFAULT.drive,
    ),
    hex_param("Tone", "tone", "TONE", Setting::Tone, Tape::DEFAULT.tone),
    hex_param("Bias", "bias", "BIAS", Setting::Bias, Tape::DEFAULT.bias),
    hex_param("Bump", "bump", "BUMP", Setting::Bump, Tape::DEFAULT.bump),
    hex_param("Wow", "wow", "WOW", Setting::Wow, Tape::DEFAULT.wow),
    hex_param(
        "Flutter",
        "flutter",
        "FLUT",
        Setting::Flutter,
        Tape::DEFAULT.flutter,
    ),
    Param {
        default: 0xFF,
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

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::TapeDrive => self.drive,
            Setting::Tone => self.tone,
            Setting::Bias => self.bias,
            Setting::Bump => self.bump,
            Setting::Wow => self.wow,
            Setting::Flutter => self.flutter,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut tape = self;
        match setting {
            Setting::TapeDrive => tape.drive = value,
            Setting::Tone => tape.tone = value,
            Setting::Bias => tape.bias = value,
            Setting::Bump => tape.bump = value,
            Setting::Wow => tape.wow = value,
            Setting::Flutter => tape.flutter = value,
            Setting::Mix => tape.mix = value,
            Setting::Gain => tape.gain = value,
            _ => {}
        }
        tape
    }
}

/// Where an exciter's high-pass is at the bottom and top of `Tone`, in Hz, as Surge's.
pub const EXCITER_TONE_HZ: (f32, f32) = (500.0, 10_000.0);
/// How fast its level detector rises and falls at the ends of `Attack` and `Release`, in ms, as
/// Surge's knobs go.
pub const EXCITER_ATTACK_MS: (f32, f32) = (5.0, 20.0);
pub const EXCITER_RELEASE_MS: (f32, f32) = (50.0, 200.0);

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
pub(crate) const EXCITER_PARAMS: [Param; 6] = [
    hex_param(
        "Drive",
        "drive",
        "DRIV",
        Setting::Drive,
        Exciter::DEFAULT.drive,
    ),
    hex_param(
        "Tone",
        "tone",
        "TONE",
        Setting::ExciterTone,
        Exciter::DEFAULT.tone,
    ),
    hex_param(
        "Attack",
        "attack",
        "ATK",
        Setting::ExciterAttack,
        Exciter::DEFAULT.attack,
    ),
    hex_param(
        "Release",
        "release",
        "REL",
        Setting::ExciterRelease,
        Exciter::DEFAULT.release,
    ),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

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

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Drive => self.drive,
            Setting::ExciterTone => self.tone,
            Setting::ExciterAttack => self.attack,
            Setting::ExciterRelease => self.release,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut exciter = self;
        match setting {
            Setting::Drive => exciter.drive = value,
            Setting::ExciterTone => exciter.tone = value,
            Setting::ExciterAttack => exciter.attack = value,
            Setting::ExciterRelease => exciter.release = value,
            Setting::Mix => exciter.mix = value,
            Setting::Gain => exciter.gain = value,
            _ => {}
        }
        exciter
    }
}
