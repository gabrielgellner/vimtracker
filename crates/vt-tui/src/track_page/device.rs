//! A page as a rack of devices: each device's title and grid of settings as the cursor sees them,
//! where each sits in the rack, and where the cursor is among them. The views draw them with
//! `views/widgets/device`.

use vt_model::grid::{GRID_COLUMNS, GRID_ROWS, Grid};

use super::Control;

/// How many columns of settings a device has, its grid's.
pub const DEVICE_COLUMNS: usize = GRID_COLUMNS;

/// How many rows the cursor goes through in a device: its title, then its grid's.
pub const DEVICE_ROWS: usize = 1 + GRID_ROWS;

/// How much of a rack's width a device takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeviceWidth {
    /// All of it, as an FX slot does.
    #[default]
    Full,
    /// Half, beside another, as the Mod page's slots are.
    Half,
}

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
    /// How much of the rack's width it takes. Two half-width devices in a row share a band of the
    /// rack, the first on the left.
    pub width: DeviceWidth,
    pub title: [Option<Control>; DEVICE_COLUMNS],
    pub grid: [[Option<Control>; DEVICE_COLUMNS]; GRID_ROWS],
}

impl DeviceSpec {
    /// A device with its title's controls over a grid a model states, such as `Insert::grid`, by
    /// the number of each setting in it, which `control` makes the cursor's control. It has no
    /// switch until it's given one, and takes the rack's whole width.
    pub fn new(
        identity: impl Into<String>,
        title: [Option<Control>; DEVICE_COLUMNS],
        grid: Grid,
        control: impl Fn(usize) -> Control,
    ) -> Self {
        Self {
            identity: identity.into(),
            switchable: false,
            width: DeviceWidth::Full,
            title,
            grid: std::array::from_fn(|row| grid.row(row).map(|at| at.map(&control))),
        }
    }

    /// The same device at half the rack's width.
    #[must_use]
    pub fn half(self) -> Self {
        Self {
            width: DeviceWidth::Half,
            ..self
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

/// Where a device sits in its rack: in which band, top to bottom, and on which side, 0 for the
/// left or the whole width and 1 for the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RackPlace {
    pub band: usize,
    pub side: usize,
}

/// Where each device sits in a rack, in the order they're given, which is the order they're read
/// in: a full-width device has a band of its own, and a half-width one goes beside the one before
/// it if that's a half on the left, and starts a band on the left otherwise.
pub fn rack_places(devices: &[DeviceSpec]) -> Vec<RackPlace> {
    let mut places: Vec<RackPlace> = Vec::with_capacity(devices.len());
    for (at, device) in devices.iter().enumerate() {
        let beside = at > 0
            && device.width == DeviceWidth::Half
            && devices[at - 1].width == DeviceWidth::Half
            && places[at - 1].side == 0;
        let place = match places.last() {
            Some(last) if beside => RackPlace {
                band: last.band,
                side: 1,
            },
            Some(last) => RackPlace {
                band: last.band + 1,
                side: 0,
            },
            None => RackPlace { band: 0, side: 0 },
        };
        places.push(place);
    }
    places
}

/// The device above or below one in its rack: the one on the same side in the band there, or that
/// band's only one, if there's a band there.
pub fn device_above_below(devices: &[DeviceSpec], device: usize, below: bool) -> Option<usize> {
    let places = rack_places(devices);
    let here = *places.get(device)?;
    let band = if below {
        here.band + 1
    } else {
        here.band.checked_sub(1)?
    };
    let in_band: Vec<usize> = (0..places.len())
        .filter(|at| places[*at].band == band)
        .collect();
    in_band
        .iter()
        .copied()
        .find(|at| places[*at].side == here.side)
        .or_else(|| in_band.first().copied())
}

/// The device beside one in its band, on its right or its left, if there's one there.
pub fn device_beside(devices: &[DeviceSpec], device: usize, right: bool) -> Option<usize> {
    let places = rack_places(devices);
    let here = *places.get(device)?;
    let side = if right {
        here.side + 1
    } else {
        here.side.checked_sub(1)?
    };
    (0..places.len()).find(|at| places[*at].band == here.band && places[*at].side == side)
}

/// The devices in one's column of the rack, top to bottom: those above it, it, and those below it,
/// each the one above or below the last.
pub fn rack_column(devices: &[DeviceSpec], device: usize) -> Vec<usize> {
    let mut column = vec![device];
    while let Some(above) = device_above_below(devices, column[column.len() - 1], false) {
        column.push(above);
    }
    column.reverse();
    while let Some(below) = device_above_below(devices, column[column.len() - 1], true) {
        column.push(below);
    }
    column
}
