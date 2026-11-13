//! The Gen page: the live generators of the phrase under the cursor, the one `gl` reopens, a
//! full-width device each in the phrase view's column order, then an `Empty` device that makes
//! another column live (the generators study, `docs/designs/generators-page.md`, §3). A device's
//! title is its fill and its `Where`, or `Acid`'s `Pitches` and `Follow`'s `Source`, then `Vary`
//! where it draws, and its grid the fill's settings, both from the declared table, [`GenParam`].
//! Every turn goes through the `Reshape` the shape panel's does, so the column is written again as
//! it turns, one undo step a press.

use vt_model::action::Action;
use vt_model::constants::FX_SLOTS;
use vt_model::generator::Generator;
use vt_model::grid::GRID_ROWS;
use vt_model::selection::Field;
use vt_model::shape::params::{Choice, Typing};
use vt_model::shape::{Context, GenParam, Shape, ShapeColumn, is_empty};
use vt_model::song::Track;
use vt_model::step::FxCommand;

use crate::app::{App, Entry};
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};
use crate::input::picker::{Item, Pick, Picker};
use crate::shaping::generator_label;

use super::{Control, DEVICE_COLUMNS, DEVICE_ROWS, DeviceSpec, DeviceWidth};

/// Where a column comes in the phrase view, which the page's devices follow: Note, Velocity,
/// Nudge, Length, then the FX columns.
fn column_order(column: ShapeColumn) -> usize {
    match column {
        ShapeColumn::Note { .. } => 0,
        ShapeColumn::Velocity => 1,
        ShapeColumn::Nudge => 2,
        ShapeColumn::Length => 3,
        ShapeColumn::Fx { slot, .. } => 4 + slot,
    }
}

/// A phrase's live generators, by their places in its list, in the order the page shows them:
/// by column as the phrase view has them, and on one column, the top one first.
pub fn order(generators: &[Generator]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..generators.len()).collect();
    order.sort_by_key(|at| {
        let generator = &generators[*at];
        let first = generator.rows().map_or(0, |rows| rows.first);
        (column_order(generator.column), first)
    });
    order
}

