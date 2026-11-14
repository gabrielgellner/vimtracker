//! The orders after Make Noise's René, which reads a phrase's rows as a grid: snakes, paths that
//! wind through the grid's cells, and the Cartesian order, where one counter moves along a line
//! and another down the lines, each at its own rate.
//!
//! Each works out the cell at any place of its round on its own, without what came before, so
//! the walk can be found from where the track is, as every fixed order's can.

use serde::{Deserialize, Serialize};

/// A phrase's rows read as a grid: lines of `width` rows, one under another, the last cut short
/// if the rows don't fill it. Four wide up to 32 rows, so a phrase of 16 is René's 4×4 and a line
/// is a beat of sixteenths, and eight wide above, so 64 rows are 8×8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
}

impl Grid {
    /// The grid this many rows are read as.
    pub const fn of(rows: usize) -> Self {
        let width = if rows > 32 { 8 } else { 4 };
        let height = rows.div_ceil(width);
        Self {
            width,
            height: if height == 0 { 1 } else { height },
        }
    }

    /// How many cells it has, rows past the last included.
    pub const fn cells(self) -> usize {
        self.width * self.height
    }

    const fn at(self, x: usize, y: usize) -> usize {
        y * self.width + x
    }
}

/// A path through every cell of a grid once, after René's snake patterns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Snake {
    /// Along each line, turning back on the next.
    #[default]
    Zigzag,
    /// Down each column in turn.
    Columns,
    /// Down the first column, up the next.
    ColumnZigzag,
    /// Along the diagonals from the top left, each from its top right.
    Diagonal,
    /// Along the diagonals from the top left, turning on each, as JPEG reads a block.
    DiagonalZigzag,
    /// Round the edge and in, clockwise from the top left.
    Spiral,
    /// The spiral backwards, from the middle out.
    SpiralOut,
    /// The cells of one colour of a chessboard, then the other.
    Checker,
}

impl Snake {
    /// Every snake, numbered from 1 in this order.
    pub const ALL: [Self; 8] = [
        Self::Zigzag,
        Self::Columns,
        Self::ColumnZigzag,
        Self::Diagonal,
        Self::DiagonalZigzag,
        Self::Spiral,
        Self::SpiralOut,
        Self::Checker,
    ];

