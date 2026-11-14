//! The Metropolix's orders that take a number, `Jump by` and the `Arp` shifts, and its two that
//! grow and shrink a run from the first row (manual v1.6.5, pp. 71–73). Each works out the row at
//! a place without the places before, as the walk needs.

use serde::{Deserialize, Serialize};

/// The most rows a jump or an arp's run can take, and the furthest a run's start moves on.
pub const MOST_ROWS: u8 = 16;

/// What `:order jump` alone and the list give: three rows on, which plays every row of a phrase
/// of 4, 8 or 16, where the Metropolix's first, `Jump by 2`, leaves half of them out.
pub const JUMP_START: u8 = 3;

/// How many places a jump of so many rows takes to come back to the first: the rows over what
/// they and the jump have in common.
pub fn jump_places(rows: usize, by: u8) -> usize {
    rows / gcd(rows, usize::from(by) % rows)
}

/// The row a jump plays at a place: so many rows on from the last, going round.
pub fn jump_row(rows: usize, by: u8, place: usize) -> usize {
    place * usize::from(by) % rows
}

/// The Metropolix's `Arp` orders: runs of `notes` rows, each starting `step` rows on from where
/// the last started, going round, so its `Arp 3 Shift -1` is 3 1 (`123 234 345`), `Arp 4 Shift -2`
/// is 4 1 (`1234 2345`) and `Arp 4 Shift -1` is 4 2 (`1234 3456`). Its shift is how far back the
/// next run starts from the last one's end; how far on it starts reads more plainly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Arp {
    pub notes: u8,
    pub step: u8,
}

impl Default for Arp {
    fn default() -> Self {
        Self::START
    }
}

impl Arp {
    /// What `:order arp` alone and the list give: runs of three, each a row on, the Metropolix's
    /// first.
    pub const START: Self = Self { notes: 3, step: 1 };

    /// The one the numbers after `:order arp` give: none, or how many rows a run takes, 2 to 16,
    /// and how far on each starts, 1 to 16.
    pub fn named(words: &[&str]) -> Option<Self> {
        let numbers: Option<Vec<u8>> = words.iter().map(|word| word.parse().ok()).collect();
        match numbers?[..] {
            [] => Some(Self::START),
            [notes, step]
                if (2..=MOST_ROWS).contains(&notes) && (1..=MOST_ROWS).contains(&step) =>
            {
                Some(Self { notes, step })
            }
            _ => None,
        }
    }

    /// How many places its round has: a run from each start it comes to before it's back at the
    /// first row.
    pub fn places(self, rows: usize) -> usize {
        usize::from(self.notes) * jump_places(rows, self.step)
    }

    /// The row it plays at a place.
    pub fn row(self, rows: usize, place: usize) -> usize {
        let notes = usize::from(self.notes);
        let start = jump_row(rows, self.step, place / notes);
        (start + place % notes) % rows
    }
}

/// How many places a `Growing` or `Shrinking` round has: runs of every length from one row to all
/// of them.
pub fn runs_places(rows: usize) -> usize {
    rows * (rows + 1) / 2
}

/// The row `Growing` plays at a place: runs from the first row, one row longer each time, `1 12
/// 123`.
pub fn growing_row(place: usize) -> usize {
    let mut run = 1;
    let mut at = place;
    while at >= run {
        at -= run;
        run += 1;
    }
    at
}

/// The row `Shrinking` plays at a place: runs from the first row, every row first and one fewer
/// each time, `1234 123 12 1`.
pub fn shrinking_row(rows: usize, place: usize) -> usize {
    let mut run = rows;
    let mut at = place;
    while run > 0 && at >= run {
        at -= run;
        run -= 1;
    }
    at
}

/// The row `Hopscotch` plays at a place: each row, then the one two on from it, `1 3 2 4 3 5`.
pub fn hopscotch_row(rows: usize, place: usize) -> usize {
    let row = place / 2;
    if place.is_multiple_of(2) {
        row
    } else {
        (row + 2) % rows
    }
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}
