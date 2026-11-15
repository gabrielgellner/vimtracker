//! The Gen page: the live generators of the phrase under the cursor, the one `gl` reopens, a
//! full-width device each in the phrase view's column order, then an `Empty` device that makes
//! another column live (the generators study, `docs/designs/generators-page.md`, §3). A device's
//! title is its fill and its `Where`, or `Acid`'s `Pitches` and `Follow`'s `Source`, then `Vary`
//! where it draws, and its grid the fill's settings, both from the declared table, [`GenParam`].
//! Every turn goes through the `Reshape` the shape panel's does, so the column is written again as
//! it turns, one undo step a press.
//!
//! The page's edits are the track page's, [`TrackPage::gen_edit`], over the [`Services`]: `K`'s
//! lists come back in an [`Outcome::Open`] and `i` in an [`Outcome::Insert`] (#784). `Enter`
//! stays `App`'s, as it opens the shape panel on the generator, and so do the choices made from
//! `K`'s lists, as the picker's are, a column made live building a shape panel too.

use vt_model::action::Action;
use vt_model::constants::FX_SLOTS;
use vt_model::generator::Generator;
use vt_model::grid::{GRID_ROWS, Grid};
use vt_model::selection::Field;
use vt_model::shape::params::{Choice, Typing};
use vt_model::shape::{Context, GenParam, Shape, ShapeColumn, is_empty};
use vt_model::song::Track;
use vt_model::step::FxCommand;

use crate::app::{App, Entry};
use crate::input::bindings::Cmd;
use crate::input::mode::View;
use crate::overlays::picker::{Item, Pick, Picker};
use crate::overlays::shape::choice_picker;
use crate::pages::{CtxMut, Outcome};
use crate::services::Services;
use crate::services::generators::{LiveGen, generator_label};

use super::{
    Control, DEVICE_COLUMNS, DEVICE_ROWS, DeviceSpec, DeviceWidth, LaneField, Page, TrackPage,
};

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

/// The track's eight macros as a full-width band, two rows of four, with nothing in its title
/// (decision 285). Its own device, so it can sit on whichever page it belongs on.
pub(crate) fn macros_device() -> DeviceSpec {
    DeviceSpec::new(
        "Macros",
        [None; DEVICE_COLUMNS],
        Grid::from_columns(&MACROS_GRID),
        Control::Macro,
    )
}

