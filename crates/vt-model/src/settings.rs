//! Every setting a step can lock or a source can move, registered once: the track's, each
//! machine's, each effect's in each slot, and each mod slot's. An entry has a key that songs store, a
//! long name for finding it, a short label for the grid, its kind and its default. A setting's
//! place in the table is its [`SettingId`], fixed when the program is built.

use crate::constants::{DB_PER_LEVEL, MAX_LEVEL, UNITY_LEVEL};
use crate::groove::GROOVE_CHOICES;
use crate::insert::{
    ALGORITHM_NAMES, CHORUS_PARAMS, COMB_PARAMS, COMPRESSOR_PARAMS, DELAY_PARAMS, DISTORT_PARAMS,
    EQ_PARAMS, FLANGER_PARAMS, GRAINS_PARAMS, INSERTS, Insert, LIMIT_PARAMS, LPG_PARAMS,
    PHASER_PARAMS, PITCH_PARAMS, PLATE_PARAMS, Param, RESO_PARAMS, REVERB_PARAMS, Setting,
    TAPE_PARAMS, Triggering, VOWEL_NAMES, VOWEL_PARAMS, WARPS_PARAMS, algorithm_blend, attack_ms,
    ceiling_db, fine_cents, flanger_delay_ms, grain_density, grain_length, grain_ms,
    grain_time_seconds, input_db, limit_release_ms, makeup_db, plate_size, range_db, ratio,
    release_ms, tape_drive_db, threshold_db, vowel_blend,
};
use crate::modulation::{MOD_SLOTS, depth_of, depth_text};
use crate::patch_names::PATCH_NAMES;
use crate::song::{DEFAULT_ACCENT, FILTER_MODE_NAMES, FULL_GATE, SynthVoice};
use crate::speed::{SPEED_NAMES, Speed};
use crate::step::{FxCommand, MAX_VELOCITY};

/// The most settings a machine has: `Acid`'s, a TB-303's knobs with two of the Devil Fish's and
/// `Drift` (decision 190).
pub const MAX_MACHINE_SETTINGS: usize = 9;

/// What values a setting takes, and what they mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A value that sweeps from `00` up to a top: `FF`, or the level scale's top for volume, sends
    /// and gain. A centred one has its middle at `80`, like pan.
    Continuous { top: u8, middle: bool, units: Units },
    /// One of a list of choices, which goes round past the end or stops.
    Stepped {
        choices: &'static [&'static str],
        wraps: bool,
    },
}

/// How a continuous value reads in real units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Units {
    /// How far along its range, or either side of the middle.
    Percent,
    /// Left or right of centre.
    Pan,
    /// The filter's cutoff, from 20 Hz to 20 kHz evenly in pitch.
    Hertz,
    /// The level scale, where `E0` is 0 dB and `00` is silent.
    Decibels,
    /// A mod slot's depth, in hex steps of what it moves, as the Mod page shows it.
    Depth,
    /// Round a cycle, where `FF` is a whole turn, so it goes round past either end.
    Degrees,
    /// Semitones either side of `80`, one a coarse step, with the fine part in cents.
    Semitones,
    /// An EQ band's boost or cut, up to `BAND_DB` either side of `80`.
    Boost,
    /// An EQ band's width, from `LEAST_Q` at `00` to `MOST_Q` at `FF`, evenly in ratio.
    Q,
    /// A compressor's threshold in dB, its ratio, and its times in ms.
    Threshold,
    Ratio,
    Attack,
    Release,
    /// A compressor's makeup in dB, from none up, and the most Glue turns down, in dB.
    Makeup,
    Range,
    /// A limiter's drive and ceiling in dB, and its release in ms.
    Input,
    Ceiling,
    LimitRelease,
    /// A flanger's delay at the middle of its sweep, in ms.
    FlangerDelay,
    /// A tape's drive, in dB either way of none.
    TapeDrive,
    /// A plate's size, as so many times Dattorro's.
    PlateSize,
    /// A pitch shifter's fine shift in cents, and its grains' length in ms.
    Cents,
    Grain,
    /// A granular effect's: how far back its grains read, how long they are and which way, and
    /// how often they start and whether at random.
    GrainTime,
    GrainLength,
    Density,
    /// Where Warps' algorithm knob is, by the algorithms it's between.
    Algorithm,
    /// Where a vowel filter is, by the vowels it's between.
    Vowel,
    /// The acid voice's cutoff, its filter envelope's decay, how long it slides and how hard it
    /// drives its filter.
    AcidCutoff,
    AcidDecay,
    Slide,
    AcidDrive,
    /// How much of its last pulse a note lasts, as [`gate_share`] gives it.
    GateLength,
    /// How far a note's length is moved up or down, as a share of its last pulse, as
    /// [`gate_delta`] gives it.
    GateDelta,
    /// How far a note's velocity is moved up or down, as [`velocity_delta`] gives it.
    VelocityDelta,
    /// How long a tied note takes to glide to the next, as [`glide_ms`] gives it, or off.
    Glide,
}

/// The acid voice's cutoff at either end of its range, in Hz, where its filter resonates.
pub const ACID_LOWEST_CUTOFF: f32 = 170.0;
pub const ACID_HIGHEST_CUTOFF: f32 = 1450.0;
/// The acid voice's Cutoff knob's taper: how far it's turned, to this power, is how far along its
/// range in octaves the cutoff is.
pub const ACID_CUTOFF_TAPER: f32 = 1.9;

/// How far along its range in octaves the acid voice's cutoff is, from 0 to 1, for a value on the
/// `00` to `FF` scale. It moves little over the knob's first quarter and most over its top half,
/// as Acid V's does and as an audio-taper pot on a 303 would (decision 117).
pub fn acid_cutoff_position(value: f32) -> f32 {
    (value / 255.0).clamp(0.0, 1.0).powf(ACID_CUTOFF_TAPER)
}

/// The acid voice's cutoff in Hz for a value on the `00` to `FF` scale.
pub fn acid_cutoff_hz(value: f32) -> f32 {
    ACID_LOWEST_CUTOFF
        * (ACID_HIGHEST_CUTOFF / ACID_LOWEST_CUTOFF).powf(acid_cutoff_position(value))
}

/// The acid voice's filter envelope decay at either end of its Decay knob, as time constants in ms.
pub const ACID_SHORTEST_DECAY: f32 = 40.0;
pub const ACID_LONGEST_DECAY: f32 = 600.0;

/// The acid voice's filter envelope decay in ms, as a time constant: a TB-303's Decay knob, evenly
/// in ratio from `ACID_SHORTEST_DECAY` to `ACID_LONGEST_DECAY`, the span Acid V's knob sweeps over
/// (decision 117). The 200 ms to 2 s given for a 303 doesn't say what it times, and read as time
/// constants, as Open303 reads them, most of the knob fell slower than a sixteenth note lasts.
pub fn acid_decay_ms(value: f32) -> f32 {
    ACID_SHORTEST_DECAY * (ACID_LONGEST_DECAY / ACID_SHORTEST_DECAY).powf(value / 255.0)
}

/// How long the acid voice takes to slide to a note, in ms: a TB-303's 60 ms at `00`, up to the
/// Devil Fish's longest, 360 ms.
pub fn slide_ms(value: f32) -> f32 {
    60.0 + 300.0 * value / 255.0
}

/// How much the acid voice drives its filter, in dB over a TB-303's level.
pub fn acid_drive_db(value: f32) -> f32 {
    24.0 * value / 255.0
}

/// The shortest and longest a tie's glide can take, in ms, at `01` and `FF`.
pub const SHORTEST_GLIDE_MS: f32 = 2.0;
pub const LONGEST_GLIDE_MS: f32 = 2000.0;

/// How long a tied note takes to glide to the next, in ms, for a `Glide` value on the `00` to
/// `FF` scale: none at `00`, where the pitch jumps, or `Acid` slides by its own `Slide`, then from
/// `SHORTEST_GLIDE_MS` at `01` to `LONGEST_GLIDE_MS` at `FF`, evenly in ratio, so `80` is about a
/// TB-303's 60 ms slide.
pub fn glide_ms(value: f32) -> Option<f32> {
    (value >= 1.0).then(|| {
        let along = (value.min(255.0) - 1.0) / 254.0;
        SHORTEST_GLIDE_MS * (LONGEST_GLIDE_MS / SHORTEST_GLIDE_MS).powf(along)
    })
}

/// The `Glide` value whose time is nearest this many ms, with `0` for off.
pub fn glide_value(ms: f32) -> u8 {
    if ms <= 0.0 {
        return 0;
    }
    let ratio = LONGEST_GLIDE_MS / SHORTEST_GLIDE_MS;
    let along = (ms.max(SHORTEST_GLIDE_MS) / SHORTEST_GLIDE_MS).ln() / ratio.ln();
    (1.0 + along * 254.0).round().clamp(1.0, 255.0) as u8
}

/// The shortest a note's gate can be, as a share of its last pulse, so `00` still sounds.
pub const SHORTEST_GATE: f32 = 0.01;