    /// Its word, as `:order snake` takes it and the phrase's title shows it.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Zigzag => "zigzag",
            Self::Columns => "columns",
            Self::ColumnZigzag => "column-zigzag",
            Self::Diagonal => "diagonal",
            Self::DiagonalZigzag => "diagonal-zigzag",
            Self::Spiral => "spiral",
            Self::SpiralOut => "spiral-out",
            Self::Checker => "checker",
        }
    }

    /// Its number, from 1, which `:order snake` takes as well as its word.
    pub fn number(self) -> usize {
        Self::ALL
            .iter()
            .position(|snake| *snake == self)
            .unwrap_or(0)
            + 1
    }

    /// The snake a word or a number from 1 names, ignoring case.
    pub fn named(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Ok(number) = text.parse::<usize>() {
            return number
                .checked_sub(1)
                .and_then(|at| Self::ALL.get(at).copied());
        }
        Self::ALL
            .into_iter()
            .find(|snake| snake.word().eq_ignore_ascii_case(text))
    }

    /// What it plays in a phrase of 16 rows, the 4×4 grid, counted from 1.
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::Zigzag => "Along a line, back along the next: 1 2 3 4 8 7 6 5 9 10 11 12 16 …",
            Self::Columns => "Down each column of the grid: 1 5 9 13 2 6 10 14 3 7 11 15 4 …",
            Self::ColumnZigzag => "Down a column, up the next: 1 5 9 13 14 10 6 2 3 7 11 15 16 …",
            Self::Diagonal => "Along the diagonals: 1 2 5 3 6 9 4 7 10 13 8 11 14 12 15 16",
            Self::DiagonalZigzag => "The diagonals, turning: 1 2 5 9 6 3 4 7 10 13 14 11 8 12 …",
            Self::Spiral => "Round the edge and in: 1 2 3 4 8 12 16 15 14 13 9 5 6 7 11 10",
            Self::SpiralOut => "From the middle out: 10 11 7 6 5 9 13 14 15 16 12 8 4 3 2 1",
            Self::Checker => "A chessboard's black, then white: 1 3 6 8 9 11 14 16 2 4 5 7 …",
        }
    }

    /// The cell at a place along the path, with the place below the grid's cells.
    pub fn cell(self, grid: Grid, place: usize) -> usize {
        let Grid { width, height } = grid;
        match self {
            Self::Zigzag => {
                let (y, along) = (place / width, place % width);
                let x = if y % 2 == 0 { along } else { width - 1 - along };
                grid.at(x, y)
            }
            Self::Columns => grid.at(place / height, place % height),
            Self::ColumnZigzag => {
                let (x, along) = (place / height, place % height);
                let y = if x % 2 == 0 {
                    along
                } else {
                    height - 1 - along
                };
                grid.at(x, y)
            }
            Self::Diagonal => {
                let (diagonal, along, _) = diagonal(grid, place);
                let x = diagonal.min(width - 1) - along;
                grid.at(x, diagonal - x)
            }
            Self::DiagonalZigzag => {
                let (diagonal, along, length) = diagonal(grid, place);
                let along = if diagonal % 2 == 0 {
                    length - 1 - along
                } else {
                    along
                };
                let x = diagonal.min(width - 1) - along;
                grid.at(x, diagonal - x)
            }
            Self::Spiral => spiral(grid, place),
            Self::SpiralOut => spiral(grid, grid.cells() - 1 - place),
            Self::Checker => {
                // The grid is always an even number of cells wide, so each line has as many of
                // one colour as of the other.
                let half = width / 2;
                let first = height * half;
                let (place, colour) = if place < first {
                    (place, 0)
                } else {
                    (place - first, 1)
                };
                let y = place / half;
                grid.at(2 * (place % half) + (y + colour) % 2, y)
            }
        }
    }
}

/// Which diagonal from the top left a place along the diagonals is on, how far along it from its
/// top right, and how many cells it has. The diagonals grow by one up to the grid's narrower side,
/// stay that long, and shrink again, so the place is found from those three runs.
fn diagonal(grid: Grid, place: usize) -> (usize, usize, usize) {
    let Grid { width, height } = grid;
    let (short, long) = (width.min(height), width.max(height));
    let growing = short * (short - 1) / 2;
    let steady = growing + (long - short + 1) * short;
    if place < growing {
        let mut diagonal = 0;
        while (diagonal + 1) * (diagonal + 2) / 2 <= place {
            diagonal += 1;
        }
        return (
            diagonal,
            place - diagonal * (diagonal + 1) / 2,
            diagonal + 1,
        );
    }
    if place < steady {
        let into = place - growing;
        return (short - 1 + into / short, into % short, short);
    }
    // The shrinking run is the growing one turned round the grid's middle.
    let (turned, along, length) = diagonal(grid, grid.cells() - 1 - place);
    let x = width - 1 - (turned.min(width - 1) - along);
    let diagonal = width + height - 2 - turned;
    (diagonal, diagonal.min(width - 1) - x, length)
}

