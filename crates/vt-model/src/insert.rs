//! The effects a track can hold, in order, after its filters.

use serde::{Deserialize, Serialize};

use crate::constants::{MAX_LEVEL, UNITY_LEVEL};
use crate::lfo::DIVISION_NAMES;

/// How many a track has.
pub const INSERTS: usize = 3;

/// One of a track's slots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Insert {
    #[default]
    Empty,
    Distort(Distort),
    Delay(Delay),
    Reverb(Reverb),
}

/// What a distortion does to the sound.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistortType {
    #[default]
    Soft,
    Hard,
    Fold,
    Sine,
    Crush,
    Down,
}

impl DistortType {
    /// Every type, in the order `+` goes through them.
    pub const ALL: [Self; 6] = [
        Self::Soft,
        Self::Hard,
        Self::Fold,
        Self::Sine,
        Self::Crush,
        Self::Down,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        DISTORT_TYPE_NAMES[self as usize]
    }
}

/// What a slot's setting is, for what moves it to read and write it on its own scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Type,
    Amount,
    Bits,
    Mix,
    Gain,
    /// A place in the list of note divisions.
    Time,
    Feedback,
    PingPong,
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
            Self::Bits => MOST_BITS,
            Self::Gain => MAX_LEVEL,
            Self::Time => DIVISION_NAMES.len() as u8 - 1,
            Self::PingPong => 1,
            _ => 0xFF,
        }
    }

    /// What full modulation moves it by, in its own units.
    pub const fn span(self) -> f32 {
        match self {
            Self::Type => (DistortType::ALL.len() - 1) as f32,
            Self::Bits => (MOST_BITS - LEAST_BITS) as f32,
            Self::Time => (DIVISION_NAMES.len() - 1) as f32,
            Self::PingPong => 1.0,
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
    /// The grid's short label, 3 or 4 letters, after the slot's digit.
    pub label: &'static str,
    /// What it is, on its own scale.
    pub setting: Setting,
    /// What it chooses between, in order, or nothing for a value in hex.
    pub choices: &'static [&'static str],
    /// Whether stepping past the last choice goes round to the first, or stops. A choice that's a
    /// scale stops, and one that's a list goes round.
    pub wraps: bool,
    pub default: u8,
}

/// The most settings any effect has.
pub const MAX_PARAMS: usize = 9;

