//! Where a device's settings sit on its page: up to four columns of three, each place holding a
//! setting by its index among those the device has now, or nothing. A device states its own grid,
//! so what its settings mean decides where they go, never the order they're listed in. An effect's
//! is [`Insert::grid`](crate::insert::Insert::grid).

/// How many columns and rows a grid has.
pub const GRID_COLUMNS: usize = 4;
pub const GRID_ROWS: usize = 3;

/// A device's settings in columns, top to bottom, by their index among its settings, or as
/// themselves where they're a type of their own, as a generator's
/// [`GenParam`](crate::shape::GenParam)s are. A place with nothing is a hole the cursor steps over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid<T = usize>([[Option<T>; GRID_ROWS]; GRID_COLUMNS]);

impl<T: Copy> Default for Grid<T> {
    fn default() -> Self {
        Self([[None; GRID_ROWS]; GRID_COLUMNS])
    }
}

impl<T: Copy> Grid<T> {
    /// A grid from all four of its columns, for a table that states one as a constant.
    pub const fn new(columns: [[Option<T>; GRID_ROWS]; GRID_COLUMNS]) -> Self {
        Self(columns)
    }

    /// A grid from its columns, each listing its settings from the top. Past four columns, or past
    /// three in a column, they're left out, which [`Grid::places_each_once`] catches.
    pub fn from_columns<C: AsRef<[Option<T>]>>(columns: &[C]) -> Self {
        let mut grid = Self::default();
        for (column, settings) in columns.iter().take(GRID_COLUMNS).enumerate() {
            for (row, setting) in settings.as_ref().iter().take(GRID_ROWS).enumerate() {
                grid.0[column][row] = *setting;
            }
        }
        grid
    }

    /// What's at a place, if anything.
    pub fn at(self, row: usize, column: usize) -> Option<T> {
        self.0.get(column)?.get(row).copied().flatten()
    }

    /// A row's places, left to right.
    pub fn row(self, row: usize) -> [Option<T>; GRID_COLUMNS] {
        std::array::from_fn(|column| self.at(row, column))
    }

    /// A column's places, top to bottom.
    pub fn column(self, column: usize) -> [Option<T>; GRID_ROWS] {
        std::array::from_fn(|row| self.at(row, column))
    }

    /// Every setting it places, column by column.
    pub fn settings(self) -> impl Iterator<Item = T> {
        self.0.into_iter().flatten().flatten()
    }
}

impl Grid {
    /// Whether it places each of a device's settings exactly once, apart from those shown
    /// elsewhere, such as an effect's type in its title, and nothing that isn't one of them.
    pub fn places_each_once(
        self,
        count: usize,
        elsewhere: impl IntoIterator<Item = usize>,
    ) -> bool {
        let mut placed: Vec<usize> = self.settings().collect();
        placed.sort_unstable();
        let elsewhere: Vec<usize> = elsewhere.into_iter().collect();
        let wanted: Vec<usize> = (0..count).filter(|at| !elsewhere.contains(at)).collect();
        placed == wanted
    }
}
