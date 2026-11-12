//! The effects a track can hold, in order, after its filters.

use serde::{Deserialize, Serialize};

use crate::constants::{MAX_GROUPS, MAX_LEVEL, MAX_SOUND_TRACKS, UNITY_LEVEL};
use crate::grid::{GRID_ROWS, Grid};
use crate::lfo::{DIVISION_NAMES, SWEEP_NAMES};

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
}

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
    /// to the clean sound, then a tone control.
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

/// How a chorus moves its copies, after Mutable Instruments' Rings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChorusType {
    /// Two copies, slowly drifting apart from each other.
    #[default]
    Chorus,
    /// Three copies on a slow and a fast wobble, a third of a turn apart, as a string machine has.
    Ensemble,
}

impl ChorusType {
    pub const ALL: [Self; 2] = [Self::Chorus, Self::Ensemble];
}

const CHORUS_TYPE_NAMES: [&str; 2] = ["Chorus", "Ensemble"];

/// A compressor's character, each modelled on its own reference and tuned against it (#338).
/// `VCA`, `FET` and `Diode` come after these, in this order, so a song's types keep their places.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressorType {
    /// Transparent and digital, with an exact ratio and knee, as Ableton's Compressor is.
    #[default]
    Clean,
    /// An SSL console's bus compressor, fitted to the one in Arturia's Bus FORCE: set ratios,
    /// attacks and releases, an automatic release, and its detector hearing what it's already
    /// turned down.
    Console,
    /// Ableton Live's Glue Compressor, another model of the same SSL, fitted to it: its own
    /// switches, a knee that softens as the ratio falls, and a limit on how far it turns down.
    Glue,
}

impl CompressorType {
    /// Every type, in the order `+` goes through them.
    pub const ALL: [Self; 3] = [Self::Clean, Self::Console, Self::Glue];
}

const COMPRESSOR_TYPE_NAMES: [&str; 3] = ["Clean", "Console", "Glue"];

/// The console compressor's ratios, as its switch has them.
pub const CONSOLE_RATIOS: [f32; 3] = [2.0, 4.0, 10.0];
const CONSOLE_RATIO_NAMES: [&str; 3] = ["2:1", "4:1", "10:1"];
/// The console compressor's attacks in ms, as its switch has them.
pub const CONSOLE_ATTACKS_MS: [f32; 6] = [0.1, 0.3, 1.0, 3.0, 10.0, 30.0];
const CONSOLE_ATTACK_NAMES: [&str; 6] = ["0.1 ms", "0.3 ms", "1 ms", "3 ms", "10 ms", "30 ms"];
/// The console compressor's releases in ms, as its switch has them, before `Auto`.
pub const CONSOLE_RELEASES_MS: [f32; 4] = [100.0, 300.0, 600.0, 1200.0];
/// Where `Auto` is among the console compressor's releases: after the set times.
pub const CONSOLE_AUTO_RELEASE: u8 = CONSOLE_RELEASES_MS.len() as u8;
const CONSOLE_RELEASE_NAMES: [&str; 5] = ["0.1 s", "0.3 s", "0.6 s", "1.2 s", "Auto"];

/// Glue's ratios, attacks and releases, as Live shows its switches. Its attacks are in ms, and
/// its releases are named in seconds, as the SSL's are, though they don't take that long: what
/// each takes is the engine's.
pub const GLUE_RATIOS: [f32; 3] = [2.0, 4.0, 10.0];
const GLUE_RATIO_NAMES: [&str; 3] = ["2:1", "4:1", "10:1"];
const GLUE_ATTACK_NAMES: [&str; 7] = [
    "0.01 ms", "0.1 ms", "0.3 ms", "1 ms", "3 ms", "10 ms", "30 ms",
];
const GLUE_RELEASE_NAMES: [&str; 7] =
    ["0.1 s", "0.2 s", "0.4 s", "0.6 s", "0.8 s", "1.2 s", "Auto"];
/// Where `Auto` is among Glue's releases: after the set ones.
pub const GLUE_AUTO_RELEASE: u8 = GLUE_RELEASE_NAMES.len() as u8 - 1;
/// The most Glue's `Range` lets it turn down, in dB, where it starts, as Live's does.
pub const MOST_RANGE_DB: f32 = 70.0;

/// Glue's `Range` in dB, for a value on the `00` to `FF` scale: from 0 dB, where it doesn't turn
/// down at all, to [`MOST_RANGE_DB`].
pub fn range_db(value: f32) -> f32 {
    value / 255.0 * MOST_RANGE_DB
}

/// What a slot's setting is, for what moves it to read and write it on its own scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Type,
    Amount,
    Bits,
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
            Self::Gain => MAX_LEVEL,
            Self::Time => DIVISION_NAMES.len() as u8 - 1,
            Self::SweepRate => SWEEP_NAMES.len() as u8 - 1,
            Self::Stages => STAGE_NAMES.len() as u8 - 1,
            Self::PingPong | Self::Freeze => 1,
            Self::Retime => RETIMES.len() as u8 - 1,
            Self::Shift => SHIFT_NAMES.len() as u8 - 1,
            Self::Wave => WAVE_NAMES.len() as u8 - 1,
            Self::GateMode => GateMode::ALL.len() as u8 - 1,
            Self::CombMode => CombMode::ALL.len() as u8 - 1,
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
            Self::Time => (DIVISION_NAMES.len() - 1) as f32,
            Self::SweepRate => (SWEEP_NAMES.len() - 1) as f32,
            Self::Stages => (STAGE_NAMES.len() - 1) as f32,
            Self::PingPong | Self::Freeze => 1.0,
            Self::Retime => (RETIMES.len() - 1) as f32,
            Self::Shift => (SHIFT_NAMES.len() - 1) as f32,
            Self::Wave => (WAVE_NAMES.len() - 1) as f32,
            Self::GateMode => (GateMode::ALL.len() - 1) as f32,
            Self::CombMode => (CombMode::ALL.len() - 1) as f32,
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