/// How much of its last pulse a note lasts for a `Gate` value on the `00` to `FF` scale: from
/// `SHORTEST_GATE` up to all of it at `FF`, which holds it to the next note as before.
pub fn gate_share(value: f32) -> f32 {
    (value / 255.0).clamp(SHORTEST_GATE, 1.0)
}

/// A `Gate` value as the whole percentage it's nearest, from 1 to 100.
pub fn gate_percent(value: u8) -> u8 {
    (gate_share(f32::from(value)) * 100.0).round() as u8
}

/// The `Gate` value nearest a percentage, which is kept from 1 to 100.
pub fn gate_value(percent: u8) -> u8 {
    (f32::from(percent.clamp(1, 100)) * 2.55).round() as u8
}

/// How far an `LDLT` value moves a note's length, on `Gate`'s `00` to `FF` scale, so a step of one
/// is a step of the other: `80` leaves it, `00` takes half a pulse off and `FF` adds half.
pub fn gate_delta(value: f32) -> f32 {
    value - 128.0
}

/// How far a `VDLT` value moves a note's velocity, so a step of one is a step of the other: `80`
/// leaves it, `90` adds `10`, and `00` and `FF` take any velocity to either end.
pub fn velocity_delta(value: f32) -> f32 {
    value - 128.0
}

/// A note's velocity moved by a `VDLT` value, kept to the velocity's range.
pub fn moved_velocity(velocity: u8, value: f32) -> u8 {
    (f32::from(velocity) + velocity_delta(value))
        .round()
        .clamp(0.0, f32::from(MAX_VELOCITY)) as u8
}

/// How far an EQ band boosts or cuts, in dB, at either end.
pub const BAND_DB: f32 = 15.0;
/// An EQ band's narrowest and widest.
pub const LEAST_Q: f32 = 0.25;
pub const MOST_Q: f32 = 16.0;

/// An EQ band's gain in dB for a value on the `00` to `FF` scale, `0` at `80`.
pub fn band_db(value: f32) -> f32 {
    side(f64::from(value)) as f32 * BAND_DB
}

/// An EQ band's Q for a value on the `00` to `FF` scale.
pub fn band_q(value: f32) -> f32 {
    LEAST_Q * (MOST_Q / LEAST_Q).powf(value / 255.0)
}

impl Kind {
    /// The highest value it takes: a list's last choice, or a level's top.
    pub fn top(self) -> u8 {
        match self {
            Self::Continuous { top, .. } => top,
            Self::Stepped { choices, .. } => choices.len().saturating_sub(1).min(0xFF) as u8,
        }
    }

    /// Whether its value has a fine part, two more digits set back: a continuous position's does,
    /// as a step between two of its values can be heard, and one on the level scale doesn't, as a
    /// quarter of a dB is its natural step, nor does a list's (decision 175).
    pub fn has_fine(self) -> bool {
        matches!(self, Self::Continuous { units, .. } if units != Units::Decibels)
    }

    /// Whether it goes round past either end, as a phase does.
    pub fn wraps(self) -> bool {
        matches!(
            self,
            Self::Continuous {
                units: Units::Degrees,
                ..
            }
        )
    }

    /// Whether it has a middle that bars draw from.
    pub fn centred(self) -> bool {
        matches!(self, Self::Continuous { middle: true, .. })
    }

    /// A value in real units, for the status line: `1.21 kHz`, `-6.0 dB`, `25 % L` or a choice's
    /// name. `value` has a coarse byte and a fine one, so `4080` is half a step above `40`.
    pub fn shown(self, value: u16) -> String {
        let coarse = f64::from(value) / 256.0;
        match self {
            Self::Stepped { choices, .. } => choices
                .get(usize::from(value >> 8))
                .map_or_else(String::new, |choice| (*choice).to_owned()),
            Self::Continuous { units, middle, top } => match units {
                Units::Hertz => hertz(20.0 * 1000_f64.powf(coarse / 255.0)),
                Units::Decibels if value == 0 => "-inf dB".to_owned(),
                Units::Decibels => format!(
                    "{:+.1} dB",
                    (coarse - f64::from(UNITY_LEVEL)) * f64::from(DB_PER_LEVEL)
                ),
                Units::Pan => match side(coarse) {
                    side if side.abs() < 0.005 => "Centre".to_owned(),
                    side if side < 0.0 => format!("{:.0} % L", side.abs() * 100.0),
                    side => format!("{:.0} % R", side * 100.0),
                },
                Units::Depth => depth_text(depth_of(coarse as f32).round() as i16),
                Units::Boost => format!("{:+.1} dB", band_db(coarse as f32)),
                Units::Q => format!("Q {:.2}", band_q(coarse as f32)),
                Units::Threshold => format!("{:.1} dB", threshold_db(coarse as f32)),
                Units::Ratio => match ratio(coarse as f32) {
                    ratio if ratio.is_infinite() => "inf:1".to_owned(),
                    ratio => format!("{ratio:.1}:1"),
                },
                Units::Attack => milliseconds(attack_ms(coarse as f32)),
                Units::Release => milliseconds(release_ms(coarse as f32)),
                Units::Makeup => format!("{:+.1} dB", makeup_db(coarse as f32)),
                Units::Range => format!("{:.1} dB", range_db(coarse as f32)),
                Units::Input => format!("{:+.1} dB", input_db(coarse as f32)),
                Units::Ceiling => format!("{:.1} dB", ceiling_db(coarse as f32)),
                Units::LimitRelease => milliseconds(limit_release_ms(coarse as f32)),
                Units::FlangerDelay => format!("{:.1} ms", flanger_delay_ms(coarse as f32)),
                Units::TapeDrive => format!("{:+.1} dB", tape_drive_db(coarse as f32)),
                Units::PlateSize => format!("{:.2}x", plate_size(coarse as f32)),
                Units::Cents => format!("{:+.0} ct", fine_cents(coarse as f32)),
                Units::Grain => milliseconds(grain_ms(coarse as f32)),
                Units::GrainTime => milliseconds(grain_time_seconds(coarse as f32) * 1000.0),
                Units::GrainLength => match grain_length(coarse as f32) {
                    (seconds, true) => format!("{} rev", milliseconds(seconds * 1000.0)),
                    (seconds, false) => milliseconds(seconds * 1000.0),
                },
                Units::Density => match grain_density(coarse as f32) {
                    Triggering::Regular(hertz) => rate(hertz),
                    Triggering::Off => "Off".to_owned(),
                    Triggering::Random(hertz) => format!("{} rnd", rate(hertz)),
                },
                Units::Algorithm => algorithm_text(coarse as f32),
                Units::Vowel => vowel_text(coarse as f32),
                Units::AcidCutoff => hertz(f64::from(acid_cutoff_hz(coarse as f32))),
                Units::AcidDecay => milliseconds(acid_decay_ms(coarse as f32)),
                Units::Slide => milliseconds(slide_ms(coarse as f32)),
                Units::AcidDrive => format!("+{:.1} dB", acid_drive_db(coarse as f32)),
                Units::GateLength => format!("{:.0} %", gate_share(coarse as f32) * 100.0),
                Units::GateDelta => format!("{:+.0} %", gate_delta(coarse as f32) / 2.55),
                Units::VelocityDelta => {
                    let moved = velocity_delta(coarse as f32).round() as i32;
                    let sign = if moved < 0 { '-' } else { '+' };
                    format!("{sign}{:02X} vel", moved.unsigned_abs())
                }
                Units::Glide => {
                    glide_ms(coarse as f32).map_or_else(|| "Off".to_owned(), milliseconds)
                }
                Units::Degrees => format!("{:.0}°", coarse / 255.0 * 360.0),
                Units::Semitones => {
                    let cents = ((coarse - 128.0) * 100.0).round() as i32;
                    match (cents / 100, cents % 100) {
                        (semitones, 0) => format!("{semitones:+} st"),
                        (semitones, cents) => format!("{semitones:+} st {cents:+} ct"),
                    }
                }
                Units::Percent if middle => format!("{:+.0} %", side(coarse) * 100.0),
                Units::Percent => format!("{:.0} %", coarse / f64::from(top) * 100.0),
            },
        }
    }
}

/// How far a centred value is to either side, from -1 to 1. The sides have their own scales, as
/// there are 128 values below the middle and 127 above.
fn side(coarse: f64) -> f64 {
    let from_middle = coarse - 128.0;
    from_middle / if from_middle < 0.0 { 128.0 } else { 127.0 }
}

/// A time as it reads best: `0.25 ms`, `12 ms` or `1.00 s`.
fn milliseconds(ms: f32) -> String {
    match ms {
        ms if ms < 1.0 => format!("{ms:.2} ms"),
        ms if ms < 1000.0 => format!("{ms:.0} ms"),
        ms => format!("{:.2} s", ms / 1000.0),
    }
}