/// The Gen page's devices for a phrase of a track: the track's macros first, then a device each
/// for its live generators, an `Acid`'s with its `Loops` under it, and for each FX column with a
/// lane, its `Lane`, under its column's generator if it has one, then `Empty`. With no phrase
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
    let mut title = [None; DEVICE_COLUMNS];
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
        let mut title = [None; DEVICE_COLUMNS];
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
            title: [None; DEVICE_COLUMNS],
            grid,
        })
    }

    /// An FX column's lane's `Length` and `Rate`, the track's, which move every phrase's lane on
    /// the column, as a device of its own, `Lane`, named for its column (decision 347).
    pub(crate) fn lane(slot: usize) -> Self {
        let mut grid = [[None; DEVICE_COLUMNS]; GRID_ROWS];
        grid[0][0] = Some(Control::Lane(slot, LaneField::Length));
        grid[0][1] = Some(Control::Lane(slot, LaneField::Rate));
        Self {
            identity: format!("FX{} {LANE}", slot + 1),
            switchable: false,
            width: DeviceWidth::Full,
            title: [None; DEVICE_COLUMNS],
            grid,
        }
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

/// What an FX column's lane's device is called, after its column.
pub(crate) const LANE: &str = "Lane";

/// The phrase view's field for a column, where the cursor goes to open the panel on it.
fn field_of(column: ShapeColumn) -> Field {
    match column {
        ShapeColumn::Note { .. } => Field::Note,
        ShapeColumn::Velocity => Field::Velocity,
        ShapeColumn::Nudge { .. } => Field::Nudge,
        ShapeColumn::Length => Field::GateLength,
        ShapeColumn::Fx { slot, .. } => Field::FxValue(slot),
    }
}

/// The columns `K` on `Empty` offers, as the phrase view has them, by their fields and names.
fn columns() -> Vec<(Field, String)> {
    let mut columns = vec![
        (Field::Note, "Note".to_owned()),
        (Field::Velocity, "Vel".to_owned()),
        (Field::Nudge, "Nudge".to_owned()),
        (Field::GateLength, "Len".to_owned()),
    ];
    columns.extend((0..FX_SLOTS).map(|slot| (Field::FxValue(slot), format!("FX{}", slot + 1))));
    columns
}

/// Whether a generator is on a field's column.
fn on_field(generator: &Generator, field: Field) -> bool {
    field_of(generator.column) == field
}

/// The generator setting a control is, on the Gen page: its phrase, its place in the phrase's
/// list and the setting.
fn gen_of(control: Control) -> Option<(u32, usize, GenParam)> {
    match control {
        Control::Gen {
            phrase,
            generator,
            param,
        } => Some((phrase, generator, param)),
        _ => None,
    }
}

/// How the generator setting `control` is typed, if it's one.
pub(super) fn gen_typing(services: &Services, control: Control) -> Option<Typing> {
    let (phrase, generator, param) = gen_of(control)?;
    let found = services.song().tracks[services.cursor.track]
        .phrases
        .get(&phrase)?;
    Some(param.typing(found.generators().get(generator)?.column))
}

/// A typed value, for the generator setting `control`, the one under the cursor.
pub(super) fn type_gen(services: &mut Services, view: View, control: Control, text: &str) {
    GenEdit {
        services,
        view,
        control,
    }
    .type_in(text);
}

/// Give a live generator of the cursor's track, by its phrase and its place in the phrase's
/// list, the shape `change` makes from its own and what its settings act on, read for `view`:
/// made whole and refused as the panel's are, then applied as one undo step if it's a change.
fn reshape_live(
    services: &mut Services,
    view: View,
    (phrase, generator): (u32, usize),
    change: impl FnOnce(&mut Services, &mut Shape, &Context),
) {
    let track = services.cursor.track;
    let Some(live) = services.live_generator(track, phrase, generator, view) else {
        return;
    };
    let mut shape = live.generator.shape;
    change(services, &mut shape, &live.context());
    live.settle(&mut shape);
    if let Some(why) = live.refusal(&shape) {
        services.status = why;
        return;
    }
    if shape != live.generator.shape {
        services.apply(&Action::Reshape {
            track,
            phrase,
            generator,
            shape,
        });
    }
}

impl TrackPage {
    /// Whether a key on the setting under the cursor, `control`, is the Gen page's: the track
    /// page is on screen, `view`, and on Gen, and the cursor isn't on the macros' band, first on
    /// the page, or a lane's `Length` and `Rate`, which turn as the other pages' settings do. The mixer, which the track page may
    /// be left on Gen behind, keeps its own keys.
    pub(super) fn on_gen(&self, view: View, control: Control) -> bool {
        view == View::Track
            && self.page == Page::Gen
            && !matches!(control, Control::Macro(_) | Control::Lane(..))
    }

    /// Run one of the Gen page's edits, which reshape the generator setting under the cursor,
    /// `control`, `times` being its count, at least 1: `=` `-` `+` `_`, `x`, `K`, `i`,
    /// `Space g a`, `dd` and `r`, and `p`, which is refused. `Enter`, which opens the shape panel
    /// in the phrase view, is passed back, as is any other key, and every key off the Gen page or on
    /// the mixer.
    pub(super) fn gen_edit(
        &self,
        cmd: Cmd,
        times: usize,
        control: Control,
        ctx: &mut CtxMut,
    ) -> Outcome {
        if !self.on_gen(ctx.modes.view, control) {
            return Outcome::Passed;
        }
        let mut edit = GenEdit {
            services: ctx.services,
            view: ctx.modes.view,
            control,
        };
        match cmd {
            Cmd::Increase => edit.adjust(true, times, false),
            Cmd::Decrease => edit.adjust(false, times, false),
            Cmd::IncreaseLarge => edit.adjust(true, times, true),
            Cmd::DecreaseLarge => edit.adjust(false, times, true),
            Cmd::ClearField => edit.reset(),
            Cmd::PickValue => return edit.open_picker(),
            Cmd::EnterInsert => return edit.start_typing(),
            Cmd::ApplyGenerator => edit.apply(),
            Cmd::RemoveRow => edit.remove(),
            Cmd::RollGenerator => edit.roll(),
            Cmd::Paste => return Outcome::Refused("p puts an effect in a slot, on the FX page"),
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }
}

/// What an edit on the Gen page reaches: the services, the view on screen, which a live
/// generator is read for, and the setting under the cursor, on the track page or the mixer.
struct GenEdit<'a> {
    services: &'a mut Services,
    view: View,
    control: Control,
}

impl GenEdit<'_> {
    /// The generator setting the cursor is on: its phrase, its place in the phrase's list and the
    /// setting.
    fn here(&self) -> Option<(u32, usize, GenParam)> {
        gen_of(self.control)
    }

    /// The generator the cursor is on, as it plays, from its place in the phrase's list.
    fn live(&self, phrase: u32, generator: usize) -> Option<LiveGen> {
        self.services
            .live_generator(self.services.cursor.track, phrase, generator, self.view)
    }

    /// Give the generator the cursor is on the shape `change` makes from its own and what its
    /// settings act on, as one undo step if it's a change.
    fn reshape(&mut self, change: impl FnOnce(&mut Services, GenParam, &mut Shape, &Context)) {
        let Some((phrase, generator, param)) = self.here() else {
            return;
        };
        reshape_live(
            self.services,
            self.view,
            (phrase, generator),
            |services, shape, context| change(services, param, shape, context),
        );
    }

    /// `=` `-` `+` `_`, `count` times.
    fn adjust(&mut self, up: bool, count: usize, large: bool) {
        let sign = if up { 1 } else { -1 };
        self.reshape(|services, param, shape, context| {
            for _ in 0..count {
                if let Err(why) = param.step(shape, context, sign, large) {
                    services.status = why.to_owned();
                    return;
                }
            }
        });
    }

    /// `x`: the setting back to what it starts as.
    fn reset(&mut self) {
        let Some((phrase, generator, _)) = self.here() else {
            return;
        };
        let Some(live) = self.live(phrase, generator) else {
            return;
        };
        let starts = live.starts;
        self.reshape(|_, param, shape, context| param.reset(shape, context, starts));
    }

    /// `r`: roll the generator's seed again, from anywhere in its device, as the panel does.
    fn roll(&mut self) {
        if self.here().is_none() {
            self.services.status = "r rolls a generator again: go to one first".to_owned();
            return;
        }
        self.reshape(|services, _, shape, _| services.roll(shape));
    }

    /// `K`: list a list setting's choices; one that's off or on switches. On `Empty`, the
    /// columns to make live.
    fn open_picker(&mut self) -> Outcome {
        if let Control::NewGenerator(_) = self.control {
            return self.open_column_picker();
        }
        let Some((phrase, generator, param)) = self.here() else {
            return Outcome::Done;
        };
        let Some(live) = self.live(phrase, generator) else {
            return Outcome::Done;
        };
        let context = live.context();
        if param.typing(context.column) == Typing::Toggled {
            self.adjust(true, 1, false);
            return Outcome::Done;
        }
        let picker = choice_picker(
            param.name(),
            param,
            &live.generator.shape,
            &context,
            |choice| Pick::GenChoice {
                phrase,
                generator,
                choice,
            },
            None,
        );
        let Some(picker) = picker else {
            self.services.status =
                format!("{} isn't a list: = and - turn it, i types it", param.name());
            return Outcome::Done;
        };
        Outcome::Open(Box::new(picker))
    }

    /// `i`: a list's choices, a switch turned over, or a value to type.
    fn start_typing(&mut self) -> Outcome {
        match gen_typing(self.services, self.control) {
            None | Some(Typing::Listed | Typing::Toggled) => self.open_picker(),
            Some(_) => Outcome::Insert(Entry::default()),
        }
    }

    /// A typed value, for the generator setting under the cursor.
    fn type_in(&mut self, text: &str) {
        self.reshape(|services, param, shape, context| {
            if let Err(why) = param.typed(shape, context, text) {
                services.status = why.to_owned();
            }
        });
    }

    /// The name of the generator the cursor is on, as the phrase's heading names it.
    fn name(&self, phrase: u32, generator: usize) -> String {
        self.services.song().tracks[self.services.cursor.track]
            .phrases
            .get(&phrase)
            .and_then(|found| found.generators().get(generator))
            .map_or_else(String::new, generator_label)
    }

    /// `Space g a`: apply the generator, leaving its values as plain ones, as last heard where
    /// something moved it; or, for one that varies, keep the variation last heard, as its seed,
    /// with `Vary` back to `Once`, so it stays live. A `Cells` generation isn't a seed, so one
    /// that evolves is applied as last heard, as one a macro moves is.
    fn apply(&mut self) {
        let Some((phrase, generator, _)) = self.here() else {
            self.services.status = "Space g a applies a generator: go to one first".to_owned();
            return;
        };
        let name = self.name(phrase, generator);
        if let Some(live) = self.live(phrase, generator)
            && live.generator.heard().varies()
            && !live.generator.heard().evolves()
        {
            self.keep_variation(phrase, generator, &name);
            return;
        }
        let track = self.services.cursor.track;
        self.services
            .apply_generator(track, phrase, generator, &name);
    }

    /// Keep the variation of a generator that varies last heard: the seeds it played that pass
    /// become its own, and `Vary` goes back to `Once`, so the pass heard plays every pass and the
    /// phrase holds it, as the Turing Machine's lock keeps the loop it has drifted to (decision
    /// 293). The engine reports each pass as it's first heard; one reported from another
    /// generator, on another column, isn't kept. A `Ring` keeps its register as that pass began,
    /// so the pass heard plays first and the ring runs on from there, holding that loop once
    /// `Change` is down (decision 324); an `Acid` with loops keeps the count that pass began at,
    /// so the pass heard plays first and its loops go on from there (decision 331).
    fn keep_variation(&mut self, phrase: u32, generator: usize, name: &str) {
        let track = self.services.cursor.track;
        let Some(live) = self.live(phrase, generator) else {
            return;
        };
        let heard = self
            .services
            .heard_passes
            .get(&(track, phrase, generator))
            .filter(|heard| heard.column == live.generator.column)
            .copied();
        let Some(heard) = heard else {
            self.services.status = format!(
                "{name} hasn't been heard varying yet: play it, then Space g a keeps the pass heard"
            );
            return;
        };
        let mut shape = live.generator.shape;
        shape.seed = heard.seed;
        shape.acid.roll = heard.roll;
        shape.vary = 0;
        if let Some(kept) = heard.ring {
            shape.ring.kept = Some(kept);
        }
        if let Some(at) = heard.acid_at {
            shape.acid.at = at;
        }
        live.settle(&mut shape);
        self.services.apply(&Action::Reshape {
            track,
            phrase,
            generator,
            shape,
        });
        self.services.status = if heard.ring.is_some() {
            format!(
                "Kept {name}'s loop as pass {} began it: Change at 0 holds it",
                heard.pass + 1
            )
        } else if heard.acid_at.is_some() {
            format!(
                "Kept {name} as pass {} played it: its loops go on from there",
                heard.pass + 1
            )
        } else {
            format!(
                "Kept {name} as pass {} played it: Vary is Once now",
                heard.pass + 1
            )
        };
    }

    /// `dd`: take the generator away, putting back what was under it.
    fn remove(&mut self) {
        let Some((phrase, generator, _)) = self.here() else {
            self.services.status = "dd takes a generator away: go to one first".to_owned();
            return;
        };
        let name = self.name(phrase, generator);
        self.services.apply(&Action::RemoveGenerator {
            track: self.services.cursor.track,
            phrase,
            generator,
        });
        self.services.status =
            format!("{name} is gone, and what was under it is back. u brings it back");
    }

    /// `K` on `Empty`: the columns of the phrase with no generator, to make one live.
    fn open_column_picker(&mut self) -> Outcome {
        let Some(phrase) = self.services.phrase_here() else {
            self.services.status = "This track has no phrase here to make live".to_owned();
            return Outcome::Done;
        };
        let generators = phrase.generators();
        let items: Vec<Item> = columns()
            .into_iter()
            .filter(|(field, _)| !generators.iter().any(|found| on_field(found, *field)))
            .map(|(field, name)| {
                Item::new(name, Pick::GenColumn(field)).meaning("Make it live over the loop")
            })
            .collect();
        if items.is_empty() {
            self.services.status = "Every column of this phrase is live".to_owned();
            return Outcome::Done;
        }
        Outcome::Open(Box::new(Picker::new("Make live", items)))
    }
}

