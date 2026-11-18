//! The order a phrase's rows play in, after the Metropolix's ORDER: forward as written, reverse,
//! four ways of going up and down, those that visit the rows out of turn or in runs
//! (`metropolix`), and four that are dealt at random from the project's seed; and after Make
//! Noise's René, which reads the rows as a grid, snakes that wind through it and a Cartesian order
//! of two counters (`rene`).
//!
//! An order is a walk the track keeps taking over the rows, while a pass of the phrase stays the
//! phrase's length in steps, what its rows' pulses add up to, as it is forward. A round of an
//! order is the visits it makes before it repeats. Most visit each row once, so a round is a pass.
//! `Up-down`, `Down-up`, their `-inc` forms, `Pedal`, `Hopscotch`, the arps and the runs visit some
//! rows more than once, so their rounds are longer than a pass and drift across the passes, as a
//! polymeter does. `Shuffle` deals a new round each pass, and `Random`, `Quasi-random` and
//! `Brownian` pick rows until the pass is full, `Brownian` walking from the first row each pass.
//! A round is a list of places,
//! each a visit to a row, but for the grid orders, whose places on cells past the last row are
//! passed over.
//!
//! A track can move the walk from its phrases' own (decision 400): its `ORD` goes so many places
//! on through the list of orders, `REV` turns the rows the other way up, and `ORDX` and `ORDY`
//! put numbers of their own in place of the order's, all read before a row plays, as a [`WalkMove`].
//!
//! A new order is a variant of [`Order`], with its own data in it if it has any, its places and
//! the row at each worked out in [`Order::places`] and [`Order::row`], and its name, its words and
//! what it means beside the others'. A family of them with parts of its own, as René's, has a
//! module of its own.

mod metropolix;
mod rene;

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::constants::MAX_PHRASE_STEPS;
use crate::random::splitmix64;
use crate::step::Step;

pub use metropolix::{Arp, JUMP_START, MOST_ROWS};
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
    /// So many rows on from the last, going round: `Jump(3)` plays `1 4 7 2 5 8 3 6` of eight,
    /// the Metropolix's `Jump by`.
    Jump(u8),
    /// Each row, then the one two on from it: `1 3 2 4 3 5`.
    Hopscotch,
    /// Runs of rows, each starting so many on from the last, after the Metropolix's `Arp` shifts.
    Arp(Arp),
    Converge,
    Diverge,
    /// Runs from the first row, one longer each time: `1 12 123`.
    Growing,
    /// Runs from the first row, all of them first and one fewer each time: `123 12 1`.
    Shrinking,
    /// The first row between each of the others.
    Pedal,
    /// A drunken walk from the first row each pass: each step a row on, a row back or staying,
    /// half, a quarter and a quarter of the time.
    Brownian,
    /// Every row once a pass, in an order dealt again each pass.
    Shuffle,
    /// Any row each time but the one just played.
    QuasiRandom,
    /// Any row each time, picked as a die is rolled.
    Random,
    /// A path through the rows read as a grid, after René's snake patterns.
    Snake(Snake),
    /// Two counters, one along a line of the grid and one down the lines, each at its own rate,
    /// after René's Cartesian channel.
    Cartesian(Cartesian),
}