/// The cell at a place along a spiral in from the top left, clockwise, a ring at a time.
fn spiral(grid: Grid, mut place: usize) -> usize {
    let (mut left, mut top) = (0, 0);
    let (mut right, mut bottom) = (grid.width - 1, grid.height - 1);
    loop {
        let (wide, high) = (right - left + 1, bottom - top + 1);
        if high == 1 {
            return grid.at(left + place, top);
        }
        if wide == 1 {
            return grid.at(left, top + place);
        }
        let ring = 2 * (wide + high) - 4;
        if place < ring {
            if place < wide {
                return grid.at(left + place, top);
            }
            place -= wide;
            if place < high - 1 {
                return grid.at(right, top + 1 + place);
            }
            place -= high - 1;
            if place < wide - 1 {
                return grid.at(right - 1 - place, bottom);
            }
            place -= wide - 1;
            return grid.at(left, bottom - 1 - place);
        }
        place -= ring;
        (left, top, right, bottom) = (left + 1, top + 1, right - 1, bottom - 1);
    }
}

/// The most rows a Cartesian counter waits before it moves.
pub const MAX_RATE: u8 = 16;

/// The most cells a Cartesian counter counts along its line or down its column.
pub const MAX_LENGTH: u8 = 8;

/// René's Cartesian channel: X moves along a line of the grid every `x_rate` rows and Y down the
/// lines every `y_rate`, each wrapping after its length, and the row played is Y × the grid's
/// width + X. A length of 0 is the grid's, up to 8 lines down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cartesian {
    pub x_rate: u8,
    pub y_rate: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub x_length: u8,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub y_length: u8,
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

impl Default for Cartesian {
    fn default() -> Self {
        Self::START
    }
}

impl Cartesian {
    /// What `:order cartesian` alone and the list give: X every row and Y every three, over the
    /// whole grid, so the two fall out of step.
    pub const START: Self = Self {
        x_rate: 1,
        y_rate: 3,
        x_length: 0,
        y_length: 0,
    };

    /// The one numbers after `:order cartesian` give: none, its rates, or its rates and lengths.
    pub fn named(words: &[&str]) -> Option<Self> {
        let numbers: Option<Vec<u8>> = words.iter().map(|word| word.parse().ok()).collect();
        let rate = |rate: u8| (1..=MAX_RATE).contains(&rate);
        let length = |length: u8| (1..=MAX_LENGTH).contains(&length);
        match numbers?[..] {
            [] => Some(Self::START),
            [x, y] if rate(x) && rate(y) => Some(Self::new(x, y)),
            [x, y, wide, high] if rate(x) && rate(y) && length(wide) && length(high) => {
                Some(Self::new(x, y).sized(wide, high))
            }
            _ => None,
        }
    }

    /// One moving X every `x_rate` rows and Y every `y_rate`, each within 1 to 16, over the grid.
    pub fn new(x_rate: u8, y_rate: u8) -> Self {
        Self {
            x_rate: x_rate.clamp(1, MAX_RATE),
            y_rate: y_rate.clamp(1, MAX_RATE),
            x_length: 0,
            y_length: 0,
        }
    }

    /// The same, with X counting along `x_length` cells and Y down `y_length` lines, 1 to 8.
    pub fn sized(self, x_length: u8, y_length: u8) -> Self {
        Self {
            x_length: x_length.clamp(1, MAX_LENGTH),
            y_length: y_length.clamp(1, MAX_LENGTH),
            ..self
        }
    }

    /// Whether its lengths are the grid's, as given by none.
    pub fn is_whole(self) -> bool {
        self.x_length == 0 && self.y_length == 0
    }

    /// How many rows each counter waits before it moves, 1 at least.
    fn rates(self) -> (usize, usize) {
        (
            usize::from(self.x_rate.clamp(1, MAX_RATE)),
            usize::from(self.y_rate.clamp(1, MAX_RATE)),
        )
    }

    /// How far each counter counts on a grid: its length, within the grid, or the grid's own.
    pub fn lengths(self, grid: Grid) -> (usize, usize) {
        let length = |set: u8, whole: usize| {
            let most = whole.min(usize::from(MAX_LENGTH));
            if set == 0 {
                most
            } else {
                usize::from(set).min(most)
            }
        };
        (
            length(self.x_length, grid.width),
            length(self.y_length, grid.height),
        )
    }

