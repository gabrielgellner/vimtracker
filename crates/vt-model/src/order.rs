//! The order a phrase's rows play in, after the Metropolix's ORDER: forward as written, reverse,
//! the two ways of going down and back, four that visit the rows out of turn, and two that are
//! dealt at random from the project's seed.
//!
//! A pass of a phrase is one time round its order. Most orders visit each row once a pass, so a
//! pass lasts what the rows' pulses add up to. `Pendulum`, `Ping-pong` and `Pedal` visit some rows
//! twice, so their passes last longer. `Random` picks rows until the pass has lasted as long as
//! the rows would forward, cutting its last pick short if it has to, so a phrase's length never
//! depends on what it picks.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::step::Step;

/// The order a phrase's rows play in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Order {
    /// Top to bottom, as every phrase played before there were orders.
    #[default]
    Forward,
    Reverse,
    /// Down and back up, with the end rows once each.
    Pendulum,
    /// Down and back up, with the end rows twice.
    PingPong,
    OddEven,
    Converge,
    Diverge,
    /// The first row between each of the others.
    Pedal,
    /// Every row once a pass, in an order dealt again each pass.
    Shuffle,
    /// Any row each time, picked as a die is rolled.
    Random,
}

impl Order {
    /// Every order, as lists show them.
    pub const ALL: [Self; 10] = [
        Self::Forward,
        Self::Reverse,
        Self::Pendulum,
        Self::PingPong,
        Self::OddEven,
        Self::Converge,
        Self::Diverge,
        Self::Pedal,
        Self::Shuffle,
        Self::Random,
    ];

    /// What it's called, as the phrase view's header and lists show it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Forward => "Forward",
            Self::Reverse => "Reverse",
            Self::Pendulum => "Pendulum",
            Self::PingPong => "Ping-pong",
            Self::OddEven => "Odd-even",
            Self::Converge => "Converge",
            Self::Diverge => "Diverge",
            Self::Pedal => "Pedal",
            Self::Shuffle => "Shuffle",
            Self::Random => "Random",
        }
    }

    /// The order a name such as `ping-pong` gives, ignoring case.
    pub fn named(name: &str) -> Option<Self> {
        let name = name.trim();
        Self::ALL
            .into_iter()
            .find(|order| order.name().eq_ignore_ascii_case(name))
    }

    /// The word `:order` takes for it.
    pub const fn word(self) -> &'static str {
        ORDER_WORDS[self as usize]
    }

    /// What it plays, with the rows of a phrase of four counted from 1 where it helps.
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::Forward => "Top to bottom, as written",
            Self::Reverse => "Bottom to top: 4 3 2 1",
            Self::Pendulum => "Down and back up, the end rows once: 1 2 3 4 3 2",
            Self::PingPong => "Down and back up, the end rows twice: 1 2 3 4 4 3 2 1",
            Self::OddEven => "Every other row, then the rows between: 1 3 2 4",
            Self::Converge => "From both ends in to the middle: 1 4 2 3",
            Self::Diverge => "From the middle out to both ends: 2 3 1 4",
            Self::Pedal => "The first row between each of the others: 1 2 1 3 1 4",
            Self::Shuffle => "Every row once a pass, in an order dealt again each pass",
            Self::Random => "Any row each time, so rows can repeat or be left out",
        }
    }

    /// Whether it's dealt from the project's seed, so which row plays when depends on the pass.
    pub const fn is_dealt(self) -> bool {
        matches!(self, Self::Shuffle | Self::Random)
    }

    /// Whether a pass lasts what the rows' pulses add up to, as each row is in it once, or as
    /// `Random` stops when it has.
    pub const fn keeps_time(self) -> bool {
        !matches!(self, Self::Pendulum | Self::PingPong | Self::Pedal)
    }

    /// Whether it's the order every phrase plays in unless it's given another, which files leave
    /// out.
    pub fn is_forward(&self) -> bool {
        *self == Self::Forward
    }

    /// How many rows a pass of this many visits, for every order but `Random`, which picks until
    /// the pass has lasted long enough.
    pub const fn visits(self, rows: usize) -> usize {
        match self {
            Self::Pendulum | Self::Pedal if rows > 1 => 2 * rows - 2,
            Self::PingPong => 2 * rows,
            _ => rows,
        }
    }

    /// The row a visit of a pass plays, for a phrase of this many rows, with the visit below
    /// [`Order::visits`]. `Random` picks among every row here; a pass leaves out those that take no
    /// time.
    pub fn row(self, rows: usize, visit: usize, deal: Deal) -> usize {
        if rows <= 1 {
            return 0;
        }
        let last = rows - 1;
        match self {
            Self::Forward => visit,
            Self::Reverse => last - visit,
            Self::Pendulum if visit < rows => visit,
            Self::Pendulum => 2 * last - visit,
            Self::PingPong if visit < rows => visit,
            Self::PingPong => 2 * rows - 1 - visit,
            Self::OddEven => {
                let evens = rows.div_ceil(2);
                if visit < evens {
                    2 * visit
                } else {
                    2 * (visit - evens) + 1
                }
            }
            Self::Converge if visit.is_multiple_of(2) => visit / 2,
            Self::Converge => last - visit / 2,
            Self::Diverge if visit.is_multiple_of(2) => last / 2 - visit / 2,
            Self::Diverge => last / 2 + 1 + visit / 2,
            Self::Pedal if visit.is_multiple_of(2) => 0,
            Self::Pedal => visit.div_ceil(2),
            Self::Shuffle => shuffled(deal, rows, visit),
            Self::Random => picked(deal, rows, visit, 0),
        }
    }
}

