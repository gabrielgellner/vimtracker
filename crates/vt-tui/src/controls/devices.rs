//! A track's settings as racks of devices, a rack for each of the track page's pages: each
//! device's title and grid of settings, as the track page's cursor goes through them and the Roto
//! lays them on its knobs. Each page's devices are in a file of their own (`sound`, `modulation`,
//! `fx`, `generators`). Where a device sits on screen and where the cursor is among them are the
//! track page's.

mod fx;
mod generators;
mod modulation;
mod sound;

use vt_model::grid::{GRID_COLUMNS, GRID_ROWS, Grid};
use vt_model::song::Track;

use super::Control;
pub(crate) use generators::macros_device;
pub(crate) use modulation::modulator_bypassed;

/// A part of the track page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Sound,
    Mod,
    Fx,
    /// The live generators of the phrase under the cursor.
    Gen,
}

impl Page {
    /// Every page, in order.
    pub const ALL: [Self; 4] = [Self::Sound, Self::Mod, Self::Fx, Self::Gen];

    /// The page's name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sound => "Sound",
            Self::Mod => "Mod",
            Self::Fx => "FX",
            Self::Gen => "Gen",
        }
    }

    /// Where the page is in [`Page::ALL`].
    pub(crate) fn index(self) -> usize {
        self as usize
    }

    /// The page next to this one, or this one at either end.
    pub(crate) fn beside(self, next: bool) -> Self {
        let at = Self::ALL.iter().position(|page| *page == self).unwrap_or(0);
        let to = if next { at + 1 } else { at.saturating_sub(1) };
        Self::ALL[to.min(Self::ALL.len() - 1)]
    }

    /// The page's devices, in the order they're read, each page's from its own file: the Sound
    /// page's machine, filter, output and play, or a fixed track's output alone, the Mod page's
    /// mod slots, accumulator and arpeggio, and the FX page's slots. The cursor, `j`, `k`,
    /// `h`, `l`, `{` and `}` go through whatever devices a page gives, laid out as its rack
    /// (the track page's `rack_places`). The Gen page's hang on a phrase as well, so they're
    /// [`Page::devices_on`]'s, and here there are none.
    pub fn devices(self, track: &Track) -> Vec<DeviceSpec> {
        self.devices_on(track, None)
    }

    /// The page's devices, the Gen page's being a device for each live generator of this phrase
    /// of the track, then `Empty`.
    pub(crate) fn devices_on(self, track: &Track, phrase: Option<u32>) -> Vec<DeviceSpec> {
        match self {
            Self::Sound => sound::devices(track),
            Self::Mod => modulation::devices(track),
            Self::Fx => fx::devices(track),
            Self::Gen => generators::devices(track, phrase),
        }
    }
}

/// How many columns of settings a device has, its grid's.
pub(crate) const DEVICE_COLUMNS: usize = GRID_COLUMNS;

/// How many controls a device's title holds at most: three more than its grid's columns, as a
/// Steps slot's has its type, mode, loop, jump, spread, bias and destination (decision 403).
pub(crate) const TITLE_PLACES: usize = DEVICE_COLUMNS + 3;

/// How many rows the cursor goes through in a device: its title, then its grid's.
pub(crate) const DEVICE_ROWS: usize = 1 + GRID_ROWS;

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
    pub(crate) switchable: bool,
    /// How much of the rack's width it takes. Two half-width devices in a row share a band of the
    /// rack, the first on the left.
    pub width: DeviceWidth,
    pub title: [Option<Control>; TITLE_PLACES],
    pub grid: [[Option<Control>; DEVICE_COLUMNS]; GRID_ROWS],
}

impl DeviceSpec {
    /// A device with its title's controls over a grid a model states, such as `Insert::grid`, by
    /// the number of each setting in it, which `control` makes the cursor's control. It has no
    /// switch until it's given one, and takes the rack's whole width.
    pub fn new(
        identity: impl Into<String>,
        title: [Option<Control>; TITLE_PLACES],
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
    pub fn rows(&self) -> impl Iterator<Item = Vec<Option<Control>>> + '_ {
        std::iter::once(self.title.to_vec()).chain(self.grid.iter().map(|row| row.to_vec()))
    }
}