/// Where Warps' `Algo` is, as the status line names it: one algorithm, the two it's between, or
/// the vocoder, frozen at the top.
fn algorithm_text(value: f32) -> String {
    let (from, to, by) = algorithm_blend(value);
    match by {
        _ if value >= 255.0 => "Vocoder hold".to_owned(),
        by if by < 0.05 => ALGORITHM_NAMES[from].to_owned(),
        by if by > 0.95 => ALGORITHM_NAMES[to].to_owned(),
        _ => format!("{}>{}", ALGORITHM_NAMES[from], ALGORITHM_NAMES[to]),
    }
}

/// Where a vowel filter is: a vowel, or the two it's between, as `A>E`.
fn vowel_text(value: f32) -> String {
    let (from, to, by) = vowel_blend(value);
    match by {
        by if by < 0.05 => VOWEL_NAMES[from].to_owned(),
        by if by > 0.95 => VOWEL_NAMES[to].to_owned(),
        _ => format!("{}>{}", VOWEL_NAMES[from], VOWEL_NAMES[to]),
    }
}

/// How often something happens, as it reads best: `0.25 Hz`, `1.5 Hz` or `12 Hz`.
fn rate(hertz: f32) -> String {
    match hertz {
        hertz if hertz < 1.0 => format!("{hertz:.2} Hz"),
        hertz if hertz < 10.0 => format!("{hertz:.1} Hz"),
        hertz => format!("{hertz:.0} Hz"),
    }
}

/// A frequency as it reads best: `440 Hz` or `1.21 kHz`.
fn hertz(hertz: f64) -> String {
    match hertz {
        hertz if hertz < 1000.0 => format!("{hertz:.0} Hz"),
        hertz if hertz < 10_000.0 => format!("{:.2} kHz", hertz / 1000.0),
        hertz => format!("{:.1} kHz", hertz / 1000.0),
    }
}

/// One setting, as its owner declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The name songs store it by, which never changes.
    pub key: &'static str,
    /// What it's called in lists and on the status line.
    pub name: &'static str,
    /// The grid's label, 3 or 4 letters. Without one, modulation can move it but steps can't.
    pub label: Option<&'static str>,
    pub kind: Kind,
    pub default: u8,
}

const fn level(key: &'static str, name: &'static str, label: &'static str, default: u8) -> Entry {
    Entry {
        key,
        name,
        label: Some(label),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Percent,
        },
        default,
    }
}

const fn centred(key: &'static str, name: &'static str, label: &'static str) -> Entry {
    Entry {
        key,
        name,
        label: Some(label),
        kind: CENTRED,
        default: 0x80,
    }
}

const fn on_level_scale(
    key: &'static str,
    name: &'static str,
    label: &'static str,
    default: u8,
) -> Entry {
    Entry {
        kind: ON_LEVEL_SCALE,
        ..level(key, name, label, default)
    }
}

const CENTRED: Kind = Kind::Continuous {
    top: 0xFF,
    middle: true,
    units: Units::Percent,
};
const ON_LEVEL_SCALE: Kind = Kind::Continuous {
    top: MAX_LEVEL,
    middle: false,
    units: Units::Decibels,
};

/// The track's own, in the order the table keeps them.
const TRACK: [Entry; 15] = [
    on_level_scale("vol", "Vol", "VOL", UNITY_LEVEL),
    Entry {
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::Pan,
        },
        ..centred("pan", "Pan", "PAN")
    },
    Entry {
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Hertz,
        },
        ..level("cut", "Cut", "CUT", 0xFF)
    },
    level("res", "Res", "RES", 0x00),
    on_level_scale("sna", "Send A", "SNA", 0x00),
    on_level_scale("snb", "Send B", "SNB", 0x00),
    Entry {
        key: "pitch",
        name: "Pitch",
        label: Some("PTCH"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::Semitones,
        },
        default: 0x80,
    },
    Entry {
        key: "mode",
        name: "Mode",
        label: Some("MODE"),
        kind: Kind::Stepped {
            choices: &FILTER_MODE_NAMES,
            wraps: false,
        },
        default: 0,
    },
    Entry {
        key: "gate",
        name: "Gate",
        label: Some("LEN"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::GateLength,
        },
        default: FULL_GATE,
    },
    // Its value is never heard alone: it's added to each note's length as it starts, so a lane
    // of it moves notes' lengths against their own.
    Entry {
        key: "gatedelta",
        name: "Gate up or down",
        label: Some("LDLT"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::GateDelta,
        },
        default: 0x80,
    },
    // A step's own groove, read before the step is played, as it says when the step lands. `00`
    // is the project's, and a track with its own holds that.
    Entry {
        key: "groove",
        name: "Groove",
        label: Some("GRV"),
        kind: Kind::Stepped {
            choices: &GROOVE_CHOICES,
            wraps: false,
        },
        default: 0,
    },
    // How long a step is, read before the step is played, as it says when the next one comes.
    Entry {
        key: "speed",
        name: "Speed",
        label: Some("RATE"),
        kind: Kind::Stepped {
            choices: &SPEED_NAMES,
            wraps: false,
        },
        default: Speed::Sixteenth.index(),
    },
    // How long a tie glides into the next note, read as the tie comes, so a lock on either row
    // sets it.
    Entry {
        key: "glide",
        name: "Glide",
        label: Some("GLD"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Glide,
        },
        default: 0,
    },
    // As `LDLT` is to a note's length: added to each note's velocity as it starts, so a lane of it
    // is an accent pattern and a glide of it a crescendo, and the note keeps its own.
    Entry {
        key: "velocitydelta",
        name: "Velocity up or down",
        label: Some("VDLT"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::VelocityDelta,
        },
        default: 0x80,
    },
    // The fader, which a step can set as it sets `PAN`, with `VOL` a stage on top of it (decision
    // 174). Its base is the track's `level`, and modulation can't reach it, as `VOL` is where
    // modulation moves the volume.
    on_level_scale("level", "Level", "LEV", UNITY_LEVEL),
];
/// What each of the track's is listed with.
const TRACK_GROUPS: [&str; 15] = [
    "Output", "Output", "Filter", "Filter", "Output", "Output", "Machine", "Filter", "Machine",
    "Machine", "Machine", "Machine", "Machine", "Machine", "Output",
];

const ACCENT: Entry = level("accent", "Accent", "ACC", DEFAULT_ACCENT);
/// How far each note of an analog-style voice strays, from none at `00`: its oscillators' tuning,
/// a slow wander in pitch, its cutoff, level and envelope times, and, but for `Acid`'s, which
/// always runs free, where its oscillators start (decision 190).
const DRIFT: Entry = level("drift", "Drift", "DRFT", 0x00);