impl Order {
    /// Every order, as lists show them, the Metropolix's in its manual's order and those that take
    /// numbers with the ones they start from.
    pub const ALL: [Self; 28] = [
        Self::Forward,
        Self::Reverse,
        Self::UpDown,
        Self::UpDownInc,
        Self::DownUp,
        Self::DownUpInc,
        Self::OddEven,
        Self::Jump(JUMP_START),
        Self::Hopscotch,
        Self::Arp(Arp::START),
        Self::Converge,
        Self::Diverge,
        Self::Growing,
        Self::Shrinking,
        Self::Pedal,
        Self::Brownian,
        Self::Shuffle,
        Self::QuasiRandom,
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

    /// What its kind is called: its whole name but for a snake's word and the numbers of a jump,
    /// an arp or Cartesian, which [`fmt::Display`] adds, as the phrase view's header and lists
    /// show it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Forward => "Forward",
            Self::Reverse => "Reverse",
            Self::UpDown => "Up-down",
            Self::UpDownInc => "Up-down-inc",
            Self::DownUp => "Down-up",
            Self::DownUpInc => "Down-up-inc",
            Self::OddEven => "Odd-even",
            Self::Jump(_) => "Jump",
            Self::Hopscotch => "Hopscotch",
            Self::Arp(_) => "Arp",
            Self::Converge => "Converge",
            Self::Diverge => "Diverge",
            Self::Growing => "Growing",
            Self::Shrinking => "Shrinking",
            Self::Pedal => "Pedal",
            Self::Brownian => "Brownian",
            Self::Shuffle => "Shuffle",
            Self::QuasiRandom => "Quasi-random",
            Self::Random => "Random",
            Self::Snake(_) => "Snake",
            Self::Cartesian(_) => "Cartesian",
        }
    }

    /// The order words such as `up-down`, `jump 5`, `arp 4 2`, `snake spiral`, `snake 6` or
    /// `cartesian 1 3 4 2` give, ignoring case: what `:order` takes, which is its name in lower
    /// case.
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
        if first.eq_ignore_ascii_case("arp") {
            return Arp::named(&rest).map(Self::Arp);
        }
        if first.eq_ignore_ascii_case("jump") {
            return match rest[..] {
                [] => Some(Self::Jump(JUMP_START)),
                [by] => by
                    .parse()
                    .ok()
                    .filter(|by| (2..=MOST_ROWS).contains(by))
                    .map(Self::Jump),
                _ => None,
            };
        }
        if !rest.is_empty() {
            return None;
        }
        Self::ALL
            .into_iter()
            .find(|order| !order.takes_words() && order.name().eq_ignore_ascii_case(first))
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
            Self::Jump(_) => "So many rows on each time, going round: jump 3 is 1 4 3 2 1 4 3 2",
            Self::Hopscotch => "Each row, then the one two on: 1 3 2 4 3 1 4 2",
            Self::Arp(_) => "Runs of rows, each a few rows on: arp 3 1 is 1 2 3 2 3 4 3 4",
            Self::Converge => "From both ends in to the middle: 1 4 2 3 1 4 2 3",
            Self::Diverge => "From the middle out to both ends: 2 3 1 4 2 3 1 4",
            Self::Growing => "Runs from the first row, one longer each time: 1 1 2 1 2 3 1 2",
            Self::Shrinking => "Runs from the first row, one shorter each time: 1 2 3 4 1 2 3 1",
            Self::Pedal => "The first row between the others: 1 2 1 3 1 4 1 2",
            Self::Brownian => "From the first row each pass, a row on, back or none at random",
            Self::Shuffle => "Every row once a pass, in an order dealt again each pass",
            Self::QuasiRandom => "Any row each time but the one just played",
            Self::Random => "Any row each time, so rows can repeat or be left out",
            Self::Snake(snake) => snake.meaning(),
            Self::Cartesian(_) => {
                "X along a line, Y down the lines, each every so many rows: 1 2 3 8 5 6 11 12 …"
            }
        }
    }

    /// What it plays, in words, for a phrase of this many rows: its meaning, for Cartesian what
    /// its counters do on the grid the rows make, and for a jump or an arp the rows of its round.
    pub fn describe(self, rows: usize) -> String {
        let round = || {
            let places = self.places(rows);
            let shown: Vec<String> = (0..places.min(ROUND_SHOWN))
                .filter_map(|place| self.row(rows, place, Deal::default()))
                .map(|row| (row + 1).to_string())
                .collect();
            let more = if places > ROUND_SHOWN { " …" } else { "" };
            format!("{}{more}", shown.join(" "))
        };
        match self {
            Self::Cartesian(cartesian) => cartesian.describe(Grid::of(rows)),
            Self::Jump(by) => format!("{by} rows on each time, going round: {}", round()),
            Self::Arp(arp) => format!(
                "Runs of {} rows, each {} on from the last: {}",
                arp.notes,
                arp.step,
                round()
            ),
            _ => self.meaning().to_owned(),
        }
    }

    /// Whether `:order` takes words or numbers after its name: a jump's, an arp's, a snake's or
    /// Cartesian's.
    pub(crate) const fn takes_words(self) -> bool {
        matches!(
            self,
            Self::Jump(_) | Self::Arp(_) | Self::Snake(_) | Self::Cartesian(_)
        )
    }

    /// Whether it takes numbers, which its list shows as the phrase has them.
    pub const fn takes_numbers(self) -> bool {
        matches!(self, Self::Jump(_) | Self::Arp(_) | Self::Cartesian(_))
    }

    /// Its place in [`Order::ALL`]: its kind's, or a snake's own.
    fn listed(self) -> usize {
        Self::ALL
            .iter()
            .position(|listed| match self {
                Self::Snake(_) => *listed == self,
                _ => listed.is_like(self),
            })
            .unwrap_or(0)
    }

    /// The same order with `x` and `y` in place of its numbers where they aren't `00`, kept within
    /// what each takes: a jump's rows, an arp's notes and step, the snake by number, and
    /// Cartesian's X and Y rates. The others take none.
    pub fn numbered(self, x: u8, y: u8) -> Self {
        let or = |own: u8, set: u8, low: u8, high: u8| match set {
            0 => own,
            set => set.clamp(low, high),
        };
        match self {
            Self::Jump(by) => Self::Jump(or(by, x, 2, MOST_ROWS)),
            Self::Arp(arp) => Self::Arp(Arp {
                notes: or(arp.notes, x, 2, MOST_ROWS),
                step: or(arp.step, y, 1, MOST_ROWS),
            }),
            Self::Snake(_) => match x {
                0 => self,
                x => Self::Snake(Snake::ALL[usize::from(x.min(Snake::ALL.len() as u8)) - 1]),
            },
            Self::Cartesian(cartesian) => Self::Cartesian(Cartesian {
                x_rate: or(cartesian.x_rate, x, 1, MAX_RATE),
                y_rate: or(cartesian.y_rate, y, 1, MAX_RATE),
                ..cartesian
            }),
            _ => self,
        }
    }

    /// Whether it's of the same kind as another, whatever their snakes or numbers.
    pub fn is_like(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }

    /// Whether it's dealt from the project's seed, so which row plays when depends on the pass.
    /// A dealt order's round is one pass.
    pub const fn is_dealt(self) -> bool {
        matches!(
            self,
            Self::Brownian | Self::Shuffle | Self::QuasiRandom | Self::Random
        )
    }

    /// Whether it picks a row at a time until the pass is full, rather than going round a round of
    /// places: `Random`, `Quasi-random` and `Brownian`.
    pub(crate) const fn picks(self) -> bool {
        matches!(self, Self::Brownian | Self::QuasiRandom | Self::Random)
    }

    /// Whether it's the order every phrase plays in unless it's given another, which files leave
    /// out.
    pub fn is_forward(&self) -> bool {
        *self == Self::Forward
    }

    /// How many places a round of this many rows has before it repeats, for every order but those
    /// that pick until the pass is full. Each is a visit to a row, but for a grid order's places on
    /// cells past the last row.
    pub(crate) fn places(self, rows: usize) -> usize {
        match self {
            Self::UpDown | Self::DownUp | Self::Pedal if rows > 1 => 2 * rows - 2,
            Self::UpDownInc | Self::DownUpInc | Self::Hopscotch => 2 * rows,
            Self::Jump(by) if rows > 1 => metropolix::jump_places(rows, by),
            Self::Arp(arp) if rows > 1 => arp.places(rows),
            Self::Growing | Self::Shrinking => metropolix::runs_places(rows),
            Self::Snake(_) if rows > 1 => Grid::of(rows).cells(),
            Self::Cartesian(cartesian) if rows > 1 => cartesian.places(Grid::of(rows)),
            _ => rows,
        }
    }

    /// The row a place of a round plays, for a phrase of this many rows, with the place below
    /// [`Order::places`], or none for a grid order's cell past the last row. The orders that pick
    /// pick among every row here, `Quasi-random` and `Brownian` walking from the pass's first
    /// place to this one; a round leaves out the rows that take no time.
    pub(crate) fn row(self, rows: usize, place: usize, deal: Deal) -> Option<usize> {
        if rows <= 1 {
            return Some(0);
        }
        let visit = place;
        let last = rows - 1;
        let row = match self {
            Self::Forward => visit,
            Self::Reverse => last - visit,
            Self::UpDown | Self::UpDownInc if visit < rows => visit,
            Self::UpDown => 2 * last - visit,
            Self::UpDownInc => 2 * rows - 1 - visit,
            Self::DownUp | Self::DownUpInc if visit < rows => last - visit,
            Self::DownUp => visit - last,
            Self::DownUpInc => visit - rows,
            Self::OddEven => {
                let evens = rows.div_ceil(2);
                if visit < evens {
                    2 * visit
                } else {
                    2 * (visit - evens) + 1
                }
            }
            Self::Jump(by) => metropolix::jump_row(rows, by, visit),
            Self::Hopscotch => metropolix::hopscotch_row(rows, visit),
            Self::Arp(arp) => arp.row(rows, visit),
            Self::Growing => metropolix::growing_row(visit),
            Self::Shrinking => metropolix::shrinking_row(rows, visit),
            Self::Converge if visit.is_multiple_of(2) => visit / 2,
            Self::Converge => last - visit / 2,
            Self::Diverge if visit.is_multiple_of(2) => last / 2 - visit / 2,
            Self::Diverge => last / 2 + 1 + visit / 2,
            Self::Pedal if visit.is_multiple_of(2) => 0,
            Self::Pedal => visit.div_ceil(2),
            Self::Shuffle => shuffled(deal, rows, visit),
            Self::Random => picked(deal, rows, visit, 0),
            Self::QuasiRandom | Self::Brownian => (0..=visit)
                .fold(None, |last, visit| {
                    Some(self.pick(deal, rows, visit, last, 0))
                })
                .unwrap_or(0),
            Self::Snake(snake) => snake.cell(Grid::of(rows), place),
            Self::Cartesian(cartesian) => cartesian.cell(Grid::of(rows), place),
        };
        (row < rows).then_some(row)
    }

    /// The row an order that picks plays at a visit, after `last`, the row it played before, on a
    /// try at it: any row for `Random`; any but the last for `Quasi-random`, with more than two
    /// rows, as the Metropolix's; and for `Brownian` the first row, then a step from the last.
    fn pick(
        self,
        deal: Deal,
        rows: usize,
        visit: usize,
        last: Option<usize>,
        attempt: u64,
    ) -> usize {
        match (self, last) {
            (Self::QuasiRandom, Some(last)) if rows > 2 => {
                let row = below(roll(deal, visit, attempt), rows - 1);
                if row >= last { row + 1 } else { row }
            }
            (Self::Brownian, None) => 0,
            (Self::Brownian, Some(last)) => {
                (last as isize + stagger(deal, visit)).rem_euclid(rows as isize) as usize
            }
            _ => picked(deal, rows, visit, attempt),
        }
    }
}