/// The words `:order` takes, one for each order in the order lists show them.
pub const ORDER_WORDS: [&str; 10] = [
    "forward",
    "reverse",
    "pendulum",
    "ping-pong",
    "odd-even",
    "converge",
    "diverge",
    "pedal",
    "shuffle",
    "random",
];

impl fmt::Display for Order {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What a track's dealt orders are dealt from: the project's seed and the track. It's a hash
/// rather than a generator, so it takes nothing from the track's generators for `CHA`, its
/// humanise and its arpeggios, and which rows a pass plays can be worked out from where the track
/// is, as a condition's pass is, whether playing up to it, from it or in a render.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Dealer(u64);

impl Dealer {
    /// A track's, for the project's seed.
    pub fn new(seed: u64, track: usize) -> Self {
        Self(mix(mix(seed) ^ mix(ORDER_STREAM.wrapping_add(track as u64))))
    }

    /// The deal for one time round a track's chain, counted from where it started, and one place
    /// in it, so a phrase twice in its chain is dealt twice.
    pub fn deal(self, lap: i64, slot: usize) -> Deal {
        Deal(mix(mix(self.0 ^ lap as u64) ^ slot as u64))
    }
}

/// One pass's deal of a dealt order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Deal(u64);

/// Keeps a dealer's hashes apart from any other the seed is hashed for.
const ORDER_STREAM: u64 = 0x6f72_6465_7200_0000;

/// SplitMix64's finaliser, which spreads every bit of what goes in over what comes out.
const fn mix(value: u64) -> u64 {
    let mut x = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// A number below `below` from a hash, without the bias a remainder has.
fn below(hash: u64, below: usize) -> usize {
    (((hash >> 32) * below as u64) >> 32) as usize
}

/// The row a `Shuffle` plays at a visit: a shuffle of the rows that's worked out a visit at a time,
/// as a small Feistel network over the next power of 4 up, walked on until it lands among the
/// rows. Every row comes once a pass.
fn shuffled(deal: Deal, rows: usize, visit: usize) -> usize {
    let bits = (usize::BITS - (rows - 1).leading_zeros())
        .max(2)
        .next_multiple_of(2);
    let half = bits / 2;
    let mask = (1u64 << half) - 1;
    let mut at = visit as u64;
    loop {
        let (mut left, mut right) = (at >> half, at & mask);
        for round in 0..4u64 {
            let turned = left ^ (mix(deal.0 ^ (round << 56) ^ right) & mask);
            left = right;
            right = turned;
        }
        at = left << half | right;
        if at < rows as u64 {
            return at as usize;
        }
    }
}

/// The row a `Random` picks at a visit, on a try at it, as a pick that takes no time is picked
/// again.
fn picked(deal: Deal, rows: usize, visit: usize, attempt: u64) -> usize {
    below(mix(deal.0 ^ mix((visit as u64) << 8 | attempt)), rows)
}

/// The rows one pass of a phrase visits, in order.
#[derive(Debug, Clone)]
pub struct Pass<'a> {
    steps: &'a [Step],
    order: Order,
    deal: Deal,
    visit: usize,
    /// How many steps `Random` has still to play, as it stops when they've gone.
    left: usize,
}

impl<'a> Pass<'a> {
    /// A pass of these rows in this order, with this deal if it's dealt.
    pub fn new(steps: &'a [Step], order: Order, deal: Deal) -> Self {
        Self {
            steps,
            order,
            deal,
            visit: 0,
            left: steps.iter().map(Step::pulses).sum(),
        }
    }
}

/// How many times `Random` picks again for a row that takes some time, before taking the next one
/// down that does.
const PICKS: u64 = 16;

impl Iterator for Pass<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        let rows = self.steps.len();
        if rows == 0 {
            return None;
        }
        let visit = self.visit;
        let row = if self.order == Order::Random {
            if self.left == 0 {
                return None;
            }
            // A row that takes no time would be no pick at all, so it's picked again.
            let takes_time = |row: &usize| self.steps[*row].pulses() > 0;
            let row = (0..PICKS)
                .map(|attempt| picked(self.deal, rows, visit, attempt))
                .find(takes_time)
                .or_else(|| {
                    let from = picked(self.deal, rows, visit, 0);
                    (0..rows).map(|at| (from + at) % rows).find(takes_time)
                })?;
            self.left = self.left.saturating_sub(self.steps[row].pulses());
            row
        } else {
            if visit >= self.order.visits(rows) {
                return None;
            }
            self.order.row(rows, visit, self.deal)
        };
        self.visit += 1;
        Some(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(order: Order, rows: usize) -> Vec<usize> {
        let steps = vec![Step::default(); rows];
        Pass::new(&steps, order, Deal::default())
            .map(|row| row + 1)
            .collect()
    }

    #[test]
    fn the_fixed_orders_play_as_the_metropolix_manual_shows_them() {
        let manual = |text: &str| -> Vec<usize> {
            text.split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect()
        };
        assert_eq!(rows(Order::Forward, 8), manual("1 2 3 4 5 6 7 8"));
        assert_eq!(rows(Order::Reverse, 8), manual("8 7 6 5 4 3 2 1"));
        assert_eq!(
            rows(Order::Pendulum, 8),
            manual("1 2 3 4 5 6 7 8 7 6 5 4 3 2")
        );
        assert_eq!(
            rows(Order::PingPong, 8),
            manual("1 2 3 4 5 6 7 8 8 7 6 5 4 3 2 1")
        );
        assert_eq!(rows(Order::OddEven, 8), manual("1 3 5 7 2 4 6 8"));
        assert_eq!(rows(Order::Converge, 8), manual("1 8 2 7 3 6 4 5"));
        assert_eq!(rows(Order::Diverge, 8), manual("4 5 3 6 2 7 1 8"));
        assert_eq!(rows(Order::Pedal, 8), manual("1 2 1 3 1 4 1 5 1 6 1 7 1 8"));
    }

    #[test]
    fn odd_lengths_and_one_row_visit_every_row() {
        for order in Order::ALL {
            for length in 1..=9 {
                let mut played = rows(order, length);
                if order == Order::Random {
                    assert_eq!(played.len(), length, "{order} picks as many as there are");
                    continue;
                }
                played.sort_unstable();
                played.dedup();
                assert_eq!(played, (1..=length).collect::<Vec<_>>(), "{order} {length}");
            }
        }
        assert_eq!(rows(Order::Pendulum, 1), [1]);
        assert_eq!(rows(Order::Pedal, 1), [1]);
        assert_eq!(rows(Order::PingPong, 1), [1, 1]);
        assert_eq!(rows(Order::Diverge, 7), [4, 5, 3, 6, 2, 7, 1]);
    }

    #[test]
    fn a_shuffle_plays_every_row_once_and_is_dealt_again_each_pass() {
        let steps = vec![Step::default(); 16];
        let dealer = Dealer::new(7, 0);
        let pass = |lap| Pass::new(&steps, Order::Shuffle, dealer.deal(lap, 0)).collect::<Vec<_>>();
        let (first, second) = (pass(0), pass(1));
        for played in [&first, &second] {
            let mut sorted = played.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (0..16).collect::<Vec<_>>());
        }
        assert_ne!(first, second);
        assert_ne!(first, (0..16).collect::<Vec<_>>());
        assert_eq!(first, pass(0), "the same pass deals the same");
    }