    /// How many places a round has before both counters are back where they started.
    pub fn places(self, grid: Grid) -> usize {
        let (x_rate, y_rate) = self.rates();
        let (x_length, y_length) = self.lengths(grid);
        lcm(x_rate * x_length, y_rate * y_length)
    }

    /// The cell at a place of its round.
    pub fn cell(self, grid: Grid, place: usize) -> usize {
        let (x_rate, y_rate) = self.rates();
        let (x_length, y_length) = self.lengths(grid);
        grid.at(place / x_rate % x_length, place / y_rate % y_length)
    }

    /// What it does on a grid, in words.
    pub fn describe(self, grid: Grid) -> String {
        let (x_rate, y_rate) = self.rates();
        let (x_length, y_length) = self.lengths(grid);
        let every = |rate: usize| match rate {
            1 => "every row".to_owned(),
            _ => format!("every {rate} rows"),
        };
        format!(
            "X moves {} over {x_length}, Y {} over {y_length}, in a grid {} wide",
            every(x_rate),
            every(y_rate),
            grid.width
        )
    }
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn lcm(a: usize, b: usize) -> usize {
    a / gcd(a, b) * b
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every cell of a path, counted from 1, as René's grid numbers its locations.
    fn path(snake: Snake, width: usize, height: usize) -> Vec<usize> {
        let grid = Grid { width, height };
        (0..grid.cells())
            .map(|place| snake.cell(grid, place) + 1)
            .collect()
    }

    fn numbers(text: &str) -> Vec<usize> {
        text.split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect()
    }

    #[test]
    fn each_snake_winds_through_the_four_by_four_grid_as_worked_by_hand() {
        let cases = [
            (Snake::Zigzag, "1 2 3 4 8 7 6 5 9 10 11 12 16 15 14 13"),
            (Snake::Columns, "1 5 9 13 2 6 10 14 3 7 11 15 4 8 12 16"),
            (
                Snake::ColumnZigzag,
                "1 5 9 13 14 10 6 2 3 7 11 15 16 12 8 4",
            ),
            (Snake::Diagonal, "1 2 5 3 6 9 4 7 10 13 8 11 14 12 15 16"),
            (
                Snake::DiagonalZigzag,
                "1 2 5 9 6 3 4 7 10 13 14 11 8 12 15 16",
            ),
            (Snake::Spiral, "1 2 3 4 8 12 16 15 14 13 9 5 6 7 11 10"),
            (Snake::SpiralOut, "10 11 7 6 5 9 13 14 15 16 12 8 4 3 2 1"),
            (Snake::Checker, "1 3 6 8 9 11 14 16 2 4 5 7 10 12 13 15"),
        ];
        for (snake, expected) in cases {
            assert_eq!(path(snake, 4, 4), numbers(expected), "{snake:?}");
        }
    }

    #[test]
    fn snakes_wind_through_grids_that_are_not_square() {
        // Four wide and two high, as eight rows are read.
        assert_eq!(path(Snake::Spiral, 4, 2), numbers("1 2 3 4 8 7 6 5"));
        assert_eq!(path(Snake::Diagonal, 4, 2), numbers("1 2 5 3 6 4 7 8"));
        assert_eq!(
            path(Snake::DiagonalZigzag, 4, 2),
            numbers("1 2 5 6 3 4 7 8")
        );
        assert_eq!(path(Snake::Columns, 4, 2), numbers("1 5 2 6 3 7 4 8"));
        // Four wide and six high, taller than wide.
        assert_eq!(
            path(Snake::Spiral, 4, 6),
            numbers("1 2 3 4 8 12 16 20 24 23 22 21 17 13 9 5 6 7 11 15 19 18 14 10")
        );
        assert_eq!(
            path(Snake::Diagonal, 4, 6),
            numbers("1 2 5 3 6 9 4 7 10 13 8 11 14 17 12 15 18 21 16 19 22 20 23 24")
        );
        // Every snake visits every cell of every grid a phrase is read as, once.
        for rows in 1..=96 {
            let grid = Grid::of(rows);
            for snake in Snake::ALL {
                let mut cells = path(snake, grid.width, grid.height);
                cells.sort_unstable();
                assert_eq!(
                    cells,
                    (1..=grid.cells()).collect::<Vec<_>>(),
                    "{snake:?} {rows}"
                );
            }
        }
    }

    #[test]
    fn a_phrase_is_read_four_wide_up_to_32_rows_and_eight_wide_above() {
        assert_eq!(
            Grid::of(16),
            Grid {
                width: 4,
                height: 4
            }
        );
        assert_eq!(
            Grid::of(12),
            Grid {
                width: 4,
                height: 3
            }
        );
        assert_eq!(
            Grid::of(13),
            Grid {
                width: 4,
                height: 4
            }
        );
        assert_eq!(
            Grid::of(32),
            Grid {
                width: 4,
                height: 8
            }
        );
        assert_eq!(
            Grid::of(64),
            Grid {
                width: 8,
                height: 8
            }
        );
        assert_eq!(
            Grid::of(1),
            Grid {
                width: 4,
                height: 1
            }
        );
    }

    fn cartesian(order: Cartesian, rows: usize) -> Vec<usize> {
        let grid = Grid::of(rows);
        (0..order.places(grid))
            .map(|place| order.cell(grid, place) + 1)
            .collect()
    }

    #[test]
    fn cartesian_with_equal_rates_moves_on_the_diagonal() {
        // Both counters on every row: X and Y move together.
        assert_eq!(cartesian(Cartesian::new(1, 1), 16), numbers("1 6 11 16"));
        // Both on every other row: each cell twice.
        assert_eq!(
            cartesian(Cartesian::new(2, 2), 16),
            numbers("1 1 6 6 11 11 16 16")
        );
    }

    #[test]
    fn cartesian_with_y_at_the_width_plays_the_grid_forward() {
        assert_eq!(
            cartesian(Cartesian::new(1, 4), 16),
            (1..=16).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cartesian_with_unequal_rates_matches_the_hand_worked_sequence() {
        // X every row over 4, Y every 3 rows over 4: twelve places, then round again.
        // place: 0 1 2 3 4 5 6 7 8 9 10 11
        // X:     0 1 2 3 0 1 2 3 0 1 2  3
        // Y:     0 0 0 1 1 1 2 2 2 3 3  3
        assert_eq!(
            cartesian(Cartesian::new(1, 3), 16),
            numbers("1 2 3 8 5 6 11 12 9 14 15 16")
        );
        // X every 2 rows over 3, Y every row over 2: lcm(6, 2) = 6 places.
        // X: 0 0 1 1 2 2; Y: 0 1 0 1 0 1
        assert_eq!(
            cartesian(Cartesian::new(2, 1).sized(3, 2), 16),
            numbers("1 5 2 6 3 7")
        );
        // X every 3 over 4 and Y every 2 over 3: lcm(12, 6) = 12 places.
        // X: 0 0 0 1 1 1 2 2 2 3 3 3; Y: 0 0 1 1 2 2 0 0 1 1 2 2
        assert_eq!(
            cartesian(Cartesian::new(3, 2).sized(4, 3), 16),
            numbers("1 1 5 6 10 10 3 3 7 8 12 12")
        );
    }

    #[test]
    fn cartesian_lengths_stay_within_the_grid() {
        let grid = Grid::of(8);
        assert_eq!(Cartesian::new(1, 1).sized(8, 8).lengths(grid), (4, 2));
        assert_eq!(Cartesian::default().lengths(Grid::of(64)), (8, 8));
        assert_eq!(Cartesian::default().lengths(Grid::of(256)), (8, 8));
        assert_eq!(Cartesian::new(0, 99), Cartesian::new(1, 16));
    }
}