/// Where `PLEN` is at rest: the loop's own rows.
pub const WINDOW_AS_WRITTEN: u8 = 0x80;

/// Where `ORD` is at rest: each phrase's own order.
pub const ORDER_AS_WRITTEN: u8 = 0x80;

/// The most `ORDX` and `ORDY` choose, past `00` for the phrase's own.
pub const MOST_ORDER_NUMBER: u8 = 16;

/// How a track moves the walks of its phrases from their own, as its settings have it as a row is
/// about to play (decision 400): `order` goes so many places on through [`Order::ALL`] from the
/// phrase's own `:order`, `80` for none, as the Metropolix's order CV does; `reverse` turns its
/// rows the other way up, as its Direction does; and `x` and `y` are numbers in place of the
/// order's own, `00` for its own. `length` and `start` are the window of rows the order walks
/// (decision 401): `length` so many rows more or fewer than the loop's, `80` for none, its rows
/// past the loop's end its own again from the first, and `start` so many rows on from the loop's
/// first, going round. At rest every phrase walks as it's written.
/// `chain` is how its chain's turns are walked (decisions 423 and 424).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkMove {
    pub order: u8,
    pub reverse: bool,
    pub x: u8,
    pub y: u8,
    pub length: u8,
    pub start: u8,
    pub chain: ChainMove,
}