const VA: [Entry; 4] = [
    centred("detune", "Detune", "DTUN"),
    level("square", "Square", "SQR", 0x00),
    level("saw", "Saw", "SAW", 0x80),
    DRIFT,
];
const FM: [Entry; 3] = [
    level("ratio", "Ratio", "RAT", 0x80),
    level("index", "Index", "INDX", 0x00),
    centred("feedback", "Feedback", "FDBK"),
];
const VCF: [Entry; 4] = [
    centred("resonance", "Resonance", "RESO"),
    level("frequency", "Frequency", "FREQ", 0x80),
    level("wave", "Wave", "WAVE", 0x40),
    DRIFT,
];
const PD: [Entry; 3] = [
    level("ratio", "Ratio", "RAT", 0x80),
    level("amount", "Amount", "AMNT", 0x80),
    level("skew", "Skew", "SKEW", 0x40),
];
const FORMANT: [Entry; 3] = [
    Entry {
        default: 0x99,
        ..centred("interval", "Interval", "INTV")
    },
    level("formant", "Formant", "FRMT", 0x80),
    level("width", "Width", "WDTH", 0x80),
];
const ADDITIVE: [Entry; 3] = [
    level("bumps", "Bumps", "BMPS", 0x00),
    level("peak", "Peak", "PEAK", 0x4D),
    level("shape", "Shape", "SHPE", 0x80),
];
const SWARM: [Entry; 3] = [
    level("spread", "Spread", "SPRD", 0x66),
    level("density", "Density", "DENS", 0x80),
    level("overlap", "Overlap", "OVLP", 0x99),
];
/// A continuous setting from `00` to `FF` that reads in these units.
const fn in_units(
    key: &'static str,
    name: &'static str,
    label: &'static str,
    default: u8,
    units: Units,
) -> Entry {
    Entry {
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units,
        },
        ..level(key, name, label, default)
    }
}
/// The acid voice's waves: the saw, and the square a TB-303 shapes from it.
const ACID_WAVES: [&str; 2] = ["Saw", "Square"];
/// The acid voice's, in the order of a TB-303's knobs, then two of the Devil Fish's, then `Drift`.
const ACID: [Entry; 9] = [
    Entry {
        key: "wave",
        name: "Wave",
        label: Some("WAVE"),
        kind: Kind::Stepped {
            choices: &ACID_WAVES,
            wraps: false,
        },
        default: 0,
    },
    in_units("cutoff", "Cutoff", "FREQ", 0x80, Units::AcidCutoff),
    level("resonance", "Resonance", "RESO", 0x80),
    level("envmod", "Env mod", "EMOD", 0x80),
    in_units("decay", "Decay", "DCAY", 0x80, Units::AcidDecay),
    level("accent", "Accent", "ACC", 0x80),
    in_units("slide", "Slide", "SLID", 0x00, Units::Slide),
    in_units("drive", "Drive", "DRIV", 0x00, Units::AcidDrive),
    DRIFT,
];
/// The chords Chord and Chiptune play, in the order Plaits lists them.
const CHORD_NAMES: [&str; 11] = [
    "Octave", "5", "sus4", "m", "m7", "m9", "m11", "6/9", "M9", "M7", "M",
];
/// A chord, chosen from the list. It starts major.
const CHORD_TYPE: Entry = Entry {
    key: "chord",
    name: "Chord",
    label: Some("CHRD"),
    kind: Kind::Stepped {
        choices: &CHORD_NAMES,
        wraps: false,
    },
    default: 10,
};
const TERRAIN: [Entry; 3] = [
    level("terrain", "Terrain", "TERR", 0x00),
    level("size", "Size", "SIZE", 0x80),
    level("offset", "Offset", "OFST", 0x80),
];
const CHORD: [Entry; 3] = [
    CHORD_TYPE,
    level("inversion", "Inversion", "INV", 0x66),
    level("wave", "Wave", "WAVE", 0x40),
];
const CHIPTUNE: [Entry; 4] = [
    CHORD_TYPE,
    level("inversion", "Inversion", "INV", 0x66),
    centred("shape", "Shape", "SHPE"),
    DRIFT,
];
const FOLD: [Entry; 3] = [
    centred("shape", "Shape", "SHPE"),
    level("fold", "Fold", "FOLD", 0x00),
    level("slope", "Slope", "SLPE", 0x80),
];
const WAVES: [Entry; 3] = [
    level("bank", "Bank", "BANK", 0x80),
    level("wave", "Wave", "WAVE", 0x00),
    level("row", "Row", "ROW", 0x80),
];
const SIX_OP: [Entry; 4] = [
    Entry {
        key: "patch",
        name: "Patch",
        label: Some("PAT"),
        kind: Kind::Stepped {
            choices: &PATCH_NAMES,
            wraps: false,
        },
        // E.PIANO 1.
        default: 0x20,
    },
    centred("bright", "Bright", "BRT"),
    centred("time", "Time", "TIME"),
    ACCENT,
];
/// A string's or a bar's: Material starts where it's in tune.
const STRUCK: [Entry; 4] = [
    level("material", "Material", "MATL", 0x40),
    level("bright", "Bright", "BRT", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];
const DUST: [Entry; 3] = [
    level("spread", "Spread", "SPRD", 0x80),
    level("density", "Density", "DENS", 0x00),
    centred("filter", "Filter", "FILT"),
];
const NOISE: [Entry; 3] = [
    centred("type", "Type", "TYPE"),
    level("clock", "Clock", "CLCK", 0x00),
    level("ring", "Ring", "RING", 0x80),
];
const KICK: [Entry; 4] = [
    level("punch", "Punch", "PNCH", 0x80),
    level("tone", "Tone", "TONE", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];
const SNARE: [Entry; 4] = [
    level("snappy", "Snappy", "SNAP", 0x80),
    level("tone", "Tone", "TONE", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];
const HAT: [Entry; 4] = [
    level("noise", "Noise", "NOIS", 0x80),
    level("tone", "Tone", "TONE", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];

/// A voice's settings, in the order it takes them.
pub const fn voice_entries(voice: SynthVoice) -> &'static [Entry] {
    match voice {
        SynthVoice::VirtualAnalog => &VA,
        SynthVoice::VirtualAnalogVcf => &VCF,
        SynthVoice::Acid => &ACID,
        SynthVoice::Fm => &FM,
        SynthVoice::PhaseDistortion => &PD,
        SynthVoice::Waveshaping => &FOLD,
        SynthVoice::Formant => &FORMANT,
        SynthVoice::Additive => &ADDITIVE,
        SynthVoice::Swarm => &SWARM,
        SynthVoice::WaveTerrain => &TERRAIN,
        SynthVoice::Chord => &CHORD,
        SynthVoice::Chiptune => &CHIPTUNE,
        SynthVoice::Wavetable => &WAVES,
        SynthVoice::SixOp => &SIX_OP,
        SynthVoice::String | SynthVoice::Modal => &STRUCK,
        SynthVoice::Dust => &DUST,
        SynthVoice::Noise => &NOISE,
        SynthVoice::Kick => &KICK,
        SynthVoice::Snare => &SNARE,
        SynthVoice::Hat => &HAT,
    }
}

/// The name a voice's settings are stored under.
pub const fn voice_key(voice: SynthVoice) -> &'static str {
    match voice {
        SynthVoice::VirtualAnalog => "va",
        SynthVoice::VirtualAnalogVcf => "vcf",
        SynthVoice::Acid => "acid",
        SynthVoice::Fm => "fm",
        SynthVoice::PhaseDistortion => "pd",
        SynthVoice::Waveshaping => "fold",
        SynthVoice::Formant => "formant",
        SynthVoice::Additive => "additive",
        SynthVoice::Swarm => "swarm",
        SynthVoice::WaveTerrain => "terrain",
        SynthVoice::Chord => "chord",
        SynthVoice::Chiptune => "chiptune",
        SynthVoice::Wavetable => "waves",
        SynthVoice::SixOp => "6op",
        SynthVoice::String => "string",
        SynthVoice::Modal => "modal",
        SynthVoice::Dust => "dust",
        SynthVoice::Noise => "noise",
        SynthVoice::Kick => "kick",
        SynthVoice::Snare => "snare",
        SynthVoice::Hat => "hat",
    }
}

/// An effect a slot can hold, apart from its settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Distort,
    Delay,
    Reverb,
    Chorus,
    Eq,
    Compressor,
    Limit,
    Phaser,
    Flanger,
    Tape,
    Plate,
    Pitch,
    Grains,
    Warps,
    Lpg,
    Reso,
    Comb,
    Vowel,
}

impl Effect {
    pub const ALL: [Self; 18] = [
        Self::Distort,
        Self::Delay,
        Self::Reverb,
        Self::Chorus,
        Self::Eq,
        Self::Compressor,
        Self::Limit,
        Self::Phaser,
        Self::Flanger,
        Self::Tape,
        Self::Plate,
        Self::Pitch,
        Self::Grains,
        Self::Warps,
        Self::Lpg,
        Self::Reso,
        Self::Comb,
        Self::Vowel,
    ];

    /// Every setting it has in any mode, in the order its slot keeps them. Distort's modes each
    /// show only their own.
    pub const fn params(self) -> &'static [Param] {
        match self {
            Self::Distort => &DISTORT_PARAMS,
            Self::Delay => &DELAY_PARAMS,
            Self::Reverb => &REVERB_PARAMS,
            Self::Chorus => &CHORUS_PARAMS,
            Self::Eq => &EQ_PARAMS,
            Self::Compressor => &COMPRESSOR_PARAMS,
            Self::Limit => &LIMIT_PARAMS,
            Self::Phaser => &PHASER_PARAMS,
            Self::Flanger => &FLANGER_PARAMS,
            Self::Tape => &TAPE_PARAMS,
            Self::Plate => &PLATE_PARAMS,
            Self::Pitch => &PITCH_PARAMS,
            Self::Grains => &GRAINS_PARAMS,
            Self::Warps => &WARPS_PARAMS,
            Self::Lpg => &LPG_PARAMS,
            Self::Reso => &RESO_PARAMS,
            Self::Comb => &COMB_PARAMS,
            Self::Vowel => &VOWEL_PARAMS,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Distort => "distort",
            Self::Delay => "delay",
            Self::Reverb => "reverb",
            Self::Chorus => "chorus",
            Self::Eq => "eq3",
            Self::Compressor => "comp",
            Self::Limit => "limit",
            Self::Phaser => "phaser",
            Self::Flanger => "flanger",
            Self::Tape => "tape",
            Self::Plate => "plate",
            Self::Pitch => "pitch",
            Self::Grains => "grains",
            Self::Warps => "warps",
            Self::Lpg => "lpg",
            Self::Reso => "reso",
            Self::Comb => "comb",
            Self::Vowel => "vowel",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Distort => "Distort",
            Self::Delay => "Delay",
            Self::Reverb => "Reverb",
            Self::Chorus => "Chorus",
            Self::Eq => "EQ3",
            Self::Compressor => "Comp",
            Self::Limit => "Limit",
            Self::Phaser => "Phaser",
            Self::Flanger => "Flanger",
            Self::Tape => "Tape",
            Self::Plate => "Plate",
            Self::Pitch => "Pitch",
            Self::Grains => "Grains",
            Self::Warps => "Warps",
            Self::Lpg => "LPG",
            Self::Reso => "Reso",
            Self::Comb => "Comb",
            Self::Vowel => "Vowel",
        }
    }

    /// The effect a slot holds, if any.
    pub fn of(insert: Insert) -> Option<Self> {
        match insert {
            Insert::Empty => None,
            Insert::Distort(_) => Some(Self::Distort),
            Insert::Delay(_) => Some(Self::Delay),
            Insert::Reverb(_) => Some(Self::Reverb),
            Insert::Chorus(_) => Some(Self::Chorus),
            Insert::Eq(_) => Some(Self::Eq),
            Insert::Compressor(_) => Some(Self::Compressor),
            Insert::Limit(_) => Some(Self::Limit),
            Insert::Phaser(_) => Some(Self::Phaser),
            Insert::Flanger(_) => Some(Self::Flanger),
            Insert::Tape(_) => Some(Self::Tape),
            Insert::Plate(_) => Some(Self::Plate),
            Insert::Pitch(_) => Some(Self::Pitch),
            Insert::Grains(_) => Some(Self::Grains),
            Insert::Warps(_) => Some(Self::Warps),
            Insert::Lpg(_) => Some(Self::Lpg),
            Insert::Reso(_) => Some(Self::Reso),
            Insert::Comb(_) => Some(Self::Comb),
            Insert::Vowel(_) => Some(Self::Vowel),
        }
    }
}

