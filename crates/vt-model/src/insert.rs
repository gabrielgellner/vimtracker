//! The effects a track can hold, in order, after its filters.

use serde::{Deserialize, Serialize};

use crate::constants::{MAX_LEVEL, UNITY_LEVEL};

/// How many a track has.
pub const INSERTS: usize = 3;

/// One of a track's slots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Insert {
    #[default]
    Empty,
    Distort(Distort),
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

/// A setting a slot can have, for step commands to name. Each has its own two letters, across every
/// effect, and a command does nothing in a slot whose effect doesn't have that setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Type,
    Amount,
    Bits,
    Mix,
    Gain,
}

impl Setting {
    /// Every setting.
    pub const ALL: [Self; 5] = [Self::Type, Self::Amount, Self::Bits, Self::Mix, Self::Gain];

    /// Its two letters, after the slot's number in a step command.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Type => "TY",
            Self::Amount => "AM",
            Self::Bits => "BI",
            Self::Mix => "MX",
            Self::Gain => "GN",
        }
    }

    /// The highest value a step can give it.
    pub const fn highest(self) -> u8 {
        match self {
            Self::Type => DistortType::ALL.len() as u8 - 1,
            Self::Bits => MOST_BITS,
            Self::Gain => MAX_LEVEL,
            Self::Amount | Self::Mix => 0xFF,
        }
    }

    /// What full modulation moves it by, in its own units.
    pub const fn span(self) -> f32 {
        match self {
            Self::Type => (DistortType::ALL.len() - 1) as f32,
            Self::Bits => (MOST_BITS - LEAST_BITS) as f32,
            Self::Amount | Self::Mix | Self::Gain => 255.0,
        }
    }
}

/// One setting an effect declares. The pages, stepping, `x`, `K` and typing read these, so a new
/// effect needs no new page code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Param {
    pub name: &'static str,
    /// What step commands call it.
    pub setting: Setting,
    /// What it chooses between, in order, or nothing for a value in hex.
    pub choices: &'static [&'static str],
    /// Whether stepping past the last choice goes round to the first, or stops. A choice that's a
    /// scale stops, and one that's a list goes round.
    pub wraps: bool,
    pub default: u8,
}

/// The most settings any effect has.
pub const MAX_PARAMS: usize = 4;

const DISTORT_TYPE_NAMES: [&str; 6] = ["Soft", "Hard", "Fold", "Sine", "Crush", "Down"];
const TYPE_PARAM: Param = Param {
    name: "Type",
    setting: Setting::Type,
    choices: &DISTORT_TYPE_NAMES,
    wraps: true,
    default: 0,
};
const MIX_PARAM: Param = Param {
    name: "Mix",
    setting: Setting::Mix,
    choices: &[],
    wraps: false,
    default: 0xFF,
};
const GAIN_PARAM: Param = Param {
    name: "Gain",
    setting: Setting::Gain,
    choices: &[],
    wraps: false,
    default: UNITY_LEVEL,
};
const DISTORT_PARAMS: [Param; 4] = [
    TYPE_PARAM,
    Param {
        name: "Amount",
        setting: Setting::Amount,
        choices: &[],
        wraps: false,
        default: 0x40,
    },
    MIX_PARAM,
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
    pub const ALL: [Self; 2] = [
        Self::Empty,
        Self::Distort(Distort {
            kind: DistortType::Soft,
            amount: DISTORT_PARAMS[1].default,
            mix: MIX_PARAM.default,
            gain: GAIN_PARAM.default,
        }),
    ];

    /// The effect's name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Empty => "---",
            Self::Distort(_) => "Distort",
        }
    }

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Empty => "Nothing here",
            Self::Distort(_) => "Clips, folds or crushes the sound",
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
            _ => 0,
        }
    }

    /// The same with a setting changed.
    #[must_use]
    pub fn with_value(self, param: usize, value: u8) -> Self {
        let Self::Distort(distort) = self else {
            return self;
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

    /// A setting as a step command gives it: whole bits for `Bits`, and a place for `Type`. Nothing
    /// if there's no effect here.
    pub fn read(self, setting: Setting) -> Option<u8> {
        let Self::Distort(distort) = self else {
            return None;
        };
        Some(match setting {
            Setting::Type => distort.kind as u8,
            Setting::Amount => distort.amount,
            Setting::Bits => crush_bits(distort.amount),
            Setting::Mix => distort.mix,
            Setting::Gain => distort.gain,
        })
    }

    /// The same with a setting moved, on the scale `read` gives, landing on a whole value within its
    /// range. It's how a stepped setting ends up stepping, whatever moved it.
    #[must_use]
    pub fn write(self, setting: Setting, value: f32) -> Self {
        let Self::Distort(distort) = self else {
            return self;
        };
        let whole = value.round().clamp(0.0, f32::from(setting.highest())) as u8;
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
    fn no_two_settings_share_a_code_and_each_is_declared_once_per_effect() {
        for (at, setting) in Setting::ALL.into_iter().enumerate() {
            for other in &Setting::ALL[at + 1..] {
                assert_ne!(setting.code(), other.code());
            }
        }
        for params in [&DISTORT_PARAMS, &CRUSH_PARAMS] {
            for (at, param) in params.iter().enumerate() {
                for other in &params[at + 1..] {
                    assert_ne!(param.setting, other.setting);
                }
            }
        }
    }

    #[test]
    fn a_setting_reads_as_it_writes() {
        let hard = Insert::ALL[1];
        for setting in Setting::ALL {
            let high = f32::from(setting.highest());
            let written = hard.write(setting, high);
            assert_eq!(
                written.read(setting),
                Some(setting.highest()),
                "{setting:?}"
            );
        }
        assert_eq!(Insert::Empty.read(Setting::Mix), None);
        assert_eq!(Insert::Empty.write(Setting::Mix, 3.0), Insert::Empty);
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
        assert_eq!(changed.stepped(true), Insert::Empty);
        assert_eq!(Insert::Empty.stepped(true), Insert::ALL[1]);
        assert_eq!(Insert::Empty.stepped(false), Insert::ALL[1]);
    }
}
