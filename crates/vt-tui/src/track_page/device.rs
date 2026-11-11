//! A page as a rack of devices: each device's title and grid of settings as the cursor sees them,
//! and where the cursor is among them. The views draw them with `views/widgets/device`.

use vt_model::grid::{GRID_COLUMNS, GRID_ROWS};
use vt_model::insert::Insert;

use super::Control;

/// How many columns of settings a device has, its grid's.
pub const DEVICE_COLUMNS: usize = GRID_COLUMNS;

/// How many rows the cursor goes through in a device: its title, then its grid's.
pub const DEVICE_ROWS: usize = 1 + GRID_ROWS;

/// One device in a page's rack, as the cursor and the page see it: which it is, the controls in
/// its title over the grid's first columns, and its settings grid, each place a control or a hole
/// the cursor steps over. Every device has as many rows, whatever it holds, so moving between
/// devices keeps the cursor's row and column. The FX page's slots are the first devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSpec {
    /// Which device it is, such as a slot's digit, dim at the start of its title.
    pub identity: String,
    /// Whether `b` can switch it out, which gives its title a switch.
    pub switchable: bool,
    pub title: [Option<Control>; DEVICE_COLUMNS],
    pub grid: [[Option<Control>; DEVICE_COLUMNS]; GRID_ROWS],
}

impl DeviceSpec {
    /// A slot on the FX page: its effect over the grid's first column and its type over the
    /// second, then its effect's grid. An empty slot has only its effect.
    pub fn slot(slot: usize, insert: Insert) -> Self {
        let mut title = [None; DEVICE_COLUMNS];
        title[0] = Some(Control::Insert(slot));
        title[1] = insert.type_param().map(|at| Control::InsertParam(slot, at));
        let grid = insert.grid();
        Self {
            identity: (slot + 1).to_string(),
            switchable: insert != Insert::Empty,
            title,
            grid: std::array::from_fn(|row| {
                grid.row(row)
                    .map(|at| at.map(|at| Control::InsertParam(slot, at)))
            }),
        }
    }

    /// Its rows as the cursor goes through them: its title, then its grid's.
    pub fn rows(&self) -> impl Iterator<Item = [Option<Control>; DEVICE_COLUMNS]> + '_ {
        std::iter::once(self.title).chain(self.grid.iter().copied())
    }
}

/// Where the cursor is in a page's rack: in which device, on which of its rows, the title being 0,
/// and in which column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceCursor {
    pub device: usize,
    pub row: usize,
    pub column: usize,
}
