//! The Gen page's devices: the track's macros, a device for each live generator of a phrase in the
//! phrase view's column order, an `Acid`'s `Loops` and an FX column's `Lane` under theirs, then
//! `Phrase`, the track's settings that move its phrases' walks (decision 400), and `Empty` (the generators study, `docs/designs/generators-page.md`, §3). A device's title and
//! grid are its fill's, from the declared table, [`GenParam`].

use vt_model::action::TrackControl;
use vt_model::constants::FX_SLOTS;
use vt_model::generator::Generator;
use vt_model::grid::{GRID_ROWS, Grid};
use vt_model::shape::{GenParam, ShapeColumn, is_empty};
use vt_model::song::Track;
use vt_model::step::FxCommand;

use super::{DEVICE_COLUMNS, DeviceSpec, DeviceWidth, TITLE_PLACES};
use crate::controls::{Control, LaneField};

/// Where a column comes in the phrase view, which the page's devices follow: Note, Velocity,
/// Nudge, Length, then the FX columns.
fn column_order(column: ShapeColumn) -> usize {
    match column {
        ShapeColumn::Note { .. } => 0,
        ShapeColumn::Velocity => 1,
        ShapeColumn::Nudge { .. } => 2,
        ShapeColumn::Length => 3,
        ShapeColumn::Fx { slot, .. } => 4 + slot,
    }
}

/// A phrase's live generators, by their places in its list, in the order the page shows them:
/// by column as the phrase view has them, and on one column, the top one first.
pub(super) fn order(generators: &[Generator]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..generators.len()).collect();
    order.sort_by_key(|at| {
        let generator = &generators[*at];
        let first = generator.rows().map_or(0, |rows| rows.first);
        (column_order(generator.column), first)
    });
    order
}

/// The macros' places in their band: two rows of four, in order along each.
const MACROS_GRID: [[Option<usize>; 2]; 4] = [
    [Some(0), Some(4)],
    [Some(1), Some(5)],
    [Some(2), Some(6)],
    [Some(3), Some(7)],
];

/// The track's eight macros as a full-width band, two rows of four, with nothing in its title. Its
/// own device, so it can sit on whichever page it belongs on.
pub(crate) fn macros_device() -> DeviceSpec {
    DeviceSpec::new(
        "Macros",
        [None; TITLE_PLACES],
        Grid::from_columns(&MACROS_GRID),
        Control::Macro,
    )
}

/// The Gen page's devices for a phrase of a track: the track's macros first, then a device each
/// for its live generators, an `Acid`'s with its `Loops` under it, and for each FX column with a
/// lane, its `Lane`, under its column's generator if it has one, then `Phrase` and `Empty`. With no phrase
/// there are only the macros.
pub(super) fn devices(track: &Track, phrase: Option<u32>) -> Vec<DeviceSpec> {
    let Some((id, found)) = phrase.and_then(|id| Some((id, track.phrases.get(&id)?))) else {
        return vec![macros_device()];
    };
    let generators = found.generators();
    // Each device with where its column comes, the macros before them all.
    let mut placed: Vec<(usize, DeviceSpec)> = vec![(0, macros_device())];
    placed.extend(order(generators).into_iter().flat_map(|at| {
        let generator = &generators[at];
        let generates = generator
            .rows()
            .is_some_and(|rows| is_empty(&generator.under(found.steps()), rows, generator.column));
        let loops = DeviceSpec::loops(id, at, generator, generates);
        let column = 1 + column_order(generator.column);
        std::iter::once(DeviceSpec::generator(id, at, generator, generates))
            .chain(loops)
            .map(move |device| (column, device))
    }));
    // A lane's goes after its column's generators, or where its column comes if it has none.
    for slot in (0..FX_SLOTS).filter(|slot| found.lane(*slot).is_some()) {
        let column = 1 + column_order(ShapeColumn::Fx {
            slot,
            command: FxCommand::Del,
        });
        let at = placed
            .iter()
            .rposition(|(placed, _)| *placed <= column)
            .map_or(0, |at| at + 1);
        placed.insert(at, (column, DeviceSpec::lane(slot)));
    }
    let mut devices: Vec<DeviceSpec> = placed.into_iter().map(|(_, device)| device).collect();
    devices.push(DeviceSpec::phrase());
    let mut title = [None; TITLE_PLACES];
    title[0] = Some(Control::NewGenerator(id));
    devices.push(DeviceSpec {
        identity: String::new(),
        switchable: false,
        width: DeviceWidth::Full,
        title,
        grid: [[None; DEVICE_COLUMNS]; GRID_ROWS],
    });
    devices
}