impl Default for WalkMove {
    fn default() -> Self {
        Self {
            order: ORDER_AS_WRITTEN,
            reverse: false,
            x: 0,
            y: 0,
            length: WINDOW_AS_WRITTEN,
            start: 0,
            chain: ChainMove::default(),
        }
    }
}

/// The names of the orders a chain's `Order` (`CORD`) chooses from, [`Order::ALL`] as they show,
/// `Forward` first, at `00` (decision 423).
pub const CHAIN_ORDER_NAMES: [&str; Order::ALL.len()] = [
    "Forward",
    "Reverse",
    "Up-down",
    "Up-down-inc",
    "Down-up",
    "Down-up-inc",
    "Odd-even",
    "Jump 3",
    "Hopscotch",
    "Arp 3 1",
    "Converge",
    "Diverge",
    "Growing",
    "Shrinking",
    "Pedal",
    "Brownian",
    "Shuffle",
    "Quasi-random",
    "Random",
    "Snake zigzag",
    "Snake columns",
    "Snake column-zigzag",
    "Snake diagonal",
    "Snake diagonal-zigzag",
    "Snake spiral",
    "Snake spiral-out",
    "Snake checker",
    "Cartesian 1 3",
];

/// How a track walks its chain's phrases, as its settings have it as a turn of the chain starts:
/// `order` a place in [`Order::ALL`], `00` forward as written, its turns walked as an order walks
/// a phrase's rows (decision 423), and `jump` the chance, `00` to `FF`, that a turn plays any of
/// the chain's phrases instead (decision 424). A turn keeps its written length whatever plays in
/// it, so the chain keeps its time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChainMove {
    pub order: u8,
    pub jump: u8,
}

impl ChainMove {
    /// Whether it plays every chain as it's written.
    pub fn is_own(self) -> bool {
        self == Self::default()
    }

    /// The order its turns are walked in.
    pub fn order(self) -> Order {
        Order::ALL[usize::from(self.order).min(Order::ALL.len() - 1)]
    }

    /// Which of a chain's `turns` plays at its turn `turn`, both counted among the turns that
    /// take time, this many times round the chain since it started: where the order walks, which
    /// carries on from one time round to the next as a phrase's walk does, so an order whose
    /// round is longer drifts across them, and a dealt one is dealt again each time round; then,
    /// at `jump`'s chance, any of them, rolled from the same deal, so a render, or playing from
    /// anywhere, plays the same chain.
    pub fn heard(self, dealer: Dealer, lap: usize, turns: usize, turn: usize) -> usize {
        if turns <= 1 {
            return 0;
        }
        let deal = dealer.chain_deal(lap);
        let order = self.order();
        let walked = if order.is_dealt() {
            order.row(turns, turn, deal)
        } else {
            let visit = lap.wrapping_mul(turns).wrapping_add(turn);
            let places = order.places(turns).max(1);
            if matches!(order, Order::Snake(_) | Order::Cartesian(_)) {
                // A grid's cells past the last turn are passed over, so its round is its visits.
                let rows = || (0..places).filter_map(|place| order.row(turns, place, deal));
                let visits = rows().count().max(1);
                rows().nth(visit % visits)
            } else {
                order.row(turns, visit % places, deal)
            }
        };
        let walked = walked.unwrap_or(turn).min(turns - 1);
        let chance = f32::from(self.jump) / 255.0;
        jump_roll(deal, turn, chance, turns).unwrap_or(walked)
    }
}

impl WalkMove {
    /// Whether it leaves every phrase walking as it's written.
    pub fn is_own(self) -> bool {
        self == Self::default()
    }

    /// How many rows the order walks over in a loop of `rows`: its own moved by `length`, from 1
    /// to [`MAX_PHRASE_STEPS`].
    pub fn heard_length(self, rows: usize) -> usize {
        let by = i64::from(self.length) - i64::from(WINDOW_AS_WRITTEN);
        (rows.max(1) as i64 + by).clamp(1, MAX_PHRASE_STEPS as i64) as usize
    }

    /// The loop's row the window starts on, in a loop of `rows`.
    pub fn heard_start(self, rows: usize) -> usize {
        usize::from(self.start) % rows.max(1)
    }

    /// Whether it moves the window of rows from the loop's own.
    pub fn moves_window(self) -> bool {
        self.length != WINDOW_AS_WRITTEN || self.start != 0
    }

    /// The rows of a loop the order walks over, as this moves them.
    pub fn rows(self, steps: &[Step]) -> Rows<'_> {
        Rows {
            steps,
            start: self.heard_start(steps.len()),
            length: self.heard_length(steps.len()),
            reverse: self.reverse,
        }
    }

    /// The order a phrase whose own is `own` plays in: so many places on through the list, kept
    /// at either end of it, with its numbers. Moved onto another kind, it has that kind's numbers
    /// as the list gives them, unless `x` or `y` gives its own.
    pub fn order_of(self, own: Order) -> Order {
        let by = i32::from(self.order) - i32::from(ORDER_AS_WRITTEN);
        let moved = match by {
            0 => own,
            by => {
                let last = Order::ALL.len() as i32 - 1;
                Order::ALL[(own.listed() as i32 + by).clamp(0, last) as usize]
            }
        };
        moved.numbered(self.x, self.y)
    }
}

