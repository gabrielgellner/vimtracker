//! The effects that put a sound in a space or repeat it: a delay, the reverbs and a stutter.

use serde::{Deserialize, Serialize};

use crate::constants::UNITY_LEVEL;
use crate::lfo::DIVISION_NAMES;

use super::{
    Columns, Entry, GAIN_PARAM, Kind, MIX_PARAM, OFF_ON, Place, RETIMES, Units, WET_MIX_PARAM,
    from_middle, hex_param, is_full, is_zero,
};

/// A delay's, in the order the page shows them.
pub(crate) const DELAY_PARAMS: [Entry; 11] = [
    Entry {
        meaning: "how long each repeat takes to come round",
        name: "Time",
        key: "time",
        label: Some("TIME"),
        kind: Kind::Stepped {
            choices: &DIVISION_NAMES,
            wraps: false,
        },
        default: Delay::DEFAULT.time,
        place: Place::Own,
    },
    hex_param("Feedback", "feedback", "FDBK", Delay::DEFAULT.feedback)
        .means("how much of each repeat comes round again; FF keeps going"),
    Entry {
        meaning: "whether the repeats go from side to side or stay put",
        name: "Ping-pong",
        key: "pingpong",
        label: Some("PING"),
        kind: Kind::Stepped {
            choices: &OFF_ON,
            wraps: true,
        },
        default: Delay::DEFAULT.ping_pong,
        place: Place::Own,
    },
    hex_param("Width", "width", "WDTH", Delay::DEFAULT.width)
        .means("how far apart the sides are, from together at 00"),
    hex_param("Low cut", "lowcut", "LCUT", Delay::DEFAULT.low_cut)
        .means("takes lows from each repeat, off at 00"),
    hex_param("High cut", "highcut", "HCUT", Delay::DEFAULT.high_cut)
        .means("takes highs from each repeat, off at FF"),
    hex_param("Drive", "drive", "DRIV", Delay::DEFAULT.drive)
        .means("how hard each repeat is saturated as it comes round"),
    Entry {
        meaning: "how a new Time comes in: gliding, which bends the pitch, or jumping",
        name: "Retime",
        key: "retime",
        label: Some("RTIM"),
        kind: Kind::Stepped {
            choices: &RETIMES,
            wraps: true,
        },
        default: Delay::DEFAULT.retime,
        place: Place::Own,
    },
    hex_param("Wow", "wow", "WOW", Delay::DEFAULT.wow)
        .means("a slow wobble in the repeats' pitch, as worn tape has"),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// Where its settings sit on the FX page. Its repeats, their tone, how it gets to a new time, then what comes out.
pub(super) const DELAY_GRID: Columns = &[
    &["time", "feedback", "pingpong"],
    &["lowcut", "highcut", "drive"],
    &["retime", "wow"],
    &["width", "mix", "gain"],
];

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

    effect_settings! {
        time,
        feedback,
        ping_pong,
        width,
        low_cut,
        high_cut,
        drive,
        retime,
        wow,
        mix,
        gain,
    }
}

/// The Space Echo's modes, as the RE-201's mode selector has them: its playback heads alone and
/// together, from the first, the closest to the record head, to the third, then the same with the
/// spring reverb, and the reverb alone. Each is named by its number on the selector, the heads it
/// plays and whether the reverb is on.
pub(crate) const ECHO_MODE_NAMES: [&str; 12] = [
    "1: H1",
    "2: H2",
    "3: H3",
    "4: H2 H3",
    "5: H1 +R",
    "6: H2 +R",
    "7: H3 +R",
    "8: H1 H2 +R",
    "9: H2 H3 +R",
    "10: H1 H3 +R",
    "11: H1 H2 H3 +R",
    "12: R",
];

/// Which of the three heads each mode plays, and whether its reverb is on.
const ECHO_MODES: [([bool; 3], bool); 12] = [
    ([true, false, false], false),
    ([false, true, false], false),
    ([false, false, true], false),
    ([false, true, true], false),
    ([true, false, false], true),
    ([false, true, false], true),
    ([false, false, true], true),
    ([true, true, false], true),
    ([false, true, true], true),
    ([true, false, true], true),
    ([true, true, true], true),
    ([false, false, false], true),
];