impl App {
    /// A choice from `K`'s list, for a generator.
    pub(crate) fn pick_gen_choice(&mut self, phrase: u32, generator: usize, choice: Choice) {
        reshape_live(
            &mut self.services,
            self.modes.view,
            (phrase, generator),
            |_, shape, _| choice.apply(shape),
        );
    }

    /// `Enter` on the Gen page: open the shape panel on the generator, in the phrase view, as
    /// `gl` does there. It stays `App`'s, as it reopens the panel, which is drawn over the phrase
    /// view.
    pub(super) fn open_gen_panel(&mut self) {
        let Some((phrase, generator, _)) = gen_of(self.track_control()) else {
            self.services.status =
                "Enter opens the shape panel on a generator: go to one first".to_owned();
            return;
        };
        let found = self.song().tracks[self.services.cursor.track]
            .phrases
            .get(&phrase)
            .and_then(|found| found.generators().get(generator))
            .map(|found| (found.column, found.rows()));
        let Some((column, Some(rows))) = found else {
            return;
        };
        self.show_view(View::Phrase);
        self.services.cursor.field = field_of(column);
        self.services.cursor.step = rows.first;
        self.reopen_generator();
    }

    /// A column chosen on `Empty`, made live with the command chosen for an empty FX column,
    /// and its device the cursor's.
    pub(crate) fn pick_gen_column(&mut self, field: Field, command: Option<FxCommand>) {
        if !self.make_live(field, command) {
            return;
        }
        let Some(phrase) = self.services.phrase_here() else {
            return;
        };
        let generators = phrase.generators();
        let Some(made) = generators.iter().position(|found| on_field(found, field)) else {
            return;
        };
        let track = &self.song().tracks[self.services.cursor.track];
        let device = devices(track, self.services.phrase_id()).iter().position(|device| {
            matches!(device.title[0], Some(Control::Gen { generator, .. }) if generator == made)
        });
        if let Some(device) = device {
            self.move_control(device * DEVICE_ROWS, 0);
        }
    }
}