/// An effect's setting as an entry.
fn param_entry(param: &Param) -> Entry {
    Entry {
        key: param.key,
        name: param.name,
        label: param.label,
        kind: if param.choices.is_empty() {
            let (middle, units) = match param.setting {
                Setting::Gain => (false, Units::Decibels),
                Setting::LowFrequency | Setting::MidFrequency | Setting::HighFrequency => {
                    (false, Units::Hertz)
                }
                Setting::Low | Setting::Mid | Setting::High => (true, Units::Boost),
                Setting::MidQ => (false, Units::Q),
                Setting::Threshold => (false, Units::Threshold),
                Setting::Ratio => (false, Units::Ratio),
                Setting::Attack => (false, Units::Attack),
                Setting::Release => (false, Units::Release),
                Setting::Makeup => (false, Units::Makeup),
                Setting::Range => (false, Units::Range),
                Setting::Input => (false, Units::Input),
                Setting::Ceiling => (false, Units::Ceiling),
                Setting::LimitRelease => (false, Units::LimitRelease),
                Setting::Centre => (false, Units::Hertz),
                Setting::FlangerDelay => (false, Units::FlangerDelay),
                Setting::FlangerFeedback | Setting::Tone | Setting::Bias => (true, Units::Percent),
                Setting::TapeDrive => (true, Units::TapeDrive),
                Setting::Size => (false, Units::PlateSize),
                Setting::Fine => (true, Units::Cents),
                Setting::Grain => (false, Units::Grain),
                Setting::Position => (false, Units::GrainTime),
                Setting::Length => (false, Units::GrainLength),
                Setting::Density => (false, Units::Density),
                Setting::Algorithm => (false, Units::Algorithm),
                Setting::Vowel => (false, Units::Vowel),
                _ => (false, Units::Percent),
            };
            Kind::Continuous {
                top: param.setting.highest(),
                middle,
                units,
            }
        } else {
            Kind::Stepped {
                choices: param.choices,
                wraps: param.wraps,
            }
        },
        default: param.default,
    }
}

/// A mod slot's settings that steps can lock: its depth and polarity, whatever its type, and an
/// LFO's phase and deform.
const MOD: [Entry; 4] = [
    Entry {
        key: "depth",
        name: "Depth",
        // With the slot's number after it, as `DEP1`.
        label: Some("DEP"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::Depth,
        },
        default: 0x80,
    },
    Entry {
        key: "phase",
        name: "Phase",
        label: Some("PHS"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Degrees,
        },
        default: 0x00,
    },
    Entry {
        key: "polarity",
        name: "Polarity",
        label: Some("POL"),
        kind: Kind::Stepped {
            choices: &POLARITY_NAMES,
            wraps: false,
        },
        // Each type has its own, which the track's value gives; this is an LFO's.
        default: 0x01,
    },
    Entry {
        key: "deform",
        name: "Deform",
        label: Some("DFM"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::Depth,
        },
        default: 0x80,
    },
];

/// A polarity's names, in the order of its values.
const POLARITY_NAMES: [&str; 2] = ["Uni", "Bi"];

/// A slot's own, whatever effect is in it.
const SLOT_OWN: [Entry; 1] = [Entry {
    key: "bypass",
    name: "Bypass",
    // With the slot's digit before it, as `1BYP`.
    label: Some("BYP"),
    kind: Kind::Stepped {
        choices: &BYPASS_CHOICES,
        wraps: false,
    },
    default: 0,
}];

/// What a slot's bypass can be: heard, at `00`, or switched out, at `01`.
pub const BYPASS_CHOICES: [&str; 2] = ["In", "Bypassed"];

/// Each choice's two hex digits, for [`BANK_CHOICES`] to point into.
static BANK_DIGITS: [[u8; 2]; 256] = {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut digits = [[0; 2]; 256];
    let mut at = 0;
    while at < 256 {
        digits[at] = [HEX[at >> 4], HEX[at & 0x0F]];
        at += 1;
    }
    digits
};

/// What a sampler's `Sample` can be: `Off`, no sample, then a bank entry's number, `01` to `FF`.
/// What each entry is called is the song's, so it's shown from the bank where there's a song.
pub const BANK_CHOICES: [&str; 256] = {
    let mut choices = ["Off"; 256];
    let mut at = 1;
    while at < 256 {
        choices[at] = match std::str::from_utf8(&BANK_DIGITS[at]) {
            Ok(digits) => digits,
            Err(_) => panic!("hex digits are text"),
        };
        at += 1;
    }
    choices
};

/// What a sampler's `Slice` can be: a slice's number, `00` to `FF`, as the sample page's ruler
/// shows it.
pub const SLICE_CHOICES: [&str; 256] = {
    let mut choices = [""; 256];
    let mut at = 0;
    while at < 256 {
        choices[at] = match std::str::from_utf8(&BANK_DIGITS[at]) {
            Ok(digits) => digits,
            Err(_) => panic!("hex digits are text"),
        };
        at += 1;
    }
    choices
};

/// How a sampler's note plays its sample: through once, round its loop, back and forth in it, or
/// on through the slices after its own, as [`PlayMode`] says.
pub const PLAY_CHOICES: [&str; 4] = ["One-shot", "Loop", "Ping-pong", "Thru"];

/// How a sampler's note plays its sample, its `Play`. A loop is the entry's own if it has one,
/// or else the whole of what plays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlayMode {
    /// From its start to its end, once.
    #[default]
    OneShot,
    /// From its start, then round its loop until the note ends.
    Loop,
    /// From its start, then back and forth in its loop.
    PingPong,
    /// From its slice's start on past the slice's end, to the entry's, once, as tracker-ce's Play
    /// Thru and Simpler's Thru do. Reversed, it plays back past the slice's start to the entry's.
    Thru,
}

impl PlayMode {
    /// The mode at a place in [`PLAY_CHOICES`], or the last for any past it.
    pub fn from_index(index: u8) -> Self {
        match index {
            0 => Self::OneShot,
            1 => Self::Loop,
            2 => Self::PingPong,
            _ => Self::Thru,
        }
    }
}

/// A share of a sample, as `Start` and `Length` take it: `00` none of it and `FF` all of it.
const fn share(key: &'static str, name: &'static str, label: &'static str, default: u8) -> Entry {
    Entry {
        key,
        name,
        label: Some(label),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Percent,
        },
        default,
    }
}

/// How many settings the sampler has.
pub const SAMPLER_SETTINGS: usize = SAMPLER.len();

/// The sampler's own, which it keeps whatever it plays. `Length` isn't `LEN`, which is `Gate`'s.
const SAMPLER: [Entry; 5] = [
    Entry {
        key: "sample",
        name: "Sample",
        label: Some("SMP"),
        kind: Kind::Stepped {
            choices: &BANK_CHOICES,
            wraps: false,
        },
        default: 0,
    },
    Entry {
        key: "slice",
        name: "Slice",
        label: Some("SLI"),
        kind: Kind::Stepped {
            choices: &SLICE_CHOICES,
            wraps: false,
        },
        default: 0,
    },
    share("start", "Start", "STA", 0),
    share("length", "Length", "SLEN", 0xFF),
    Entry {
        key: "play",
        name: "Play",
        label: Some("PLY"),
        kind: Kind::Stepped {
            choices: &PLAY_CHOICES,
            wraps: false,
        },
        default: 0,
    },
];

const fn voices_size() -> usize {
    let mut total = 0;
    let mut at = 0;
    while at < SynthVoice::ALL.len() {
        total += voice_entries(SynthVoice::ALL[at]).len();
        at += 1;
    }
    total
}

