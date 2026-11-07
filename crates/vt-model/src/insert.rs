//! The effects a track can hold, in order, after its filters.

use serde::{Deserialize, Serialize};

use crate::constants::UNITY_LEVEL;

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

/// One setting an effect declares. The pages, stepping, `x`, `K` and typing read these, so a new
/// effect needs no new page code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Param {
    pub name: &'static str,
    /// What it chooses between, in order, or nothing for a value in hex.
    pub choices: &'static [&'static str],
    pub default: u8,
}

/// The most settings any effect has.
pub const MAX_PARAMS: usize = 4;

const DISTORT_TYPE_NAMES: [&str; 6] = ["Soft", "Hard", "Fold", "Sine", "Crush", "Down"];
const DISTORT_PARAMS: [Param; 4] = [
    Param {
        name: "Type",
        choices: &DISTORT_TYPE_NAMES,
        default: 0,
    },
    Param {
        name: "Amount",
        choices: &[],
        default: 0x40,
    },
    Param {
        name: "Mix",
        choices: &[],
        default: 0xFF,
    },
    Param {
        name: "Gain",
        choices: &[],
        default: UNITY_LEVEL,
    },
];

impl Insert {
    /// Every effect a slot can hold, as it starts, in the order `+` goes through them.
    pub const ALL: [Self; 2] = [
        Self::Empty,
        Self::Distort(Distort {
            kind: DistortType::Soft,
            amount: DISTORT_PARAMS[1].default,
            mix: DISTORT_PARAMS[2].default,
            gain: DISTORT_PARAMS[3].default,
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
    /// More of what its type does.
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