/// The rows a walk goes over: a window of a phrase's loop, `length` rows from `start`, going round
/// it, so a window longer than the loop plays its rows again from the first, as a lane heard
/// longer does (decision 401); and the other way up for a track's `Reverse`, so its first place is
/// the window's last row. An order walks its places, and each place is a row of the loop.
#[derive(Debug, Clone, Copy)]
pub struct Rows<'a> {
    steps: &'a [Step],
    start: usize,
    length: usize,
    reverse: bool,
}

impl<'a> Rows<'a> {
    /// A loop's rows, the other way up if `reverse`.
    pub fn new(steps: &'a [Step], reverse: bool) -> Self {
        Self {
            steps,
            start: 0,
            length: steps.len(),
            reverse,
        }
    }

    /// How many there are.
    pub fn len(self) -> usize {
        if self.steps.is_empty() {
            0
        } else {
            self.length
        }
    }

    /// Whether there are none.
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// The row of the loop at a place among them.
    pub fn row(self, at: usize) -> usize {
        let at = if self.reverse {
            self.length - 1 - at
        } else {
            at
        };
        (self.start + at) % self.steps.len()
    }

    /// The step at a place among them.
    fn step(self, at: usize) -> &'a Step {
        &self.steps[self.row(at)]
    }

    /// How many steps a pass of the loop lasts: every row's pulses.
    fn time(self) -> usize {
        self.steps.iter().map(Step::pulses).sum()
    }
}

/// How many of a round's rows a jump's or an arp's description shows.
const ROUND_SHOWN: usize = 16;

/// The words `:order` starts with, in the order lists show them: each order's, `jump` and `arp`
/// with numbers after them if wanted, then `snake` and `cartesian`, which take a snake or numbers
/// after them.
pub const ORDER_WORDS: [&str; 21] = [
    "forward",
    "reverse",
    "up-down",
    "up-down-inc",
    "down-up",
    "down-up-inc",
    "odd-even",
    "jump",
    "hopscotch",
    "arp",
    "converge",
    "diverge",
    "growing",
    "shrinking",
    "pedal",
    "brownian",
    "shuffle",
    "quasi-random",
    "random",
    "snake",
    "cartesian",
];

impl fmt::Display for Order {
    /// Its name, with a snake's word or the numbers of a jump, an arp or Cartesian as `:order`
    /// takes them: `Jump 3`, `Arp 4 2`, `Snake spiral`, `Cartesian 1 3`, or `Cartesian 1 3 4 2`
    /// with its lengths.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())?;
        match self {
            Self::Jump(by) => write!(f, " {by}"),
            Self::Arp(arp) => write!(f, " {} {}", arp.notes, arp.step),
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
        Self(splitmix64(
            splitmix64(seed) ^ splitmix64(ORDER_STREAM.wrapping_add(track as u64)),
        ))
    }

    /// The deal for one time round a track's chain, counted from where it started, and one place
    /// in it, so a phrase twice in its chain is dealt twice.
    pub(crate) fn deal(self, lap: i64, slot: usize) -> Deal {
        Deal(splitmix64(splitmix64(self.0 ^ lap as u64) ^ slot as u64))
    }

    /// The deal for one time round a track's chain of the chain's own order and jump, apart from
    /// its phrases' (decisions 423 and 424).
    pub(crate) fn chain_deal(self, lap: usize) -> Deal {
        Deal(splitmix64(self.deal(lap as i64, 0).0 ^ CHAIN_STREAM))
    }
}

/// One pass's deal of a dealt order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Deal(u64);

impl Deal {
    /// The deal for a phrase played again within one turn of its chain, a turn longer than it,
    /// `again` times since the turn started: this one for the first.
    pub(crate) fn again(self, again: usize) -> Self {
        match again {
            0 => self,
            again => Self(splitmix64(self.0 ^ again as u64)),
        }
    }
}

/// Keeps a dealer's hashes apart from any other the seed is hashed for.
const ORDER_STREAM: u64 = 0x6f72_6465_7200_0000;

/// Keeps a chain's own deals apart from its phrases'.
const CHAIN_STREAM: u64 = 0x6368_6169_6e00_0000;

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
            let turned = left ^ (splitmix64(deal.0 ^ (round << 56) ^ right) & mask);
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
    below(roll(deal, visit, attempt), rows)
}

/// Keeps `Jump`'s rolls apart from an order's own.
const JUMP_STREAM: u64 = 0x6a75_6d70_0000_0000;

/// The row a `Jump` plays at a visit in place of the one the order walks to, with its chance
/// from 0 to 1, or none to play the walk's: any of the rows that take time, picked from the
/// pass's deal, so a render, or playing from anywhere, jumps the same (decision 391). The
/// rows' times are left to the walk, as the row jumped to plays for as long as the visit lasts.
pub fn jumped(steps: &[Step], deal: Deal, visit: usize, chance: f32) -> Option<usize> {
    let timed = steps.iter().filter(|step| step.pulses() > 0).count();
    let nth = jump_roll(deal, visit, chance, timed)?;
    steps
        .iter()
        .enumerate()
        .filter(|(_, step)| step.pulses() > 0)
        .nth(nth)
        .map(|(row, _)| row)
}

