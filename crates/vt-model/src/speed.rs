//! A track's speed: how long each of its steps is, as a note division, so one track can run in
//! eighths or triplets against the others' sixteenths. It's the Metropolix's clock division and
//! Elektron's per-track scale, whose ×3/2 and ×3/4 are the sixteenth and eighth triplets.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::constants::TICKS_PER_STEP;
use crate::step::most_nudge;

/// How long each of a track's steps is.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum Speed {
    ThirtySecond,
    SixteenthTriplet,
    /// The song's own step, as every track played before there was a speed.
    #[default]
    Sixteenth,
    EighthTriplet,
    Eighth,
    QuarterTriplet,
    Quarter,
}

/// What each speed is called, in order, as `RATE` lists them.
pub const SPEED_NAMES: [&str; 7] = ["1/32", "1/16T", "1/16", "1/8T", "1/8", "1/4T", "1/4"];

impl Speed {
    /// Every speed, from the shortest step to the longest.
    pub const ALL: [Self; 7] = [
        Self::ThirtySecond,
        Self::SixteenthTriplet,
        Self::Sixteenth,
        Self::EighthTriplet,
        Self::Eighth,
        Self::QuarterTriplet,
        Self::Quarter,
    ];

    /// How long a step is, in ticks, 240 to a sixteenth.
    pub const fn ticks(self) -> u64 {
        match self {
            Self::ThirtySecond => TICKS_PER_STEP / 2,
            Self::SixteenthTriplet => TICKS_PER_STEP * 2 / 3,
            Self::Sixteenth => TICKS_PER_STEP,
            Self::EighthTriplet => TICKS_PER_STEP * 4 / 3,
            Self::Eighth => TICKS_PER_STEP * 2,
            Self::QuarterTriplet => TICKS_PER_STEP * 8 / 3,
            Self::Quarter => TICKS_PER_STEP * 4,
        }
    }

    /// The furthest a row's nudge goes either way at this speed, just under half its step: from
    /// `5` at `1/32` to `2F` at `1/4`.
    pub const fn most_nudge(self) -> i8 {
        most_nudge(self.ticks())
    }

    /// Its place in the list, which is its value on a step.
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// The speed at a place in the list, or the last for a place past its end, as a step's value
    /// past the list plays the last choice.
    pub fn from_index(index: usize) -> Self {
        Self::ALL[index.min(Self::ALL.len() - 1)]
    }

    /// The speed a name such as `1/8T` gives, ignoring case.
    pub fn named(name: &str) -> Option<Self> {
        let name = name.trim();
        Self::ALL
            .into_iter()
            .find(|speed| speed.name().eq_ignore_ascii_case(name))
    }

    /// Its name, as a note division.
    pub const fn name(self) -> &'static str {
        SPEED_NAMES[self as usize]
    }

    /// What it plays at, against the song's sixteenths.
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::ThirtySecond => "Twice as fast as the song's sixteenths",
            Self::SixteenthTriplet => "Three steps in two sixteenths",
            Self::Sixteenth => "The song's own steps",
            Self::EighthTriplet => "Three steps in four sixteenths",
            Self::Eighth => "Half as fast, a step to two sixteenths",
            Self::QuarterTriplet => "Three steps in eight sixteenths",
            Self::Quarter => "A quarter as fast, a step to a beat",
        }
    }

    /// How many of the song's steps a run of this many of a track's steps lasts, rounded up, as a
    /// triplet's don't always end on one.
    pub const fn song_steps(self, steps: usize) -> usize {
        (steps as u64 * self.ticks()).div_ceil(TICKS_PER_STEP) as usize
    }
}

impl fmt::Display for Speed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_are_note_divisions() {
        let ticks: Vec<u64> = Speed::ALL.iter().map(|speed| speed.ticks()).collect();
        assert_eq!(ticks, [120, 160, 240, 320, 480, 640, 960]);
        let most: Vec<i8> = Speed::ALL.iter().map(|speed| speed.most_nudge()).collect();
        assert_eq!(most, [5, 7, 11, 15, 23, 31, 47]);
        assert_eq!(Speed::default(), Speed::Sixteenth);
        assert_eq!(Speed::from_index(4), Speed::Eighth);
        assert_eq!(Speed::from_index(99), Speed::Quarter);
        assert_eq!(Speed::named("1/8t"), Some(Speed::EighthTriplet));
        assert_eq!(Speed::named("1/12"), None);
        for (at, speed) in Speed::ALL.iter().enumerate() {
            assert_eq!(usize::from(speed.index()), at);
        }
    }

    #[test]
    fn song_steps_round_a_triplet_up() {
        assert_eq!(Speed::Eighth.song_steps(16), 32);
        assert_eq!(Speed::SixteenthTriplet.song_steps(24), 16);
        assert_eq!(Speed::SixteenthTriplet.song_steps(16), 11);
        assert_eq!(Speed::ThirtySecond.song_steps(3), 2);
    }
}