/// Where a voice's settings start among the voices'.
const fn voice_start(voice: SynthVoice) -> usize {
    let mut start = 0;
    let mut at = 0;
    while at < SynthVoice::ALL.len() {
        if SynthVoice::ALL[at] as usize == voice as usize {
            return start;
        }
        start += voice_entries(SynthVoice::ALL[at]).len();
        at += 1;
    }
    start
}

const fn effect_start(effect: Effect) -> usize {
    match effect {
        Effect::Distort => 0,
        Effect::Delay => DISTORT_PARAMS.len(),
        Effect::Reverb => DISTORT_PARAMS.len() + DELAY_PARAMS.len(),
        Effect::Chorus => DISTORT_PARAMS.len() + DELAY_PARAMS.len() + REVERB_PARAMS.len(),
        Effect::Eq => {
            DISTORT_PARAMS.len() + DELAY_PARAMS.len() + REVERB_PARAMS.len() + CHORUS_PARAMS.len()
        }
        Effect::Compressor => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
        }
        Effect::Limit => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
        }
        Effect::Phaser => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
                + LIMIT_PARAMS.len()
        }
        Effect::Flanger => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
                + LIMIT_PARAMS.len()
                + PHASER_PARAMS.len()
        }
        Effect::Tape => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
                + LIMIT_PARAMS.len()
                + PHASER_PARAMS.len()
                + FLANGER_PARAMS.len()
        }
        Effect::Plate => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
                + LIMIT_PARAMS.len()
                + PHASER_PARAMS.len()
                + FLANGER_PARAMS.len()
                + TAPE_PARAMS.len()
        }
        Effect::Pitch => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
                + LIMIT_PARAMS.len()
                + PHASER_PARAMS.len()
                + FLANGER_PARAMS.len()
                + TAPE_PARAMS.len()
                + PLATE_PARAMS.len()
        }
        Effect::Grains => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
                + LIMIT_PARAMS.len()
                + PHASER_PARAMS.len()
                + FLANGER_PARAMS.len()
                + TAPE_PARAMS.len()
                + PLATE_PARAMS.len()
                + PITCH_PARAMS.len()
        }
        Effect::Warps => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
                + LIMIT_PARAMS.len()
                + PHASER_PARAMS.len()
                + FLANGER_PARAMS.len()
                + TAPE_PARAMS.len()
                + PLATE_PARAMS.len()
                + PITCH_PARAMS.len()
                + GRAINS_PARAMS.len()
        }
        Effect::Lpg => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
                + LIMIT_PARAMS.len()
                + PHASER_PARAMS.len()
                + FLANGER_PARAMS.len()
                + TAPE_PARAMS.len()
                + PLATE_PARAMS.len()
                + PITCH_PARAMS.len()
                + GRAINS_PARAMS.len()
                + WARPS_PARAMS.len()
        }
        Effect::Reso => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
                + LIMIT_PARAMS.len()
                + PHASER_PARAMS.len()
                + FLANGER_PARAMS.len()
                + TAPE_PARAMS.len()
                + PLATE_PARAMS.len()
                + PITCH_PARAMS.len()
                + GRAINS_PARAMS.len()
                + WARPS_PARAMS.len()
                + LPG_PARAMS.len()
        }
        Effect::Comb => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
                + LIMIT_PARAMS.len()
                + PHASER_PARAMS.len()
                + FLANGER_PARAMS.len()
                + TAPE_PARAMS.len()
                + PLATE_PARAMS.len()
                + PITCH_PARAMS.len()
                + GRAINS_PARAMS.len()
                + WARPS_PARAMS.len()
                + LPG_PARAMS.len()
                + RESO_PARAMS.len()
        }
        Effect::Vowel => {
            DISTORT_PARAMS.len()
                + DELAY_PARAMS.len()
                + REVERB_PARAMS.len()
                + CHORUS_PARAMS.len()
                + EQ_PARAMS.len()
                + COMPRESSOR_PARAMS.len()
                + LIMIT_PARAMS.len()
                + PHASER_PARAMS.len()
                + FLANGER_PARAMS.len()
                + TAPE_PARAMS.len()
                + PLATE_PARAMS.len()
                + PITCH_PARAMS.len()
                + GRAINS_PARAMS.len()
                + WARPS_PARAMS.len()
                + LPG_PARAMS.len()
                + RESO_PARAMS.len()
                + COMB_PARAMS.len()
        }
    }
}

const VOICES_START: usize = TRACK.len();
const SLOTS_START: usize = VOICES_START + voices_size();
/// The places a slot's settings take: one for each setting of each effect it could hold.
const SLOT_SIZE: usize = DISTORT_PARAMS.len()
    + DELAY_PARAMS.len()
    + REVERB_PARAMS.len()
    + CHORUS_PARAMS.len()
    + EQ_PARAMS.len()
    + COMPRESSOR_PARAMS.len()
    + LIMIT_PARAMS.len()
    + PHASER_PARAMS.len()
    + FLANGER_PARAMS.len()
    + TAPE_PARAMS.len()
    + PLATE_PARAMS.len()
    + PITCH_PARAMS.len()
    + GRAINS_PARAMS.len()
    + WARPS_PARAMS.len()
    + LPG_PARAMS.len()
    + RESO_PARAMS.len()
    + COMB_PARAMS.len()
    + VOWEL_PARAMS.len();
const MODS_START: usize = SLOTS_START + INSERTS * SLOT_SIZE;
const BYPASS_START: usize = MODS_START + MOD_SLOTS * MOD.len();
const SAMPLER_START: usize = BYPASS_START + INSERTS * SLOT_OWN.len();

/// What a setting belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Track,
    Voice(SynthVoice),
    Slot {
        slot: usize,
        effect: Effect,
    },
    /// One of the track's mod slots, by its place, whatever its type.
    Mod(usize),
    /// A slot's bypass, which is the slot's whatever effect is in it.
    Bypass(usize),
    /// The sampler machine's.
    Sampler,
}

/// A setting's place in the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SettingId(u16);

/// How many faders a track has.
pub const FADER_COUNT: usize = 4;

/// A track's faders, in the mixer's order: the settings a step's `LEV`, `PAN`, `SNA` and `SNB` set
/// absolutely, which the mixer, the Output device and the Roto show as the steps move them, and
/// which a hand takes over from the steps (decision 174).
pub const FADERS: [SettingId; FADER_COUNT] = [
    SettingId::LEVEL,
    SettingId::PAN,
    SettingId::SEND_A,
    SettingId::SEND_B,
];

impl SettingId {
    pub const VOL: Self = Self(0);
    pub const PAN: Self = Self(1);
    pub const CUT: Self = Self(2);
    pub const RES: Self = Self(3);
    pub const SEND_A: Self = Self(4);
    pub const SEND_B: Self = Self(5);
    /// In semitones, from the track's own pitch.
    pub const PITCH: Self = Self(6);
    /// The filter's mode, a place in [`FilterMode::ALL`](crate::song::FilterMode::ALL).
    pub const FILTER_MODE: Self = Self(7);
    /// How much of its last pulse a note lasts, as [`gate_share`] gives it.
    pub const GATE_LENGTH: Self = Self(8);
    /// How far a note's length is moved up or down from its own, as [`gate_delta`] gives it.
    pub const GATE_DELTA: Self = Self(9);
    /// The groove a step plays with: `0` for the project's, then a place in
    /// [`GROOVE_NAMES`](crate::groove::GROOVE_NAMES) after it.
    pub const GROOVE: Self = Self(10);
    /// How long a step is: a place in [`Speed::ALL`].
    pub const SPEED: Self = Self(11);
    /// How long a tie glides into the next note, as [`glide_ms`] gives it.
    pub const GLIDE: Self = Self(12);
    /// How far a note's velocity is moved up or down from its own, as [`velocity_delta`] gives it.
    pub const VELOCITY_DELTA: Self = Self(13);
    /// The track's fader, its `Level`, on the level scale.
    pub const LEVEL: Self = Self(14);
    /// Which of the bank's entries a sampler plays: `00` none, or an entry's number.
    pub const SAMPLE: Self = Self(SAMPLER_START as u16);
    /// Which of its entry's slices a sampler plays, by number, clamped to the last.
    pub const SLICE: Self = Self(SAMPLER_START as u16 + 1);
    /// Where in its slice a sampler's note starts, as a share of the whole.
    pub const SAMPLE_START: Self = Self(SAMPLER_START as u16 + 2);
    /// How much of its slice a sampler's note plays from its start, as a share of the whole.
    pub const SAMPLE_LENGTH: Self = Self(SAMPLER_START as u16 + 3);
    /// How a sampler's note plays: a place in [`PLAY_CHOICES`].
    pub const PLAY: Self = Self(SAMPLER_START as u16 + 4);

    /// How many settings there are.
    pub const COUNT: usize = SAMPLER_START + SAMPLER.len();