/// The most settings any effect shows at once, in the type or mode it's in.
pub const MAX_PARAMS: usize = 12;
/// The most settings any effect keeps across all of its types and modes, of which it shows at
/// most [`MAX_PARAMS`]. A compressor keeps each type's own.
pub const MAX_KEPT_PARAMS: usize = 17;

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
const TYPE_PARAM: Param = Param {
    name: "Type",
    key: "type",
    // Choosing a mode chooses a sound, so steps don't lock it.
    label: None,
    setting: Setting::Type,
    choices: &DISTORT_TYPE_NAMES,
    wraps: true,
    default: 0,
};
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
pub(crate) const DISTORT_PARAMS: [Param; 7] = [
    TYPE_PARAM,
    AMOUNT_PARAM,
    BITS_PARAM,
    RATE_PARAM,
    MIX_PARAM,
    GAIN_PARAM,
    PEDAL_TONE_PARAM,
];
/// What the shaping modes have: how hard it drives.
const SHAPING_PARAMS: [Param; 4] = [TYPE_PARAM, AMOUNT_PARAM, MIX_PARAM, GAIN_PARAM];
/// The pedals have a tone control as well.
const PEDAL_PARAMS: [Param; 5] = [
    TYPE_PARAM,
    AMOUNT_PARAM,
    PEDAL_TONE_PARAM,
    MIX_PARAM,
    GAIN_PARAM,
];
/// `Crush` has whole bits in place of an `Amount`.
const CRUSH_PARAMS: [Param; 4] = [TYPE_PARAM, BITS_PARAM, MIX_PARAM, GAIN_PARAM];
/// `Down` has how long it holds each sample.
const DOWN_PARAMS: [Param; 4] = [TYPE_PARAM, RATE_PARAM, MIX_PARAM, GAIN_PARAM];

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
/// A reverb's, in the order the page shows them.
pub(crate) const REVERB_PARAMS: [Param; 7] = [
    hex_param("Pre", "pre", "PRE", Setting::Pre, Reverb::DEFAULT.pre),
    hex_param(
        "Decay",
        "decay",
        "DCAY",
        Setting::Decay,
        Reverb::DEFAULT.decay,
    ),
    hex_param("Damp", "damp", "DAMP", Setting::Damp, Reverb::DEFAULT.damp),
    hex_param(
        "Low cut",
        "lowcut",
        "LCUT",
        Setting::LowCut,
        Reverb::DEFAULT.low_cut,
    ),
    hex_param(
        "High cut",
        "highcut",
        "HCUT",
        Setting::HighCut,
        Reverb::DEFAULT.high_cut,
    ),
    WET_MIX_PARAM,
    GAIN_PARAM,
];
/// A delay's, in the order the page shows them.
pub(crate) const DELAY_PARAMS: [Param; 11] = [
    Param {
        name: "Time",
        key: "time",
        label: Some("TIME"),
        setting: Setting::Time,
        choices: &DIVISION_NAMES,
        wraps: false,
        default: Delay::DEFAULT.time,
    },
    hex_param(
        "Feedback",
        "feedback",
        "FDBK",
        Setting::Feedback,
        Delay::DEFAULT.feedback,
    ),
    Param {
        name: "Ping-pong",
        key: "pingpong",
        label: Some("PING"),
        setting: Setting::PingPong,
        choices: &OFF_ON,
        wraps: true,
        default: Delay::DEFAULT.ping_pong,
    },
    hex_param(
        "Width",
        "width",
        "WDTH",
        Setting::Width,
        Delay::DEFAULT.width,
    ),
    hex_param(
        "Low cut",
        "lowcut",
        "LCUT",
        Setting::LowCut,
        Delay::DEFAULT.low_cut,
    ),
    hex_param(
        "High cut",
        "highcut",
        "HCUT",
        Setting::HighCut,
        Delay::DEFAULT.high_cut,
    ),
    hex_param(
        "Drive",
        "drive",
        "DRIV",
        Setting::Drive,
        Delay::DEFAULT.drive,
    ),
    Param {
        name: "Retime",
        key: "retime",
        label: Some("RTIM"),
        setting: Setting::Retime,
        choices: &RETIMES,
        wraps: true,
        default: Delay::DEFAULT.retime,
    },
    hex_param("Wow", "wow", "WOW", Setting::Wow, Delay::DEFAULT.wow),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// A chorus's, in the order the page shows them.
pub(crate) const CHORUS_PARAMS: [Param; 5] = [
    Param {
        name: "Type",
        key: "type",
        // Choosing a mode chooses a sound, so steps don't lock it.
        label: None,
        setting: Setting::ChorusType,
        choices: &CHORUS_TYPE_NAMES,
        wraps: true,
        default: 0,
    },
    hex_param("Rate", "rate", "RATE", Setting::Rate, Chorus::DEFAULT.rate),
    hex_param(
        "Depth",
        "depth",
        "DPTH",
        Setting::Depth,
        Chorus::DEFAULT.depth,
    ),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// An EQ's, in the order the page shows them.
pub(crate) const EQ_PARAMS: [Param; 8] = [
    hex_param(
        "Low freq",
        "lowfreq",
        "LFRQ",
        Setting::LowFrequency,
        Equaliser::DEFAULT.low_frequency,
    ),
    hex_param("Low", "low", "LOW", Setting::Low, Equaliser::DEFAULT.low),
    hex_param(
        "Mid freq",
        "midfreq",
        "MFRQ",
        Setting::MidFrequency,
        Equaliser::DEFAULT.mid_frequency,
    ),
    hex_param("Mid", "mid", "MID", Setting::Mid, Equaliser::DEFAULT.mid),
    hex_param(
        "High freq",
        "highfreq",
        "HFRQ",
        Setting::HighFrequency,
        Equaliser::DEFAULT.high_frequency,
    ),
    hex_param(
        "High",
        "high",
        "HIGH",
        Setting::High,
        Equaliser::DEFAULT.high,
    ),
    // After the bands, so the page shows each band's frequency over its boost.
    hex_param(
        "Mid Q",
        "midq",
        "MIDQ",
        Setting::MidQ,
        Equaliser::DEFAULT.mid_q,
    ),
    GAIN_PARAM,
];

/// What a compressor can hear: its own input, a track by its number, a return, or a group. The
/// groups come after the returns, so the returns keep the numbers songs saved them by.
pub const SOURCE_NAMES: [&str; MAX_SOUND_TRACKS + 3 + MAX_GROUPS] = [
    "Self", "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "13", "14",
    "15", "16", "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30",
    "31", "32", "33", "34", "35", "36", "37", "38", "39", "40", "41", "42", "43", "44", "45", "46",
    "47", "48", "49", "50", "51", "52", "53", "54", "55", "56", "57", "58", "59", "60", "61", "62",
    "63", "64", "A", "B", "G1", "G2", "G3", "G4",
];

/// A compressor's threshold in dB, for a value on the `00` to `FF` scale: from -60 dB to 0 dB.
pub fn threshold_db(value: f32) -> f32 {
    value / 255.0 * 60.0 - 60.0
}

/// A compressor's ratio for a value on the `00` to `FF` scale: 1:1 at `00`, rising faster as it
/// goes, to 20:1 just below the top, and as hard as a limiter at `FF`.
pub fn ratio(value: f32) -> f32 {
    if value >= 255.0 {
        f32::INFINITY
    } else {
        1.0 + (value / 255.0).powi(2) * 19.0
    }
}

/// A compressor's attack in ms: from 0.1 ms at `00` to 100 ms at `FF`, evenly in ratio.
pub fn attack_ms(value: f32) -> f32 {
    0.1 * 1000_f32.powf(value / 255.0)
}

/// A compressor's release in ms: from 10 ms at `00` to a second at `FF`, evenly in ratio.
pub fn release_ms(value: f32) -> f32 {
    10.0 * 100_f32.powf(value / 255.0)
}

const COMPRESSOR_TYPE_PARAM: Param = Param {
    name: "Type",
    key: "type",
    // Choosing a type chooses a sound, so steps don't lock it.
    label: None,
    setting: Setting::CompressorType,
    choices: &COMPRESSOR_TYPE_NAMES,
    wraps: true,
    default: 0,
};
const SOURCE_PARAM: Param = Param {
    name: "Source",
    key: "source",
    // What it hears is part of how it's set up, so steps don't change it.
    label: None,
    setting: Setting::Source,
    choices: &SOURCE_NAMES,
    wraps: false,
    default: 0,
};
const THRESHOLD_PARAM: Param = hex_param(
    "Threshold",
    "threshold",
    "THRS",
    Setting::Threshold,
    Compressor::DEFAULT.threshold,
);
const RATIO_PARAM: Param = hex_param(
    "Ratio",
    "ratio",
    "RTIO",
    Setting::Ratio,
    Compressor::DEFAULT.ratio,
);
const ATTACK_PARAM: Param = hex_param(
    "Attack",
    "attack",
    "ATK",
    Setting::Attack,
    Compressor::DEFAULT.attack,
);
const RELEASE_PARAM: Param = hex_param(
    "Release",
    "release",
    "REL",
    Setting::Release,
    Compressor::DEFAULT.release,
);
const AUTO_GAIN_PARAM: Param = Param {
    name: "Auto",
    key: "auto",
    label: Some("AUTO"),
    setting: Setting::AutoGain,
    choices: &OFF_ON,
    wraps: true,
    default: 0,
};
/// Every type's makeup, from 0 dB at `00` to [`MOST_MAKEUP_DB`] at `FF`, on top of what `Auto`
/// gives back.
const MAKEUP_PARAM: Param = hex_param(
    "Makeup",
    "makeup",
    "MKUP",
    Setting::Makeup,
    Compressor::DEFAULT.makeup,
);
/// The bus type's switches, shown by the same names as the clean type's knobs. Their labels are
/// their own, as a slot's labels name one setting each.
const CONSOLE_RATIO_PARAM: Param = Param {
    name: "Ratio",
    key: "consoleratio",
    label: Some("CRTO"),
    setting: Setting::ConsoleRatio,
    choices: &CONSOLE_RATIO_NAMES,
    wraps: false,
    default: Compressor::DEFAULT.console_ratio,
};
const CONSOLE_ATTACK_PARAM: Param = Param {
    name: "Attack",
    key: "consoleattack",
    label: Some("CATK"),
    setting: Setting::ConsoleAttack,
    choices: &CONSOLE_ATTACK_NAMES,
    wraps: false,
    default: Compressor::DEFAULT.console_attack,
};
const CONSOLE_RELEASE_PARAM: Param = Param {
    name: "Release",
    key: "consolerelease",
    label: Some("CREL"),
    setting: Setting::ConsoleRelease,
    choices: &CONSOLE_RELEASE_NAMES,
    wraps: false,
    default: Compressor::DEFAULT.console_release,
};
/// Glue's switches, shown by the same names again, with labels of their own.
const GLUE_RATIO_PARAM: Param = Param {
    name: "Ratio",
    key: "glueratio",
    label: Some("GRTO"),
    setting: Setting::GlueRatio,
    choices: &GLUE_RATIO_NAMES,
    wraps: false,
    default: Compressor::DEFAULT.glue_ratio,
};
const GLUE_ATTACK_PARAM: Param = Param {
    name: "Attack",
    key: "glueattack",
    label: Some("GATK"),
    setting: Setting::GlueAttack,
    choices: &GLUE_ATTACK_NAMES,
    wraps: false,
    default: Compressor::DEFAULT.glue_attack,
};
const GLUE_RELEASE_PARAM: Param = Param {
    name: "Release",
    key: "gluerelease",
    label: Some("GREL"),
    setting: Setting::GlueRelease,
    choices: &GLUE_RELEASE_NAMES,
    wraps: false,
    default: Compressor::DEFAULT.glue_release,
};
/// Glue's `Range`, the most it turns down, from none at `00` to [`MOST_RANGE_DB`] at `FF`.
const RANGE_PARAM: Param = hex_param(
    "Range",
    "range",
    "RNGE",
    Setting::Range,
    Compressor::DEFAULT.range,
);
/// Glue's soft clip at the very end, after its makeup and its mix, as Live's `Peak Clip In` is:
/// off, as Live loads it, or on.
const PEAK_CLIP_PARAM: Param = Param {
    name: "Peak Clip",
    key: "peakclip",
    label: Some("PCLP"),
    setting: Setting::PeakClip,
    choices: &OFF_ON,
    wraps: true,
    default: Compressor::DEFAULT.peak_clip,
};
/// A compressor's whole mix, which it starts at, for compressing in parallel when turned down.
const COMPRESSOR_MIX_PARAM: Param = Param {
    default: 0xFF,
    ..MIX_PARAM
};
/// Every setting a compressor has in any of its types, in the order songs number them. Each type
/// shows only its own.
pub(crate) const COMPRESSOR_PARAMS: [Param; 17] = [
    COMPRESSOR_TYPE_PARAM,
    SOURCE_PARAM,
    THRESHOLD_PARAM,
    RATIO_PARAM,
    ATTACK_PARAM,
    RELEASE_PARAM,
    CONSOLE_RATIO_PARAM,
    CONSOLE_ATTACK_PARAM,
    CONSOLE_RELEASE_PARAM,
    GLUE_RATIO_PARAM,
    GLUE_ATTACK_PARAM,
    GLUE_RELEASE_PARAM,
    RANGE_PARAM,
    AUTO_GAIN_PARAM,
    COMPRESSOR_MIX_PARAM,
    MAKEUP_PARAM,
    PEAK_CLIP_PARAM,
];
/// `Clean`'s, in the order the page shows them: the type over what it hears, then the threshold
/// over the ratio, the attack over the release, and Auto over the mix, with the makeup last.
const CLEAN_PARAMS: [Param; 9] = [
    COMPRESSOR_TYPE_PARAM,
    SOURCE_PARAM,
    THRESHOLD_PARAM,
    RATIO_PARAM,
    ATTACK_PARAM,
    RELEASE_PARAM,
    AUTO_GAIN_PARAM,
    COMPRESSOR_MIX_PARAM,
    MAKEUP_PARAM,
];
/// `Console`'s: its switches in place of the clean type's knobs, and the rest as `Clean` has them.
const CONSOLE_PARAMS: [Param; 9] = [
    COMPRESSOR_TYPE_PARAM,
    SOURCE_PARAM,
    THRESHOLD_PARAM,
    CONSOLE_RATIO_PARAM,
    CONSOLE_ATTACK_PARAM,
    CONSOLE_RELEASE_PARAM,
    AUTO_GAIN_PARAM,
    COMPRESSOR_MIX_PARAM,
    MAKEUP_PARAM,
];
/// `Glue`'s: its switches as `Console` has its own, and its `Range` over the makeup, so the rest
/// keep the places they have in the other types, then its `Peak Clip` in a column of its own, last
/// as it's last in what Glue does.
const GLUE_PARAMS: [Param; 11] = [
    COMPRESSOR_TYPE_PARAM,
    SOURCE_PARAM,
    THRESHOLD_PARAM,
    GLUE_RATIO_PARAM,
    GLUE_ATTACK_PARAM,
    GLUE_RELEASE_PARAM,
    AUTO_GAIN_PARAM,
    COMPRESSOR_MIX_PARAM,
    RANGE_PARAM,
    MAKEUP_PARAM,
    PEAK_CLIP_PARAM,
];

/// The most a compressor's `Makeup` turns up what it's turned down, in dB, as a limiter's `Input`
/// turns up what goes in.
pub const MOST_MAKEUP_DB: f32 = 24.0;

/// A compressor's makeup in dB, for a value on the `00` to `FF` scale: from 0 dB to +24 dB.
pub fn makeup_db(value: f32) -> f32 {
    value / 255.0 * MOST_MAKEUP_DB
}

/// How much a limiter's `Input` turns up what goes in at most, in dB.
pub const MOST_INPUT_DB: f32 = 24.0;
/// How far below 0 dB a limiter's `Ceiling` goes.
pub const LOWEST_CEILING_DB: f32 = -24.0;

/// A limiter's drive in dB, for a value on the `00` to `FF` scale: from 0 dB to +24 dB.
pub fn input_db(value: f32) -> f32 {
    value / 255.0 * MOST_INPUT_DB
}

/// A limiter's ceiling in dB, for a value on the `00` to `FF` scale: from -24 dB to 0 dB.
pub fn ceiling_db(value: f32) -> f32 {
    LOWEST_CEILING_DB * (1.0 - value / 255.0)
}

/// A limiter's release in ms: from 1 ms at `00` to a second at `FF`, evenly in ratio, so `AA` is
/// 100 ms.
pub fn limit_release_ms(value: f32) -> f32 {
    1000_f32.powf(value / 255.0)
}

/// A limiter's, in the order the page shows them.
pub(crate) const LIMIT_PARAMS: [Param; 5] = [
    hex_param(
        "Input",
        "input",
        "INPT",
        Setting::Input,
        Limiter::DEFAULT.input,
    ),
    hex_param(
        "Ceiling",
        "ceiling",
        "CEIL",
        Setting::Ceiling,
        Limiter::DEFAULT.ceiling,
    ),
    hex_param(
        "Release",
        "release",
        "REL",
        Setting::LimitRelease,
        Limiter::DEFAULT.release,
    ),
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// How many semitones a phaser's `Depth` sweeps either way of its centre, at full.
pub const MOST_PHASER_SEMITONES: f32 = 48.0;
/// A flanger's shortest and longest delay at the middle of its sweep, in ms.
pub const SHORTEST_FLANGE_MS: f32 = 0.5;
pub const LONGEST_FLANGE_MS: f32 = 10.0;

/// A flanger's delay in ms at the middle of its sweep, for a value on the `00` to `FF` scale: from
/// 0.5 ms to 10 ms, evenly in ratio.
pub fn flanger_delay_ms(value: f32) -> f32 {
    SHORTEST_FLANGE_MS * (LONGEST_FLANGE_MS / SHORTEST_FLANGE_MS).powf(value / 255.0)
}

/// How far a value on the `00` to `FF` scale is from `80`, from -1 to 1. The sides have their own
/// scales, as there are 128 values below the middle and 127 above.
pub fn from_middle(value: f32) -> f32 {
    let from_middle = value - 128.0;
    from_middle / if from_middle < 0.0 { 128.0 } else { 127.0 }
}

/// A flanger's feedback from -1 to 1 for a value on the `00` to `FF` scale, with none at `80`.
pub fn flanger_feedback(value: f32) -> f32 {
    from_middle(value)
}

/// How many all-passes a phaser can have, as Vital's does: each four make two notches.
pub const STAGE_NAMES: [&str; 3] = ["4", "8", "12"];

/// The `Rate` of a phaser or flanger: how long its sweep takes, in note lengths.
const fn sweep_param(default: u8) -> Param {
    Param {
        name: "Rate",
        key: "rate",
        label: Some("RATE"),
        setting: Setting::SweepRate,
        choices: &SWEEP_NAMES,
        wraps: false,
        default,
    }
}

/// A phaser's, in the order the page shows them.
pub(crate) const PHASER_PARAMS: [Param; 7] = [
    sweep_param(Phaser::DEFAULT.rate),
    hex_param(
        "Depth",
        "depth",
        "DPTH",
        Setting::Depth,
        Phaser::DEFAULT.depth,
    ),
    hex_param(
        "Centre",
        "centre",
        "CNTR",
        Setting::Centre,
        Phaser::DEFAULT.centre,
    ),
    hex_param(
        "Feedback",
        "feedback",
        "FDBK",
        Setting::Feedback,
        Phaser::DEFAULT.feedback,
    ),
    Param {
        name: "Stages",
        key: "stages",
        label: Some("STGS"),
        setting: Setting::Stages,
        choices: &STAGE_NAMES,
        wraps: false,
        default: Phaser::DEFAULT.stages,
    },
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// A flanger's, in the order the page shows them.
pub(crate) const FLANGER_PARAMS: [Param; 6] = [
    sweep_param(Flanger::DEFAULT.rate),
    hex_param(
        "Depth",
        "depth",
        "DPTH",
        Setting::Depth,
        Flanger::DEFAULT.depth,
    ),
    hex_param(
        "Delay",
        "delay",
        "DLAY",
        Setting::FlangerDelay,
        Flanger::DEFAULT.delay,
    ),
    hex_param(
        "Feedback",
        "feedback",
        "FDBK",
        Setting::FlangerFeedback,
        Flanger::DEFAULT.feedback,
    ),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

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

/// A plate's, in the order the page shows them. Those it shares with the reverb are named as its.
pub(crate) const PLATE_PARAMS: [Param; 8] = [
    hex_param("Pre", "pre", "PRE", Setting::Pre, Plate::DEFAULT.pre),
    hex_param(
        "Decay",
        "decay",
        "DCAY",
        Setting::Decay,
        Plate::DEFAULT.decay,
    ),
    hex_param("Damp", "damp", "DAMP", Setting::Damp, Plate::DEFAULT.damp),
    hex_param(
        "High cut",
        "highcut",
        "HCUT",
        Setting::HighCut,
        Plate::DEFAULT.high_cut,
    ),
    hex_param("Size", "size", "SIZE", Setting::Size, Plate::DEFAULT.size),
    hex_param(
        "Mod",
        "mod",
        "MOD",
        Setting::Excursion,
        Plate::DEFAULT.excursion,
    ),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// How far a pitch shifter goes either way, in semitones.
pub const MOST_SHIFT: u8 = 24;
/// The shifts it chooses between, from two octaves down to two up, written as the track's `Pitch`
/// shows semitones.
const SHIFT_NAMES: [&str; (MOST_SHIFT * 2 + 1) as usize] = [
    "-24 st", "-23 st", "-22 st", "-21 st", "-20 st", "-19 st", "-18 st", "-17 st", "-16 st",
    "-15 st", "-14 st", "-13 st", "-12 st", "-11 st", "-10 st", "-9 st", "-8 st", "-7 st", "-6 st",
    "-5 st", "-4 st", "-3 st", "-2 st", "-1 st", "+0 st", "+1 st", "+2 st", "+3 st", "+4 st",
    "+5 st", "+6 st", "+7 st", "+8 st", "+9 st", "+10 st", "+11 st", "+12 st", "+13 st", "+14 st",
    "+15 st", "+16 st", "+17 st", "+18 st", "+19 st", "+20 st", "+21 st", "+22 st", "+23 st",
    "+24 st",
];
/// How far a pitch shifter's `Fine` goes either way, in cents.
pub const MOST_FINE_CENTS: f32 = 50.0;
/// A pitch shifter's shortest and longest grain, in ms.
pub const SHORTEST_GRAIN_MS: f32 = 10.0;
pub const LONGEST_GRAIN_MS: f32 = 100.0;

/// How many semitones a pitch shifter's `Pitch` shifts by, for a place in its list.
pub fn shift_semitones(value: f32) -> f32 {
    value - f32::from(MOST_SHIFT)
}

/// A pitch shifter's fine shift in cents, for a value on the `00` to `FF` scale: from -50, through
/// none at `80`, to +50.
pub fn fine_cents(value: f32) -> f32 {
    from_middle(value) * MOST_FINE_CENTS
}

/// How long a pitch shifter's grains are in ms, for a value on the `00` to `FF` scale: from 10 ms
/// to 100 ms, evenly in ratio.
pub fn grain_ms(value: f32) -> f32 {
    SHORTEST_GRAIN_MS * (LONGEST_GRAIN_MS / SHORTEST_GRAIN_MS).powf(value / 255.0)
}

/// A pitch shifter's, in the order the page shows them.
pub(crate) const PITCH_PARAMS: [Param; 6] = [
    Param {
        name: "Pitch",
        key: "pitch",
        label: Some("PTCH"),
        setting: Setting::Shift,
        choices: &SHIFT_NAMES,
        wraps: false,
        default: Pitch::DEFAULT.shift,
    },
    hex_param("Fine", "fine", "FINE", Setting::Fine, Pitch::DEFAULT.fine),
    hex_param("Size", "size", "SIZE", Setting::Grain, Pitch::DEFAULT.size),
    hex_param(
        "Feedback",
        "feedback",
        "FDBK",
        Setting::Feedback,
        Pitch::DEFAULT.feedback,
    ),
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// A granular effect's, in the order the page shows them, as columns of two: where and how long,
/// how often and how wide, freezing and straying, then pitch and envelope, whose short names fit
/// the narrow fourth column.
pub(crate) const GRAINS_PARAMS: [Param; 10] = [
    hex_param(
        "Time",
        "time",
        "TIME",
        Setting::Position,
        Grains::DEFAULT.time,
    ),
    hex_param(
        "Size",
        "size",
        "SIZE",
        Setting::Length,
        Grains::DEFAULT.size,
    ),
    hex_param(
        "Density",
        "density",
        "DENS",
        Setting::Density,
        Grains::DEFAULT.density,
    ),
    hex_param(
        "Spread",
        "spread",
        "SPRD",
        Setting::Spread,
        Grains::DEFAULT.spread,
    ),
    Param {
        name: "Freeze",
        key: "freeze",
        label: Some("FRZ"),
        setting: Setting::Freeze,
        choices: &OFF_ON,
        wraps: true,
        default: Grains::DEFAULT.freeze,
    },
    hex_param(
        "Random",
        "random",
        "RAND",
        Setting::Random,
        Grains::DEFAULT.random,
    ),
    Param {
        default: Grains::DEFAULT.pitch,
        ..PITCH_PARAMS[0]
    },
    hex_param(
        "Shape",
        "shape",
        "SHAP",
        Setting::Shape,
        Grains::DEFAULT.shape,
    ),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// How much a granular effect records, in seconds, as Beads' does at its best quality.
pub const GRAIN_BUFFER_SECONDS: f32 = 4.0;
/// How often a granular effect's grains can start at most, in Hz, as Beads': the C an octave below
/// middle C. At random they start half as often at most, on average.
pub const MOST_GRAIN_HERTZ: f32 = 130.812_78;

/// How far back a granular effect's grains read, in seconds, for a value on the `00` to `FF` scale:
/// from just behind what's being recorded to the whole recording, evenly.
pub fn grain_time_seconds(value: f32) -> f32 {
    value / 255.0 * GRAIN_BUFFER_SECONDS
}

/// How long a granular effect's grains are, in seconds, and whether they play backwards, for a
/// value on the `00` to `FF` scale, as Beads' `Size` knob: from the whole recording backwards at
/// `00`, shortening to about 8 ms at a third, then lengthening forwards to the whole of it at `FF`.
pub fn grain_length(value: f32) -> (f32, bool) {
    let size = value / 255.0;
    let reversed = size < 1.0 / 3.0;
    let shortness = if reversed {
        size * 3.0 * (2.0 - size * 3.0)
    } else {
        1.0 - 1.5 * (size - 1.0 / 3.0)
    };
    let shortness = (shortness * 1.025).min(1.0);
    ((shortness * -9.0).exp2() * GRAIN_BUFFER_SECONDS, reversed)
}

/// How a granular effect's grains start, for a `Density`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Triggering {
    /// Evenly, so many a second.
    Regular(f32),
    /// Not at all.
    Off,
    /// At random, about so many a second.
    Random(f32),
}

/// How a granular effect's grains start, for a value on the `00` to `FF` scale, as Beads' `Density`
/// knob: evenly and fastest at `00`, slowing to none in the middle, then at random, faster
/// towards `FF`. Each side spans 16 octaves.
pub fn grain_density(value: f32) -> Triggering {
    let density = value / 255.0;
    if density < 0.48 {
        Triggering::Regular(MOST_GRAIN_HERTZ * (-density * 16.0).exp2())
    } else if density > 0.52 {
        Triggering::Random(0.5 * MOST_GRAIN_HERTZ * (-(1.0 - density) * 16.0).exp2())
    } else {
        Triggering::Off
    }
}

/// What Warps can take as its modulator: its own oscillator, a track by its number, or a return,
/// as the compressor's `Source` but with the oscillator in place of `Self`.
pub const WARPS_SOURCE_NAMES: [&str; SOURCE_NAMES.len()] = {
    let mut names = SOURCE_NAMES;
    names[0] = "Osc";
    names
};

/// The waveforms Warps' oscillator plays, as Warps' own: a sine, a triangle, a saw, a square, and
/// noise low-passed at four times its pitch.
pub const WAVE_NAMES: [&str; 5] = ["Sine", "Tri", "Saw", "Square", "Noise"];

/// What Warps' `Algo` goes through from `00` to `FF`, as the status line names them: its six
/// cross-modulations, the modulator on its own as it fades over, and the vocoder.
pub const ALGORITHM_NAMES: [&str; 8] = [
    "Xfade", "Fold", "Ring", "Digital", "XOR", "Compare", "Mod", "Vocoder",
];

/// Where Warps' `Algo` starts fading from its cross-modulations to the bare modulator, where it's
/// only the modulator, and where it's only the vocoder, on its `0` to `1` scale.
pub const ALGORITHM_FADE_START: f32 = 0.675;
pub const ALGORITHM_MODULATOR: f32 = 0.7;
pub const ALGORITHM_VOCODER: f32 = 0.725;

/// Where Warps' algorithm knob is, from `0` to `1`, for `Algo` on the `00` to `FF` scale. It's
/// out of 256, so each cross-modulation, an eighth of the way from the last, lands on a value:
/// `00`, `20`, `40`, `60`, `80` and `A0`.
pub fn algorithm_knob(value: f32) -> f32 {
    (value / 256.0).clamp(0.0, 1.0)
}

/// Where Warps' `Algo` is, for a value on the `00` to `FF` scale, as the two of `ALGORITHM_NAMES`
/// it's between and how far it is from the first to the second. Each cross-modulation is an
/// eighth of the way from the last, as Warps' knob has them, then it fades through the modulator
/// into the vocoder.
pub fn algorithm_blend(value: f32) -> (usize, usize, f32) {
    let at = algorithm_knob(value);
    if at < 0.625 {
        let place = at * 8.0;
        let from = place as usize;
        (from, from + 1, place - from as f32)
    } else if at < ALGORITHM_MODULATOR {
        (5, 6, (at - 0.625) / (ALGORITHM_MODULATOR - 0.625))
    } else if at < ALGORITHM_VOCODER {
        (
            6,
            7,
            (at - ALGORITHM_MODULATOR) / (ALGORITHM_VOCODER - ALGORITHM_MODULATOR),
        )
    } else {
        (7, 7, 0.0)
    }
}

/// Warps', in the order the page shows them, as columns of two: the algorithm and its setting,
/// the modulator and how hard it's driven, then the oscillator's pitch and waveform.
pub(crate) const WARPS_PARAMS: [Param; 8] = [
    hex_param(
        "Algo",
        "algo",
        "ALGO",
        Setting::Algorithm,
        Warps::DEFAULT.algorithm,
    ),
    hex_param(
        "Timbre",
        "timbre",
        "TMBR",
        Setting::Timbre,
        Warps::DEFAULT.timbre,
    ),
    Param {
        name: "Source",
        key: "source",
        // What it hears is part of how it's set up, so steps don't change it.
        label: None,
        setting: Setting::Source,
        choices: &WARPS_SOURCE_NAMES,
        wraps: false,
        default: Warps::DEFAULT.source,
    },
    hex_param(
        "Level",
        "level",
        "LEVL",
        Setting::ModulatorLevel,
        Warps::DEFAULT.level,
    ),
    Param {
        default: Warps::DEFAULT.pitch,
        ..PITCH_PARAMS[0]
    },
    Param {
        name: "Shape",
        key: "shape",
        label: Some("SHAP"),
        setting: Setting::Wave,
        choices: &WAVE_NAMES,
        wraps: true,
        default: Warps::DEFAULT.shape,
    },
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// What a low-pass gate opens as what it hears gets louder, after Mutable Instruments' Streams.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateMode {
    /// Streams' vactrol: its level and its filter's cutoff together, opening quickly and closing
    /// slowly, as a low-pass gate does.
    #[default]
    Gate,
    /// Streams' follower on the cutoff alone, so it brightens as it gets louder: an envelope
    /// filter, or an auto-wah.
    Filter,
    /// Streams' follower on the level alone.
    Vca,
}

impl GateMode {
    pub const ALL: [Self; 3] = [Self::Gate, Self::Filter, Self::Vca];
}

const GATE_MODE_NAMES: [&str; 3] = ["Gate", "Filter", "VCA"];

/// A low-pass gate's, in the order the page shows them, as columns of two: what it opens and how
/// far, where it rests and its resonance, then how slowly it responds and what it hears.
pub(crate) const LPG_PARAMS: [Param; 8] = [
    Param {
        name: "Mode",
        key: "mode",
        // Choosing a mode chooses a sound, so steps don't lock it.
        label: None,
        setting: Setting::GateMode,
        choices: &GATE_MODE_NAMES,
        wraps: true,
        default: 0,
    },
    Param {
        default: LowPassGate::DEFAULT.amount,
        ..AMOUNT_PARAM
    },
    hex_param(
        "Base",
        "base",
        "BASE",
        Setting::Base,
        LowPassGate::DEFAULT.base,
    ),
    hex_param(
        "Res",
        "res",
        "RESO",
        Setting::Resonance,
        LowPassGate::DEFAULT.resonance,
    ),
    hex_param(
        "Response",
        "response",
        "RESP",
        Setting::Response,
        LowPassGate::DEFAULT.response,
    ),
    Param {
        name: "Source",
        key: "source",
        // What it hears is part of how it's set up, so steps don't change it.
        label: None,
        setting: Setting::Source,
        choices: &SOURCE_NAMES,
        wraps: false,
        default: LowPassGate::DEFAULT.source,
    },
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// A resonator's, in the order the page shows them, as columns of two: its pitch and how its
/// partials spread, how bright and how long it rings, then where it's struck and how wide it is.
pub(crate) const RESO_PARAMS: [Param; 8] = [
    Param {
        name: "Tune",
        key: "tune",
        label: Some("TUNE"),
        default: Resonator::DEFAULT.tune,
        ..PITCH_PARAMS[0]
    },
    hex_param(
        "Material",
        "material",
        "MATL",
        Setting::Material,
        Resonator::DEFAULT.material,
    ),
    hex_param(
        "Bright",
        "bright",
        "BRT",
        Setting::Bright,
        Resonator::DEFAULT.bright,
    ),
    hex_param(
        "Decay",
        "decay",
        "DCAY",
        Setting::Decay,
        Resonator::DEFAULT.decay,
    ),
    hex_param(
        "Position",
        "position",
        "POS",
        Setting::Pickup,
        Resonator::DEFAULT.position,
    ),
    hex_param(
        "Width",
        "width",
        "WDTH",
        Setting::Width,
        Resonator::DEFAULT.width,
    ),
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// What a comb does with what it delays, after Vital's comb filter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombMode {
    /// Feedback alone: the sound and its echoes, each a cycle of the note apart, so it rings.
    #[default]
    Comb,
    /// The sound with a delayed copy, which feedback sharpens: notches between the note's
    /// harmonics, and peaks on them.
    Flange,
}

impl CombMode {
    pub const ALL: [Self; 2] = [Self::Comb, Self::Flange];
}

const COMB_MODE_NAMES: [&str; 2] = ["Comb", "Flange"];

/// A comb's, in the order the page shows them, as columns of two: what it is and its pitch, how
/// long it rings and how bright, then how wide a band it keeps and the mix, then its gain.
pub(crate) const COMB_PARAMS: [Param; 7] = [
    Param {
        name: "Mode",
        key: "mode",
        // Choosing a mode chooses a sound, so steps don't lock it.
        label: None,
        setting: Setting::CombMode,
        choices: &COMB_MODE_NAMES,
        wraps: true,
        default: 0,
    },
    Param {
        name: "Tune",
        key: "tune",
        label: Some("TUNE"),
        default: Comb::DEFAULT.tune,
        ..PITCH_PARAMS[0]
    },
    hex_param(
        "Feedback",
        "feedback",
        "FDBK",
        Setting::FlangerFeedback,
        Comb::DEFAULT.feedback,
    ),
    hex_param("Tone", "tone", "TONE", Setting::Bright, Comb::DEFAULT.tone),
    hex_param(
        "Spread",
        "spread",
        "SPRD",
        Setting::Spread,
        Comb::DEFAULT.spread,
    ),
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// The vowels a vowel filter moves through, in order along its `Vowel`.
pub const VOWEL_NAMES: [&str; 5] = ["A", "E", "I", "O", "U"];

/// Where a vowel filter's `Vowel` or `Size` is among its five vowels or voices, for a value on the
/// `00` to `FF` scale: the one it's coming from, the next, and how far it is from one to the other.
/// They sit at `00`, `40`, `80`, `C0` and `FF`, so `A` is at `00`, `E` at `40`, `I` at `80`, `O` at
/// `C0` and `U` at `FF`.
pub fn vowel_blend(value: f32) -> (usize, usize, f32) {
    let value = value.clamp(0.0, 255.0);
    let from = ((value / 64.0) as usize).min(VOWEL_NAMES.len() - 2);
    // The last is a step shorter, as `FF` is one short of `100`.
    let width = if from == VOWEL_NAMES.len() - 2 {
        63.0
    } else {
        64.0
    };
    (from, from + 1, (value - from as f32 * 64.0) / width)
}

/// A vowel filter's, in the order the page shows them, as columns of two: which vowel and how big
/// a voice, how sharp its formants are and how far they follow the note, then the mix and gain.
pub(crate) const VOWEL_PARAMS: [Param; 6] = [
    hex_param(
        "Vowel",
        "vowel",
        "VOWL",
        Setting::Vowel,
        Vowel::DEFAULT.vowel,
    ),
    hex_param(
        "Size",
        "size",
        "SIZE",
        Setting::VoiceSize,
        Vowel::DEFAULT.size,
    ),
    hex_param(
        "Res",
        "res",
        "RESO",
        Setting::Resonance,
        Vowel::DEFAULT.resonance,
    ),
    hex_param(
        "Track",
        "track",
        "TRK",
        Setting::KeyTrack,
        Vowel::DEFAULT.track,
    ),
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// The bit depths `Crush` chooses between. Above these it can't be heard.
const LEAST_BITS: u8 = 2;
const MOST_BITS: u8 = 12;
const BIT_NAMES: [&str; (MOST_BITS - LEAST_BITS + 1) as usize] =
    ["2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"];

impl Insert {
    /// Every effect a slot can hold, as it starts, in the order `+` goes through them.
    pub const ALL: [Self; 19] = [
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
        }
    }

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Empty => "Nothing here",
            Self::Distort(_) => "Clips, folds or crushes the sound, or drives it as a pedal",
            Self::Delay(_) => "Repeats the sound, bouncing side to side",
            Self::Reverb(_) => "Puts the sound in a room",
            Self::Chorus(_) => "Thickens it with drifting copies, or an ensemble",
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
            Self::Empty => return None,
        };
        Some(match setting {
            Setting::Type => distort.kind as u8,
            Setting::Amount => distort.amount,
            Setting::Bits => distort.bits - LEAST_BITS,
            Setting::Rate => distort.rate,
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
            Self::Empty => return self,
        };
        let mut distort = distort;
        match setting {
            Setting::Type => distort.kind = DistortType::ALL[usize::from(whole)],
            Setting::Amount => distort.amount = whole,
            Setting::Bits => distort.bits = whole + LEAST_BITS,
            Setting::Rate => distort.rate = whole,
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
}

/// A delay's settings. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Delay {
    /// A place in the list of note divisions.
    pub time: u8,
    /// How much of each repeat comes round again. `FF` keeps repeating.
    pub feedback: u8,
    /// Whether it goes from side to side: `1`, or stays on the side it came in on: `0`.
    pub ping_pong: u8,
    /// From `00`, both sides together, to `FF`, as wide as they are.
    pub width: u8,
    /// A high-pass on what comes round again, which `00` turns off.
    pub low_cut: u8,
    /// A low-pass on what comes round again, which `FF` turns off.
    pub high_cut: u8,
    /// How hard what comes round again is pushed into a soft clip.
    pub drive: u8,
    /// How it goes to a new time: `0` glides there, and `1` jumps, crossfading.
    #[serde(skip_serializing_if = "is_zero")]
    pub retime: u8,
    /// How far a slow wobble bends the repeats' pitch, the sides opposite ways, as tape's wow does.
    #[serde(skip_serializing_if = "is_zero")]
    pub wow: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Delay {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Delay {
    /// As it starts: bouncing at a dotted eighth, with a little feedback.
    pub const DEFAULT: Self = Self {
        time: 11,
        feedback: 0x66,
        ping_pong: 1,
        width: 0xFF,
        low_cut: 0,
        high_cut: 0xFF,
        drive: 0,
        retime: 0,
        wow: 0,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Time => self.time,
            Setting::Feedback => self.feedback,
            Setting::PingPong => self.ping_pong,
            Setting::Width => self.width,
            Setting::LowCut => self.low_cut,
            Setting::HighCut => self.high_cut,
            Setting::Drive => self.drive,
            Setting::Retime => self.retime,
            Setting::Wow => self.wow,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut delay = self;
        match setting {
            Setting::Time => delay.time = value,
            Setting::Feedback => delay.feedback = value,
            Setting::PingPong => delay.ping_pong = value,
            Setting::Width => delay.width = value,
            Setting::LowCut => delay.low_cut = value,
            Setting::HighCut => delay.high_cut = value,
            Setting::Drive => delay.drive = value,
            Setting::Retime => delay.retime = value,
            Setting::Wow => delay.wow = value,
            Setting::Mix => delay.mix = value,
            Setting::Gain => delay.gain = value,
            _ => {}
        }
        delay
    }
}

/// A reverb's settings. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reverb {
    /// How long before anything comes out, up to a quarter of a second.
    pub pre: u8,
    /// How long it rings.
    pub decay: u8,
    /// How quickly its highs fall away as it rings.
    pub damp: u8,
    /// A high-pass on what goes in, which `00` turns off.
    pub low_cut: u8,
    /// A low-pass on what goes in, which `FF` turns off.
    pub high_cut: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Reverb {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Reverb {
    /// As it starts.
    pub const DEFAULT: Self = Self {
        pre: 0,
        decay: 0x50,
        damp: 0x55,
        low_cut: 0,
        high_cut: 0xFF,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Pre => self.pre,
            Setting::Decay => self.decay,
            Setting::Damp => self.damp,
            Setting::LowCut => self.low_cut,
            Setting::HighCut => self.high_cut,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut reverb = self;
        match setting {
            Setting::Pre => reverb.pre = value,
            Setting::Decay => reverb.decay = value,
            Setting::Damp => reverb.damp = value,
            Setting::LowCut => reverb.low_cut = value,
            Setting::HighCut => reverb.high_cut = value,
            Setting::Mix => reverb.mix = value,
            Setting::Gain => reverb.gain = value,
            _ => {}
        }
        reverb
    }
}

/// A chorus's settings. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chorus {
    pub kind: ChorusType,
    /// How fast the copies move: as Rings has it at `80`, two octaves slower at `00` and faster at
    /// `FF`.
    pub rate: u8,
    /// How far they move.
    pub depth: u8,
    /// From `00`, all dry, to `FF`, all wet. Half is the classic chorus.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Chorus {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Chorus {
    /// As it starts.
    pub const DEFAULT: Self = Self {
        kind: ChorusType::Chorus,
        rate: 0x80,
        depth: 0x80,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::ChorusType => self.kind as u8,
            Setting::Rate => self.rate,
            Setting::Depth => self.depth,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut chorus = self;
        match setting {
            Setting::ChorusType => {
                chorus.kind = ChorusType::ALL[usize::from(value).min(ChorusType::ALL.len() - 1)];
            }
            Setting::Rate => chorus.rate = value,
            Setting::Depth => chorus.depth = value,
            Setting::Mix => chorus.mix = value,
            Setting::Gain => chorus.gain = value,
            _ => {}
        }
        chorus
    }
}

/// An EQ's settings. Each band's boost or cut is centred on `80`, where it leaves the sound as it
/// is. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Equaliser {
    /// Where each band sits, from 20 Hz at `00` to 20 kHz at `FF`, evenly in pitch, as the cutoff.
    pub low_frequency: u8,
    pub low: u8,
    pub mid_frequency: u8,
    pub mid: u8,
    /// How narrow the middle band is.
    pub mid_q: u8,
    pub high_frequency: u8,
    pub high: u8,
    pub gain: u8,
}

impl Default for Equaliser {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Equaliser {
    /// As it starts: flat, with the shelves at about 100 Hz and 8 kHz, and the middle at 1 kHz.
    pub const DEFAULT: Self = Self {
        low_frequency: 0x3B,
        low: 0x80,
        mid_frequency: 0x90,
        mid: 0x80,
        mid_q: 0x40,
        high_frequency: 0xDD,
        high: 0x80,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::LowFrequency => self.low_frequency,
            Setting::Low => self.low,
            Setting::MidFrequency => self.mid_frequency,
            Setting::Mid => self.mid,
            Setting::MidQ => self.mid_q,
            Setting::HighFrequency => self.high_frequency,
            Setting::High => self.high,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut eq = self;
        match setting {
            Setting::LowFrequency => eq.low_frequency = value,
            Setting::Low => eq.low = value,
            Setting::MidFrequency => eq.mid_frequency = value,
            Setting::Mid => eq.mid = value,
            Setting::MidQ => eq.mid_q = value,
            Setting::HighFrequency => eq.high_frequency = value,
            Setting::High => eq.high = value,
            Setting::Gain => eq.gain = value,
            _ => {}
        }
        eq
    }
}

/// A compressor's settings. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Compressor {
    /// Its character. Each type keeps its own settings, so changing type and back finds them as
    /// they were; those that mean the same in each are shared.
    pub kind: CompressorType,
    pub threshold: u8,
    /// `Clean`'s ratio, attack and release, on the `00` to `FF` scale.
    pub ratio: u8,
    pub attack: u8,
    pub release: u8,
    /// `Console`'s ratio, attack and release, each a place among its switch's.
    pub console_ratio: u8,
    pub console_attack: u8,
    pub console_release: u8,
    /// `Glue`'s, the same, and the most it turns down, from 0 dB at `00` to 70 dB at `FF`, as
    /// [`range_db`] has it.
    pub glue_ratio: u8,
    pub glue_attack: u8,
    pub glue_release: u8,
    pub range: u8,
    /// `1` for `Glue` to soft clip what comes out, after its makeup and its mix, to -0.5 dBFS.
    pub peak_clip: u8,
    /// What it hears: `0` for its own input, a track's number, or a return after the tracks.
    pub source: u8,
    /// `1` to make up by itself how much quieter it comes out than it went in, each measured over
    /// about a second, so it comes out as loud as it went in. Off for ducking, which shouldn't be louder unducked.
    pub auto: u8,
    /// From `00`, all dry, to `FF`, all compressed, for compressing in parallel.
    pub mix: u8,
    /// How much it turns up what it's turned down, before the mix and on top of `Auto`: from
    /// 0 dB at `00` to +24 dB at `FF`, as [`makeup_db`] has it.
    pub makeup: u8,
}

impl Default for Compressor {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Compressor {
    /// As it starts: `Clean`, at -15 dB, about 3.7:1, a quick attack and a 100 ms release,
    /// hearing itself. `Console` starts at 4:1 with a 10 ms attack and `Auto` release, the G-bus's
    /// usual place on a drum bus, and `Glue` where Live loads it: 4:1, a 1 ms attack, a 0.6 s
    /// release, all of its range and no clip.
    pub const DEFAULT: Self = Self {
        kind: CompressorType::Clean,
        threshold: 0xC0,
        ratio: 0x60,
        attack: 0x55,
        release: 0x80,
        console_ratio: 1,
        console_attack: 4,
        console_release: CONSOLE_AUTO_RELEASE,
        glue_ratio: 1,
        glue_attack: 3,
        glue_release: 3,
        range: 0xFF,
        peak_clip: 0,
        source: 0,
        auto: 0,
        mix: 0xFF,
        makeup: 0,
    };

    /// The track it hears, by its place among the tracks, if not its own input. The returns come
    /// after the sound tracks, at `sounds` and the one after.
    pub fn source_track(self, sounds: usize) -> Option<usize> {
        source_track(self.source, sounds)
    }

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::CompressorType => self.kind as u8,
            Setting::Threshold => self.threshold,
            Setting::Ratio => self.ratio,
            Setting::Attack => self.attack,
            Setting::Release => self.release,
            Setting::ConsoleRatio => self.console_ratio,
            Setting::ConsoleAttack => self.console_attack,
            Setting::ConsoleRelease => self.console_release,
            Setting::GlueRatio => self.glue_ratio,
            Setting::GlueAttack => self.glue_attack,
            Setting::GlueRelease => self.glue_release,
            Setting::Range => self.range,
            Setting::PeakClip => self.peak_clip,
            Setting::Source => self.source,
            Setting::AutoGain => self.auto,
            Setting::Mix => self.mix,
            Setting::Makeup => self.makeup,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut compressor = self;
        match setting {
            Setting::CompressorType => compressor.kind = CompressorType::ALL[usize::from(value)],
            Setting::Threshold => compressor.threshold = value,
            Setting::Ratio => compressor.ratio = value,
            Setting::Attack => compressor.attack = value,
            Setting::Release => compressor.release = value,
            Setting::ConsoleRatio => compressor.console_ratio = value,
            Setting::ConsoleAttack => compressor.console_attack = value,
            Setting::ConsoleRelease => compressor.console_release = value,
            Setting::GlueRatio => compressor.glue_ratio = value,
            Setting::GlueAttack => compressor.glue_attack = value,
            Setting::GlueRelease => compressor.glue_release = value,
            Setting::Range => compressor.range = value,
            Setting::PeakClip => compressor.peak_clip = value,
            Setting::Source => compressor.source = value,
            Setting::AutoGain => compressor.auto = value,
            Setting::Mix => compressor.mix = value,
            Setting::Makeup => compressor.makeup = value,
            _ => {}
        }
        compressor
    }
}

/// A look-ahead limiter's settings. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Limiter {
    /// How much it turns up what goes in, from 0 dB at `00` to +24 dB at `FF`, to push it harder.
    pub input: u8,
    /// The most that comes out, from -24 dB at `00` to 0 dB at `FF`.
    pub ceiling: u8,
    /// How fast it lets go once a peak has passed.
    pub release: u8,
    /// From `00`, all dry, to `FF`, all limited. The dry sound is as late as the limited one.
    pub mix: u8,
    /// What comes out, on the level scale.
    pub gain: u8,
}

impl Default for Limiter {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Limiter {
    /// As it starts: nothing added going in, a ceiling just under 0 dB at about -0.3 dB, and a
    /// 100 ms release.
    pub const DEFAULT: Self = Self {
        input: 0x00,
        ceiling: 0xFC,
        release: 0xAA,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Input => self.input,
            Setting::Ceiling => self.ceiling,
            Setting::LimitRelease => self.release,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut limiter = self;
        match setting {
            Setting::Input => limiter.input = value,
            Setting::Ceiling => limiter.ceiling = value,
            Setting::LimitRelease => limiter.release = value,
            Setting::Mix => limiter.mix = value,
            Setting::Gain => limiter.gain = value,
            _ => {}
        }
        limiter
    }
}

/// A phaser's settings, after Vital's. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Phaser {
    /// How long a sweep takes, as a place in the list of sweeps.
    pub rate: u8,
    /// How far it sweeps either way of its centre: up to 48 semitones at `FF`.
    pub depth: u8,
    /// The frequency it sweeps around, from 20 Hz to 20 kHz as the filter's cutoff.
    pub centre: u8,
    /// How much of what comes out goes round again, which sharpens the notches into peaks.
    pub feedback: u8,
    /// How many all-passes, as a place among 4, 8 and 12.
    pub stages: u8,
    /// From `00`, all dry, to `FF`, all through the all-passes. Half has the deepest notches.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Phaser {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Phaser {
    /// As it starts: a sweep every 2 bars, 24 semitones either way of about 640 Hz, half
    /// feedback and 8 stages, as Vital's starts, and half wet.
    pub const DEFAULT: Self = Self {
        rate: 18,
        depth: 0x80,
        centre: 0x80,
        feedback: 0x80,
        stages: 1,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::SweepRate => self.rate,
            Setting::Depth => self.depth,
            Setting::Centre => self.centre,
            Setting::Feedback => self.feedback,
            Setting::Stages => self.stages,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut phaser = self;
        match setting {
            Setting::SweepRate => phaser.rate = value,
            Setting::Depth => phaser.depth = value,
            Setting::Centre => phaser.centre = value,
            Setting::Feedback => phaser.feedback = value,
            Setting::Stages => phaser.stages = value,
            Setting::Mix => phaser.mix = value,
            Setting::Gain => phaser.gain = value,
            _ => {}
        }
        phaser
    }
}

/// A flanger's settings, after Vital's. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flanger {
    /// How long a sweep takes, as a place in the list of sweeps.
    pub rate: u8,
    /// How far the delay sweeps: at `FF`, from 0.5 ms to twice the middle and back.
    pub depth: u8,
    /// The delay at the middle of the sweep, from 0.5 ms to 10 ms.
    pub delay: u8,
    /// How much of the delay goes round again, from all of it upside down at `00`, through none at
    /// `80`, to all of it at `FF`. It never quite reaches all, so it can't run away.
    pub feedback: u8,
    /// From `00`, all dry, to `FF`, all delayed. Half has the deepest comb.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Flanger {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Flanger {
    /// As it starts: a sweep every bar, half depth around about 2.2 ms, and half feedback, as
    /// Vital's starts, and half wet.
    pub const DEFAULT: Self = Self {
        rate: 17,
        depth: 0x80,
        delay: 0x80,
        feedback: 0xC0,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::SweepRate => self.rate,
            Setting::Depth => self.depth,
            Setting::FlangerDelay => self.delay,
            Setting::FlangerFeedback => self.feedback,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut flanger = self;
        match setting {
            Setting::SweepRate => flanger.rate = value,
            Setting::Depth => flanger.depth = value,
            Setting::FlangerDelay => flanger.delay = value,
            Setting::FlangerFeedback => flanger.feedback = value,
            Setting::Mix => flanger.mix = value,
            Setting::Gain => flanger.gain = value,
            _ => {}
        }
        flanger
    }
}

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
    fn read(self, setting: Setting) -> u8 {
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
    fn written(self, setting: Setting, value: u8) -> Self {
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

/// A plate's settings, after Dattorro's. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plate {
    /// How long before anything comes out, up to a quarter of a second.
    pub pre: u8,
    /// How long it rings: from barely at all to about a minute, never for ever.
    pub decay: u8,
    /// How quickly its highs fall away as it rings.
    pub damp: u8,
    /// A low-pass on what goes in, which `FF` turns off.
    pub high_cut: u8,
    /// How big the plate is, from half to twice Dattorro's, evenly in ratio, with his at `80`.
    pub size: u8,
    /// How far its tank's all-passes wobble, which keeps the tail from ringing metallic. Dattorro's
    /// is at `80`.
    pub excursion: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Plate {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Plate {
    /// As it starts: Dattorro's decay, size and wobble, a little damping, and half wet.
    pub const DEFAULT: Self = Self {
        pre: 0,
        decay: 0x80,
        damp: 0x20,
        high_cut: 0xFF,
        size: 0x80,
        excursion: 0x80,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Pre => self.pre,
            Setting::Decay => self.decay,
            Setting::Damp => self.damp,
            Setting::HighCut => self.high_cut,
            Setting::Size => self.size,
            Setting::Excursion => self.excursion,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut plate = self;
        match setting {
            Setting::Pre => plate.pre = value,
            Setting::Decay => plate.decay = value,
            Setting::Damp => plate.damp = value,
            Setting::HighCut => plate.high_cut = value,
            Setting::Size => plate.size = value,
            Setting::Excursion => plate.excursion = value,
            Setting::Mix => plate.mix = value,
            Setting::Gain => plate.gain = value,
            _ => {}
        }
        plate
    }
}

/// How many times Dattorro's a plate's size is, for a value on the `00` to `FF` scale: from half
/// at `00`, through his own at `80`, to twice at `FF`, evenly in ratio.
pub fn plate_size(value: f32) -> f32 {
    from_middle(value).exp2()
}

/// A pitch shifter's settings, after Clouds'. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pitch {
    /// How far it shifts, as a place in the list from two octaves down to two up, none in the
    /// middle.
    pub shift: u8,
    /// How far it shifts besides, from 50 cents down to 50 up, none at `80`.
    pub fine: u8,
    /// How long each of its two grains is, from 10 ms to 100 ms.
    pub size: u8,
    /// How much of what comes out goes back in to be shifted again.
    pub feedback: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Pitch {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Pitch {
    /// As it starts: no shift, Clouds' grain of about 64 ms, no feedback, and all wet.
    pub const DEFAULT: Self = Self {
        shift: MOST_SHIFT,
        fine: 0x80,
        size: 0xCE,
        feedback: 0,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Shift => self.shift,
            Setting::Fine => self.fine,
            Setting::Grain => self.size,
            Setting::Feedback => self.feedback,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut pitch = self;
        match setting {
            Setting::Shift => pitch.shift = value,
            Setting::Fine => pitch.fine = value,
            Setting::Grain => pitch.size = value,
            Setting::Feedback => pitch.feedback = value,
            Setting::Mix => pitch.mix = value,
            Setting::Gain => pitch.gain = value,
            _ => {}
        }
        pitch
    }
}

/// A granular effect's settings, after Beads'. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grains {
    /// How far back the grains start reading, from just behind what's being recorded to four
    /// seconds back.
    pub time: u8,
    /// How long each grain is, and below a third whether it plays backwards, as Beads' `Size`.
    pub size: u8,
    /// How far each grain is shifted, as a place in the list from two octaves down to two up.
    pub pitch: u8,
    /// Each grain's envelope, as Beads' `Shape`: from a square, through a decay and a bell, to a
    /// reversed decay.
    pub shape: u8,
    /// How often grains start, as Beads' `Density`: evenly below `80`, at random above it.
    pub density: u8,
    /// How far the grains spread across the sides.
    pub spread: u8,
    /// Whether it stops recording, `1`, so the grains keep playing what's held.
    pub freeze: u8,
    /// How far each grain's time, size and shape stray at random from what's set.
    pub random: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Grains {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Grains {
    /// As it starts: half a second back, grains of about 150 ms in a bell, no shift, about seven
    /// a second at random, spread half way, recording, nothing straying, and half wet.
    pub const DEFAULT: Self = Self {
        time: 0x20,
        size: 0xA8,
        pitch: MOST_SHIFT,
        shape: 0xAA,
        density: 0xCC,
        spread: 0x80,
        freeze: 0,
        random: 0,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Position => self.time,
            Setting::Length => self.size,
            Setting::Shift => self.pitch,
            Setting::Shape => self.shape,
            Setting::Density => self.density,
            Setting::Spread => self.spread,
            Setting::Freeze => self.freeze,
            Setting::Random => self.random,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut grains = self;
        match setting {
            Setting::Position => grains.time = value,
            Setting::Length => grains.size = value,
            Setting::Shift => grains.pitch = value,
            Setting::Shape => grains.shape = value,
            Setting::Density => grains.density = value,
            Setting::Spread => grains.spread = value,
            Setting::Freeze => grains.freeze = value,
            Setting::Random => grains.random = value,
            Setting::Mix => grains.mix = value,
            Setting::Gain => grains.gain = value,
            _ => {}
        }
        grains
    }
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

/// Warps' settings, after Mutable Instruments' Warps. The slot's own sound is the carrier, Warps'
/// first input. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Warps {
    /// Warps' algorithm knob, from crossfading, through folding, ring modulation, XOR and the
    /// comparator, to the vocoder, each blending into the next.
    pub algorithm: u8,
    /// Warps' timbre knob: what each algorithm does with it, from how far it crossfades to the
    /// vocoder's formant shift.
    pub timbre: u8,
    /// The modulator, Warps' second input: `0` for its own oscillator, a track's number, or a
    /// return after the tracks.
    pub source: u8,
    /// How hard the modulator is driven, Warps' second level knob.
    pub level: u8,
    /// How far the oscillator is from the track's note, as a place in the list from two octaves
    /// down to two up.
    pub pitch: u8,
    /// The oscillator's waveform, as a place among `WAVE_NAMES`.
    pub shape: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Warps {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Warps {
    /// As it starts: the digital ring modulation of the sound by a sine an octave above its note,
    /// driven a little over unity, all wet.
    pub const DEFAULT: Self = Self {
        algorithm: 0x60,
        timbre: 0x80,
        source: 0,
        level: 0xA0,
        pitch: MOST_SHIFT + 12,
        shape: 0,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    /// The track it takes as its modulator, by its place among the tracks, if not its own
    /// oscillator.
    pub fn source_track(self, sounds: usize) -> Option<usize> {
        source_track(self.source, sounds)
    }

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Algorithm => self.algorithm,
            Setting::Timbre => self.timbre,
            Setting::Source => self.source,
            Setting::ModulatorLevel => self.level,
            Setting::Shift => self.pitch,
            Setting::Wave => self.shape,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut warps = self;
        match setting {
            Setting::Algorithm => warps.algorithm = value,
            Setting::Timbre => warps.timbre = value,
            Setting::Source => warps.source = value,
            Setting::ModulatorLevel => warps.level = value,
            Setting::Shift => warps.pitch = value,
            Setting::Wave => warps.shape = value,
            Setting::Mix => warps.mix = value,
            Setting::Gain => warps.gain = value,
            _ => {}
        }
        warps
    }
}

/// A low-pass gate's settings, after Mutable Instruments' Streams. One a file doesn't have starts
/// as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LowPassGate {
    pub mode: GateMode,
    /// How far what it hears opens it, from where it rests to all the way.
    pub amount: u8,
    /// Where it rests when it's closed: its filter's cutoff, or its level as a VCA.
    pub base: u8,
    /// Its filter's resonance.
    pub resonance: u8,
    /// Streams' shape knob: how slowly it opens and closes. As a gate, the vactrol's decay grows
    /// to its longest at `80`, then its attack slows as well. As a follower, both lengthen.
    pub response: u8,
    /// What it hears: `0` for its own input, a track's number, or a return after the tracks.
    pub source: u8,
    /// From `00`, all dry, to `FF`, all through the gate.
    pub mix: u8,
    pub gain: u8,
}

impl Default for LowPassGate {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl LowPassGate {
    /// As it starts: a gate on its own sound, opening most of the way. Let go, its cutoff falls at
    /// once while its level holds a moment, then fades out over about a second, as a vactrol's
    /// does.
    pub const DEFAULT: Self = Self {
        mode: GateMode::Gate,
        amount: 0xC0,
        base: 0x00,
        resonance: 0x00,
        response: 0x10,
        source: 0,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    /// The track it hears, by its place among the tracks, if not its own input.
    pub fn source_track(self, sounds: usize) -> Option<usize> {
        source_track(self.source, sounds)
    }

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::GateMode => self.mode as u8,
            Setting::Amount => self.amount,
            Setting::Base => self.base,
            Setting::Resonance => self.resonance,
            Setting::Response => self.response,
            Setting::Source => self.source,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut gate = self;
        match setting {
            Setting::GateMode => gate.mode = GateMode::ALL[usize::from(value)],
            Setting::Amount => gate.amount = value,
            Setting::Base => gate.base = value,
            Setting::Resonance => gate.resonance = value,
            Setting::Response => gate.response = value,
            Setting::Source => gate.source = value,
            Setting::Mix => gate.mix = value,
            Setting::Gain => gate.gain = value,
            _ => {}
        }
        gate
    }
}

/// A resonator's settings: a bank of modes tuned to the track's note, after Mutable Instruments'
/// Plaits, Rings and Elements, which the slot's sound strikes. One a file doesn't have starts as
/// new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Resonator {
    /// How far its pitch is from the track's note, as a place in the list from two octaves down to
    /// two up.
    pub tune: u8,
    /// Plaits' structure: its partials bunched low, in tune from `40`, then spread wider, as a
    /// bell's are.
    pub material: u8,
    /// How long its high partials ring against its lowest.
    pub bright: u8,
    /// How long it rings, Plaits' damping.
    pub decay: u8,
    /// Where it's struck and heard, from its end at `00` to its middle at `FF`.
    pub position: u8,
    /// How far a second pickup, wandering slowly along it, spreads it across the sides, as
    /// Elements' does.
    pub width: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Resonator {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Resonator {
    /// As it starts: in tune with the note, its partials whole multiples of it, ringing for about
    /// a third of a second at middle C, struck near its end as Plaits strikes its own, a little
    /// wide, and all wet.
    pub const DEFAULT: Self = Self {
        tune: MOST_SHIFT,
        material: 0x40,
        bright: 0x80,
        decay: 0x80,
        position: 0x08,
        width: 0x80,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Shift => self.tune,
            Setting::Material => self.material,
            Setting::Bright => self.bright,
            Setting::Decay => self.decay,
            Setting::Pickup => self.position,
            Setting::Width => self.width,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut resonator = self;
        match setting {
            Setting::Shift => resonator.tune = value,
            Setting::Material => resonator.material = value,
            Setting::Bright => resonator.bright = value,
            Setting::Decay => resonator.decay = value,
            Setting::Pickup => resonator.position = value,
            Setting::Width => resonator.width = value,
            Setting::Mix => resonator.mix = value,
            Setting::Gain => resonator.gain = value,
            _ => {}
        }
        resonator
    }
}

/// A comb's settings: a delay tuned to the track's note, fed back through a filter, after Vital's
/// comb filter. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Comb {
    pub mode: CombMode,
    /// How far its pitch is from the track's note, as a place in the list from two octaves down to
    /// two up.
    pub tune: u8,
    /// How long it rings, from none at `80`: above, its peaks sit on all the note's harmonics, and
    /// below, on its odd ones, for a hollower ring at the same pitch.
    pub feedback: u8,
    /// How bright what goes round is: its low-pass, from the note at `00` to everything at `FF`.
    pub tone: u8,
    /// How far down from `Tone` what goes round reaches, from a narrow band at `00` to everything
    /// under it at `FF`.
    pub spread: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Comb {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Comb {
    /// As it starts: a comb in tune with the note, ringing for about half a second at middle C,
    /// losing what's over four octaves up as it rings, and all wet.
    pub const DEFAULT: Self = Self {
        mode: CombMode::Comb,
        tune: MOST_SHIFT,
        feedback: 0xF0,
        tone: 0x80,
        spread: 0xFF,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::CombMode => self.mode as u8,
            Setting::Shift => self.tune,
            Setting::FlangerFeedback => self.feedback,
            Setting::Bright => self.tone,
            Setting::Spread => self.spread,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut comb = self;
        match setting {
            Setting::CombMode => comb.mode = CombMode::ALL[usize::from(value)],
            Setting::Shift => comb.tune = value,
            Setting::FlangerFeedback => comb.feedback = value,
            Setting::Bright => comb.tone = value,
            Setting::Spread => comb.spread = value,
            Setting::Mix => comb.mix = value,
            Setting::Gain => comb.gain = value,
            _ => {}
        }
        comb
    }
}

/// A vowel filter's settings: a voice's formants, fixed in pitch as a vowel's are, morphing from
/// one vowel to the next, after Plaits' speech synth and Vital's formant filter. One a file
/// doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Vowel {
    /// Which vowel, from `A` at `00` through `E`, `I` and `O` to `U` at `FF`, blending between.
    pub vowel: u8,
    /// How big a voice, from a child's at `00` to a deep one at `FF`: its formants lower as it
    /// grows.
    pub size: u8,
    /// How sharp its formants are, from broad at `00` to ringing at `FF`.
    pub resonance: u8,
    /// How far its formants follow the track's note from middle C, from not at all at `00` to as
    /// far as the note moves at `FF`.
    pub track: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Vowel {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Vowel {
    /// As it starts: an `A` in a voice of middle size, its formants fixed, and all wet.
    pub const DEFAULT: Self = Self {
        vowel: 0x00,
        size: 0x80,
        resonance: 0x80,
        track: 0x00,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Vowel => self.vowel,
            Setting::VoiceSize => self.size,
            Setting::Resonance => self.resonance,
            Setting::KeyTrack => self.track,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut vowel = self;
        match setting {
            Setting::Vowel => vowel.vowel = value,
            Setting::VoiceSize => vowel.size = value,
            Setting::Resonance => vowel.resonance = value,
            Setting::KeyTrack => vowel.track = value,
            Setting::Mix => vowel.mix = value,
            Setting::Gain => vowel.gain = value,
            _ => {}
        }
        vowel
    }
}

fn unity_gain() -> u8 {
    UNITY_LEVEL
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(value: &u8) -> bool {
    *value == 0
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
    };
}

impl Default for Distort {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GRID_COLUMNS;

    #[test]
    fn every_effect_starts_at_its_declared_defaults_and_fits() {
        for effect in Insert::ALL {
            assert!(effect.params().len() <= MAX_PARAMS, "{effect:?}");
            for (index, param) in effect.params().iter().enumerate() {
                assert_eq!(effect.value(index), param.default, "{}", param.name);
            }
        }
        assert_eq!(Insert::ALL[1], Insert::Distort(Distort::default()));
        for effect in crate::settings::Effect::ALL {
            assert!(effect.params().len() <= MAX_KEPT_PARAMS, "{effect:?}");
        }
    }

    /// Every effect in each of its types and modes, as it starts.
    fn every_kind() -> Vec<Insert> {
        Insert::ALL
            .into_iter()
            .flat_map(|effect| match effect.type_param() {
                Some(at) => (0..effect.params()[at].choices.len())
                    .map(|kind| effect.with_value(at, kind as u8))
                    .collect(),
                None => vec![effect],
            })
            .collect()
    }

    #[test]
    fn the_settings_that_are_off_or_on_are_toggles_and_no_others() {
        let mut toggles: Vec<&str> = every_kind()
            .iter()
            .flat_map(|effect| effect.params().iter())
            .filter(|param| param.is_toggle())
            .map(|param| param.name)
            .collect();
        toggles.sort_unstable();
        toggles.dedup();
        assert_eq!(toggles, ["Auto", "Freeze", "Peak Clip", "Ping-pong"]);
        // A two-way list of anything else isn't one.
        let chorus = Insert::ALL
            .into_iter()
            .find(|effect| effect.name() == "Chorus")
            .unwrap();
        assert!(!chorus.params()[chorus.type_param().unwrap()].is_toggle());
    }

    #[test]
    fn every_effects_grid_places_each_of_its_settings_once_with_what_comes_out_last() {
        for effect in every_kind()
            .into_iter()
            .filter(|effect| *effect != Insert::Empty)
        {
            let grid = effect.grid();
            let params = effect.params();
            assert!(
                grid.places_each_once(params.len(), effect.type_param()),
                "{effect:?}: {grid:?}"
            );
            // The column of what comes out is the last one used.
            let used = (0..GRID_COLUMNS)
                .filter(|column| (0..GRID_ROWS).any(|row| grid.at(row, *column).is_some()))
                .collect::<Vec<_>>();
            let last = *used.last().unwrap_or(&0);
            assert_eq!(
                used,
                (0..=last).collect::<Vec<_>>(),
                "{effect:?}: no empty column"
            );
            assert!(
                (0..GRID_ROWS)
                    .filter_map(|row| grid.at(row, last))
                    .any(|at| matches!(params[at].setting, Setting::Mix | Setting::Gain)),
                "{effect:?}: {grid:?}"
            );
            // A name has its column's ten characters and a space, and a choice the value's and
            // the bar's fourteen.
            for at in grid.settings() {
                assert!(params[at].name.len() < 10, "{}", params[at].name);
                for choice in params[at].choices {
                    assert!(choice.chars().count() <= 14, "{choice}");
                }
            }
        }
        assert_eq!(Insert::Empty.grid(), Grid::default());
    }

    #[test]
    fn a_few_effects_place_their_settings_by_what_they_mean() {
        let names = |effect: Insert, row: usize| {
            effect
                .grid()
                .row(row)
                .map(|at| at.map_or("", |at| effect.params()[at].name))
        };
        let glue = compressor(CompressorType::Glue);
        assert_eq!(names(glue, 0), ["Threshold", "Attack", "Makeup", "Mix"]);
        assert_eq!(names(glue, 1), ["Ratio", "Release", "Auto", "Peak Clip"]);
        assert_eq!(names(glue, 2), ["Range", "Source", "", ""]);
        assert_eq!(glue.type_param(), Some(0));
        let clean = compressor(CompressorType::Clean);
        assert_eq!(names(clean, 2), ["", "Source", "", ""]);
        let eq = Insert::Eq(Equaliser::DEFAULT);
        assert_eq!(names(eq, 0), ["Low freq", "Mid freq", "High freq", "Gain"]);
        assert_eq!(names(eq, 2), ["", "Mid Q", "", ""]);
        assert_eq!(eq.type_param(), None);
        let delay = Insert::Delay(Delay::DEFAULT);
        assert_eq!(names(delay, 2), ["Ping-pong", "Drive", "", "Gain"]);
        // The rest go in order, with what comes out after.
        let chorus = Insert::Chorus(Chorus::DEFAULT);
        assert_eq!(names(chorus, 0), ["Rate", "Mix", "", ""]);
        assert_eq!(names(chorus, 1), ["Depth", "Gain", "", ""]);
        let reverb = Insert::Reverb(Reverb::DEFAULT);
        assert_eq!(names(reverb, 0), ["Pre", "Low cut", "Mix", ""]);
        assert_eq!(names(reverb, 2), ["Damp", "", "", ""]);
    }

    /// A compressor of a type, as it starts.
    fn compressor(kind: CompressorType) -> Insert {
        Insert::Compressor(Compressor {
            kind,
            ..Compressor::DEFAULT
        })
    }

    #[test]
    fn a_compressor_starts_clean_and_each_type_shows_only_its_own() {
        assert_eq!(Compressor::default().kind, CompressorType::Clean);
        let names = |kind| {
            compressor(kind)
                .params()
                .iter()
                .map(|param| param.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(CompressorType::Clean),
            [
                "Type",
                "Source",
                "Threshold",
                "Ratio",
                "Attack",
                "Release",
                "Auto",
                "Mix",
                "Makeup"
            ]
        );
        assert_eq!(
            names(CompressorType::Console),
            [
                "Type",
                "Source",
                "Threshold",
                "Ratio",
                "Attack",
                "Release",
                "Auto",
                "Mix",
                "Makeup"
            ]
        );
        // The console's ratio, attack and release are its switches', with `Auto` after the times.
        let console = compressor(CompressorType::Console);
        let choices = |at: usize| console.params()[at].choices;
        assert_eq!(choices(3), ["2:1", "4:1", "10:1"]);
        assert_eq!(
            choices(4),
            ["0.1 ms", "0.3 ms", "1 ms", "3 ms", "10 ms", "30 ms"]
        );
        assert_eq!(choices(5), ["0.1 s", "0.3 s", "0.6 s", "1.2 s", "Auto"]);
        assert_eq!(console.value(3), 1, "4:1");
        assert_eq!(console.value(5), CONSOLE_AUTO_RELEASE);
        // Glue's are Live's, and it has its range, and its clip after everything else.
        assert_eq!(
            names(CompressorType::Glue),
            [
                "Type",
                "Source",
                "Threshold",
                "Ratio",
                "Attack",
                "Release",
                "Auto",
                "Mix",
                "Range",
                "Makeup",
                "Peak Clip"
            ]
        );
        let glue = compressor(CompressorType::Glue);
        let choices = |at: usize| glue.params()[at].choices;
        assert_eq!(choices(3), ["2:1", "4:1", "10:1"]);
        assert_eq!(
            choices(4),
            [
                "0.01 ms", "0.1 ms", "0.3 ms", "1 ms", "3 ms", "10 ms", "30 ms"
            ]
        );
        assert_eq!(
            choices(5),
            ["0.1 s", "0.2 s", "0.4 s", "0.6 s", "0.8 s", "1.2 s", "Auto"]
        );
        assert_eq!(GLUE_AUTO_RELEASE, 6);
        // It starts where Live loads it: 4:1, 1 ms, `.6`, all 70 dB of its range and no clip.
        assert_eq!(
            (
                glue.value(3),
                glue.value(4),
                glue.value(5),
                glue.value(8),
                glue.value(10)
            ),
            (1, 3, 3, 0xFF, 0)
        );
        let clip = glue.params()[10];
        assert_eq!(
            (clip.key, clip.label, clip.choices),
            ("peakclip", Some("PCLP"), &["Off", "On"][..])
        );
        assert_eq!(range_db(255.0), MOST_RANGE_DB);
        assert_eq!(range_db(0.0), 0.0);
    }

    #[test]
    fn a_compressors_type_and_switches_save_and_load_and_an_old_one_loads_clean() {
        let console = Insert::Compressor(Compressor {
            kind: CompressorType::Console,
            console_ratio: 2,
            console_attack: 0,
            console_release: 3,
            ..Compressor::DEFAULT
        });
        let saved = ron::to_string(&console).unwrap();
        assert_eq!(ron::from_str::<Insert>(&saved).unwrap(), console);
        let glue = Insert::Compressor(Compressor {
            kind: CompressorType::Glue,
            glue_ratio: 2,
            glue_attack: 0,
            glue_release: GLUE_AUTO_RELEASE,
            range: 0x40,
            ..Compressor::DEFAULT
        });
        let saved = ron::to_string(&glue).unwrap();
        assert_eq!(ron::from_str::<Insert>(&saved).unwrap(), glue);
        // A compressor saved before it had types is the clean one, as it sounded.
        let old: Compressor = ron::from_str("(threshold: 170, ratio: 101, mix: 255)").unwrap();
        assert_eq!(old.kind, CompressorType::Clean);
        assert_eq!((old.threshold, old.ratio), (170, 101));
        // Each switch has a key of its own to save and lock it by.
        use crate::settings::{Effect, SettingId};
        let id = SettingId::from_key("1.comp.consolerelease").unwrap();
        assert_eq!(id.name(), "Release");
        assert_eq!(id.label().as_deref(), Some("1CREL"));
        let id = SettingId::from_key("1.comp.gluerelease").unwrap();
        assert_eq!(id.label().as_deref(), Some("1GREL"));
        let id = SettingId::from_key("1.comp.range").unwrap();
        assert_eq!((id.name(), id.label().as_deref()), ("Range", Some("1RNGE")));
        let at = Effect::Compressor
            .params()
            .iter()
            .position(|param| param.key == "type")
            .unwrap();
        assert_eq!(
            SettingId::slot(0, Effect::Compressor, at).unwrap().label(),
            None
        );
    }

    #[test]
    fn changing_a_compressors_type_and_back_finds_each_types_settings() {
        let clean = compressor(CompressorType::Clean).write(Setting::Ratio, f32::from(0x30_u8));
        let bus = clean
            .write(Setting::CompressorType, 1.0)
            .write(Setting::ConsoleRatio, 9.0);
        assert_eq!(
            bus.read(Setting::ConsoleRatio),
            Some(2),
            "a switch stops at its last"
        );
        assert_eq!(
            bus.read(Setting::Ratio),
            Some(0x30),
            "kept while it's not shown"
        );
        let back = bus.write(Setting::CompressorType, 0.0);
        assert_eq!(back.read(Setting::Ratio), Some(0x30));
        assert_eq!(back.read(Setting::ConsoleRatio), Some(2));
    }

    #[test]
    fn every_effect_ends_with_a_gain_and_a_compressor_with_its_makeup() {
        for effect in Insert::ALL.into_iter().filter(|e| *e != Insert::Empty) {
            let last = effect.params().last().unwrap();
            let expected = match effect {
                Insert::Compressor(_) => ("Makeup", 0),
                _ => ("Gain", UNITY_LEVEL),
            };
            assert_eq!((last.name, last.default), expected, "{effect:?}");
        }
        // Glue's clip comes after its makeup, as it does in what Glue does.
        let glue = compressor(CompressorType::Glue);
        let names: Vec<_> = glue.params().iter().rev().take(2).map(|p| p.name).collect();
        assert_eq!(names, ["Peak Clip", "Makeup"]);
    }

    #[test]
    fn a_compressors_makeup_runs_from_unity_to_its_most_in_db() {
        assert_eq!(makeup_db(0.0), 0.0);
        assert_eq!(makeup_db(255.0), MOST_MAKEUP_DB);
        assert_eq!(MOST_MAKEUP_DB, 24.0);
        // Every type has it, with a label so steps lock it and LFOs move it over its whole range.
        for kind in CompressorType::ALL {
            let makeup = compressor(kind)
                .params()
                .iter()
                .find(|param| param.setting == Setting::Makeup)
                .copied()
                .unwrap();
            assert_eq!((makeup.key, makeup.label), ("makeup", Some("MKUP")));
            assert_eq!(Setting::Makeup.highest(), 0xFF);
            assert_eq!(Setting::Makeup.span(), 255.0);
        }
        let turned_up = compressor(CompressorType::Console).write(Setting::Makeup, 255.0);
        assert_eq!(turned_up.read(Setting::Makeup), Some(0xFF));
    }

    #[test]
    fn a_delays_time_is_a_note_division_and_stops_at_the_ends() {
        let delay = Insert::ALL[2];
        let time = delay.params()[0];
        assert_eq!(time.choices[usize::from(delay.value(0))], "1/8d");
        assert_eq!(*time.choices.last().unwrap(), "1 bar");
        assert!(!time.wraps);
        assert_eq!(
            delay.write(Setting::Time, 300.0).read(Setting::Time),
            Some(17)
        );
    }

    #[test]
    fn every_setting_of_every_effect_reads_as_it_writes() {
        for effect in Insert::ALL {
            for (at, param) in effect.params().iter().enumerate() {
                let highest = param.setting.highest();
                let written = effect.write(param.setting, f32::from(highest));
                assert_eq!(written.read(param.setting), Some(highest), "{}", param.name);
                // A type or mode shows its own settings, so the others are another list.
                if written.params() != effect.params() {
                    continue;
                }
                for other in (0..effect.params().len()).filter(|other| *other != at) {
                    let same = effect.params()[other].name;
                    assert_eq!(written.value(other), effect.value(other), "{same}");
                }
            }
        }
    }

    #[test]
    fn a_delay_saved_with_fewer_settings_loads_with_the_rest_as_new() {
        let delay: Delay = ron::from_str("(mix: 255, gain: 224)").unwrap();
        assert_eq!(
            delay,
            Delay {
                mix: 0xFF,
                ..Delay::DEFAULT
            }
        );
    }

    #[test]
    fn a_saved_delay_keeps_its_settings() {
        let delay = Insert::RETURN_DELAY.with_value(1, 0x12);
        let text = ron::to_string(&delay).unwrap();
        assert_eq!(ron::from_str::<Insert>(&text).unwrap(), delay);
    }

    #[test]
    fn a_reverbs_settings_change_alone_and_read_back() {
        let effect = Insert::ALL[3];
        let names: Vec<&str> = effect.params().iter().map(|param| param.name).collect();
        assert_eq!(
            names,
            ["Pre", "Decay", "Damp", "Low cut", "High cut", "Mix", "Gain"]
        );
        let mixed = effect.with_value(5, 0x12);
        assert_eq!((mixed.value(5), mixed.value(6)), (0x12, UNITY_LEVEL));
        assert_eq!(
            effect.write(Setting::Mix, 300.0).read(Setting::Mix),
            Some(0xFF)
        );
        assert_eq!(effect.read(Setting::Amount), None);
        assert_eq!(effect.stepped(true).stepped(false), effect);
    }

    #[test]
    fn a_reverb_saved_with_fewer_settings_loads_with_the_rest_as_new() {
        let reverb: Reverb = ron::from_str("(mix: 255, gain: 224)").unwrap();
        assert_eq!(
            reverb,
            Reverb {
                mix: 0xFF,
                ..Reverb::DEFAULT
            }
        );
    }

    #[test]
    fn a_distort_saved_without_a_gain_loads_at_unity() {
        let distort: Distort = ron::from_str("(kind: Fold, amount: 16, mix: 32)").unwrap();
        assert_eq!(distort.gain, UNITY_LEVEL);
    }

    #[test]
    fn a_setting_changes_alone_and_reads_back() {
        let effect = Insert::ALL[1];
        for (index, param) in effect.params().iter().enumerate() {
            let value = if param.choices.is_empty() { 0x12 } else { 2 };
            let changed = effect.with_value(index, value);
            assert_eq!(changed.value(index), value, "{}", param.name);
            for other in (0..effect.params().len()).filter(|other| *other != index) {
                assert_eq!(changed.value(other), effect.value(other));
            }
        }
        assert_eq!(Insert::Empty.with_value(1, 0x12), Insert::Empty);
    }

    fn distort(kind: DistortType) -> Insert {
        Insert::Distort(Distort {
            kind,
            ..Distort::DEFAULT
        })
    }

    #[test]
    fn no_two_settings_of_an_effect_share_a_name() {
        let modes = DistortType::ALL.map(|kind| distort(kind).params());
        for params in modes.into_iter().chain([&DISTORT_PARAMS[..]]) {
            for (at, param) in params.iter().enumerate() {
                for other in &params[at + 1..] {
                    assert_ne!(param.name, other.name);
                }
            }
        }
    }

    #[test]
    fn writing_lands_on_whole_values_inside_the_range() {
        let effect = distort(DistortType::Crush);
        assert_eq!(
            effect.write(Setting::Bits, 6.4).read(Setting::Bits),
            Some(6)
        );
        assert_eq!(
            effect.write(Setting::Bits, 6.6).read(Setting::Bits),
            Some(7)
        );
        assert_eq!(
            effect.write(Setting::Bits, -3.0).read(Setting::Bits),
            Some(0)
        );
        assert_eq!(
            effect.write(Setting::Bits, 99.0).read(Setting::Bits),
            Some(MOST_BITS - LEAST_BITS)
        );
        assert_eq!(
            effect.write(Setting::Type, 2.2).read(Setting::Type),
            Some(2)
        );
        assert_eq!(
            effect.write(Setting::Gain, 300.0).read(Setting::Gain),
            Some(MAX_LEVEL)
        );
    }

    #[test]
    fn each_mode_has_its_own_setting_beside_mix_and_gain() {
        let names = |kind| -> Vec<&str> {
            distort(kind)
                .params()
                .iter()
                .map(|param| param.name)
                .collect()
        };
        assert_eq!(names(DistortType::Soft), ["Type", "Amount", "Mix", "Gain"]);
        assert_eq!(names(DistortType::Fold), ["Type", "Amount", "Mix", "Gain"]);
        assert_eq!(names(DistortType::Crush), ["Type", "Bits", "Mix", "Gain"]);
        assert_eq!(names(DistortType::Down), ["Type", "Rate", "Mix", "Gain"]);
        for pedal in [DistortType::Overdrive, DistortType::Distortion] {
            assert!(pedal.is_pedal());
            assert_eq!(names(pedal), ["Type", "Amount", "Tone", "Mix", "Gain"]);
        }
        // Bits read as their names, and every one goes there and back.
        let bits = distort(DistortType::Crush).params()[1];
        assert_eq!(bits.choices[usize::from(bits.default)], "9");
        for index in 0..bits.choices.len() as u8 {
            assert_eq!(
                distort(DistortType::Crush).with_value(1, index).value(1),
                index
            );
        }
        assert!(!bits.wraps);
        assert_eq!(TYPE_PARAM.label, None, "a mode isn't locked from a step");
    }

    #[test]
    fn changing_mode_keeps_each_modes_own_setting() {
        let crushed = distort(DistortType::Crush).with_value(1, 3);
        let soft = crushed.write(Setting::Type, 0.0).with_value(1, 0xF0);
        let back = soft.write(Setting::Type, DistortType::Crush as u8 as f32);
        assert_eq!(back.value(1), 3, "Bits as it was");
        assert_eq!(back.read(Setting::Amount), Some(0xF0));
    }

    #[test]
    fn a_types_name_matches_its_place_in_the_choices() {
        for (index, kind) in DistortType::ALL.into_iter().enumerate() {
            assert_eq!(DISTORT_PARAMS[0].choices[index], kind.name());
        }
    }

    #[test]
    fn a_phaser_and_a_flanger_sweep_in_note_lengths() {
        let phaser = Insert::Phaser(Phaser::DEFAULT);
        let flanger = Insert::Flanger(Flanger::DEFAULT);
        let rate = |effect: Insert| effect.params()[0].choices[usize::from(effect.value(0))];
        assert_eq!(rate(phaser), "2 bars");
        assert_eq!(rate(flanger), "1 bar");
        assert_eq!(*SWEEP_NAMES.last().unwrap(), "16 bars");
        assert_eq!(
            phaser
                .write(Setting::SweepRate, 300.0)
                .read(Setting::SweepRate),
            Some(SWEEP_NAMES.len() as u8 - 1)
        );
        let stages = phaser.params()[4];
        assert_eq!(stages.choices[usize::from(phaser.value(4))], "8");
        assert_eq!(
            phaser.write(Setting::Stages, 9.0).read(Setting::Stages),
            Some(2)
        );
    }

    #[test]
    fn a_flangers_settings_read_in_real_units() {
        assert!((flanger_delay_ms(0.0) - 0.5).abs() < 1e-6);
        assert!((flanger_delay_ms(255.0) - 10.0).abs() < 1e-4);
        assert_eq!(flanger_feedback(128.0), 0.0);
        assert_eq!(flanger_feedback(0.0), -1.0);
        assert_eq!(flanger_feedback(255.0), 1.0);
        // It reads nothing for a setting it hasn't got, as a delay's feedback.
        let flanger = Insert::Flanger(Flanger::DEFAULT);
        assert_eq!(flanger.read(Setting::Feedback), None);
        assert_eq!(flanger.read(Setting::FlangerFeedback), Some(0xC0));
    }

    #[test]
    fn a_tapes_drive_reads_in_db_around_the_middle() {
        assert_eq!(tape_drive_db(128.0), 0.0);
        assert_eq!(tape_drive_db(0.0), -MOST_TAPE_DRIVE_DB);
        assert_eq!(tape_drive_db(255.0), MOST_TAPE_DRIVE_DB);
        let tape = Insert::Tape(Tape::DEFAULT);
        // It reads nothing for a setting it hasn't got, as a delay's drive.
        assert_eq!(tape.read(Setting::Drive), None);
        assert_eq!(tape.read(Setting::TapeDrive), Some(0x80));
        assert_eq!(tape.read(Setting::Mix), Some(0xFF));
    }

    #[test]
    fn a_plates_settings_share_the_reverbs_names_and_read_back() {
        let plate = Insert::Plate(Plate::DEFAULT);
        let names: Vec<&str> = plate.params().iter().map(|param| param.name).collect();
        assert_eq!(
            names,
            [
                "Pre", "Decay", "Damp", "High cut", "Size", "Mod", "Mix", "Gain"
            ]
        );
        let reverb_labels: Vec<_> = REVERB_PARAMS.iter().map(|param| param.label).collect();
        for shared in ["Pre", "Decay", "Damp", "High cut", "Mix", "Gain"] {
            let label = PLATE_PARAMS.iter().find(|param| param.name == shared);
            assert!(reverb_labels.contains(&label.unwrap().label), "{shared}");
        }
        let bigger = plate.with_value(4, 0xFF);
        assert_eq!(bigger.read(Setting::Size), Some(0xFF));
        assert_eq!(bigger.read(Setting::LowCut), None);
        let text = ron::to_string(&bigger).unwrap();
        assert_eq!(ron::from_str::<Insert>(&text).unwrap(), bigger);
        assert_eq!(plate_size(0.0), 0.5);
        assert_eq!(plate_size(128.0), 1.0);
        assert_eq!(plate_size(255.0), 2.0);
    }

    #[test]
    fn a_pitch_shifters_settings_read_back_and_save() {
        let pitch = Insert::Pitch(Pitch::DEFAULT);
        let names: Vec<&str> = pitch.params().iter().map(|param| param.name).collect();
        assert_eq!(names, ["Pitch", "Fine", "Size", "Feedback", "Mix", "Gain"]);
        assert_eq!(pitch.read(Setting::Shift), Some(MOST_SHIFT));
        assert_eq!(PITCH_PARAMS[0].choices[usize::from(MOST_SHIFT)], "+0 st");
        let up = pitch.with_value(0, 36);
        assert_eq!(up.read(Setting::Shift), Some(36));
        assert_eq!(shift_semitones(36.0), 12.0);
        // It stops at two octaves.
        assert_eq!(pitch.with_value(0, 0xFF).read(Setting::Shift), Some(48));
        assert_eq!(pitch.read(Setting::Decay), None);
        let text = ron::to_string(&up).unwrap();
        assert_eq!(ron::from_str::<Insert>(&text).unwrap(), up);
        assert_eq!(fine_cents(0.0), -50.0);
        assert_eq!(fine_cents(128.0), 0.0);
        assert_eq!(fine_cents(255.0), 50.0);
        assert_eq!(grain_ms(0.0), 10.0);
        assert!((grain_ms(255.0) - 100.0).abs() < 1e-3);
        // Clouds' own grain, 2047 frames at 32 kHz.
        let clouds = 2047.0 / 32.0;
        assert!((grain_ms(f32::from(Pitch::DEFAULT.size)) - clouds).abs() < 0.5);
    }

    #[test]
    fn a_granular_effects_settings_read_back_and_save() {
        let grains = Insert::Grains(Grains::DEFAULT);
        let names: Vec<&str> = grains.params().iter().map(|param| param.name).collect();
        assert_eq!(
            names,
            [
                "Time", "Size", "Density", "Spread", "Freeze", "Random", "Pitch", "Shape", "Mix",
                "Gain"
            ]
        );
        assert_eq!(grains.read(Setting::Shift), Some(MOST_SHIFT));
        assert_eq!(grains.read(Setting::Freeze), Some(0));
        let frozen = grains.with_value(4, 1);
        assert_eq!(frozen.read(Setting::Freeze), Some(1));
        // It's a switch.
        assert_eq!(grains.with_value(4, 0xFF).read(Setting::Freeze), Some(1));
        assert_eq!(grains.with_value(6, 0xFF).read(Setting::Shift), Some(48));
        assert_eq!(grains.read(Setting::Decay), None);
        let text = ron::to_string(&frozen).unwrap();
        assert_eq!(ron::from_str::<Insert>(&text).unwrap(), frozen);
        assert_eq!(grain_time_seconds(0.0), 0.0);
        assert_eq!(grain_time_seconds(255.0), 4.0);
        // Beads' `Size`: the whole recording backwards, the shortest at a third, and the whole of
        // it forwards.
        assert_eq!(grain_length(0.0), (4.0, true));
        let (shortest, _) = grain_length(85.0);
        assert!(shortest <= 0.008, "{shortest}");
        assert!(grain_length(84.0).1);
        assert!(!grain_length(86.0).1);
        let (longest, reversed) = grain_length(255.0);
        assert!((longest - 4.0).abs() < 1e-4 && !reversed, "{longest}");
        let (default, _) = grain_length(f32::from(Grains::DEFAULT.size));
        assert!((default - 0.15).abs() < 0.02, "{default}");
        assert_eq!(grain_density(0.0), Triggering::Regular(MOST_GRAIN_HERTZ));
        assert_eq!(grain_density(128.0), Triggering::Off);
        assert_eq!(
            grain_density(255.0),
            Triggering::Random(MOST_GRAIN_HERTZ * 0.5)
        );
    }

    #[test]
    fn warps_settings_read_back_and_save_and_its_algo_reads_as_warps_knob() {
        let warps = Insert::Warps(Warps::DEFAULT);
        let names: Vec<&str> = warps.params().iter().map(|param| param.name).collect();
        assert_eq!(
            names,
            [
                "Algo", "Timbre", "Source", "Level", "Pitch", "Shape", "Mix", "Gain"
            ]
        );
        assert_eq!(WARPS_PARAMS[2].choices[0], "Osc");
        assert_eq!(WARPS_PARAMS[2].choices[1..], SOURCE_NAMES[1..]);
        assert_eq!(
            WARPS_PARAMS[4].choices[usize::from(warps.value(4))],
            "+12 st"
        );
        assert_eq!(WARPS_PARAMS[5].choices[usize::from(warps.value(5))], "Sine");
        assert_eq!(warps.with_value(5, 0xFF).read(Setting::Wave), Some(4));
        let fed = warps.with_value(2, 2);
        assert_eq!(
            match fed {
                Insert::Warps(warps) => warps.source_track(8),
                _ => None,
            },
            Some(1)
        );
        assert_eq!(Warps::DEFAULT.source_track(8), None);
        assert_eq!(warps.read(Setting::Shape), None);
        let text = ron::to_string(&fed).unwrap();
        assert_eq!(ron::from_str::<Insert>(&text).unwrap(), fed);
        // Each cross-modulation is an eighth further, on a value of its own, then the modulator,
        // then the vocoder.
        for (at, name) in ["Xfade", "Fold", "Ring", "Digital", "XOR", "Compare"]
            .into_iter()
            .enumerate()
        {
            let value = at as f32 * 32.0;
            assert_eq!(algorithm_blend(value), (at, at + 1, 0.0), "{name}");
            assert_eq!(ALGORITHM_NAMES[at], name);
        }
        let (from, to, by) = algorithm_blend(f32::from(0xB3_u8));
        assert_eq!((from, to), (5, 6));
        assert!(by > 0.95, "{by}");
        assert_eq!(algorithm_blend(255.0), (7, 7, 0.0));
    }

    #[test]
    fn a_source_names_the_returns_by_their_old_numbers_and_the_groups_after() {
        // With 8 sound tracks, the groups are at 8 to 11, the returns at 12 and 13.
        let heard = |source: u8| source_track(source, 8);
        assert_eq!(SOURCE_NAMES[65], "A");
        assert_eq!(SOURCE_NAMES[66], "B");
        assert_eq!(heard(65), Some(8 + MAX_GROUPS));
        assert_eq!(heard(66), Some(9 + MAX_GROUPS));
        assert_eq!(SOURCE_NAMES[67], "G1");
        assert_eq!(heard(67), Some(8));
        assert_eq!(SOURCE_NAMES[66 + MAX_GROUPS], format!("G{MAX_GROUPS}"));
        assert_eq!(heard(66 + MAX_GROUPS as u8), Some(7 + MAX_GROUPS));
        assert_eq!(Setting::Source.highest(), 66 + MAX_GROUPS as u8);
    }

    #[test]
    fn a_low_pass_gates_settings_read_back_and_save() {
        let gate = Insert::Lpg(LowPassGate::DEFAULT);
        let names: Vec<&str> = gate.params().iter().map(|param| param.name).collect();
        assert_eq!(
            names,
            [
                "Mode", "Amount", "Base", "Res", "Response", "Source", "Mix", "Gain"
            ]
        );
        assert_eq!(LPG_PARAMS[0].choices, ["Gate", "Filter", "VCA"]);
        assert_eq!(LPG_PARAMS[0].label, None, "a mode isn't locked from a step");
        // It goes round the modes, stopping at the last when written.
        assert_eq!(gate.with_value(0, 0xFF).read(Setting::GateMode), Some(2));
        let filter = gate.with_value(0, 1);
        assert!(matches!(
            filter,
            Insert::Lpg(LowPassGate {
                mode: GateMode::Filter,
                ..
            })
        ));
        let keyed = filter.with_value(5, 2);
        assert_eq!(
            match keyed {
                Insert::Lpg(gate) => gate.source_track(8),
                _ => None,
            },
            Some(1)
        );
        assert_eq!(LowPassGate::DEFAULT.source_track(8), None);
        assert_eq!(gate.read(Setting::Resonance), Some(0));
        assert_eq!(gate.read(Setting::Decay), None);
        let text = ron::to_string(&keyed).unwrap();
        assert_eq!(ron::from_str::<Insert>(&text).unwrap(), keyed);
    }

    #[test]
    fn stepping_goes_round_the_effects_and_keeps_nothing() {
        let changed = Insert::ALL[1].with_value(1, 0x12);
        assert_eq!(changed.stepped(true), Insert::ALL[2]);
        let last = Insert::ALL[Insert::ALL.len() - 1];
        assert_eq!(last.stepped(true), Insert::Empty);
        assert_eq!(Insert::Empty.stepped(true), Insert::ALL[1]);
        assert_eq!(Insert::Empty.stepped(false), last);
    }
}
