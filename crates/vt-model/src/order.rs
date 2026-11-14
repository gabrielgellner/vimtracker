//! The order a phrase's rows play in, after the Metropolix's ORDER: forward as written, reverse,
//! four ways of going up and down, four that visit the rows out of turn, and two that are dealt at
//! random from the project's seed; and after Make Noise's René, which reads the rows as a grid,
//! snakes that wind through it and a Cartesian order of two counters (`rene`).
//!
//! An order is a walk the track keeps taking over the rows, while a pass of the phrase stays the
//! phrase's length in steps, what its rows' pulses add up to, as it is forward. A round of an
//! order is the visits it makes before it repeats. Most visit each row once, so a round is a pass.
//! `Up-down`, `Down-up`, their `-inc` forms and `Pedal` visit some rows twice, so their rounds are
//! longer than a pass and drift across the passes, as a polymeter does. `Shuffle` deals a new
//! round each pass, and `Random` picks rows until the pass is full. A round is a list of places,
//! each a visit to a row, but for the grid orders, whose places on cells past the last row are
//! passed over.
//!
//! A new order is a variant of [`Order`], with its own data in it if it has any, its places and
//! the row at each worked out in [`Order::places`] and [`Order::row`], and its name, its words and
//! what it means beside the others'. A family of them with parts of its own, as René's, has a
//! module of its own.

mod rene;

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::step::Step;

pub use rene::{Cartesian, Grid, MAX_LENGTH, MAX_RATE, Snake};

/// The order a phrase's rows play in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Order {
    /// Top to bottom, as every phrase played before there were orders.
    #[default]
    Forward,
    Reverse,
    /// First row to last and back, turning on the end rows: `1 2 3 4 3 2 1 2`, the Metropolix's
    /// Pendulum.
    UpDown,
    /// First row to last and back, playing the end rows twice: `1 2 3 4 4 3 2 1 1 2`, the
    /// Metropolix's Ping Pong.
    UpDownInc,
    /// Last row to first and back, turning on the end rows: `4 3 2 1 2 3 4 3`, the Metropolix's
    /// Pendulum in reverse.
    DownUp,
    /// Last row to first and back, playing the end rows twice: `4 3 2 1 1 2 3 4 4`, the
    /// Metropolix's Ping Pong in reverse.
    DownUpInc,
    OddEven,
    Converge,
    Diverge,
    /// The first row between each of the others.
    Pedal,
    /// Every row once a pass, in an order dealt again each pass.
    Shuffle,
    /// Any row each time, picked as a die is rolled.
    Random,
    /// A path through the rows read as a grid, after René's snake patterns.
    Snake(Snake),
    /// Two counters, one along a line of the grid and one down the lines, each at its own rate,
    /// after René's Cartesian channel.
    Cartesian(Cartesian),
}

impl Order {
    /// Every order, as lists show them, Cartesian with the rates it starts from.
    pub const ALL: [Self; 21] = [
        Self::Forward,
        Self::Reverse,
        Self::UpDown,
        Self::UpDownInc,
        Self::DownUp,
        Self::DownUpInc,
        Self::OddEven,
        Self::Converge,
        Self::Diverge,
        Self::Pedal,
        Self::Shuffle,
        Self::Random,
        Self::Snake(Snake::Zigzag),
        Self::Snake(Snake::Columns),
        Self::Snake(Snake::ColumnZigzag),
        Self::Snake(Snake::Diagonal),
        Self::Snake(Snake::DiagonalZigzag),
        Self::Snake(Snake::Spiral),
        Self::Snake(Snake::SpiralOut),
        Self::Snake(Snake::Checker),
        Self::Cartesian(Cartesian::START),
    ];