    #[test]
    fn dealers_differ_by_seed_track_lap_and_slot() {
        let deal = |seed, track, lap, slot| Dealer::new(seed, track).deal(lap, slot);
        let first = deal(0, 0, 0, 0);
        assert_ne!(first, deal(1, 0, 0, 0));
        assert_ne!(first, deal(0, 1, 0, 0));
        assert_ne!(first, deal(0, 0, 1, 0));
        assert_ne!(first, deal(0, 0, 0, 1));
        assert_eq!(first, deal(0, 0, 0, 0));
    }

    #[test]
    fn random_picks_rows_that_take_time_until_the_pass_has_lasted() {
        let mut steps: Vec<Step> = (0..8)
            .map(|_| "C-4 -- SGL1 --- --- ---".parse().unwrap())
            .collect();
        for skipped in [1, 3, 5, 7] {
            steps[skipped] = "C-4 -- SGL0 --- --- ---".parse().unwrap();
        }
        let dealer = Dealer::new(3, 2);
        let mut seen = [false; 8];
        for lap in 0..32 {
            let played: Vec<usize> =
                Pass::new(&steps, Order::Random, dealer.deal(lap, 0)).collect();
            assert_eq!(played.len(), 4, "four steps' worth of one-pulse rows");
            for row in played {
                assert_eq!(row % 2, 0, "never a skipped row");
                seen[row] = true;
            }
        }
        assert_eq!(seen, [true, false, true, false, true, false, true, false]);
    }

    #[test]
    fn orders_are_named_ignoring_case() {
        for order in Order::ALL {
            assert_eq!(Order::named(order.word()), Some(order));
        }
        assert_eq!(Order::named("PING-PONG"), Some(Order::PingPong));
        assert_eq!(Order::named("sideways"), None);
    }
}