/// Whether a visit jumps, with its chance from 0 to 1, and to which of `rows` if it does.
fn jump_roll(deal: Deal, visit: usize, chance: f32, rows: usize) -> Option<usize> {
    if chance <= 0.0 {
        return None;
    }
    let roll = splitmix64(deal.0 ^ splitmix64(JUMP_STREAM ^ visit as u64));
    if (roll >> 40) as f32 / (1u64 << 24) as f32 >= chance {
        return None;
    }
    Some(below(splitmix64(roll), rows))
}

/// Keeps a lane's deals apart from its track's chain's.
const LANE_STREAM: u64 = 0x6c61_6e65_0000_0000;

/// The row of its own a lane's `Jump` plays in place of the one it counts to, with its chance from
/// 0 to 1, or none to play the count's: any of its `rows`, rolled as `Jump` rolls a phrase's, from
/// the track's dealer, the lane's column, how many times it has come round and its row in that
/// lap, so a render, or playing from anywhere, jumps the same (decision 415).
pub fn lane_jumped(
    dealer: Dealer,
    column: usize,
    lap: i64,
    row: usize,
    rows: usize,
    chance: f32,
) -> Option<usize> {
    let deal = dealer.deal(lap, column);
    jump_roll(Deal(splitmix64(deal.0 ^ LANE_STREAM)), row, chance, rows)
}

/// A pass's roll of the dice for a visit, on a try at it.
fn roll(deal: Deal, visit: usize, attempt: u64) -> u64 {
    splitmix64(deal.0 ^ splitmix64((visit as u64) << 8 | attempt))
}

/// Which way `Brownian` steps at a visit, as the Metropolix's: a row on half the time, staying a
/// quarter and a row back a quarter.
fn stagger(deal: Deal, visit: usize) -> isize {
    match below(roll(deal, visit, 0), 4) {
        0 | 1 => 1,
        2 => 0,
        _ => -1,
    }
}

/// The rows one round of an order visits, in turn: the visits the walk repeats.
#[derive(Debug, Clone)]
pub struct Round<'a> {
    /// The rows it walks, each place a row of the loop.
    rows: Rows<'a>,
    order: Order,
    deal: Deal,
    /// The place the round is at, which for an order that picks is its visit.
    visit: usize,
    /// How many places the round has, for every order but those that pick.
    places: usize,
    /// How many steps an order that picks has still to fill, as it stops when the pass is full.
    left: usize,
    /// The place it played last, which `Quasi-random` doesn't play again and `Brownian` steps
    /// from.
    last: Option<usize>,
}

impl<'a> Round<'a> {
    /// A round of these rows in this order, with this deal if it's dealt.
    pub fn new(steps: &'a [Step], order: Order, deal: Deal) -> Self {
        Self::over(Rows::new(steps, false), order, deal)
    }

    /// A round over rows as a track's walk has them, which an order that picks fills until a
    /// pass of the loop is full.
    pub fn over(rows: Rows<'a>, order: Order, deal: Deal) -> Self {
        Self {
            rows,
            order,
            deal,
            visit: 0,
            places: order.places(rows.len()),
            left: rows.time(),
            last: None,
        }
    }

    /// The row `Random` or `Quasi-random` picks next. A row that takes no time would be no pick at
    /// all, and one that runs past the end of the pass would split its pulses, so either is picked
    /// again, and so is the last row for `Quasi-random`. Only when no row fits what's left is the
    /// last row played again, or the last pick cut short.
    fn pick(&self) -> Option<usize> {
        let (rows, visit, left) = (self.rows.len(), self.visit, self.left);
        let pick = |fits: &dyn Fn(usize) -> bool| {
            (0..PICKS)
                .map(|attempt| self.order.pick(self.deal, rows, visit, self.last, attempt))
                .find(|row| fits(*row))
                .or_else(|| {
                    let from = self.order.pick(self.deal, rows, visit, self.last, 0);
                    (0..rows)
                        .map(|at| (from + at) % rows)
                        .find(|row| fits(*row))
                })
        };
        let pulses = |row: usize| self.rows.step(row).pulses();
        let again =
            |row: usize| self.order == Order::QuasiRandom && rows > 2 && self.last == Some(row);
        pick(&|row| (1..=left).contains(&pulses(row)) && !again(row))
            .or_else(|| pick(&|row| (1..=left).contains(&pulses(row))))
            .or_else(|| pick(&|row| pulses(row) > 0))
    }

    /// The row `Brownian` steps to next: the first row that takes time at the start of a pass, and
    /// then a row on, back or the same, passing over rows that take no time the way it's going. A
    /// row that runs past the end of the pass is cut short, as a walk can't pick again.
    fn stagger(&self) -> Option<usize> {
        let rows = self.rows.len() as isize;
        let (from, way) = match self.last {
            None => (-1, 1),
            Some(last) => match stagger(self.deal, self.visit) {
                0 => return Some(last),
                way => (last as isize, way),
            },
        };
        (1..=rows)
            .map(|step| (from + way * step).rem_euclid(rows) as usize)
            .find(|row| self.rows.step(*row).pulses() > 0)
    }
}

/// How many times `Random` and `Quasi-random` pick again for a row that fits, before taking the
/// next one down that does.
const PICKS: u64 = 16;