    /// What its kind is called: its whole name but for a snake's word and Cartesian's numbers,
    /// which [`fmt::Display`] adds, as the phrase view's header and lists show it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Forward => "Forward",
            Self::Reverse => "Reverse",
            Self::UpDown => "Up-down",
            Self::UpDownInc => "Up-down-inc",
            Self::DownUp => "Down-up",
            Self::DownUpInc => "Down-up-inc",
            Self::OddEven => "Odd-even",
            Self::Converge => "Converge",
            Self::Diverge => "Diverge",
            Self::Pedal => "Pedal",
            Self::Shuffle => "Shuffle",
            Self::Random => "Random",
            Self::Snake(_) => "Snake",
            Self::Cartesian(_) => "Cartesian",
        }
    }

    /// The order words such as `up-down`, `snake spiral`, `snake 6` or `cartesian 1 3 4 2` give,
    /// ignoring case: what `:order` takes, which is its name in lower case.
    pub fn named(text: &str) -> Option<Self> {
        let mut words = text.split_whitespace();
        let first = words.next()?;
        let rest: Vec<&str> = words.collect();
        if first.eq_ignore_ascii_case("snake") {
            return match rest[..] {
                [] => Some(Self::Snake(Snake::default())),
                [snake] => Snake::named(snake).map(Self::Snake),
                _ => None,
            };
        }
        if first.eq_ignore_ascii_case("cartesian") {
            return Cartesian::named(&rest).map(Self::Cartesian);
        }
        if !rest.is_empty() {
            return None;
        }
        Self::ALL.into_iter().find(|order| {
            !matches!(order, Self::Snake(_) | Self::Cartesian(_))
                && order.name().eq_ignore_ascii_case(first)
        })
    }

    /// What it plays over two passes of a phrase of four rows, counted from 1.
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::Forward => "Top to bottom, as written: 1 2 3 4 1 2 3 4",
            Self::Reverse => "Bottom to top: 4 3 2 1 4 3 2 1",
            Self::UpDown => "First to last and back, the ends once: 1 2 3 4 3 2 1 2",
            Self::UpDownInc => "First to last and back, the ends twice: 1 2 3 4 4 3 2 1",
            Self::DownUp => "Last to first and back, the ends once: 4 3 2 1 2 3 4 3",
            Self::DownUpInc => "Last to first and back, the ends twice: 4 3 2 1 1 2 3 4",
            Self::OddEven => "Every other row, then the rows between: 1 3 2 4 1 3 2 4",
            Self::Converge => "From both ends in to the middle: 1 4 2 3 1 4 2 3",
            Self::Diverge => "From the middle out to both ends: 2 3 1 4 2 3 1 4",
            Self::Pedal => "The first row between the others: 1 2 1 3 1 4 1 2",
            Self::Shuffle => "Every row once a pass, in an order dealt again each pass",
            Self::Random => "Any row each time, so rows can repeat or be left out",
            Self::Snake(snake) => snake.meaning(),
            Self::Cartesian(_) => {
                "X along a line, Y down the lines, each every so many rows: 1 2 3 8 5 6 11 12 …"
            }
        }
    }

    /// What it plays, in words, for a phrase of this many rows: its meaning, or for Cartesian
    /// what its counters do on the grid the rows make.
    pub fn describe(self, rows: usize) -> String {
        match self {
            Self::Cartesian(cartesian) => cartesian.describe(Grid::of(rows)),
            _ => self.meaning().to_owned(),
        }
    }

    /// Whether it's of the same kind as another, whatever their snakes or numbers.
    pub fn is_like(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }

    /// Whether it's dealt from the project's seed, so which row plays when depends on the pass.
    /// A dealt order's round is one pass.
    pub const fn is_dealt(self) -> bool {
        matches!(self, Self::Shuffle | Self::Random)
    }

    /// Whether it's the order every phrase plays in unless it's given another, which files leave
    /// out.
    pub fn is_forward(&self) -> bool {
        *self == Self::Forward
    }

    /// How many places a round of this many rows has before it repeats, for every order but
    /// `Random`, which picks until the pass is full. Each is a visit to a row, but for a grid
    /// order's places on cells past the last row.
    pub fn places(self, rows: usize) -> usize {
        match self {
            Self::UpDown | Self::DownUp | Self::Pedal if rows > 1 => 2 * rows - 2,
            Self::UpDownInc | Self::DownUpInc => 2 * rows,
            Self::Snake(_) if rows > 1 => Grid::of(rows).cells(),
            Self::Cartesian(cartesian) if rows > 1 => cartesian.places(Grid::of(rows)),
            _ => rows,
        }
    }

    /// The row a place of a round plays, for a phrase of this many rows, with the place below
    /// [`Order::places`], or none for a grid order's cell past the last row. `Random` picks among
    /// every row here; a round leaves out those that take no time.
    pub fn row(self, rows: usize, place: usize, deal: Deal) -> Option<usize> {
        if rows <= 1 {
            return Some(0);
        }
        let visit = place;
        let last = rows - 1;
        let row = match self {
            Self::Forward => visit,
            Self::Reverse => last - visit,
            Self::UpDown if visit < rows => visit,
            Self::UpDown => 2 * last - visit,
            Self::UpDownInc if visit < rows => visit,
            Self::UpDownInc => 2 * rows - 1 - visit,
            Self::DownUp if visit < rows => last - visit,
            Self::DownUp => visit - last,
            Self::DownUpInc if visit < rows => last - visit,
            Self::DownUpInc => visit - rows,
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
            Self::Snake(snake) => snake.cell(Grid::of(rows), place),
            Self::Cartesian(cartesian) => cartesian.cell(Grid::of(rows), place),
        };
        (row < rows).then_some(row)
    }
}