    /// Every setting, in the table's order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..Self::COUNT as u16).map(Self)
    }

    /// Its place, for an array of every setting.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Its place among the track's faders, [`FADERS`], if it's one.
    pub fn fader(self) -> Option<usize> {
        FADERS.iter().position(|fader| *fader == self)
    }

    /// One of the track's own, by its place among them.
    pub const fn track(setting: usize) -> Option<Self> {
        if setting < TRACK.len() {
            Some(Self(setting as u16))
        } else {
            None
        }
    }

    /// One of a voice's settings, by its place among the voice's.
    pub const fn voice(voice: SynthVoice, setting: usize) -> Option<Self> {
        if setting < voice_entries(voice).len() {
            Some(Self((VOICES_START + voice_start(voice) + setting) as u16))
        } else {
            None
        }
    }

    /// One of an effect's settings, in a slot.
    pub const fn slot(slot: usize, effect: Effect, setting: usize) -> Option<Self> {
        if slot < INSERTS && setting < effect.params().len() {
            Some(Self(
                (SLOTS_START + slot * SLOT_SIZE + effect_start(effect) + setting) as u16,
            ))
        } else {
            None
        }
    }

    /// One of a mod slot's settings: 0 is depth, 1 is phase, 2 is polarity.
    pub const fn modulator(slot: usize, setting: usize) -> Option<Self> {
        if slot < MOD_SLOTS && setting < MOD.len() {
            Some(Self((MODS_START + slot * MOD.len() + setting) as u16))
        } else {
            None
        }
    }

    /// One of the sampler's settings, by its place among them: 0 is `Sample`.
    pub const fn sampler(setting: usize) -> Option<Self> {
        if setting < SAMPLER.len() {
            Some(Self((SAMPLER_START + setting) as u16))
        } else {
            None
        }
    }

    /// A slot's bypass: `00` heard, `01` switched out.
    pub const fn bypass(slot: usize) -> Option<Self> {
        if slot < INSERTS {
            Some(Self((BYPASS_START + slot * SLOT_OWN.len()) as u16))
        } else {
            None
        }
    }

    /// What it belongs to, and its place among that owner's settings.
    pub fn owner(self) -> (Owner, usize) {
        let at = self.index();
        if at >= SAMPLER_START {
            return (Owner::Sampler, at - SAMPLER_START);
        }
        if at >= BYPASS_START {
            let within = at - BYPASS_START;
            return (
                Owner::Bypass(within / SLOT_OWN.len()),
                within % SLOT_OWN.len(),
            );
        }
        if at < VOICES_START {
            return (Owner::Track, at);
        }
        if at < SLOTS_START {
            let within = at - VOICES_START;
            for voice in SynthVoice::ALL {
                let start = voice_start(voice);
                if within < start + voice_entries(voice).len() {
                    return (Owner::Voice(voice), within - start);
                }
            }
        }
        if at < MODS_START {
            let within = at - SLOTS_START;
            let (slot, within) = (within / SLOT_SIZE, within % SLOT_SIZE);
            for effect in Effect::ALL {
                let start = effect_start(effect);
                if within < start + effect.params().len() {
                    return (Owner::Slot { slot, effect }, within - start);
                }
            }
        }
        let within = at - MODS_START;
        (Owner::Mod(within / MOD.len()), within % MOD.len())
    }

    /// What its owner declares.
    pub fn entry(self) -> Entry {
        match self.owner() {
            (Owner::Track, at) => TRACK[at],
            (Owner::Voice(voice), at) => voice_entries(voice)[at],
            (Owner::Slot { effect, .. }, at) => param_entry(&effect.params()[at]),
            (Owner::Mod(_), at) => MOD[at],
            (Owner::Bypass(_), at) => SLOT_OWN[at],
            (Owner::Sampler, at) => SAMPLER[at],
        }
    }

    /// The name songs store it by, which never changes, like `cut`, `string.decay`,
    /// `2.delay.feedback` or `mod4.depth`.
    pub fn key(self) -> String {
        let key = self.entry().key;
        match self.owner().0 {
            Owner::Track => key.to_owned(),
            Owner::Voice(voice) => format!("{}.{key}", voice_key(voice)),
            Owner::Slot { slot, effect } => format!("{}.{}.{key}", slot + 1, effect.key()),
            Owner::Mod(slot) => format!("mod{}.{key}", slot + 1),
            Owner::Bypass(slot) => format!("{}.{key}", slot + 1),
            Owner::Sampler => format!("sampler.{key}"),
        }
    }

    /// The setting a stored key names.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::all().find(|id| id.key() == key)
    }

    /// Its label in the grid, with its slot's digit before an effect's and its mod slot's number
    /// after a modulator's, if steps can lock it.
    pub fn label(self) -> Option<String> {
        let label = self.entry().label?;
        Some(match self.owner().0 {
            Owner::Slot { slot, .. } | Owner::Bypass(slot) => format!("{}{label}", slot + 1),
            Owner::Mod(slot) => format!("{label}{}", slot + 1),
            _ => label.to_owned(),
        })
    }

    /// What it's called in lists.
    pub fn name(self) -> &'static str {
        self.entry().name
    }

    /// What it's listed with.
    pub fn group(self) -> String {
        match self.owner() {
            (Owner::Track, at) => TRACK_GROUPS[at].to_owned(),
            (Owner::Voice(voice), _) => voice.name().to_owned(),
            (Owner::Slot { slot, effect }, _) => format!("FX {} {}", slot + 1, effect.name()),
            (Owner::Mod(slot), _) => format!("Mod {}", slot + 1),
            (Owner::Bypass(slot), _) => format!("FX {}", slot + 1),
            (Owner::Sampler, _) => "Sampler".to_owned(),
        }
    }
}

/// Songs store a setting by its key, which never changes.
impl serde::Serialize for SettingId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.key())
    }
}

impl<'de> serde::Deserialize<'de> for SettingId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Self::from_key(&key)
            .ok_or_else(|| serde::de::Error::custom(format!("there's no setting called {key}")))
    }
}