/// The heads a Space Echo's mode plays, the first to the third, and whether its reverb is on.
pub fn echo_mode(mode: u8) -> ([bool; 3], bool) {
    ECHO_MODES[usize::from(mode).min(ECHO_MODES.len() - 1)]
}

/// How many choices a Space Echo's `Sync` has: `Free`, then every note division.
pub(crate) const ECHO_SYNCS: usize = DIVISION_NAMES.len() + 1;
/// Their names: `Free`, where `Rate` sets the tape's speed, then the divisions the first head can
/// be timed to.
pub(crate) const ECHO_SYNC_NAMES: [&str; ECHO_SYNCS] = {
    let mut names = ["Free"; ECHO_SYNCS];
    let mut at = 0;
    while at < DIVISION_NAMES.len() {
        names[at + 1] = DIVISION_NAMES[at];
        at += 1;
    }
    names
};

/// The first head's delay at the Space Echo's fastest and slowest `Rate`, in seconds, as
/// Arturia's Delay TAPE-201's `Repeat Rate` goes; the second and third are two and three times it.
pub const ECHO_HEAD_SECONDS: (f32, f32) = (0.01, 1.0);

/// The first head's delay in seconds, for a `Rate` on the `00` to `FF` scale: the tape at its
/// slowest at `00`, a second, to its fastest at `FF`, 10 ms, evenly in ratio, so `69` is 150 ms,
/// TAPE-201's own, and the RE-201's 69 to 177 ms, as Universal Audio measured it, is `60` to `93`.
pub fn echo_head_seconds(rate: f32) -> f32 {
    let (fastest, slowest) = ECHO_HEAD_SECONDS;
    slowest * (fastest / slowest).powf((rate / 255.0).clamp(0.0, 1.0))
}

/// How much a Space Echo's `Intensity` feeds back at most, on TAPE-201's scale, where it starts to
/// run away a little past 1.
pub const MOST_INTENSITY: f32 = 1.2;
/// How far a Space Echo's `Input` turns what goes in up or down at most, in dB.
pub(crate) const MOST_ECHO_INPUT_DB: f32 = 24.0;

/// How far a Space Echo's `Input` turns what goes in up or down, in dB, for a value on the `00`
/// to `FF` scale: evenly from 24 dB down at `00`, through none at `80`, to 24 up at `FF`.
pub fn echo_input_db(value: f32) -> f32 {
    from_middle(value) * MOST_ECHO_INPUT_DB
}

/// A Space Echo's tone controls, fitted to TAPE-201's `Bass` and `Treble`: a first-order shelf
/// each, the bass's pole and the treble's zero where they're fixed, in Hz, how far each lifts or
/// cuts at its end in dB, and the power its knob is raised to, as a pot's taper.
pub const ECHO_BASS: (f32, f32, f32) = (24.0, 23.15, 1.161);
pub const ECHO_TREBLE: (f32, f32, f32) = (1038.0, 15.38, 2.077);

/// How far a Space Echo's shelf lifts or cuts where it's flat again, past its corners, in dB,
/// for a value on the `00` to `FF` scale: none at `80`, and as its taper goes to its most either
/// way, so `C0` lifts the bass 10 dB and the treble 3.6.
pub(crate) fn echo_shelf_db(value: f32, (_, most_db, power): (f32, f32, f32)) -> f32 {
    let k = from_middle(value);
    most_db * k.abs().powf(power) * k.signum()
}

