//! An FX column's lane: a run of a phrase's stored rows that the column loops on by itself, with
//! a division of its own, and what a track's `Length` and `Rate` for the column move it to as it's
//! heard (the lanes study, `docs/designs/lanes.md`).

use serde::{Deserialize, Serialize};

use crate::constants::MAX_PHRASE_STEPS;

/// The most of a track's steps a lane's row lasts, as Kria's divisions go.
pub const MOST_DIVISION: usize = 16;

/// What a lane's `Rate` does to its own division, in order from the slowest to the fastest, as
/// `=` goes up it: `÷16` to `÷2`, which play it that many times slower, `×1`, then `×2` to `×16`,
/// that many times faster, as a clock divider's `÷2` gives half the pulses.
pub const LANE_RATES: [&str; 31] = [
    "÷16", "÷15", "÷14", "÷13", "÷12", "÷11", "÷10", "÷9", "÷8", "÷7", "÷6", "÷5", "÷4", "÷3",
    "÷2", "×1", "×2", "×3", "×4", "×5", "×6", "×7", "×8", "×9", "×10", "×11", "×12", "×13", "×14",
    "×15", "×16",
];

/// Where a lane's `Length` and `Rate` sit at rest, `+0` and `×1`, which leave every phrase's lane
/// as it's written.
pub(crate) const LENGTH_AS_WRITTEN: u8 = 0x80;
pub const RATE_AS_WRITTEN: u8 = 15;

/// Where an FX column loops on its own, among a phrase's stored steps. It drifts against the
/// notes, which follow the phrase's loop, and moves on a row every `division` of the track's
/// steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lane {
    pub start: usize,
    pub length: usize,
    /// How many of the track's steps each of its rows lasts, from 1 to [`MOST_DIVISION`], saved
    /// only where it isn't 1.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub division: usize,
}

fn one() -> usize {
    1
}

fn is_one(division: &usize) -> bool {
    *division == 1
}

/// How a lane is heard: how many rows it plays before it comes round, and how many of the track's
/// steps each lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heard {
    pub length: usize,
    pub division: usize,
    /// How many of its own rows on from its first it starts, within its own length.
    pub start: usize,
}

impl Lane {
    /// A lane of rows a step each.
    pub const fn new(start: usize, length: usize) -> Self {
        Self {
            start,
            length,
            division: 1,
        }
    }

    /// Whether a stored step is in it.
    pub fn contains(self, step: usize) -> bool {
        (self.start..self.start + self.length).contains(&step)
    }

    /// The stored step it reads at a row of the length it's heard at: one heard longer than its
    /// own plays its own rows again, from the first, so its rows past its length are copies.
    pub fn stored(self, row: usize) -> usize {
        self.start + row % self.length.max(1)
    }

    /// The stored step it reads at a row of the length it's heard at, heard starting so many of
    /// its own rows on, going round them (decision 401).
    pub fn stored_from(self, row: usize, start: usize) -> usize {
        self.stored(row + start)
    }

    /// How it's heard with a track's `Length` and `Rate` for its column at these values: its own
    /// length moved by that many rows, from 1 to [`MAX_PHRASE_STEPS`], and its own division slowed
    /// or sped up by the rate, rounded to the nearest step, from 1 to [`MOST_DIVISION`].
    pub fn heard(self, moved: LaneMove) -> Heard {
        Heard {
            length: self.heard_length(length_rows(moved.length)),
            division: rate_division(self.division, usize::from(moved.rate)),
            start: usize::from(moved.start) % self.length.max(1),
        }
    }

    /// Its own length moved by this many rows, from 1 to [`MAX_PHRASE_STEPS`].
    pub fn heard_length(self, by: i32) -> usize {
        (self.length.max(1) as i64 + i64::from(by)).clamp(1, MAX_PHRASE_STEPS as i64) as usize
    }
}

/// How many rows a `Length` moves a lane by: `80` is none, and each step is a row, so `00` is
/// `-128` and `FF` is `+127`, as `Pitch` is a semitone a step.
pub fn length_rows(value: u8) -> i32 {
    i32::from(value) - i32::from(LENGTH_AS_WRITTEN)
}

/// A division at a `Rate`, by its place in [`LANE_RATES`]: times `n` at `÷n`, slower, and over `n`
/// at `×n`, faster, rounded to the nearest whole step and kept from 1 to [`MOST_DIVISION`], so a
/// lane of a step a row goes no faster.
pub(crate) fn rate_division(division: usize, rate: usize) -> usize {
    let division = division.clamp(1, MOST_DIVISION);
    let rate = rate.min(LANE_RATES.len() - 1);
    let at_rest = usize::from(RATE_AS_WRITTEN);
    let heard = match rate.checked_sub(at_rest) {
        // Halves round up, so a division of 3 at `×2` is 2.
        Some(above) => {
            let by = above + 1;
            (2 * division + by) / (2 * by)
        }
        None => division * (at_rest + 1 - rate),
    };
    heard.clamp(1, MOST_DIVISION)
}