/// The events, which act on a note and aren't settings. Their labels share the grid with
/// settings', so none may match one.
pub const EVENTS: [FxCommand; 8] = [
    FxCommand::Del,
    FxCommand::Kil,
    FxCommand::Off,
    FxCommand::Ret,
    FxCommand::Cha,
    FxCommand::Cnd,
    FxCommand::Acu,
    FxCommand::Arp,
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn labels(ids: impl Iterator<Item = SettingId>) -> Vec<String> {
        ids.filter_map(SettingId::label).collect()
    }

    #[test]
    fn every_place_names_the_setting_it_holds() {
        for id in SettingId::all() {
            let back = match id.owner() {
                (Owner::Track, at) => SettingId::track(at),
                (Owner::Voice(voice), at) => SettingId::voice(voice, at),
                (Owner::Slot { slot, effect }, at) => SettingId::slot(slot, effect, at),
                (Owner::Mod(slot), at) => SettingId::modulator(slot, at),
                (Owner::Bypass(slot), _) => SettingId::bypass(slot),
                (Owner::Sampler, at) => SettingId::sampler(at),
            };
            assert_eq!(back, Some(id), "{}", id.key());
        }
        assert_eq!(SettingId::all().count(), SettingId::COUNT);
    }

    #[test]
    fn every_key_is_unique_and_finds_its_setting() {
        let mut seen = HashSet::new();
        for id in SettingId::all() {
            assert!(seen.insert(id.key()), "{} twice", id.key());
            assert_eq!(SettingId::from_key(&id.key()), Some(id));
        }
        assert_eq!(
            SettingId::from_key("2.delay.feedback"),
            SettingId::slot(1, Effect::Delay, 1)
        );
        assert_eq!(
            SettingId::from_key("string.decay"),
            SettingId::voice(SynthVoice::String, 2)
        );
    }

    #[test]
    fn the_level_scale_is_two_digits_and_positions_are_four() {
        for id in [
            SettingId::VOL,
            SettingId::LEVEL,
            SettingId::SEND_A,
            SettingId::SEND_B,
        ] {
            assert!(!id.entry().kind.has_fine(), "{}", id.name());
            assert!(FxCommand::Set(id).glides(), "{} still glides", id.name());
        }
        for id in [
            SettingId::PAN,
            SettingId::CUT,
            SettingId::RES,
            SettingId::PITCH,
        ] {
            assert!(id.entry().kind.has_fine(), "{}", id.name());
        }
        assert!(!SettingId::SPEED.entry().kind.has_fine(), "a list");
        assert_eq!(SettingId::LEVEL.label().as_deref(), Some("LEV"));
        assert_eq!(SettingId::LEVEL.group(), "Output");
        assert_eq!(
            FADERS.map(|id| id.fader()),
            [Some(0), Some(1), Some(2), Some(3)]
        );
        assert_eq!(
            SettingId::VOL.fader(),
            None,
            "VOL is a stage on top, not a fader"
        );
    }

    #[test]
    fn every_label_is_three_or_four_letters_after_a_slots_digit() {
        for id in SettingId::all() {
            let Some(label) = id.label() else { continue };
            let letters = match id.owner().0 {
                Owner::Slot { slot, .. } | Owner::Bypass(slot) => {
                    assert!(label.starts_with(&(slot + 1).to_string()), "{label}");
                    &label[1..]
                }
                Owner::Mod(slot) => {
                    assert!(label.ends_with(&(slot + 1).to_string()), "{label}");
                    &label[..label.len() - 1]
                }
                _ => &label[..],
            };
            assert!(
                (3..=4).contains(&letters.len()) && letters.bytes().all(|b| b.is_ascii_uppercase()),
                "{label}"
            );
        }
        for event in EVENTS {
            assert_eq!(event.name().len(), 3);
        }
    }

    #[test]
    fn every_setting_has_a_name_and_a_group() {
        for id in SettingId::all() {
            assert!(!id.name().is_empty(), "{}", id.key());
            assert!(!id.group().is_empty(), "{}", id.key());
        }
    }

    #[test]
    fn no_two_labels_on_a_track_match() {
        let track_and_events = || {
            labels((0..TRACK.len()).filter_map(SettingId::track))
                .into_iter()
                .chain(labels((0..MOD_SLOTS).flat_map(|slot| {
                    (0..MOD.len()).filter_map(move |at| SettingId::modulator(slot, at))
                })))
                .chain(EVENTS.map(|event| event.name().to_owned()))
        };
        for voice in SynthVoice::ALL {
            let mut seen = HashSet::new();
            let voice_labels = labels(
                (0..voice_entries(voice).len()).filter_map(|at| SettingId::voice(voice, at)),
            );
            for label in track_and_events().chain(voice_labels) {
                assert!(seen.insert(label.clone()), "{label} twice on {voice:?}");
            }
        }
        let mut seen = HashSet::new();
        let sampler = labels((0..SAMPLER.len()).filter_map(SettingId::sampler));
        assert_eq!(sampler, ["SMP", "SLI", "STA", "SLEN", "PLY"]);
        for label in track_and_events().chain(sampler) {
            assert!(seen.insert(label.clone()), "{label} twice on the sampler");
        }
        for effect in Effect::ALL {
            let mut seen = HashSet::from([SettingId::bypass(0).unwrap().label().unwrap()]);
            for at in 0..effect.params().len() {
                let Some(label) = SettingId::slot(0, effect, at).unwrap().label() else {
                    continue;
                };
                assert!(seen.insert(label.clone()), "{label} twice on {effect:?}");
            }
        }
    }

    #[test]
    fn a_voices_defaults_are_its_own() {
        let six_op = |at| SettingId::voice(SynthVoice::SixOp, at).unwrap().entry();
        assert_eq!(PATCH_NAMES[usize::from(six_op(0).default)], "E.PIANO 1");
        assert!(matches!(six_op(0).kind, Kind::Stepped { choices, .. } if choices.len() == 96));
        assert_eq!(six_op(1).default, 0x80, "Bright as the patch was made");
        let string = SettingId::voice(SynthVoice::String, 0).unwrap().entry();
        assert_eq!(string.default, 0x40);
        assert_eq!(
            SettingId::voice(SynthVoice::Kick, 3)
                .unwrap()
                .entry()
                .default,
            DEFAULT_ACCENT
        );
        let drift = SettingId::voice(SynthVoice::VirtualAnalog, 3)
            .unwrap()
            .entry();
        assert_eq!(
            (drift.key, drift.default),
            ("drift", 0x00),
            "none unless asked"
        );
        assert_eq!(SettingId::voice(SynthVoice::VirtualAnalog, 4), None);
    }

    #[test]
    fn a_value_reads_in_its_settings_units() {
        let shown = |id: SettingId, value: u16| id.entry().kind.shown(value);
        assert_eq!(shown(SettingId::CUT, 0xFF00), "20.0 kHz");
        assert_eq!(shown(SettingId::CUT, 0x0000), "20 Hz");
        assert_eq!(shown(SettingId::VOL, 0xE000), "+0.0 dB");
        assert_eq!(shown(SettingId::VOL, 0xC000), "-8.0 dB");
        assert_eq!(shown(SettingId::VOL, 0x0000), "-inf dB");
        assert_eq!(shown(SettingId::PAN, 0x8000), "Centre");
        assert_eq!(shown(SettingId::PAN, 0x0000), "100 % L");
        assert_eq!(shown(SettingId::PAN, 0xFF00), "100 % R");
        assert_eq!(shown(SettingId::RES, 0x8000), "50 %");
        let patch = SettingId::voice(SynthVoice::SixOp, 0).unwrap();
        assert_eq!(shown(patch, 0x2000), "E.PIANO 1");
        let detune = SettingId::voice(SynthVoice::VirtualAnalog, 0).unwrap();
        assert_eq!(shown(detune, 0x8000), "+0 %");
        let gain = SettingId::slot(0, Effect::Reverb, 6).unwrap();
        assert_eq!(shown(gain, 0xE000), "+0.0 dB");
        let (depth, phase) = (
            SettingId::modulator(0, 0).unwrap(),
            SettingId::modulator(5, 1).unwrap(),
        );
        assert_eq!(shown(depth, crate::modulation::depth_value(0x40)), "+40");
        assert_eq!(shown(depth, 0x8000), "00");
        assert_eq!(shown(phase, crate::lfo::phase_value(90)), "90°");
        assert_eq!(shown(SettingId::PITCH, 0x8700), "+7 st");
        assert_eq!(shown(SettingId::PITCH, 0x8780), "+7 st +50 ct");
        assert_eq!(shown(SettingId::PITCH, 0x7F80), "+0 st -50 ct");
        assert_eq!(shown(SettingId::GATE_DELTA, 0x8000), "+0 %");
        assert_eq!(shown(SettingId::GATE_DELTA, 0x0000), "-50 %");
        assert_eq!(shown(SettingId::GATE_DELTA, 0xC000), "+25 %");
        assert_eq!(SettingId::GATE_DELTA.label(), Some("LDLT".into()));
        assert_eq!(shown(SettingId::VELOCITY_DELTA, 0x8000), "+00 vel");
        assert_eq!(shown(SettingId::VELOCITY_DELTA, 0x9000), "+10 vel");
        assert_eq!(shown(SettingId::VELOCITY_DELTA, 0x0000), "-80 vel");
        assert_eq!(SettingId::VELOCITY_DELTA.label(), Some("VDLT".into()));
        assert_eq!(
            (depth.label(), phase.label(), SettingId::PITCH.label()),
            (
                Some("DEP1".into()),
                Some("PHS6".into()),
                Some("PTCH".into())
            )
        );
        assert!(phase.entry().kind.wraps() && !depth.entry().kind.wraps());
        let eq = |at| SettingId::slot(0, Effect::Eq, at).unwrap();
        assert_eq!(shown(eq(0), 0x3B00), "99 Hz");
        assert_eq!(shown(eq(1), 0x8000), "+0.0 dB");
        assert_eq!(shown(eq(1), 0xFF00), "+15.0 dB");
        assert_eq!(shown(eq(1), 0x0000), "-15.0 dB");
        assert_eq!(shown(eq(6), 0x4000), "Q 0.71");
        let makeup = SettingId::from_key("1.comp.makeup").unwrap();
        assert_eq!(makeup.label(), Some("1MKUP".into()));
        assert_eq!(shown(makeup, 0x0000), "+0.0 dB");
        assert_eq!(shown(makeup, 0x8000), "+12.0 dB");
        assert_eq!(shown(makeup, 0xFF00), "+24.0 dB");
        let algo = SettingId::slot(0, Effect::Warps, 0).unwrap();
        assert_eq!(shown(algo, 0x0000), "Xfade");
        assert_eq!(shown(algo, 0x6000), "Digital");
        assert_eq!(shown(algo, 0x7000), "Digital>XOR");
        assert_eq!(shown(algo, 0xB300), "Mod");
        assert_eq!(shown(algo, 0xD000), "Vocoder");
        assert_eq!(shown(algo, 0xFF00), "Vocoder hold");
        let source = SettingId::slot(0, Effect::Warps, 2).unwrap();
        assert_eq!(shown(source, 0x0000), "Osc");
        assert_eq!(shown(source, 0x4100), "A");
    }

    #[test]
    fn a_lock_shows_its_settings_label_with_its_slots_digit() {
        assert_eq!(FxCommand::CUT.name(), "CUT");
        assert_eq!(FxCommand::Del.name(), "DEL");
        let punch = SettingId::voice(SynthVoice::Kick, 0).unwrap();
        assert_eq!(FxCommand::Set(punch).name(), "PNCH");
        let feedback = SettingId::slot(1, Effect::Delay, 1).unwrap();
        assert_eq!(FxCommand::Set(feedback).name(), "2FDBK");
        assert_eq!(FxCommand::Set(feedback).key(), "2.delay.feedback");
        assert_eq!(
            FxCommand::from_key("2.delay.feedback"),
            Some(FxCommand::Set(feedback))
        );
        assert_eq!(FxCommand::from_key("del"), Some(FxCommand::Del));
        assert_eq!(FxCommand::from_key("CUT"), None, "keys are lower case");
    }
}