/// A Space Echo's, in the order the slot keeps them: its mode, which the title shows; the tape's
/// speed, whether it follows the song's tempo, and how slowly its motor gets there; the loop's
/// feedback, how hard it's driven and how much the tape wobbles; the tone, the reverb, and the
/// echo's level, the mix and the gain.
pub(crate) const ECHO_PARAMS: [Entry; 13] = [
    Entry {
        meaning: "which playback heads play, and whether the spring reverb does",
        name: "Mode",
        key: "mode",
        label: Some("MODE"),
        kind: Kind::Stepped {
            choices: &ECHO_MODE_NAMES,
            wraps: true,
        },
        default: Echo::DEFAULT.mode,
        place: Place::Type,
    },
    hex_param("Rate", "rate", "RATE", Echo::DEFAULT.rate)
        .units(Units::EchoRate)
        .means("the tape's speed, as the first head's delay; moving it bends the repeats"),
    Entry {
        meaning: "a note division the first head keeps to, or Free for Rate to set it",
        name: "Sync",
        key: "sync",
        label: Some("SYNC"),
        kind: Kind::Stepped {
            choices: &ECHO_SYNC_NAMES,
            wraps: false,
        },
        default: Echo::DEFAULT.sync,
        place: Place::Own,
    },
    hex_param("Inertia", "inertia", "INRT", Echo::DEFAULT.inertia)
        .means("how slowly the motor gets to a new speed"),
    hex_param("Intensity", "intensity", "INTS", Echo::DEFAULT.intensity)
        .means("how much goes round again; from about DC it runs away"),
    hex_param("Input", "input", "INPT", Echo::DEFAULT.input)
        .units(Units::EchoInput)
        .centred()
        .means("how hard what goes in drives the tape, as it comes at 80"),
    hex_param("Wow", "wow", "WOW", Echo::DEFAULT.wow).means("how far the tape's speed wavers"),
    hex_param("Bass", "bass", "BASS", Echo::DEFAULT.bass)
        .units(Units::EchoBass)
        .centred()
        .means("lifts or cuts the lows of what comes out"),
    hex_param("Treble", "treble", "TREB", Echo::DEFAULT.treble)
        .units(Units::EchoTreble)
        .centred()
        .means("lifts or cuts the highs of what comes out"),
    hex_param("Reverb", "reverb", "VERB", Echo::DEFAULT.reverb)
        .means("how much of the spring reverb is heard, in the modes that have it"),
    hex_param("Echo", "echo", "ECHO", Echo::DEFAULT.echo).means("how loud the repeats are"),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// Where its settings sit on the FX page. The tape's speed and how it gets there; the loop; the tone and the reverb; then the
/// repeats' level over what comes out. Its mode is in the title.
pub(super) const ECHO_GRID: Columns = &[
    &["rate", "sync", "inertia"],
    &["intensity", "input", "wow"],
    &["bass", "treble", "reverb"],
    &["echo", "mix", "gain"],
];

/// A Space Echo's settings: a Roland RE-201's tape loop, three playback heads and spring reverb,
/// fitted to Arturia's Delay TAPE-201. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Echo {
    /// A place in [`ECHO_MODE_NAMES`]: which heads play, and whether the reverb does.
    pub mode: u8,
    /// How fast the tape runs, from its slowest at `00`, the first head a second behind the record
    /// head, to its fastest at `FF`, 10 ms. Moving it moves the tape, so the repeats bend in pitch.
    pub rate: u8,
    /// `0`, `Free`, where `Rate` sets the speed, or a place among the note divisions after it, the
    /// first head's delay at the song's tempo.
    pub sync: u8,
    /// How slowly the motor comes to a new speed, from about a fifth of a second at `00` to two.
    pub inertia: u8,
    /// How much of what the heads play goes round again: the repeats run away from about `DC`,
    /// 1.03 on TAPE-201's scale.
    pub intensity: u8,
    /// How hard what goes in drives the tape, from 24 dB down at `00`, through as it is at `80`,
    /// to 24 dB up at `FF`.
    pub input: u8,
    /// How far the tape's speed wavers, from not at all at `00`.
    pub wow: u8,
    /// The tone of what comes out: a low and a high shelf, flat at `80`.
    pub bass: u8,
    pub treble: u8,
    /// How much of the spring reverb is heard, in the modes that have it.
    pub reverb: u8,
    /// How loud the repeats are.
    pub echo: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Echo {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Echo {
    /// As it starts: TAPE-201's `Default`, the first head with the reverb at 150 ms, a third of the
    /// way to running away, a little wow, flat, and half wet, with a little reverb in.
    pub const DEFAULT: Self = Self {
        mode: 4,
        rate: 0x69,
        sync: 0,
        inertia: 0,
        intensity: 0x4A,
        input: 0x80,
        wow: 0x0D,
        bass: 0x80,
        treble: 0x80,
        reverb: 0x40,
        echo: 0xFF,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        mode,
        rate,
        sync,
        inertia,
        intensity,
        input,
        wow,
        bass,
        treble,
        reverb,
        echo,
        mix,
        gain,
    }
}

/// The shortest and longest a reverb's `Decay` sets its tail, in seconds, to fall 60 dB.
pub const REVERB_DECAY_SECONDS: (f32, f32) = (0.25, 64.0);

/// How long a reverb's tail takes to fall 60 dB, in seconds, for a `Decay` on the `00` to `FF`
/// scale: from a quarter of a second at `00`, about doubling each `20`, through 2 s at `60`, to 64
/// s at `FF`, evenly in ratio. Each of its loops keeps `0.001` to the power of its length over this
/// each time round, so its size and the rate leave the tail as long (decision 439).
pub fn reverb_decay_seconds(value: f32) -> f32 {
    let (shortest, longest) = REVERB_DECAY_SECONDS;
    shortest * (longest / shortest).powf((value / 255.0).clamp(0.0, 1.0))
}

/// The lowest and highest corner of a reverb's `Low damp`, the high-pass in its loops, in Hz.
pub const LOW_DAMP_HZ: (f32, f32) = (20.0, 1000.0);

/// The corner of a reverb's `Low damp` in Hz, for a value on the `00` to `FF` scale, or nothing
/// at `00`, where it's off: from 20 Hz at `01` to 1 kHz at `FF`, evenly in ratio, about 140 Hz at
/// `80`.
pub fn low_damp_hz(value: u8) -> Option<f32> {
    let (lowest, highest) = LOW_DAMP_HZ;
    (value > 0).then(|| lowest * (highest / lowest).powf(f32::from(value - 1) / 254.0))
}

/// A reverb's `Decay`, which its plate shares.
const fn decay_param(default: u8) -> Entry {
    hex_param("Decay", "decay", "DCAY", default)
        .units(Units::ReverbDecay)
        .means("how long its tail takes to fall 60 dB, from a quarter of a second to a minute")
}

/// A reverb's `Low damp`, which its plate shares.
const fn low_damp_param(default: u8) -> Entry {
    hex_param("Low damp", "lowdamp", "LDMP", default)
        .means("how quickly its lows fall away as it rings, off at 00")
}

/// A reverb's `Width`, which its plate shares.
const fn width_param(default: u8) -> Entry {
    hex_param("Width", "width", "WDTH", default)
        .means("how far apart its sides are, from together at 00")
}

/// A reverb's, in the order the page shows them.
pub(crate) const REVERB_PARAMS: [Entry; 9] = [
    hex_param("Pre", "pre", "PRE", Reverb::DEFAULT.pre)
        .means("how long before it comes in, up to a quarter of a second"),
    decay_param(Reverb::DEFAULT.decay),
    hex_param("Damp", "damp", "DAMP", Reverb::DEFAULT.damp)
        .means("how quickly its highs fall away as it rings"),
    low_damp_param(Reverb::DEFAULT.low_damp),
    hex_param("Low cut", "lowcut", "LCUT", Reverb::DEFAULT.low_cut)
        .means("takes lows from what goes in, off at 00"),
    hex_param("High cut", "highcut", "HCUT", Reverb::DEFAULT.high_cut)
        .means("takes highs from what goes in, off at FF"),
    width_param(Reverb::DEFAULT.width),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// Where its settings sit on the FX page: its pre-delay and tail, how the tail darkens and thins,
/// what goes in, then what comes out.
pub(super) const REVERB_GRID: Columns = &[
    &["pre", "decay"],
    &["damp", "lowdamp"],
    &["lowcut", "highcut"],
    &["width", "mix", "gain"],
];

/// A reverb's settings. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reverb {
    /// How long before anything comes out, up to a quarter of a second.
    pub pre: u8,
    /// How long its tail takes to fall 60 dB, as [`reverb_decay_seconds`] gives it.
    pub decay: u8,
    /// How quickly its highs fall away as it rings.
    pub damp: u8,
    /// How quickly its lows fall away as it rings: a high-pass in its loops, its corner as
    /// [`low_damp_hz`] gives it, off at `00`.
    #[serde(skip_serializing_if = "is_zero")]
    pub low_damp: u8,
    /// A high-pass on what goes in, which `00` turns off.
    pub low_cut: u8,
    /// A low-pass on what goes in, which `FF` turns off.
    pub high_cut: u8,
    /// From `00`, both sides together, to `FF`, as wide as they come out.
    #[serde(skip_serializing_if = "is_full")]
    pub width: u8,
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
    /// As it starts: a tail of about 2 s.
    pub const DEFAULT: Self = Self {
        pre: 0,
        decay: 0x60,
        damp: 0x55,
        low_damp: 0,
        low_cut: 0,
        high_cut: 0xFF,
        width: 0xFF,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        pre,
        decay,
        damp,
        low_damp,
        low_cut,
        high_cut,
        width,
        mix,
        gain,
    }
}

/// A plate's, in the order the page shows them. Those it shares with the reverb are named as its.
pub(crate) const PLATE_PARAMS: [Entry; 10] = [
    hex_param("Pre", "pre", "PRE", Plate::DEFAULT.pre)
        .means("how long before it comes in, up to a quarter of a second"),
    decay_param(Plate::DEFAULT.decay),
    hex_param("Damp", "damp", "DAMP", Plate::DEFAULT.damp)
        .means("how quickly its highs fall away as it rings"),
    low_damp_param(Plate::DEFAULT.low_damp),
    hex_param("High cut", "highcut", "HCUT", Plate::DEFAULT.high_cut)
        .means("takes highs from what goes in, off at FF"),
    hex_param("Size", "size", "SIZE", Plate::DEFAULT.size)
        .units(Units::PlateSize)
        .means("how big the plate is, against Dattorro's; its tail stays as long"),
    hex_param("Mod", "mod", "MOD", Plate::DEFAULT.excursion)
        .means("how much its tail wobbles, which keeps it from ringing metallic"),
    width_param(Plate::DEFAULT.width),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// Where its settings sit on the FX page: its pre-delay, tail and size, how the tail darkens and
/// thins, what goes in and the wobble, then what comes out.
pub(super) const PLATE_GRID: Columns = &[
    &["pre", "decay", "size"],
    &["damp", "lowdamp"],
    &["highcut", "mod"],
    &["width", "mix", "gain"],
];

/// A plate's settings, after Dattorro's. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plate {
    /// How long before anything comes out, up to a quarter of a second.
    pub pre: u8,
    /// How long its tail takes to fall 60 dB, as [`reverb_decay_seconds`] gives it, at any size.
    pub decay: u8,
    /// How quickly its highs fall away as it rings.
    pub damp: u8,
    /// How quickly its lows fall away as it rings, as the reverb's.
    #[serde(skip_serializing_if = "is_zero")]
    pub low_damp: u8,
    /// A low-pass on what goes in, which `FF` turns off.
    pub high_cut: u8,
    /// How big the plate is, from half to twice Dattorro's, evenly in ratio, with his at `80`.
    pub size: u8,
    /// How far its tank's all-passes wobble, which keeps the tail from ringing metallic. Dattorro's
    /// is at `80`.
    pub excursion: u8,
    /// From `00`, both sides together, to `FF`, as wide as they come out.
    #[serde(skip_serializing_if = "is_full")]
    pub width: u8,
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
    /// As it starts: a tail of about 2 s, Dattorro's size and wobble, a little damping, and half
    /// wet.
    pub const DEFAULT: Self = Self {
        pre: 0,
        decay: 0x60,
        damp: 0x20,
        low_damp: 0,
        high_cut: 0xFF,
        size: 0x80,
        excursion: 0x80,
        width: 0xFF,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        pre,
        decay,
        damp,
        low_damp,
        high_cut,
        size,
        excursion,
        width,
        mix,
        gain,
    }
}

/// How many times Dattorro's a plate's size is, for a value on the `00` to `FF` scale: from half
/// at `00`, through his own at `80`, to twice at `FF`, evenly in ratio.
pub fn plate_size(value: f32) -> f32 {
    from_middle(value).exp2()
}

/// A spring reverb's, in the order the page shows them, as columns: how long it rings, how big
/// its springs are and how dark; then its chirp and its reflections; then its wander and its
/// knock; then the mix and gain. Those it shares with the reverb are named as its.
pub(crate) const SPRING_PARAMS: [Entry; 9] = [
    hex_param("Decay", "decay", "DCAY", Spring::DEFAULT.decay)
        .means("how long it rings, from a tenth of a second to about four"),
    hex_param("Size", "size", "SIZE", Spring::DEFAULT.size)
        .means("how long the springs are: how far apart the drips come round"),
    hex_param("Damp", "damp", "DAMP", Spring::DEFAULT.damp).means("how dark it is"),
    hex_param("Spin", "spin", "SPIN", Spring::DEFAULT.spin)
        .means("how much each drip is smeared into a chirp, most at 00"),
    hex_param("Reflect", "reflect", "REFL", Spring::DEFAULT.reflect)
        .means("how much comes back from the ends of the springs, which thickens it"),
    hex_param("Chaos", "chaos", "CHAO", Spring::DEFAULT.chaos)
        .means("how much the springs' length wanders, as a tank that's moving"),
    Entry {
        meaning: "switched on, knocks the tank once, as when it's kicked",
        name: "Knock",
        key: "knock",
        label: Some("KNCK"),
        kind: Kind::Stepped {
            choices: &OFF_ON,
            wraps: true,
        },
        default: Spring::DEFAULT.knock,
        place: Place::Own,
    },
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// Where its settings sit on the FX page.
pub(super) const SPRING_GRID: Columns = &[
    &["decay", "size", "damp"],
    &["spin", "reflect"],
    &["chaos", "knock"],
    &["mix", "gain"],
];

/// A spring reverb's settings: ChowDSP's model of a spring tank, as Surge ships it. One a file
/// doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spring {
    /// How long it rings, from about a tenth of a second to about four.
    pub decay: u8,
    /// How long its springs are: their loop's delay, their all-passes' and their reflections'.
    pub size: u8,
    /// How dark it is, its highs taken from what comes out and from what's reflected.
    pub damp: u8,
    /// How much its all-passes smear a sound out, from a long chirp at `00` to a short one.
    pub spin: u8,
    /// How much comes back from the ends of the springs.
    pub reflect: u8,
    /// How much its loop's delay wanders, at random.
    pub chaos: u8,
    /// Switched on, the tank is knocked once, as a spring tank is when it's kicked.
    pub knock: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Spring {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Spring {
    /// As it starts: Surge's, half way on everything but full reflections, no chaos, and half wet.
    pub const DEFAULT: Self = Self {
        decay: 0x80,
        size: 0x80,
        damp: 0x80,
        spin: 0x80,
        reflect: 0xFF,
        chaos: 0x00,
        knock: 0,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        decay,
        size,
        damp,
        spin,
        reflect,
        chaos,
        knock,
        mix,
        gain,
    }
}

/// A space's, in the order the page shows them: how long it rings, how big and how dark it is,
/// then its wobble, then the mix and gain. Those it shares with the reverbs are named as theirs.
pub(crate) const SPACE_PARAMS: [Entry; 6] = [
    hex_param("Decay", "decay", "DCAY", Space::DEFAULT.decay)
        .means("how long it rings, on and on towards FF"),
    hex_param("Size", "size", "SIZE", Space::DEFAULT.size)
        .means("how big its tank is, small and metallic at 00"),
    hex_param("Damp", "damp", "DAMP", Space::DEFAULT.damp)
        .means("how dark what goes in and what comes out are"),
    hex_param("Mod", "mod", "MOD", Space::DEFAULT.excursion)
        .means("how fast and far what goes in wobbles in pitch"),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// A space's settings, after Airwindows' Galactic: a huge, bright wash that rings on and on. One a
/// file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Space {
    /// How long it rings: Galactic's `Replace` turned round, so `FF` keeps the most of what's in
    /// its tank each time round, though never all of it.
    pub decay: u8,
    /// How big its tank is, from a tenth of Galactic's lengths to nearly twice them.
    pub size: u8,
    /// How dark it is, what goes in and what comes out both darkened.
    pub damp: u8,
    /// How fast and far what goes in wobbles in pitch, from not at all at `00`.
    pub excursion: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Space {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Space {
    /// As it starts: near Galactic's own, its tank at its biggest, and half wet.
    pub const DEFAULT: Self = Self {
        decay: 0x80,
        size: 0xFF,
        damp: 0x80,
        excursion: 0x80,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        decay,
        size,
        damp,
        excursion,
        mix,
        gain,
    }
}

/// How many of the note divisions a stutter's `Length` goes through, shortest first: up to a
/// beat, `1/4`, as a stutter is a roll or a glitch rather than a loop.
pub const REPEAT_LENGTHS: usize = 13;
/// Their names.
pub(crate) const REPEAT_LENGTH_NAMES: [&str; REPEAT_LENGTHS] = {
    let mut names = [""; REPEAT_LENGTHS];
    let mut at = 0;
    while at < REPEAT_LENGTHS {
        names[at] = DIVISION_NAMES[at];
        at += 1;
    }
    names
};
/// How many times a stutter plays its piece again after it's first heard, and `Hold`, which
/// repeats it for as long as `Trig` is on.
pub(crate) const REPEAT_NAMES: [&str; 17] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "Hold",
];
/// The place of `Hold` among them.
pub const HOLD_REPEATS: u8 = REPEAT_NAMES.len() as u8 - 1;
/// How much quieter each of a stutter's repeats is than the one before at most, in dB.
pub(crate) const MOST_REPEAT_DECAY_DB: f32 = 12.0;

/// How much quieter each of a stutter's repeats is than the one before, in dB, for a value on the
/// `00` to `FF` scale: none at `00`, in proportion to 12 dB at `FF`, so `80` about halves each.
pub fn repeat_decay_db(value: f32) -> f32 {
    (value / 255.0).clamp(0.0, 1.0) * MOST_REPEAT_DECAY_DB
}

/// A stutter's, in the order the page shows them, as columns: its trigger; then how long a piece
/// it repeats, how many times and how it fades; then the mix and gain.
pub(crate) const STUTTER_PARAMS: [Entry; 6] = [
    Entry {
        meaning: "switched on, repeats the next piece of what comes in",
        name: "Trig",
        key: "trig",
        label: Some("TRIG"),
        kind: Kind::Stepped {
            choices: &OFF_ON,
            wraps: true,
        },
        default: Stutter::DEFAULT.trig,
        place: Place::Own,
    },
    Entry {
        meaning: "how long a piece it repeats",
        name: "Length",
        key: "length",
        label: Some("LEN"),
        kind: Kind::Stepped {
            choices: &REPEAT_LENGTH_NAMES,
            wraps: false,
        },
        default: Stutter::DEFAULT.length,
        place: Place::Own,
    },
    Entry {
        meaning: "how many times it plays the piece again; Hold while Trig is on",
        name: "Repeats",
        key: "repeats",
        label: Some("RPTS"),
        kind: Kind::Stepped {
            choices: &REPEAT_NAMES,
            wraps: false,
        },
        default: Stutter::DEFAULT.repeats,
        place: Place::Own,
    },
    hex_param("Decay", "decay", "DCAY", Stutter::DEFAULT.decay)
        .units(Units::RepeatDecay)
        .means("how much quieter each repeat is than the one before"),
    Entry {
        meaning: "how much the repeats take the place of the sound",
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// Where its settings sit on the FX page.
pub(super) const STUTTER_GRID: Columns =
    &[&["trig"], &["length", "repeats", "decay"], &["mix", "gain"]];

/// A stutter's settings: a piece of what comes in, repeated on the grid when a step switches
/// `Trig` on, after sst's Microgate. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stutter {
    /// Switched on, by a step's lock or by hand, it takes the next piece of what comes in and
    /// repeats it. A step that locks it on starts again, even if it's on already.
    pub trig: u8,
    /// How long the piece is, as a place among the note divisions up to `1/4`.
    pub length: u8,
    /// How many times it plays the piece again after it's first heard, from 1 at `0` to 16, and
    /// at `HOLD_REPEATS` for as long as `Trig` stays on.
    pub repeats: u8,
    /// How much quieter each repeat is than the one before, from not at all at `00` to 12 dB.
    pub decay: u8,
    /// From `00`, all dry, to `FF`, all wet, where the repeats take the place of the sound.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Stutter {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Stutter {
    /// As it starts: off, waiting for a step, and then a sixteenth played three times more, a
    /// beat in all, as loud each time, all wet.
    pub const DEFAULT: Self = Self {
        trig: 0,
        length: 6,
        repeats: 2,
        decay: 0x00,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        trig,
        length,
        repeats,
        decay,
        mix,
        gain,
    }
}

/// How long a hall's lows ring against its `Decay`, as so many times, for a value on the `00` to
/// `FF` scale: from a quarter at `00`, through as long at `80`, to four times at `FF`, evenly in
/// ratio.
pub fn hall_lows(value: f32) -> f32 {
    (from_middle(value) * 2.0).exp2()
}

/// How big a hall is, as so many times its lines' lengths, for a value on the `00` to `FF` scale:
/// from half at `00`, a big room, through its own at `80`, to twice at `FF`, evenly in ratio.
pub fn hall_size(value: f32) -> f32 {
    from_middle(value).exp2()
}

/// A hall's, in the order the page shows them. Those it shares with the reverbs are named as
/// theirs.
pub(crate) const HALL_PARAMS: [Entry; 12] = [
    hex_param("Pre", "pre", "PRE", Hall::DEFAULT.pre)
        .means("how long before it comes in, up to a quarter of a second"),
    decay_param(Hall::DEFAULT.decay),
    hex_param("Lows", "lows", "LOWS", Hall::DEFAULT.lows)
        .units(Units::LowDecay)
        .centred()
        .means("how long its lows ring against Decay, as long at 80"),
    hex_param("Damp", "damp", "DAMP", Hall::DEFAULT.damp)
        .means("how quickly its highs fall away as it rings"),
    hex_param("Size", "size", "SIZE", Hall::DEFAULT.size)
        .units(Units::PlateSize)
        .means("how big the hall is, from a big room to twice its own; its tail stays as long"),
    hex_param("Early", "early", "ERLY", Hall::DEFAULT.early)
        .means("how loud its first reflections are, off at 00"),
    hex_param("Mod", "mod", "MOD", Hall::DEFAULT.excursion)
        .means("how much its lines wobble, which keeps the tail from ringing"),
    hex_param("Low cut", "lowcut", "LCUT", Hall::DEFAULT.low_cut)
        .means("takes lows from what goes in, off at 00"),
    hex_param("High cut", "highcut", "HCUT", Hall::DEFAULT.high_cut)
        .means("takes highs from what goes in, off at FF"),
    width_param(Hall::DEFAULT.width),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// Where its settings sit on the FX page: its pre-delay, reflections and size; its tail in three
/// bands; what goes in and the wobble; then what comes out.
pub(super) const HALL_GRID: Columns = &[
    &["pre", "early", "size"],
    &["decay", "lows", "damp"],
    &["lowcut", "highcut", "mod"],
    &["width", "mix", "gain"],
];

/// A hall's settings: a feedback delay network of eight lines, after Jot's and zita-rev1. One a
/// file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hall {
    /// How long before anything comes out, up to a quarter of a second.
    pub pre: u8,
    /// How long its middle takes to fall 60 dB, as [`reverb_decay_seconds`] gives it, at any size.
    pub decay: u8,
    /// How long its lows ring against that, as [`hall_lows`] gives it.
    pub lows: u8,
    /// How quickly its highs fall away: as long as the middle at `00`, a tenth as long at `FF`.
    pub damp: u8,
    /// How big it is, as [`hall_size`] gives it.
    pub size: u8,
    /// How loud its early reflections are, off at `00`.
    pub early: u8,
    /// How far its lines' reads wobble, from not at all at `00`.
    pub excursion: u8,
    /// A high-pass on what goes in, which `00` turns off.
    pub low_cut: u8,
    /// A low-pass on what goes in, which `FF` turns off.
    pub high_cut: u8,
    /// From `00`, both sides together, to `FF`, as wide as they come out.
    pub width: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Hall {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Hall {
    /// As it starts: a tail of about 2 s, its lows a little longer and its highs a little
    /// shorter, its own size, some early reflections, and half wet.
    pub const DEFAULT: Self = Self {
        pre: 0x08,
        decay: 0x60,
        lows: 0x98,
        damp: 0x30,
        size: 0x80,
        early: 0x80,
        excursion: 0x40,
        low_cut: 0,
        high_cut: 0xFF,
        width: 0xFF,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        pre,
        decay,
        lows,
        damp,
        size,
        early,
        excursion,
        low_cut,
        high_cut,
        width,
        mix,
        gain,
    }
}