/// The words `:order` starts with, in the order lists show them: each order's, then `snake` and
/// `cartesian`, which take a snake or numbers after them.
pub const ORDER_WORDS: [&str; 14] = [
    "forward",
    "reverse",
    "up-down",
    "up-down-inc",
    "down-up",
    "down-up-inc",
    "odd-even",
    "converge",
    "diverge",
    "pedal",
    "shuffle",
    "random",
    "snake",
    "cartesian",
];

impl fmt::Display for Order {
    /// Its name, with a snake's word or Cartesian's numbers as `:order` takes them: `Snake
    /// spiral`, `Cartesian 1 3`, or `Cartesian 1 3 4 2` with its lengths.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())?;
        match self {
            Self::Snake(snake) => write!(f, " {}", snake.word()),
            Self::Cartesian(cartesian) => {
                write!(f, " {} {}", cartesian.x_rate, cartesian.y_rate)?;
                if !cartesian.is_whole() {
                    write!(f, " {} {}", cartesian.x_length, cartesian.y_length)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
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

/// The row a `Random` picks at a visit, on a try at it, as a pick that takes no time, or more than
/// the pass has left, is picked again.
fn picked(deal: Deal, rows: usize, visit: usize, attempt: u64) -> usize {
    below(mix(deal.0 ^ mix((visit as u64) << 8 | attempt)), rows)
}

/// The rows one round of an order visits, in turn: the visits the walk repeats.
#[derive(Debug, Clone)]
pub struct Round<'a> {
    steps: &'a [Step],
    order: Order,
    deal: Deal,
    /// The place the round is at, which for `Random` is its visit.
    visit: usize,
    /// How many places the round has, for every order but `Random`.
    places: usize,
    /// How many steps `Random` has still to fill, as it stops when the pass is full.
    left: usize,
}

impl<'a> Round<'a> {
    /// A round of these rows in this order, with this deal if it's dealt.
    pub fn new(steps: &'a [Step], order: Order, deal: Deal) -> Self {
        Self {
            steps,
            order,
            deal,
            visit: 0,
            places: order.places(steps.len()),
            left: steps.iter().map(Step::pulses).sum(),
        }
    }
}

/// How many times `Random` picks again for a row that fits, before taking the next one down that
/// does.
const PICKS: u64 = 16;

impl Iterator for Round<'_> {
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
            // A row that takes no time would be no pick at all, and one that runs past the end of
            // the pass would split its pulses, so either is picked again. Only when no row fits
            // what's left is the last pick cut short.
            let left = self.left;
            let pick = |fits: &dyn Fn(usize) -> bool| {
                (0..PICKS)
                    .map(|attempt| picked(self.deal, rows, visit, attempt))
                    .find(|row| fits(*row))
                    .or_else(|| {
                        let from = picked(self.deal, rows, visit, 0);
                        (0..rows)
                            .map(|at| (from + at) % rows)
                            .find(|row| fits(*row))
                    })
            };
            let pulses = |row: usize| self.steps[row].pulses();
            let row = pick(&|row| (1..=left).contains(&pulses(row)))
                .or_else(|| pick(&|row| pulses(row) > 0))?;
            self.left = self.left.saturating_sub(self.steps[row].pulses());
            row
        } else {
            // A grid order's cells past the last row are passed over.
            let mut place = visit;
            let row = loop {
                if place >= self.places {
                    self.visit = place;
                    return None;
                }
                if let Some(row) = self.order.row(rows, place, self.deal) {
                    break row;
                }
                place += 1;
            };
            self.visit = place;
            row
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
        Round::new(&steps, order, Deal::default())
            .map(|row| row + 1)
            .collect()
    }

    fn manual(text: &str) -> Vec<usize> {
        text.split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect()
    }

    #[test]
    fn the_fixed_orders_go_round_as_the_metropolix_manual_shows_them() {
        assert_eq!(rows(Order::Forward, 8), manual("1 2 3 4 5 6 7 8"));
        assert_eq!(rows(Order::Reverse, 8), manual("8 7 6 5 4 3 2 1"));
        // Pendulum, forward and reverse.
        assert_eq!(
            rows(Order::UpDown, 8),
            manual("1 2 3 4 5 6 7 8 7 6 5 4 3 2")
        );
        assert_eq!(
            rows(Order::DownUp, 8),
            manual("8 7 6 5 4 3 2 1 2 3 4 5 6 7")
        );
        // Ping Pong, forward and reverse.
        assert_eq!(
            rows(Order::UpDownInc, 8),
            manual("1 2 3 4 5 6 7 8 8 7 6 5 4 3 2 1")
        );
        assert_eq!(
            rows(Order::DownUpInc, 8),
            manual("8 7 6 5 4 3 2 1 1 2 3 4 5 6 7 8")
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
                if matches!(order, Order::Cartesian(_)) {
                    // Its counters can leave cells out, as René's do.
                    continue;
                }
                played.sort_unstable();
                played.dedup();
                assert_eq!(played, (1..=length).collect::<Vec<_>>(), "{order} {length}");
            }
        }
        assert_eq!(rows(Order::UpDown, 1), [1]);
        assert_eq!(rows(Order::DownUp, 1), [1]);
        assert_eq!(rows(Order::Pedal, 1), [1]);
        assert_eq!(rows(Order::UpDownInc, 1), [1, 1]);
        assert_eq!(rows(Order::DownUpInc, 2), [2, 1, 1, 2]);
        assert_eq!(rows(Order::DownUp, 2), [2, 1]);
        assert_eq!(rows(Order::Diverge, 7), [4, 5, 3, 6, 2, 7, 1]);
    }