const DISTORT_TYPE_NAMES: [&str; 6] = ["Soft", "Hard", "Fold", "Sine", "Crush", "Down"];
const TYPE_PARAM: Param = Param {
    name: "Type",
    key: "type",
    label: "TYPE",
    setting: Setting::Type,
    choices: &DISTORT_TYPE_NAMES,
    wraps: true,
    default: 0,
};
const MIX_PARAM: Param = Param {
    name: "Mix",
    key: "mix",
    label: "MIX",
    setting: Setting::Mix,
    choices: &[],
    wraps: false,
    default: 0xFF,
};
const GAIN_PARAM: Param = Param {
    name: "Gain",
    key: "gain",
    label: "GAIN",
    setting: Setting::Gain,
    choices: &[],
    wraps: false,
    default: UNITY_LEVEL,
};
pub(crate) const DISTORT_PARAMS: [Param; 4] = [
    TYPE_PARAM,
    Param {
        name: "Amount",
        key: "amount",
        label: "AMNT",
        setting: Setting::Amount,
        choices: &[],
        wraps: false,
        default: 0x40,
    },
    MIX_PARAM,
    GAIN_PARAM,
];

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
        label,
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
pub(crate) const DELAY_PARAMS: [Param; 9] = [
    Param {
        name: "Time",
        key: "time",
        label: "TIME",
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
        label: "PING",
        setting: Setting::PingPong,
        choices: &["Off", "On"],
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
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// The bit depths `Crush` chooses between. Above these it can't be heard.
const LEAST_BITS: u8 = 2;
const MOST_BITS: u8 = 12;
const BIT_NAMES: [&str; (MOST_BITS - LEAST_BITS + 1) as usize] =
    ["2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"];
/// How much of `amount` is one bit, as `Crush` shares it with the other types.
const AMOUNT_PER_BIT: f32 = 255.0 / (MOST_BITS - LEAST_BITS) as f32;
/// `Crush` has whole bits in place of an `Amount`. It moves from steps, with no in-between to sweep.
const CRUSH_PARAMS: [Param; 4] = [
    TYPE_PARAM,
    Param {
        name: "Bits",
        key: "amount",
        label: "AMNT",
        setting: Setting::Bits,
        choices: &BIT_NAMES,
        wraps: false,
        default: 7,
    },
    MIX_PARAM,
    GAIN_PARAM,
];

/// The bit depth for an `amount`, so a low one stays gentle and a high one harsh across types.
pub fn crush_bits(amount: u8) -> u8 {
    MOST_BITS - (f32::from(amount) / AMOUNT_PER_BIT).round() as u8
}

impl Insert {
    /// Every effect a slot can hold, as it starts, in the order `+` goes through them.
    pub const ALL: [Self; 4] = [
        Self::Empty,
        Self::Distort(Distort {
            kind: DistortType::Soft,
            amount: DISTORT_PARAMS[1].default,
            mix: MIX_PARAM.default,
            gain: GAIN_PARAM.default,
        }),
        Self::Delay(Delay::DEFAULT),
        Self::Reverb(Reverb::DEFAULT),
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
        }
    }

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Empty => "Nothing here",
            Self::Distort(_) => "Clips, folds or crushes the sound",
            Self::Delay(_) => "Repeats the sound, bouncing side to side",
            Self::Reverb(_) => "Puts the sound in a room",
        }
    }

    /// The settings it has.
    pub fn params(self) -> &'static [Param] {
        match self {
            Self::Empty => &[],
            Self::Distort(Distort {
                kind: DistortType::Crush,
                ..
            }) => &CRUSH_PARAMS,
            Self::Distort(_) => &DISTORT_PARAMS,
            Self::Delay(_) => &DELAY_PARAMS,
            Self::Reverb(_) => &REVERB_PARAMS,
        }
    }

    /// A setting's value: a place in its choices, or the value itself.
    pub fn value(self, param: usize) -> u8 {
        match (self, param) {
            (Self::Distort(distort), 0) => DistortType::ALL
                .iter()
                .position(|kind| *kind == distort.kind)
                .unwrap_or(0) as u8,
            (Self::Distort(distort), 1) if distort.kind == DistortType::Crush => {
                crush_bits(distort.amount) - LEAST_BITS
            }
            (Self::Distort(distort), 1) => distort.amount,
            (Self::Distort(distort), 2) => distort.mix,
            (Self::Distort(distort), 3) => distort.gain,
            (Self::Delay(delay), param) => DELAY_PARAMS
                .get(param)
                .map_or(0, |found| delay.read(found.setting)),
            (Self::Reverb(reverb), param) => REVERB_PARAMS
                .get(param)
                .map_or(0, |found| reverb.read(found.setting)),
            _ => 0,
        }
    }

    /// The same with a setting changed.
    #[must_use]
    pub fn with_value(self, param: usize, value: u8) -> Self {
        let distort = match self {
            Self::Distort(distort) => distort,
            Self::Delay(delay) => {
                return DELAY_PARAMS.get(param).map_or(self, |found| {
                    Self::Delay(delay.written(found.setting, value))
                });
            }
            Self::Reverb(reverb) => {
                return REVERB_PARAMS.get(param).map_or(self, |found| {
                    Self::Reverb(reverb.written(found.setting, value))
                });
            }
            Self::Empty => return self,
        };
        Self::Distort(match param {
            0 => Distort {
                kind: DistortType::ALL
                    .get(usize::from(value))
                    .copied()
                    .unwrap_or(distort.kind),
                ..distort
            },
            1 if distort.kind == DistortType::Crush => Distort {
                amount: (f32::from(MOST_BITS - LEAST_BITS - value.min(MOST_BITS - LEAST_BITS))
                    * AMOUNT_PER_BIT)
                    .round() as u8,
                ..distort
            },
            1 => Distort {
                amount: value,
                ..distort
            },
            2 => Distort {
                mix: value,
                ..distort
            },
            3 => Distort {
                gain: value,
                ..distort
            },
            _ => distort,
        })
    }

    /// A setting as a step gives it: whole bits for `Bits`, and a place for `Type`. Nothing if
    /// there's no effect here.
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
            Self::Empty => return None,
        };
        Some(match setting {
            Setting::Type => distort.kind as u8,
            Setting::Amount => distort.amount,
            Setting::Bits => crush_bits(distort.amount),
            Setting::Mix => distort.mix,
            Setting::Gain => distort.gain,
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
            Self::Empty => return self,
        };
        Self::Distort(match setting {
            Setting::Type => Distort {
                kind: DistortType::ALL[usize::from(whole)],
                ..distort
            },
            Setting::Amount => Distort {
                amount: whole,
                ..distort
            },
            Setting::Bits => Distort {
                amount: (f32::from(MOST_BITS - whole.max(LEAST_BITS)) * AMOUNT_PER_BIT).round()
                    as u8,
                ..distort
            },
            Setting::Mix => Distort {
                mix: whole,
                ..distort
            },
            Setting::Gain => Distort {
                gain: whole,
                ..distort
            },
            _ => distort,
        })
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

/// A distortion's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Distort {
    pub kind: DistortType,
    /// More of what its type does, which for `Crush` is fewer whole bits.
    pub amount: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    /// What comes out, on the level scale, so `E0` leaves it as it is. It's right there to turn
    /// down as `Amount` makes it louder.
    #[serde(default = "unity_gain")]
    pub gain: u8,
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

fn unity_gain() -> u8 {
    UNITY_LEVEL
}

impl Default for Distort {
    fn default() -> Self {
        Self {
            kind: DistortType::default(),
            amount: 0x40,
            mix: 0xFF,
            gain: UNITY_LEVEL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_starts_at_its_declared_defaults_and_fits() {
        for effect in Insert::ALL {
            assert!(effect.params().len() <= MAX_PARAMS, "{effect:?}");
            for (index, param) in effect.params().iter().enumerate() {
                assert_eq!(effect.value(index), param.default, "{}", param.name);
            }
        }
        assert_eq!(Insert::ALL[1], Insert::Distort(Distort::default()));
    }

    #[test]
    fn every_effect_ends_with_a_gain() {
        for effect in Insert::ALL.into_iter().filter(|e| *e != Insert::Empty) {
            let last = effect.params().last().unwrap();
            assert_eq!(
                (last.name, last.default),
                ("Gain", UNITY_LEVEL),
                "{effect:?}"
            );
        }
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

    fn crush(amount: u8) -> Insert {
        Insert::Distort(Distort {
            kind: DistortType::Crush,
            amount,
            ..Distort::default()
        })
    }

    #[test]
    fn no_two_settings_of_an_effect_share_a_name() {
        for params in [&DISTORT_PARAMS, &CRUSH_PARAMS] {
            for (at, param) in params.iter().enumerate() {
                for other in &params[at + 1..] {
                    assert_ne!(param.name, other.name);
                }
            }
        }
    }

    #[test]
    fn writing_lands_on_whole_values_inside_the_range() {
        let effect = crush(0x40);
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
            Some(LEAST_BITS)
        );
        assert_eq!(
            effect.write(Setting::Bits, 99.0).read(Setting::Bits),
            Some(MOST_BITS)
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
    fn crush_has_whole_bits_in_place_of_an_amount() {
        let names: Vec<&str> = crush(0).params().iter().map(|param| param.name).collect();
        assert_eq!(names, ["Type", "Bits", "Mix", "Gain"]);
        assert_eq!(Insert::ALL[1].params()[1].name, "Amount");
        assert_eq!(
            (crush_bits(0x00), crush_bits(0xFF)),
            (MOST_BITS, LEAST_BITS)
        );
        // Bits read as their names, and every one goes there and back.
        let bits = crush(0).params()[1];
        for index in 0..bits.choices.len() as u8 {
            let set = crush(0).with_value(1, index);
            assert_eq!(set.value(1), index);
            let Insert::Distort(distort) = set else {
                unreachable!();
            };
            assert_eq!(
                bits.choices[usize::from(index)],
                crush_bits(distort.amount).to_string()
            );
        }
        // A default that means what the other types' does.
        assert_eq!(crush(DISTORT_PARAMS[1].default).value(1), bits.default);
        assert!(!bits.wraps);
    }

    #[test]
    fn changing_type_keeps_how_hard_it_is() {
        let hard = Insert::ALL[1].with_value(1, 0xF0);
        let crushed = hard.with_value(0, DistortType::Crush as u8);
        assert_eq!(BIT_NAMES[usize::from(crushed.value(1))], "3");
        assert_eq!(crushed.with_value(0, 0).value(1), 0xF0);
    }

    #[test]
    fn a_types_name_matches_its_place_in_the_choices() {
        for (index, kind) in DistortType::ALL.into_iter().enumerate() {
            assert_eq!(DISTORT_PARAMS[0].choices[index], kind.name());
        }
    }

    #[test]
    fn stepping_goes_round_the_effects_and_keeps_nothing() {
        let changed = Insert::ALL[1].with_value(1, 0x12);
        assert_eq!(changed.stepped(true), Insert::ALL[2]);
        assert_eq!(Insert::ALL[3].stepped(true), Insert::Empty);
        assert_eq!(Insert::Empty.stepped(true), Insert::ALL[1]);
        assert_eq!(Insert::Empty.stepped(false), Insert::ALL[3]);
    }
}
