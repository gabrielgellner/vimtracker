//! The effects a track can hold, in order, after its filters.

use serde::{Deserialize, Serialize};

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
        match self {
            Self::Soft => "Soft",
            Self::Hard => "Hard",
            Self::Fold => "Fold",
            Self::Sine => "Sine",
            Self::Crush => "Crush",
            Self::Down => "Down",
        }
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
}

impl Default for Distort {
    fn default() -> Self {
        Self {
            kind: DistortType::default(),
            amount: 0x40,
            mix: 0xFF,
        }
    }
}
