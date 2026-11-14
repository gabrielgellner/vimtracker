//! The effects that put a sound in a space or repeat it: a delay, the reverbs and a stutter.

use serde::{Deserialize, Serialize};

use crate::constants::UNITY_LEVEL;
use crate::lfo::DIVISION_NAMES;

use super::{
    GAIN_PARAM, MIX_PARAM, OFF_ON, Param, RETIMES, Setting, WET_MIX_PARAM, from_middle, hex_param,
    is_zero,
};

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
    pub(super) fn read(self, setting: Setting) -> u8 {
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
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
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

/// The Space Echo's modes, as the RE-201's mode selector has them: its playback heads alone and
/// together, from the first, the closest to the record head, to the third, then the same with the
/// spring reverb, and the reverb alone. Each is named by its number on the selector, the heads it
/// plays and whether the reverb is on.
pub const ECHO_MODE_NAMES: [&str; 12] = [
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
pub const ECHO_SYNCS: usize = DIVISION_NAMES.len() + 1;
/// Their names: `Free`, where `Rate` sets the tape's speed, then the divisions the first head can
/// be timed to.
pub const ECHO_SYNC_NAMES: [&str; ECHO_SYNCS] = {
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
pub const MOST_ECHO_INPUT_DB: f32 = 24.0;

/// A Space Echo's, in the order the slot keeps them: its mode, which the title shows; the tape's
/// speed, whether it follows the song's tempo, and how slowly its motor gets there; the loop's
/// feedback, how hard it's driven and how much the tape wobbles; the tone, the reverb, and the
/// echo's level, the mix and the gain.
pub(crate) const ECHO_PARAMS: [Param; 13] = [
    Param {
        name: "Mode",
        key: "mode",
        label: Some("MODE"),
        setting: Setting::EchoMode,
        choices: &ECHO_MODE_NAMES,
        wraps: true,
        default: Echo::DEFAULT.mode,
    },
    hex_param("Rate", "rate", "RATE", Setting::Rate, Echo::DEFAULT.rate),
    Param {
        name: "Sync",
        key: "sync",
        label: Some("SYNC"),
        setting: Setting::EchoSync,
        choices: &ECHO_SYNC_NAMES,
        wraps: false,
        default: Echo::DEFAULT.sync,
    },
    hex_param(
        "Inertia",
        "inertia",
        "INRT",
        Setting::Inertia,
        Echo::DEFAULT.inertia,
    ),
    hex_param(
        "Intensity",
        "intensity",
        "INTS",
        Setting::Feedback,
        Echo::DEFAULT.intensity,
    ),
    hex_param(
        "Input",
        "input",
        "INPT",
        Setting::Input,
        Echo::DEFAULT.input,
    ),
    hex_param("Wow", "wow", "WOW", Setting::Wow, Echo::DEFAULT.wow),
    hex_param("Bass", "bass", "BASS", Setting::Low, Echo::DEFAULT.bass),
    hex_param(
        "Treble",
        "treble",
        "TREB",
        Setting::High,
        Echo::DEFAULT.treble,
    ),
    hex_param(
        "Reverb",
        "reverb",
        "VERB",
        Setting::ReverbAmount,
        Echo::DEFAULT.reverb,
    ),
    hex_param(
        "Echo",
        "echo",
        "ECHO",
        Setting::EchoLevel,
        Echo::DEFAULT.echo,
    ),
    WET_MIX_PARAM,
    GAIN_PARAM,
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
    /// How much of what the heads play goes round again: the repeats run away a little past `E0`.
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

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::EchoMode => self.mode,
            Setting::Rate => self.rate,
            Setting::EchoSync => self.sync,
            Setting::Inertia => self.inertia,
            Setting::Feedback => self.intensity,
            Setting::Input => self.input,
            Setting::Wow => self.wow,
            Setting::Low => self.bass,
            Setting::High => self.treble,
            Setting::ReverbAmount => self.reverb,
            Setting::EchoLevel => self.echo,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut echo = self;
        match setting {
            Setting::EchoMode => echo.mode = value,
            Setting::Rate => echo.rate = value,
            Setting::EchoSync => echo.sync = value,
            Setting::Inertia => echo.inertia = value,
            Setting::Feedback => echo.intensity = value,
            Setting::Input => echo.input = value,
            Setting::Wow => echo.wow = value,
            Setting::Low => echo.bass = value,
            Setting::High => echo.treble = value,
            Setting::ReverbAmount => echo.reverb = value,
            Setting::EchoLevel => echo.echo = value,
            Setting::Mix => echo.mix = value,
            Setting::Gain => echo.gain = value,
            _ => {}
        }
        echo
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
    pub(super) fn read(self, setting: Setting) -> u8 {
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
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
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
    pub(super) fn read(self, setting: Setting) -> u8 {
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
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
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

/// A spring reverb's, in the order the page shows them, as columns: how long it rings, how big
/// its springs are and how dark; then its chirp and its reflections; then its wander and its
/// knock; then the mix and gain. Those it shares with the reverb are named as its.
pub(crate) const SPRING_PARAMS: [Param; 9] = [
    hex_param(
        "Decay",
        "decay",
        "DCAY",
        Setting::Decay,
        Spring::DEFAULT.decay,
    ),
    hex_param(
        "Size",
        "size",
        "SIZE",
        Setting::SpringSize,
        Spring::DEFAULT.size,
    ),
    hex_param("Damp", "damp", "DAMP", Setting::Damp, Spring::DEFAULT.damp),
    hex_param("Spin", "spin", "SPIN", Setting::Spin, Spring::DEFAULT.spin),
    hex_param(
        "Reflect",
        "reflect",
        "REFL",
        Setting::Reflections,
        Spring::DEFAULT.reflect,
    ),
    hex_param(
        "Chaos",
        "chaos",
        "CHAO",
        Setting::Chaos,
        Spring::DEFAULT.chaos,
    ),
    Param {
        name: "Knock",
        key: "knock",
        label: Some("KNCK"),
        setting: Setting::Knock,
        choices: &OFF_ON,
        wraps: true,
        default: Spring::DEFAULT.knock,
    },
    WET_MIX_PARAM,
    GAIN_PARAM,
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

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Decay => self.decay,
            Setting::SpringSize => self.size,
            Setting::Damp => self.damp,
            Setting::Spin => self.spin,
            Setting::Reflections => self.reflect,
            Setting::Chaos => self.chaos,
            Setting::Knock => self.knock,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut spring = self;
        match setting {
            Setting::Decay => spring.decay = value,
            Setting::SpringSize => spring.size = value,
            Setting::Damp => spring.damp = value,
            Setting::Spin => spring.spin = value,
            Setting::Reflections => spring.reflect = value,
            Setting::Chaos => spring.chaos = value,
            Setting::Knock => spring.knock = value,
            Setting::Mix => spring.mix = value,
            Setting::Gain => spring.gain = value,
            _ => {}
        }
        spring
    }
}

/// A space's, in the order the page shows them: how long it rings, how big and how dark it is,
/// then its wobble, then the mix and gain. Those it shares with the reverbs are named as theirs.
pub(crate) const SPACE_PARAMS: [Param; 6] = [
    hex_param(
        "Decay",
        "decay",
        "DCAY",
        Setting::Decay,
        Space::DEFAULT.decay,
    ),
    hex_param(
        "Size",
        "size",
        "SIZE",
        Setting::SpaceSize,
        Space::DEFAULT.size,
    ),
    hex_param("Damp", "damp", "DAMP", Setting::Damp, Space::DEFAULT.damp),
    hex_param(
        "Mod",
        "mod",
        "MOD",
        Setting::Excursion,
        Space::DEFAULT.excursion,
    ),
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

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Decay => self.decay,
            Setting::SpaceSize => self.size,
            Setting::Damp => self.damp,
            Setting::Excursion => self.excursion,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut space = self;
        match setting {
            Setting::Decay => space.decay = value,
            Setting::SpaceSize => space.size = value,
            Setting::Damp => space.damp = value,
            Setting::Excursion => space.excursion = value,
            Setting::Mix => space.mix = value,
            Setting::Gain => space.gain = value,
            _ => {}
        }
        space
    }
}

/// How many of the note divisions a stutter's `Length` goes through, shortest first: up to a
/// beat, `1/4`, as a stutter is a roll or a glitch rather than a loop.
pub const REPEAT_LENGTHS: usize = 13;
/// Their names.
pub const REPEAT_LENGTH_NAMES: [&str; REPEAT_LENGTHS] = {
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
pub const REPEAT_NAMES: [&str; 17] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "Hold",
];
/// The place of `Hold` among them.
pub const HOLD_REPEATS: u8 = REPEAT_NAMES.len() as u8 - 1;
/// How much quieter each of a stutter's repeats is than the one before at most, in dB.
pub const MOST_REPEAT_DECAY_DB: f32 = 12.0;

/// How much quieter each of a stutter's repeats is than the one before, in dB, for a value on the
/// `00` to `FF` scale: none at `00`, in proportion to 12 dB at `FF`, so `80` about halves each.
pub fn repeat_decay_db(value: f32) -> f32 {
    (value / 255.0).clamp(0.0, 1.0) * MOST_REPEAT_DECAY_DB
}

/// A stutter's, in the order the page shows them, as columns: its trigger; then how long a piece
/// it repeats, how many times and how it fades; then the mix and gain.
pub(crate) const STUTTER_PARAMS: [Param; 6] = [
    Param {
        name: "Trig",
        key: "trig",
        label: Some("TRIG"),
        setting: Setting::Trig,
        choices: &OFF_ON,
        wraps: true,
        default: Stutter::DEFAULT.trig,
    },
    Param {
        name: "Length",
        key: "length",
        label: Some("LEN"),
        setting: Setting::RepeatLength,
        choices: &REPEAT_LENGTH_NAMES,
        wraps: false,
        default: Stutter::DEFAULT.length,
    },
    Param {
        name: "Repeats",
        key: "repeats",
        label: Some("RPTS"),
        setting: Setting::Repeats,
        choices: &REPEAT_NAMES,
        wraps: false,
        default: Stutter::DEFAULT.repeats,
    },
    hex_param(
        "Decay",
        "decay",
        "DCAY",
        Setting::RepeatDecay,
        Stutter::DEFAULT.decay,
    ),
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

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

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Trig => self.trig,
            Setting::RepeatLength => self.length,
            Setting::Repeats => self.repeats,
            Setting::RepeatDecay => self.decay,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut stutter = self;
        match setting {
            Setting::Trig => stutter.trig = value,
            Setting::RepeatLength => stutter.length = value,
            Setting::Repeats => stutter.repeats = value,
            Setting::RepeatDecay => stutter.decay = value,
            Setting::Mix => stutter.mix = value,
            Setting::Gain => stutter.gain = value,
            _ => {}
        }
        stutter
    }
}