/// What a track's `Length` and `Rate` for an FX column move every phrase's lane on that column
/// by, relative to its own: `Length` in rows either side of `80`, and `Rate` a place in
/// [`LANE_RATES`]. A column without a lane isn't touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneMove {
    #[serde(default = "length_as_written")]
    pub length: u8,
    #[serde(default = "rate_as_written")]
    pub rate: u8,
    /// So many of its own rows on that each lane starts, going round them (decision 401).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub start: u8,
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

fn length_as_written() -> u8 {
    LENGTH_AS_WRITTEN
}

fn rate_as_written() -> u8 {
    RATE_AS_WRITTEN
}

impl Default for LaneMove {
    fn default() -> Self {
        Self {
            length: LENGTH_AS_WRITTEN,
            rate: RATE_AS_WRITTEN,
            start: 0,
        }
    }
}

impl LaneMove {
    /// Whether it leaves every lane as it's written.
    pub(crate) fn at_rest(self) -> bool {
        self == Self::default()
    }

    /// One of its values, by its place: 0 is `Length`, 1 is `Rate`, 2 is `Start`.
    pub fn value(self, at: usize) -> u8 {
        match at {
            0 => self.length,
            1 => self.rate,
            _ => self.start,
        }
    }

    /// Set one of its values, by its place, `Rate` kept to its list.
    pub(crate) fn set(&mut self, at: usize, value: u8) {
        match at {
            0 => self.length = value,
            1 => self.rate = value.min(LANE_RATES.len() as u8 - 1),
            _ => self.start = value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stretched_lane_copies_its_rows_and_a_shrunk_one_plays_its_first() {
        let lane = Lane::new(2, 4);
        let rows =
            |length: usize| -> Vec<usize> { (0..length).map(|row| lane.stored(row)).collect() };
        assert_eq!(rows(lane.heard_length(-1)), [2, 3, 4]);
        assert_eq!(rows(lane.heard_length(0)), [2, 3, 4, 5]);
        assert_eq!(rows(lane.heard_length(2)), [2, 3, 4, 5, 2, 3]);
        assert_eq!(rows(lane.heard_length(4)), [2, 3, 4, 5, 2, 3, 4, 5]);
        assert_eq!(lane.heard_length(-100), 1, "never below a row");
        assert_eq!(Lane::new(0, 4000).heard_length(127), MAX_PHRASE_STEPS);
    }

    #[test]
    fn a_divided_rate_is_slower_and_a_multiplied_one_faster_kept_to_the_range() {
        assert_eq!(LANE_RATES[usize::from(RATE_AS_WRITTEN)], "×1");
        assert_eq!(rate_division(1, 15), 1);
        assert_eq!(rate_division(1, 14), 2, "÷2, half as fast");
        assert_eq!(rate_division(1, 0), 16, "÷16");
        assert_eq!(rate_division(3, 0), 16, "kept to 16");
        assert_eq!(rate_division(8, 16), 4, "×2, twice as fast");
        assert_eq!(rate_division(3, 16), 2, "1.5 rounds up");
        assert_eq!(rate_division(1, 30), 1, "never faster than a row a step");
        assert_eq!(rate_division(1, 16), 1);
        assert_eq!(rate_division(16, 30), 1, "×16");
        assert_eq!(rate_division(5, 17), 2, "×3 of 5 is 1.67");
        // Up the list is faster, never slower.
        for division in 1..=MOST_DIVISION {
            let heard: Vec<usize> = (0..LANE_RATES.len())
                .map(|rate| rate_division(division, rate))
                .collect();
            assert!(heard.windows(2).all(|two| two[1] <= two[0]), "{heard:?}");
        }
    }

    #[test]
    fn a_track_at_rest_hears_a_lane_as_written() {
        let lane = Lane {
            division: 3,
            ..Lane::new(0, 7)
        };
        let heard = lane.heard(LaneMove::default());
        assert_eq!((heard.length, heard.division), (7, 3));
        let moved = LaneMove {
            length: 0x82,
            rate: 14,
            start: 0,
        };
        assert_eq!(
            lane.heard(moved),
            Heard {
                length: 9,
                division: 6,
                start: 0,
            },
            "÷2"
        );
        assert_eq!(length_rows(0x00), -128);
        assert_eq!(length_rows(0xFF), 127);
    }

    #[test]
    fn a_division_of_one_and_a_track_at_rest_are_not_saved() {
        let text = ron::to_string(&Lane::new(1, 4)).unwrap();
        assert!(!text.contains("division"), "{text}");
        let slow = Lane {
            division: 2,
            ..Lane::new(1, 4)
        };
        let text = ron::to_string(&slow).unwrap();
        assert_eq!(ron::from_str::<Lane>(&text).unwrap(), slow);
        assert_eq!(
            ron::from_str::<Lane>("(start:1,length:4)").unwrap(),
            Lane::new(1, 4)
        );
        assert!(LaneMove::default().at_rest());
    }

    #[test]
    fn a_track_saves_its_lanes_length_and_rate_only_while_one_moves() {
        let mut track = crate::song::Track::default();
        let text = ron::to_string(&track).unwrap();
        assert!(!text.contains("lanes"), "{text}");
        track.lanes[1] = LaneMove {
            length: 0x82,
            ..LaneMove::default()
        };
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("lanes"), "{text}");
        assert_eq!(ron::from_str::<crate::song::Track>(&text).unwrap(), track);
    }
}
