//! What values a setting takes and how they read in real units, with the conversions the
//! engine shares with the status line.

use crate::constants::{DB_PER_LEVEL, UNITY_LEVEL};
use crate::envelope::{Hold, hold_at, time_at};
use crate::groove::swing_tenths;
use crate::insert::{
    ALGORITHM_NAMES, ECHO_BASS, ECHO_TREBLE, Triggering, VOWEL_NAMES, algorithm_blend, attack_ms,
    ceiling_db, colour_drive_db, colour_emphasis_db, deck_azimuth_us, deck_crosstalk_db,
    deck_drive_db, deck_flutter_rate, deck_wow_rate, echo_head_seconds, echo_input_db,
    echo_shelf_db, exciter_attack_ms, exciter_release_ms, exciter_tone_hz, fine_cents,
    flanger_delay_ms, grain_density, grain_length, grain_ms, grain_time_seconds, hall_lows,
    input_db, limit_release_ms, makeup_db, plate_size, range_db, ratio, reel_drive_db, reel_gap_um,
    reel_spacing_um, reel_thickness_um, release_ms, repeat_decay_db, reverb_decay_seconds,
    shift_delay_ms, shift_hertz, spread_hertz, tape_drive_db, threshold_db, vowel_blend,
};
use crate::modulation::{depth_of, depth_text};
use crate::step::MAX_VELOCITY;

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
    /// A tape's drive, in dB either way of none, a reel's, as an input gain, and a deck's.
    TapeDrive,
    ReelDrive,
    DeckDrive,
    /// How fast a deck.s wow and flutter go against its format.s, as so many times, how late one
    /// side plays by its azimuth, in µs, and its crosstalk in dB, or none.
    DeckWowRate,
    DeckFlutterRate,
    DeckAzimuth,
    DeckCrosstalk,
    /// An analog colour's drive in dB from none up, and how far its emphasis lifts the highs
    /// before its curve, in dB either way.
    ColourDrive,
    ColourEmphasis,
    /// A reel's playback gap, its tape's spacing from the head and the depth it's read from, in µm.
    ReelGap,
    ReelSpacing,
    ReelThickness,
    /// A Space Echo's first head's delay, its drive and its shelves, in ms and dB.
    EchoRate,
    EchoInput,
    EchoBass,
    EchoTreble,
    /// A reverb's tail, as how long it takes to fall 60 dB.
    ReverbDecay,
    /// How long a hall's lows ring against its `Decay`, as so many times.
    LowDecay,
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
    /// A frequency shifter's shift in Hz either way, how far apart its sides are in Hz, and the
    /// delay in its loop in ms.
    ShiftHertz,
    SpreadHertz,
    LoopTime,
    /// How much quieter each of a stutter's repeats is than the one before.
    RepeatDecay,
    /// Where an exciter's highs start, and how fast its detector rises and falls.
    ExciterTone,
    ExciterAttack,
    ExciterRelease,
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
    /// An envelope's time, as [`time_at`] gives it.
    EnvelopeTime,
    /// An AHD's hold, as [`hold_at`] gives it: a time, or the note at the top.
    HoldTime,
    /// Rows either side of `80`, one a step, as a lane's `Length` moves its lane.
    Rows,
    /// The track's utility high-pass and low-pass, in Hz as [`Units::Hertz`] reads, but off at
    /// the end where each stands aside: the bottom for the high-pass, the top for the low-pass.
    LowCut,
    HighCut,
    /// Places in the list of orders either side of `80`, one a step, as `ORD` moves a phrase's.
    Orders,
    /// A track's swing, from 50.0% at `00` up a tenth of a percent a step, as
    /// [`swing_tenths`](crate::groove::swing_tenths) gives it.
    Swing,
}

/// How close to an end the utility filter's cuts count as off, as a share of their range: the
/// engine's `TRIM_OPEN`.
const CUT_OFF: f64 = 0.001;

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

/// A chance of `chance` out of `top` moved by a `CDLT` value, out of the same `top` (decision
/// 405). Below `80` every chance is scaled down toward never, so `40` halves each and `00` plays
/// nothing; above it the rest of the way to always is closed, so `C0` brings in half of what each
/// was missing and `FF` plays every one. `80` leaves it as it is, to the bit. Either way a row's
/// chances keep their order, and the fader's turn is the share of rows heard.
pub fn moved_chance(chance: f32, top: f32, value: f32) -> f32 {
    let side = side(f64::from(value.clamp(0.0, 255.0))) as f32;
    if side < 0.0 {
        chance * (1.0 + side)
    } else {
        chance + (top - chance) * side
    }
}