impl Iterator for Round<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        let rows = self.rows.len();
        if rows == 0 {
            return None;
        }
        let visit = self.visit;
        let row = if self.order.picks() {
            if self.left == 0 {
                return None;
            }
            let row = if self.order == Order::Brownian {
                self.stagger()
            } else {
                self.pick()
            }?;
            self.left = self.left.saturating_sub(self.rows.step(row).pulses());
            self.last = Some(row);
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
        Some(self.rows.row(row))
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
                if order.picks() {
                    assert_eq!(played.len(), length, "{order} picks as many as there are");
                    continue;
                }
                if matches!(order, Order::Cartesian(_)) {
                    // Its counters can leave cells out, as René's do.
                    continue;
                }
                if order == Order::Jump(JUMP_START) && length % 3 == 0 {
                    // Three on from the first of 3, 6 or 9 rows comes back before the others.
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
    fn the_rest_of_the_metropolix_goes_round_as_its_manual_shows() {
        // Jump by 2 to 5.
        assert_eq!(rows(Order::Jump(2), 8), manual("1 3 5 7"));
        assert_eq!(rows(Order::Jump(3), 8), manual("1 4 7 2 5 8 3 6"));
        assert_eq!(rows(Order::Jump(4), 8), manual("1 5"));
        assert_eq!(rows(Order::Jump(5), 8), manual("1 6 3 8 5 2 7 4"));
        assert_eq!(
            rows(Order::Hopscotch, 8),
            manual("1 3 2 4 3 5 4 6 5 7 6 8 7 1 8 2")
        );
        // Arp 3 Shift -1, Arp 4 Shift -2 and Arp 4 Shift -1.
        let arp = |notes, step| Order::Arp(Arp { notes, step });
        assert_eq!(
            rows(arp(3, 1), 8),
            manual("1 2 3 2 3 4 3 4 5 4 5 6 5 6 7 6 7 8 7 8 1 8 1 2")
        );
        assert_eq!(
            rows(arp(4, 1), 8),
            manual("1 2 3 4 2 3 4 5 3 4 5 6 4 5 6 7 5 6 7 8 6 7 8 1 7 8 1 2 8 1 2 3")
        );
        assert_eq!(
            rows(arp(4, 2), 8),
            manual("1 2 3 4 3 4 5 6 5 6 7 8 7 8 1 2")
        );
        assert_eq!(
            rows(Order::Growing, 8),
            manual("1 1 2 1 2 3 1 2 3 4 1 2 3 4 5 1 2 3 4 5 6 1 2 3 4 5 6 7 1 2 3 4 5 6 7 8")
        );
        assert_eq!(
            rows(Order::Shrinking, 8),
            manual("1 2 3 4 5 6 7 8 1 2 3 4 5 6 7 1 2 3 4 5 6 1 2 3 4 5 1 2 3 4 1 2 3 1 2 1")
        );
    }

    #[test]
    fn a_jump_or_an_arp_describes_its_round_on_the_rows_there_are() {
        assert_eq!(
            Order::Jump(2).describe(8),
            "2 rows on each time, going round: 1 3 5 7"
        );
        assert_eq!(
            Order::Arp(Arp { notes: 4, step: 2 }).describe(8),
            "Runs of 4 rows, each 2 on from the last: 1 2 3 4 3 4 5 6 5 6 7 8 7 8 1 2"
        );
        assert!(Order::Arp(Arp::START).describe(8).ends_with(" …"));
    }

    /// The rows an order that picks plays over passes of eight rows of a step each.
    fn picked_passes(order: Order, seed: u64, passes: i64) -> Vec<Vec<usize>> {
        let steps = vec![Step::default(); 8];
        let dealer = Dealer::new(seed, 0);
        (0..passes)
            .map(|lap| Round::new(&steps, order, dealer.deal(lap, 0)).collect())
            .collect()
    }

    #[test]
    fn brownian_walks_from_the_first_row_a_row_at_a_time() {
        let heard = picked_passes(Order::Brownian, 5, 400);
        let (mut on, mut stays, mut back) = (0, 0, 0);
        for pass in &heard {
            assert_eq!(pass.len(), 8, "a pick every step");
            assert_eq!(pass[0], 0, "each pass starts on the first row");
            for pair in pass.windows(2) {
                match (pair[1] + 8 - pair[0]) % 8 {
                    1 => on += 1,
                    0 => stays += 1,
                    7 => back += 1,
                    _ => panic!("a step of more than a row: {pass:?}"),
                }
            }
        }
        // Half on, a quarter staying and a quarter back, of 2800 steps.
        let total = f64::from(on + stays + back);
        assert!((f64::from(on) / total - 0.5).abs() < 0.03, "{on} on");
        assert!(
            (f64::from(stays) / total - 0.25).abs() < 0.03,
            "{stays} stay"
        );
        assert!((f64::from(back) / total - 0.25).abs() < 0.03, "{back} back");
        assert!(
            heard.iter().any(|pass| pass.contains(&7)),
            "it goes back round the top"
        );
        assert_eq!(
            picked_passes(Order::Brownian, 5, 400),
            heard,
            "a seed repeats"
        );
        assert_ne!(picked_passes(Order::Brownian, 6, 400), heard);
        // Order::row walks the same.
        let deal = Dealer::new(5, 0).deal(3, 0);
        let walked: Vec<usize> = (0..8)
            .map(|place| Order::Brownian.row(8, place, deal).unwrap())
            .collect();
        assert_eq!(walked, heard[3]);
    }

    #[test]
    fn brownian_passes_over_rows_that_take_no_time() {
        let mut steps: Vec<Step> = (0..8)
            .map(|_| "C-3 -- SGL1 --- --- ---".parse().unwrap())
            .collect();
        for skipped in [0, 3, 4] {
            steps[skipped] = "C-3 -- SGL0 --- --- ---".parse().unwrap();
        }
        let dealer = Dealer::new(9, 1);
        for lap in 0..64 {
            let played: Vec<usize> =
                Round::new(&steps, Order::Brownian, dealer.deal(lap, 0)).collect();
            assert_eq!(played[0], 1, "the first row that takes time");
            assert_eq!(played.len(), 5, "five steps' worth of one-pulse rows");
            assert!(
                played.iter().all(|row| ![0, 3, 4].contains(row)),
                "{played:?}"
            );
        }
    }

    #[test]
    fn quasi_random_never_plays_a_row_twice_running() {
        let heard = picked_passes(Order::QuasiRandom, 7, 200);
        let mut seen = [false; 8];
        for pass in &heard {
            assert_eq!(pass.len(), 8);
            assert!(pass.windows(2).all(|pair| pair[0] != pair[1]), "{pass:?}");
            for row in pass {
                seen[*row] = true;
            }
        }
        assert_eq!(seen, [true; 8]);
        assert_eq!(picked_passes(Order::QuasiRandom, 7, 200), heard);
        // With two rows it may, as the Metropolix's does.
        let two = vec![Step::default(); 2];
        let dealer = Dealer::new(7, 0);
        let repeats = (0..32).any(|lap| {
            let pass: Vec<usize> =
                Round::new(&two, Order::QuasiRandom, dealer.deal(lap, 0)).collect();
            pass[0] == pass[1]
        });
        assert!(repeats);
    }

    #[test]
    fn jumps_and_arps_are_named_by_their_numbers() {
        assert_eq!(Order::named("jump"), Some(Order::Jump(3)));
        assert_eq!(Order::named("Jump 5"), Some(Order::Jump(5)));
        assert_eq!(Order::Jump(5).to_string(), "Jump 5");
        assert_eq!(Order::named("arp"), Some(Order::Arp(Arp::START)));
        let arp = Order::named("arp 4 2").unwrap();
        assert_eq!(arp, Order::Arp(Arp { notes: 4, step: 2 }));
        assert_eq!(arp.to_string(), "Arp 4 2");
        assert_eq!(Order::named("quasi-random"), Some(Order::QuasiRandom));
        assert_eq!(Order::named("hopscotch"), Some(Order::Hopscotch));
        for wrong in [
            "jump 1",
            "jump 17",
            "jump 2 3",
            "jump x",
            "arp 4",
            "arp 1 1",
            "arp 4 0",
            "arp 17 1",
            "hopscotch 2",
        ] {
            assert_eq!(Order::named(wrong), None, "{wrong}");
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
    fn a_walk_moves_through_the_list_from_the_phrases_own_and_numbers_it() {
        let by = |order: u8| WalkMove {
            order,
            ..WalkMove::default()
        };
        assert_eq!(WalkMove::default().order_of(Order::Jump(5)), Order::Jump(5));
        assert_eq!(by(0x81).order_of(Order::Forward), Order::Reverse);
        assert_eq!(
            by(0x7F).order_of(Order::Forward),
            Order::Forward,
            "kept at the start"
        );
        assert_eq!(
            by(0xFF).order_of(Order::Forward),
            Order::Cartesian(Cartesian::START),
            "and at the end"
        );
        // Onto another kind with the list's numbers; a snake is its own place.
        assert_eq!(by(0x7F).order_of(Order::Hopscotch), Order::Jump(JUMP_START));
        assert_eq!(by(0x81).order_of(Order::Jump(5)), Order::Hopscotch);
        assert_eq!(
            by(0x81).order_of(Order::Snake(Snake::Zigzag)),
            Order::Snake(Snake::Columns)
        );
        let numbered = |x, y| WalkMove {
            x,
            y,
            ..WalkMove::default()
        };
        assert_eq!(
            numbered(1, 0).order_of(Order::Jump(5)),
            Order::Jump(2),
            "kept to 2"
        );
        assert_eq!(
            numbered(16, 0).order_of(Order::Snake(Snake::Zigzag)),
            Order::Snake(Snake::Checker),
            "kept to the eighth"
        );
        assert_eq!(
            numbered(0, 4).order_of(Order::Arp(Arp::START)),
            Order::Arp(Arp { notes: 3, step: 4 })
        );
        let sized = Cartesian::new(1, 3).sized(4, 2);
        assert_eq!(
            numbered(2, 5).order_of(Order::Cartesian(sized)),
            Order::Cartesian(Cartesian::new(2, 5).sized(4, 2)),
            "its lengths kept"
        );
        assert_eq!(numbered(5, 5).order_of(Order::Pedal), Order::Pedal);
    }

    #[test]
    fn reversed_rows_are_walked_the_other_way_up() {
        let steps = vec![Step::default(); 4];
        let walked = |order| {
            Round::over(Rows::new(&steps, true), order, Deal::default())
                .map(|row| row + 1)
                .collect::<Vec<_>>()
        };
        assert_eq!(walked(Order::Forward), manual("4 3 2 1"));
        assert_eq!(walked(Order::UpDown), rows(Order::DownUp, 4));
        assert_eq!(walked(Order::UpDownInc), rows(Order::DownUpInc, 4));
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