    #[test]
    fn a_shuffle_plays_every_row_once_and_is_dealt_again_each_pass() {
        let steps = vec![Step::default(); 16];
        let dealer = Dealer::new(7, 0);
        let pass =
            |lap| Round::new(&steps, Order::Shuffle, dealer.deal(lap, 0)).collect::<Vec<_>>();
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
    fn random_picks_rows_that_take_time_until_the_pass_is_full() {
        let mut steps: Vec<Step> = (0..8)
            .map(|_| "C-3 -- SGL1 --- --- ---".parse().unwrap())
            .collect();
        for skipped in [1, 3, 5, 7] {
            steps[skipped] = "C-3 -- SGL0 --- --- ---".parse().unwrap();
        }
        let dealer = Dealer::new(3, 2);
        let mut seen = [false; 8];
        for lap in 0..32 {
            let played: Vec<usize> =
                Round::new(&steps, Order::Random, dealer.deal(lap, 0)).collect();
            assert_eq!(played.len(), 4, "four steps' worth of one-pulse rows");
            for row in played {
                assert_eq!(row % 2, 0, "never a skipped row");
                seen[row] = true;
            }
        }
        assert_eq!(seen, [true, false, true, false, true, false, true, false]);
    }

    #[test]
    fn random_picks_rows_that_fit_what_the_pass_has_left() {
        // Three steps a pass, from a row of two and a row of one: whatever comes first, the pass
        // is filled without splitting a row.
        let steps: Vec<Step> = ["C-3 -- SGL2 --- --- ---", "C-3 -- SGL1 --- --- ---"]
            .iter()
            .map(|text| text.parse().unwrap())
            .collect();
        let dealer = Dealer::new(5, 0);
        for lap in 0..32 {
            let played: Vec<usize> =
                Round::new(&steps, Order::Random, dealer.deal(lap, 0)).collect();
            let time: usize = played.iter().map(|row| steps[*row].pulses()).sum();
            assert_eq!(time, 3, "{played:?}");
        }
    }

    #[test]
    fn orders_are_named_ignoring_case() {
        for order in Order::ALL {
            assert_eq!(Order::named(&order.to_string().to_lowercase()), Some(order));
            assert!(ORDER_WORDS.contains(&order.name().to_lowercase().as_str()));
        }
        assert_eq!(Order::named("UP-DOWN-INC"), Some(Order::UpDownInc));
        assert_eq!(Order::named("pendulum"), None, "the old names are gone");
        assert_eq!(Order::named("sideways"), None);
        assert_eq!(Order::named("up-down 2"), None);
    }

    #[test]
    fn snakes_are_named_by_word_or_number_and_cartesian_by_its_numbers() {
        assert_eq!(Order::named("snake"), Some(Order::Snake(Snake::Zigzag)));
        assert_eq!(Order::named("snake 6"), Some(Order::Snake(Snake::Spiral)));
        assert_eq!(
            Order::named("Snake Spiral"),
            Some(Order::Snake(Snake::Spiral))
        );
        assert_eq!(Order::named("snake 9"), None);
        assert_eq!(Order::named("snake 0"), None);
        assert_eq!(Order::named("snake wiggle"), None);
        assert_eq!(
            Order::named("cartesian"),
            Some(Order::Cartesian(Cartesian::START))
        );
        let rates = Order::named("cartesian 2 5").unwrap();
        assert_eq!(rates, Order::Cartesian(Cartesian::new(2, 5)));
        assert_eq!(rates.to_string(), "Cartesian 2 5");
        let sized = Order::named("cartesian 1 3 4 2").unwrap();
        assert_eq!(sized, Order::Cartesian(Cartesian::new(1, 3).sized(4, 2)));
        assert_eq!(sized.to_string(), "Cartesian 1 3 4 2");
        for wrong in [
            "cartesian 1",
            "cartesian 0 3",
            "cartesian 1 17",
            "cartesian 1 3 9 2",
            "cartesian 1 3 4",
            "cartesian a b",
        ] {
            assert_eq!(Order::named(wrong), None, "{wrong}");
        }
        assert_eq!(
            Order::Snake(Snake::ColumnZigzag).to_string(),
            "Snake column-zigzag"
        );
    }

    #[test]
    fn a_snake_passes_over_the_cells_past_the_last_row() {
        // Six rows are a grid four wide and two high, its last two cells empty.
        assert_eq!(rows(Order::Snake(Snake::Zigzag), 6), manual("1 2 3 4 6 5"));
        assert_eq!(rows(Order::Snake(Snake::Columns), 6), manual("1 5 2 6 3 4"));
        assert_eq!(rows(Order::Snake(Snake::Spiral), 16), {
            manual("1 2 3 4 8 12 16 15 14 13 9 5 6 7 11 10")
        });
    }

    #[test]
    fn cartesian_rounds_visit_its_cells_in_the_rows_there_are() {
        assert_eq!(
            rows(Order::Cartesian(Cartesian::new(1, 3)), 16),
            manual("1 2 3 8 5 6 11 12 9 14 15 16")
        );
        // On six rows, a grid of four by two: X every row, Y every row, so the diagonal, whose
        // second cell is row 6.
        assert_eq!(
            rows(Order::Cartesian(Cartesian::new(1, 1)), 6),
            manual("1 6 3")
        );
        assert_eq!(
            Order::Cartesian(Cartesian::new(1, 1)).places(6),
            4,
            "its fourth place, row 8, is past the last"
        );
    }
}