/// A row's nudge, in ticks, as it plays under a `QNT` value: brought that far toward the grid, so
/// `00` plays it as written, `80` halves it and `FF` plays it on the grid.
pub fn quantised_nudge(ticks: i64, value: f32) -> i64 {
    let kept = 1.0 - value.clamp(0.0, 255.0) / 255.0;
    (ticks as f32 * kept).round() as i64
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
        matches!(
            self,
            Self::Continuous { units, .. } if !matches!(units, Units::Decibels | Units::Rows | Units::Orders | Units::Swing)
        )
    }

    /// Whether it goes round past either end, as a phase does.
    pub(crate) fn wraps(self) -> bool {
        matches!(
            self,
            Self::Continuous {
                units: Units::Degrees,
                ..
            }
        )
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
                Units::LowCut if coarse / 255.0 <= CUT_OFF => "Off".to_owned(),
                Units::HighCut if coarse / 255.0 >= 1.0 - CUT_OFF => "Off".to_owned(),
                Units::Hertz | Units::LowCut | Units::HighCut => {
                    hertz(20.0 * 1000_f64.powf(coarse / 255.0))
                }
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
                Units::ReelDrive => format!("{:+.1} dB", reel_drive_db(coarse as f32)),
                Units::DeckDrive => format!("{:+.1} dB", deck_drive_db(coarse as f32)),
                Units::DeckWowRate => format!("{:.2}x", deck_wow_rate(coarse as f32)),
                Units::DeckFlutterRate => format!("{:.2}x", deck_flutter_rate(coarse as f32)),
                Units::DeckAzimuth => match deck_azimuth_us(coarse as f32) {
                    us if us.abs() < 0.5 => "Square".to_owned(),
                    us if us < 0.0 => format!("R {:.0} µs late", -us),
                    us => format!("L {us:.0} µs late"),
                },
                Units::DeckCrosstalk => deck_crosstalk_db(coarse as f32)
                    .map_or_else(|| "Off".to_owned(), |db| format!("{db:.1} dB")),
                Units::ColourDrive => format!("+{:.1} dB", colour_drive_db(coarse as f32)),
                Units::ColourEmphasis => {
                    format!("{:+.1} dB", colour_emphasis_db(coarse as f32))
                }
                Units::ReelGap => microns(reel_gap_um(coarse as f32)),
                Units::ReelSpacing => microns(reel_spacing_um(coarse as f32)),
                Units::ReelThickness => microns(reel_thickness_um(coarse as f32)),
                Units::EchoRate => milliseconds(echo_head_seconds(coarse as f32) * 1000.0),
                Units::EchoInput => format!("{:+.1} dB", echo_input_db(coarse as f32)),
                Units::EchoBass => format!("{:+.1} dB", echo_shelf_db(coarse as f32, ECHO_BASS)),
                Units::EchoTreble => {
                    format!("{:+.1} dB", echo_shelf_db(coarse as f32, ECHO_TREBLE))
                }
                Units::ReverbDecay => milliseconds(reverb_decay_seconds(coarse as f32) * 1000.0),
                Units::LowDecay => format!("{:.2}x", hall_lows(coarse as f32)),
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
                Units::ShiftHertz => match shift_hertz(coarse as f32) {
                    shift if shift.abs() < 0.005 => "0 Hz".to_owned(),
                    shift if shift < 0.0 => format!("-{}", shift_text(-shift)),
                    shift => format!("+{}", shift_text(shift)),
                },
                Units::SpreadHertz => shift_text(spread_hertz(coarse as f32)),
                Units::LoopTime => match shift_delay_ms(coarse as f32) {
                    ms if ms < 0.005 => "0 ms".to_owned(),
                    ms => milliseconds(ms),
                },
                Units::RepeatDecay => match repeat_decay_db(coarse as f32) {
                    db if db < 0.05 => "0 dB".to_owned(),
                    db => format!("-{db:.1} dB"),
                },
                Units::ExciterTone => hertz(f64::from(exciter_tone_hz(coarse as f32))),
                Units::ExciterAttack => milliseconds(exciter_attack_ms(coarse as f32)),
                Units::ExciterRelease => milliseconds(exciter_release_ms(coarse as f32)),
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
                Units::EnvelopeTime => format!("{} ms", time_at(coarse as f32)),
                Units::HoldTime => match hold_at(coarse as f32) {
                    Hold::Note => "Note".to_owned(),
                    Hold::Ms(ms) => format!("{ms} ms"),
                },
                Units::Rows if middle => rows_text((coarse.round() as i32 - 128).clamp(-128, 127)),
                Units::Rows => rows_text(coarse.round() as i32),
                Units::Orders => orders_text((coarse.round() as i32 - 128).clamp(-128, 127)),
                Units::Swing => {
                    let tenths = swing_tenths(coarse.round().clamp(0.0, 255.0) as u8);
                    format!("{}.{} %", tenths / 10, tenths % 10)
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

/// A count of rows either way, as `+2 rows` or `-1 row`.
pub fn rows_text(rows: i32) -> String {
    let unit = if rows.abs() == 1 { "row" } else { "rows" };
    format!("{rows:+} {unit}")
}

/// How far on in the list of orders `ORD` moves a phrase's own: `Own` at none, else `+2 on`.
pub fn orders_text(places: i32) -> String {
    match places {
        0 => "Own".to_owned(),
        places => format!("{places:+} on"),
    }
}

/// A time as it reads best: `0.25 ms`, `12 ms` or `1.00 s`.
/// A length in µm: to two places below one, and one above.
fn microns(um: f32) -> String {
    if um < 1.0 {
        format!("{um:.2} µm")
    } else {
        format!("{um:.1} µm")
    }
}

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

/// A frequency shifter's shift as it reads best, finer than a cutoff, as a shift of a few Hz is
/// heard: `0.25 Hz`, `2.5 Hz`, `16 Hz` or `1.00 kHz`.
fn shift_text(hertz: f32) -> String {
    match hertz {
        hertz if hertz < 1000.0 => rate(hertz),
        hertz => format!("{:.2} kHz", hertz / 1000.0),
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