/// The Gen page's devices for a phrase of a track: a device each for its live generators, then
/// `Empty`. With no phrase there are none.
pub(super) fn devices(track: &Track, phrase: Option<u32>) -> Vec<DeviceSpec> {
    let Some((id, found)) = phrase.and_then(|id| Some((id, track.phrases.get(&id)?))) else {
        return Vec::new();
    };
    let generators = found.generators();
    let mut devices: Vec<DeviceSpec> = order(generators)
        .into_iter()
        .map(|at| {
            let generator = &generators[at];
            let generates = generator.rows().is_some_and(|rows| {
                is_empty(&generator.under(found.steps()), rows, generator.column)
            });
            DeviceSpec::generator(id, at, generator, generates)
        })
        .collect();
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
        let columns = GenParam::grid(shape, generates);
        let grid = std::array::from_fn(|row| {
            std::array::from_fn(|column| {
                columns.get(column).and_then(|places| control(places[row]))
            })
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

/// The phrase view's field for a column, where the cursor goes to open the panel on it.
fn field_of(column: ShapeColumn) -> Field {
    match column {
        ShapeColumn::Note { .. } => Field::Note,
        ShapeColumn::Velocity => Field::Velocity,
        ShapeColumn::Nudge => Field::Nudge,
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

impl App {
    /// The generator setting the cursor is on, on the Gen page: its phrase, its place in the
    /// phrase's list and the setting.
    pub(crate) fn gen_here(&self) -> Option<(u32, usize, GenParam)> {
        match self.track_control() {
            Control::Gen {
                phrase,
                generator,
                param,
            } => Some((phrase, generator, param)),
            _ => None,
        }
    }

    /// Give the generator the cursor is on the shape `change` makes from its own and what its
    /// settings act on, as one undo step if it's a change.
    fn reshape_here(&mut self, change: impl FnOnce(&mut App, GenParam, &mut Shape, &Context)) {
        let Some((phrase, generator, param)) = self.gen_here() else {
            return;
        };
        let track = self.cursor.track;
        let Some(live) = self.live_generator(track, phrase, generator) else {
            return;
        };
        let mut shape = live.generator.shape;
        change(self, param, &mut shape, &live.context());
        live.settle(&mut shape);
        if shape != live.generator.shape {
            self.apply(&Action::Reshape {
                track,
                phrase,
                generator,
                shape,
            });
        }
    }

    /// `=` `-` `+` `_`, `count` times.
    pub(super) fn adjust_gen(&mut self, up: bool, count: usize, large: bool) {
        let sign = if up { 1 } else { -1 };
        self.reshape_here(|app, param, shape, context| {
            for _ in 0..count {
                if let Err(why) = param.step(shape, context, sign, large) {
                    app.status = why.to_owned();
                    return;
                }
            }
        });
    }

    /// `x`: the setting back to what it starts as.
    pub(super) fn reset_gen(&mut self) {
        let Some((phrase, generator, _)) = self.gen_here() else {
            return;
        };
        let Some(live) = self.live_generator(self.cursor.track, phrase, generator) else {
            return;
        };
        let starts = live.starts;
        self.reshape_here(|_, param, shape, context| param.reset(shape, context, starts));
    }

    /// `r`: roll the generator's seed again, from anywhere in its device, as the panel does.
    pub(super) fn roll_gen(&mut self) {
        if self.gen_here().is_none() {
            self.status = "r rolls a generator again: go to one first".to_owned();
            return;
        }
        self.reshape_here(|app, _, shape, _| app.roll(shape));
    }

    /// `K`: list a list setting's choices; one that's off or on switches. On `Empty`, the
    /// columns to make live.
    pub(super) fn open_gen_picker(&mut self) {
        if let Control::NewGenerator(_) = self.track_control() {
            self.open_column_picker();
            return;
        }
        let Some((phrase, generator, param)) = self.gen_here() else {
            return;
        };
        let Some(live) = self.live_generator(self.cursor.track, phrase, generator) else {
            return;
        };
        let context = live.context();
        if param.typing(context.column) == Typing::Toggled {
            self.adjust_gen(true, 1, false);
            return;
        }
        let Some(choices) = param.choices(&context) else {
            self.status = format!("{} isn't a list: = and - turn it, i types it", param.name());
            return;
        };
        let position = param.chosen(&live.generator.shape, &context).unwrap_or(0);
        let items = choices
            .into_iter()
            .map(|choice| {
                let item = Item::new(
                    choice.name(),
                    Pick::GenChoice {
                        phrase,
                        generator,
                        choice,
                    },
                );
                match choice.description() {
                    Some(meaning) => item.meaning(meaning),
                    None => item,
                }
            })
            .collect();
        self.show_picker(Picker::new(param.name(), items).opened_on(Some(position)));
    }

    /// A choice from `K`'s list, for a generator.
    pub(crate) fn pick_gen_choice(&mut self, phrase: u32, generator: usize, choice: Choice) {
        let track = self.cursor.track;
        let Some(live) = self.live_generator(track, phrase, generator) else {
            return;
        };
        let mut shape = live.generator.shape;
        choice.apply(&mut shape);
        live.settle(&mut shape);
        if shape != live.generator.shape {
            self.apply(&Action::Reshape {
                track,
                phrase,
                generator,
                shape,
            });
        }
    }

    /// `i`: a list's choices, a switch turned over, or a value to type.
    pub(super) fn start_gen_typing(&mut self) {
        match self.gen_typing() {
            None | Some(Typing::Listed | Typing::Toggled) => self.open_gen_picker(),
            Some(_) => {
                self.entry = Entry::default();
                self.mode = Mode::Insert;
            }
        }
    }

    /// How the generator setting under the cursor is typed, if it's one.
    pub(super) fn gen_typing(&self) -> Option<Typing> {
        let (phrase, generator, param) = self.gen_here()?;
        let found = self.song().tracks[self.cursor.track].phrases.get(&phrase)?;
        Some(param.typing(found.generators().get(generator)?.column))
    }

    /// A typed value, for the generator setting under the cursor.
    pub(super) fn type_gen(&mut self, text: &str) {
        self.reshape_here(|app, param, shape, context| {
            if let Err(why) = param.typed(shape, context, text) {
                app.status = why.to_owned();
            }
        });
    }

    /// The name of the generator the cursor is on, as the phrase's heading names it.
    fn gen_name(&self, phrase: u32, generator: usize) -> String {
        self.song().tracks[self.cursor.track]
            .phrases
            .get(&phrase)
            .and_then(|found| found.generators().get(generator))
            .map_or_else(String::new, generator_label)
    }

    /// `Space g a`: apply the generator, leaving its values as plain ones; or, for one that varies,
    /// keep the variation last heard, as its seed, with `Vary` back to `Once`, so it stays live.
    pub(super) fn apply_gen(&mut self) {
        let Some((phrase, generator, _)) = self.gen_here() else {
            self.status = "Space g a applies a generator: go to one first".to_owned();
            return;
        };
        let name = self.gen_name(phrase, generator);
        if let Some(live) = self.live_generator(self.cursor.track, phrase, generator)
            && live.generator.shape.varies()
        {
            self.keep_variation(phrase, generator, &name);
            return;
        }
        self.apply(&Action::ApplyGenerator {
            track: self.cursor.track,
            phrase,
            generator,
        });
        self.status = format!("{name} is applied, as plain values");
    }

    /// Keep the variation of a generator that varies last heard: the seeds it played that pass
    /// become its own, and `Vary` goes back to `Once`, so the pass heard plays every pass and the
    /// phrase holds it, as the Turing Machine's lock keeps the loop it has drifted to (decision
    /// 293). The engine reports each pass as it's first heard; one reported from another
    /// generator, on another column, isn't kept.
    fn keep_variation(&mut self, phrase: u32, generator: usize, name: &str) {
        let track = self.cursor.track;
        let Some(live) = self.live_generator(track, phrase, generator) else {
            return;
        };
        let heard = self
            .heard_passes
            .get(&(track, phrase, generator))
            .filter(|heard| heard.column == live.generator.column)
            .copied();
        let Some(heard) = heard else {
            self.status = format!(
                "{name} hasn't been heard varying yet: play it, then Space g a keeps the pass heard"
            );
            return;
        };
        let mut shape = live.generator.shape;
        shape.seed = heard.seed;
        shape.acid.roll = heard.roll;
        shape.vary = 0;
        live.settle(&mut shape);
        self.apply(&Action::Reshape {
            track,
            phrase,
            generator,
            shape,
        });
        self.status = format!(
            "Kept {name} as pass {} played it: Vary is Once now",
            heard.pass + 1
        );
    }

    /// `dd`: take the generator away, putting back what was under it.
    pub(super) fn remove_gen(&mut self) {
        let Some((phrase, generator, _)) = self.gen_here() else {
            self.status = "dd takes a generator away: go to one first".to_owned();
            return;
        };
        let name = self.gen_name(phrase, generator);
        self.apply(&Action::RemoveGenerator {
            track: self.cursor.track,
            phrase,
            generator,
        });
        self.status = format!("{name} is gone, and what was under it is back. u brings it back");
    }

    /// `Enter`: open the shape panel on the generator, in the phrase view, as `gl` does there.
    pub(super) fn open_gen_panel(&mut self) {
        let Some((phrase, generator, _)) = self.gen_here() else {
            self.status = "Enter opens the shape panel on a generator: go to one first".to_owned();
            return;
        };
        let found = self.song().tracks[self.cursor.track]
            .phrases
            .get(&phrase)
            .and_then(|found| found.generators().get(generator))
            .map(|found| (found.column, found.rows()));
        let Some((column, Some(rows))) = found else {
            return;
        };
        self.show_view(View::Phrase);
        self.cursor.field = field_of(column);
        self.cursor.step = rows.first;
        self.reopen_generator();
    }

    /// `K` on `Empty`: the columns of the phrase with no generator, to make one live.
    fn open_column_picker(&mut self) {
        let Some(phrase) = self.phrase_here() else {
            self.status = "This track has no phrase here to make live".to_owned();
            return;
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
            self.status = "Every column of this phrase is live".to_owned();
            return;
        }
        self.show_picker(Picker::new("Make live", items));
    }

    /// A column chosen on `Empty`, made live with the command chosen for an empty FX column,
    /// and its device the cursor's.
    pub(crate) fn pick_gen_column(&mut self, field: Field, command: Option<FxCommand>) {
        if !self.make_live(field, command) {
            return;
        }
        let Some(phrase) = self.phrase_here() else {
            return;
        };
        let generators = phrase.generators();
        let device = order(generators)
            .iter()
            .position(|at| on_field(&generators[*at], field));
        if let Some(device) = device {
            self.move_control(device * DEVICE_ROWS, 0);
        }
    }
}

impl App {
    /// A key on the Gen page that means something else there: the edits, which reshape the
    /// generator the cursor is on. Says whether it was one.
    pub(super) fn run_gen_page(&mut self, cmd: Cmd, times: usize) -> bool {
        match cmd {
            Cmd::Increase => self.adjust_gen(true, times, false),
            Cmd::Decrease => self.adjust_gen(false, times, false),
            Cmd::IncreaseLarge => self.adjust_gen(true, times, true),
            Cmd::DecreaseLarge => self.adjust_gen(false, times, true),
            Cmd::ClearField => self.reset_gen(),
            Cmd::PickValue => self.open_gen_picker(),
            Cmd::EnterInsert => self.start_gen_typing(),
            Cmd::ApplyGenerator => self.apply_gen(),
            Cmd::RemoveRow => self.remove_gen(),
            Cmd::RollGenerator => self.roll_gen(),
            Cmd::ReopenGenerator => self.open_gen_panel(),
            Cmd::Paste => self.status = "p puts an effect in a slot, on the FX page".to_owned(),
            _ => return false,
        }
        true
    }
}