impl DeviceSpec {
    /// A live generator as a device: its title's settings, then its fill's grid.
    fn generator(phrase: u32, at: usize, generator: &Generator, generates: bool) -> Self {
        let control = |param: Option<GenParam>| {
            param.map(|param| Control::Gen {
                phrase,
                generator: at,
                param,
            })
        };
        let shape = &generator.shape;
        let mut title = [None; TITLE_PLACES];
        for (place, param) in GenParam::title(shape).into_iter().enumerate() {
            title[place] = control(param);
        }
        let columns = GenParam::grid(shape, generator.column, generates);
        let grid = std::array::from_fn(|row| {
            std::array::from_fn(|column| control(columns.at(row, column)))
        });
        Self {
            identity: String::new(),
            switchable: false,
            width: DeviceWidth::Full,
            title,
            grid,
        }
    }
}

impl DeviceSpec {
    /// An `Acid`'s loops as a device of their own, `Loops`, with nothing in its title, as the
    /// fill's grid is full; none for a fill without them.
    fn loops(phrase: u32, at: usize, generator: &Generator, generates: bool) -> Option<Self> {
        let columns = GenParam::loops_grid(&generator.shape, generator.column, generates)?;
        let grid = std::array::from_fn(|row| {
            std::array::from_fn(|column| {
                columns.at(row, column).map(|param| Control::Gen {
                    phrase,
                    generator: at,
                    param,
                })
            })
        });
        Some(Self {
            identity: LOOPS.to_owned(),
            switchable: false,
            width: DeviceWidth::Full,
            title: [None; TITLE_PLACES],
            grid,
        })
    }

    /// An FX column's lane's `Length`, `Rate`, `Start` and `Jump`, the track's, which move every phrase's lane on
    /// the column, as a device of its own, `Lane`, named for its column.
    pub(crate) fn lane(slot: usize) -> Self {
        let mut grid = [[None; DEVICE_COLUMNS]; GRID_ROWS];
        grid[0][0] = Some(Control::Lane(slot, LaneField::Length));
        grid[0][1] = Some(Control::Lane(slot, LaneField::Rate));
        grid[0][2] = Some(Control::Lane(slot, LaneField::Start));
        // How often a row plays any of the lane's rows instead, beside where it starts.
        grid[0][3] = Some(Control::Lane(slot, LaneField::Jump));
        Self {
            identity: format!("FX{} {LANE}", slot + 1),
            switchable: false,
            width: DeviceWidth::Full,
            title: [None; TITLE_PLACES],
            grid,
        }
    }

    /// The track's settings that move the walk of its phrases' rows, `Order`, `Reverse`,
    /// `Order X` and `Order Y`, with the window's `Length` and `Start` and its `Cue` under them,
    /// and the chain's `Order` and `Jump` at the foot, as a device of their own, `Phrase`
    /// (decisions 400 to 402, 423 and 424).
    pub(crate) fn phrase() -> Self {
        let mut grid = [[None; DEVICE_COLUMNS]; GRID_ROWS];
        for (column, control) in WALK_CONTROLS.into_iter().enumerate() {
            grid[0][column] = Some(Control::Base(control));
        }
        // The window under the order, as a lane's `Length` comes first on its device.
        grid[1][0] = Some(Control::Base(TrackControl::PhraseLength));
        grid[1][1] = Some(Control::Base(TrackControl::PhraseStart));
        // Whether a turn of the window, or of a lane's, waits for its loop (decision 402).
        grid[1][2] = Some(Control::Base(TrackControl::Cue));
        // The walk of the chain's phrases under the walk of their rows (decisions 423 and 424).
        grid[2][0] = Some(Control::Base(TrackControl::ChainOrder));
        grid[2][1] = Some(Control::Base(TrackControl::ChainJump));
        Self {
            identity: PHRASE.to_owned(),
            switchable: false,
            width: DeviceWidth::Full,
            title: [None; TITLE_PLACES],
            grid,
        }
    }

    /// Whether it's `Phrase`.
    pub(crate) fn is_phrase(&self) -> bool {
        self.grid[0][0] == Some(Control::Base(TrackControl::Order))
    }

    /// The FX column whose lane it is, if it's a `Lane`.
    pub(crate) fn lane_slot(&self) -> Option<usize> {
        match self.grid[0][0] {
            Some(Control::Lane(slot, _)) => Some(slot),
            _ => None,
        }
    }

    /// Whether it's an `Acid`'s `Loops`.
    pub(crate) fn is_loops(&self) -> bool {
        self.identity == LOOPS
            && self
                .grid
                .iter()
                .flatten()
                .flatten()
                .any(|control| matches!(control, Control::Gen { param, .. } if param.is_loop()))
    }
}

/// What an `Acid`'s loops' device is called.
pub(crate) const LOOPS: &str = "Loops";

/// What the device of the settings that move a track's phrases' walks is called.
pub(crate) const PHRASE: &str = "Phrase";

/// The settings on `Phrase`, in its grid's order.
const WALK_CONTROLS: [TrackControl; 4] = [
    TrackControl::Order,
    TrackControl::Reverse,
    TrackControl::OrderX,
    TrackControl::OrderY,
];

/// What an FX column's lane's device is called, after its column.
pub(crate) const LANE: &str = "Lane";
