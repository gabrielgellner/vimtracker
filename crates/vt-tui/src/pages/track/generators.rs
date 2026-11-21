//! The Gen page, the phrase view's second page (decision 466): the live generators of the phrase
//! under the cursor, the one `gl` reopens, a full-width device each in the phrase view's column
//! order, then an `Empty` device that makes another column live (the generators study,
//! `docs/designs/generators-page.md`, §3). A device's title is its fill and its `Where`, or
//! `Acid`'s `Pitches` and `Follow`'s `Source`, then `Vary` where it draws, and its grid the fill's
//! settings, both from the declared table, [`GenParam`]. Every turn goes through the `Reshape` the
//! shape panel's does, so the column is written again as it turns, one undo step a press.
//!
//! It's a rack, walked as the track page's pages are, so it's a [`TrackPage`] of its own, `App`'s
//! `gen_page`, always on [`Page::Gen`], with its own keys, [`TrackPage::GEN_KEYS`], under
//! `Mode::Gen`. Its edits are the track page's, [`TrackPage::gen_edit`], over the [`Services`]: `K`'s
//! lists come back in an [`Outcome::Open`] and `i` in an [`Outcome::Insert`]. `Enter` stays
//! `App`'s, as it opens the shape panel on the generator, and so do the choices made from `K`'s
//! lists, as the picker's are, a column made live building a shape panel too.

use vt_model::action::Action;
use vt_model::constants::FX_SLOTS;
use vt_model::generator::Generator;
use vt_model::notation::mini_start;
use vt_model::selection::{Field, Motion};
use vt_model::shape::params::{Choice, Typing};
use vt_model::shape::{Context, FillKind, GenParam, OwnRhythm, Shape, ShapeColumn};
use vt_model::step::FxCommand;

use crate::app::Entry;
use crate::input::bindings::Cmd;
use crate::input::mode::View;
use crate::overlays::picker::{Item, Pick, Picker};
use crate::overlays::shape::{choice_picker, dropped_said, from_named, typed_mini, typed_rhythm};
use crate::pages::{CtxMut, Outcome};
use crate::services::Services;
use crate::services::generators::{LiveGen, generator_label};
use crate::widgets::param::edit;

use super::TrackPage;
use super::following::empty_named;
use crate::controls::{Control, DEVICE_ROWS, Page};

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
pub(super) fn columns() -> Vec<(Field, String)> {
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
pub(super) fn on_field(generator: &Generator, field: Field) -> bool {
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
        named: None,
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
        // A named rhythm made `Own` starts from its text (decision 461), and a `Mini` from what's
        // in its rows (decision 462).
        let mut rhythm = from_named(live.generator.shape.place, shape.place);
        if shape.kind == FillKind::Mini
            && live.generator.shape.kind != FillKind::Mini
            && let Some(rows) = live.generator.rows()
        {
            rhythm = Some(mini_start(&live.under, rows, live.generator.column));
        }
        services.apply(&Action::Reshape {
            track,
            phrase,
            generator,
            shape,
            list: None,
            rhythm: rhythm.map(Into::into),
        });
        say_dropped(services, view, (phrase, generator));
    }
}

/// Give a live generator of the cursor's track a `Mini`'s line, typed, as one undo step if it's
/// a change; one that doesn't read, or stacks layers on its one track, is refused, saying why
/// (decision 462).
pub(super) fn reline_live(
    services: &mut Services,
    view: View,
    (phrase, generator): (u32, usize),
    text: &str,
) {
    let track = services.cursor.track;
    let Some(live) = services.live_generator(track, phrase, generator, view) else {
        return;
    };
    if let Err(why) = typed_mini(text, live.generator.column, 1, &live.names) {
        services.status = why;
        return;
    }
    let text = text.trim();
    if live.generator.rhythm() != text {
        services.apply(&Action::Reshape {
            track,
            phrase,
            generator,
            shape: live.generator.shape,
            list: None,
            rhythm: Some(text.into()),
        });
        say_dropped(services, view, (phrase, generator));
    }
}

/// The status line says how many of a live `Mini`'s hits share a row and are dropped, and the
/// `Step` that keeps them, where any are.
fn say_dropped(services: &mut Services, view: View, (phrase, generator): (u32, usize)) {
    let track = services.cursor.track;
    let Some(live) = services.live_generator(track, phrase, generator, view) else {
        return;
    };
    let rows = live.generator.rows().map_or(0, |rows| rows.len());
    let own = OwnRhythm::kept_as(
        &live.generator.shape,
        live.generator.column,
        live.generator.rhythm(),
        live.generator.kit_name(),
    );
    if let Some(said) = dropped_said(&live.generator.shape, &own, rows) {
        services.status = said;
    }
}

/// Give a live generator of the cursor's track an `Own` rhythm's text, typed, a named rhythm
/// made `Own` by it, as one undo step if it's a change (decision 461).
pub(super) fn rerhythm_live(
    services: &mut Services,
    view: View,
    (phrase, generator): (u32, usize),
    text: &str,
) {
    let track = services.cursor.track;
    let Some(live) = services.live_generator(track, phrase, generator, view) else {
        return;
    };
    let mut shape = live.generator.shape;
    if let Err(why) = typed_rhythm(&mut shape, text) {
        services.status = why.to_owned();
        return;
    }
    let text = text.trim();
    if shape != live.generator.shape || live.generator.rhythm() != text {
        services.apply(&Action::Reshape {
            track,
            phrase,
            generator,
            shape,
            list: None,
            rhythm: Some(text.into()),
        });
    }
}

/// Give a live generator of the cursor's track the `List` values `list`, as one undo step if
/// they're a change.
fn relist_live(
    services: &mut Services,
    view: View,
    (phrase, generator): (u32, usize),
    list: &[u8],
) {
    let track = services.cursor.track;
    let Some(live) = services.live_generator(track, phrase, generator, view) else {
        return;
    };
    if live.generator.list() != list {
        services.apply(&Action::Reshape {
            track,
            phrase,
            generator,
            shape: live.generator.shape,
            list: Some(list.into()),
            rhythm: None,
        });
    }
}

impl TrackPage {
    /// The Gen page's rows of the binding table, under `Mode::Gen`: the track page's moves and
    /// edits, without its pages' own, `H` going back to the grid (decision 466).
    pub(crate) const GEN_KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("j", Cmd::Move(Motion::Down), "Next row"),
        ("k", Cmd::Move(Motion::Up), "Previous row"),
        (
            "g g",
            Cmd::Move(Motion::First),
            "First row, of the first device",
        ),
        ("G", Cmd::Move(Motion::Last), "Last row, of the last device"),
        ("H", Cmd::PreviousPage, "Back to the phrase view's grid"),
        (
            "L",
            Cmd::NextPage,
            "Next page: Gen is the phrase view's last",
        ),
        ("/", Cmd::FieldJump, "Jump to a setting by its name"),
        (
            "M",
            Cmd::MapMacro,
            "Map this setting to a macro, or move it, or take it off",
        ),
        (
            "g m",
            Cmd::MacroMapping,
            "This macro's mapping, or the one this setting is on",
        ),
        ("g d", Cmd::OpenItem, "Go to the macro holding this setting"),
        (
            "Space g a",
            Cmd::ApplyGenerator,
            "Apply this generator; with Vary on, keep the pass heard",
        ),
        ("r", Cmd::RollGenerator, "Roll this generator again"),
        (
            "Enter",
            Cmd::ReopenGenerator,
            "Open the shape panel on this generator, or a text's buffer",
        ),
        ("}", Cmd::Move(Motion::NextBeat), "Next device down"),
        ("{", Cmd::Move(Motion::PreviousBeat), "Previous device up"),
        ("h", Cmd::Move(Motion::Left), "Previous setting"),
        ("l", Cmd::Move(Motion::Right), "Next setting"),
        ("0", Cmd::Move(Motion::RowStart), "First setting"),
        ("$", Cmd::Move(Motion::RowEnd), "Last setting"),
        ("=", Cmd::Increase, "Increase the value"),
        ("-", Cmd::Decrease, "Decrease the value"),
        ("+", Cmd::IncreaseLarge, "Increase the value, a large step"),
        ("_", Cmd::DecreaseLarge, "Decrease the value, a large step"),
        ("C-a", Cmd::Increase, "Increase the value"),
        ("C-x", Cmd::Decrease, "Decrease the value"),
        ("x", Cmd::ClearField, "Back to its default"),
        (
            "d d",
            Cmd::ToNothing,
            "To nothing; on a generator's Fill, take it away",
        ),
        ("K", Cmd::PickValue, "Choose this setting from a list"),
        (
            "a",
            Cmd::Append,
            "Type; on a text, its buffer typing at the end, keeping it",
        ),
        (
            "S",
            Cmd::Retype,
            "Type; on a text, its buffer typing it all again",
        ),
        (
            "c c",
            Cmd::Retype,
            "Type; on a text, its buffer typing it all again",
        ),
    ];

    /// Whether a key on the setting under the cursor, `control`, is the Gen page's own: the Gen
    /// page is on screen, `view`, and the cursor isn't on `Phrase`'s settings or a lane's
    /// `Length` and `Rate`, which turn as the track page's settings do.
    pub(super) fn on_gen(&self, view: View, control: Control) -> bool {
        view == View::Gen
            && self.page == Page::Gen
            && !matches!(
                control,
                Control::Macro(_) | Control::Lane(..) | Control::Base(_)
            )
    }

    /// Run one of the Gen page's edits, which reshape the generator setting under the cursor,
    /// `control`, `times` being its count, at least 1: `=` `-` `+` `_`, `x`, `K`, `i`,
    /// `Space g a`, `dd`, which on `Fill` takes the generator away, and `r`, and `p`, which is refused. `Enter`, which opens the shape panel
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
        // Beside the grid, `Empty` is for the grid's cursor's column when that has nothing live
        // (decision 468).
        let cursor = ctx.services.cursor;
        let named = self
            .beside
            .then(|| empty_named(ctx.services.phrase_here()?, cursor.field, cursor.step))
            .flatten();
        let mut edit = GenEdit {
            services: ctx.services,
            view: ctx.modes.view,
            control,
            named,
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
            Cmd::ToNothing => edit.set_nothing(),
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
    /// The column `Empty` names beside the grid, which `K` on it makes live.
    named: Option<Field>,
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
        let Some((phrase, generator, param)) = self.here() else {
            return;
        };
        let Some(live) = self.live(phrase, generator) else {
            return;
        };
        // A list goes back to the two ends, as it starts.
        if param == GenParam::Values {
            let ends = [live.generator.shape.from, live.generator.shape.to];
            relist_live(self.services, self.view, (phrase, generator), &ends);
            return;
        }
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
    /// columns to make live, or beside the grid, the column it names made live, as choosing it
    /// from that list does.
    fn open_picker(&mut self) -> Outcome {
        if let Control::NewGenerator(_) = self.control {
            return match self.named {
                Some(field) => Outcome::Choose(Pick::GenColumn(field)),
                None => self.open_column_picker(),
            };
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

    /// A typed value, for the generator setting under the cursor. A `List`'s values go on the
    /// generator, beside its shape.
    fn type_in(&mut self, text: &str) {
        if let Some((phrase, generator, GenParam::Values)) = self.here() {
            let Some(live) = self.live(phrase, generator) else {
                return;
            };
            match GenParam::typed_list(live.generator.column, text) {
                Ok(list) => relist_live(self.services, self.view, (phrase, generator), &list),
                Err(why) => self.services.status = why.to_owned(),
            }
            return;
        }
        if let Some((phrase, generator, GenParam::OwnText)) = self.here() {
            rerhythm_live(self.services, self.view, (phrase, generator), text);
            return;
        }
        if let Some((phrase, generator, GenParam::MiniText)) = self.here() {
            reline_live(self.services, self.view, (phrase, generator), text);
            return;
        }
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
    /// that evolves is applied as last heard, as one a macro moves is, and so is one whose
    /// `Mutate` keeps rows from earlier passes (decision 430).
    fn apply(&mut self) {
        let Some((phrase, generator, _)) = self.here() else {
            self.services.status = "Space g a applies a generator: go to one first".to_owned();
            return;
        };
        let name = self.name(phrase, generator);
        if let Some(live) = self.live(phrase, generator)
            && live.generator.heard().varies()
            && !live.generator.heard().evolves()
            && !live.generator.heard().mutates()
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
    /// phrase holds it, as the Turing Machine's lock keeps the loop it has drifted to. The engine
    /// reports each pass as it's first heard; one reported from another generator, on another
    /// column, isn't kept. A `Ring` keeps its register as that pass began, so the pass heard plays
    /// first and the ring runs on from there, holding that loop once `Change` is down; an `Acid`
    /// with loops keeps the count that pass began at, so the pass heard plays first and its loops
    /// go on from there.
    fn keep_variation(&mut self, phrase: u32, generator: usize, name: &str) {
        let track = self.services.cursor.track;
        let Some(live) = self.live(phrase, generator) else {
            return;
        };
        let heard = self
            .services
            .reported
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
            list: None,
            rhythm: None,
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

    /// `dd`: the setting to nothing, as the track page's settings go, or on `Fill`, which says what
    /// the generator is, the generator taken away. A setting that's only typed, as a `List`'s values
    /// are, has no nothing, and the status line says so.
    fn set_nothing(&mut self) {
        let Some((phrase, generator, param)) = self.here() else {
            self.services.status = "Empty has nothing to clear: K makes a column live".to_owned();
            return;
        };
        if param == GenParam::Fill {
            return self.remove(phrase, generator);
        }
        let Some(live) = self.live(phrase, generator) else {
            return;
        };
        let context = live.context();
        let Some(bounds) = param.bounds(&context) else {
            self.services.status = format!("{} is only typed: x puts it back", param.name());
            return;
        };
        let centred = param.described(&live.generator.shape, &context).centred;
        let value = edit::nothing_within(bounds, centred);
        let said = self.services.status.clone();
        self.reshape(|_, param, shape, context| param.set_number(shape, context, value));
        // A shape refused says why, which stays.
        if self.services.status != said {
            return;
        }
        if let Some(live) = self.live(phrase, generator) {
            let shown = param.shown(&live.generator.shape, &live.context());
            self.services.status = format!("{} to {}", param.name(), shown.trim());
        }
    }

    /// `dd` on `Fill`: take the generator away, putting back what was under it.
    fn remove(&mut self, phrase: u32, generator: usize) {
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

impl Services {
    /// A choice from `K`'s list, for a generator.
    pub(crate) fn pick_gen_choice(
        &mut self,
        phrase: u32,
        generator: usize,
        choice: Choice,
        view: View,
    ) {
        reshape_live(self, view, (phrase, generator), |_, shape, _| {
            choice.apply(shape);
        });
    }
}

impl TrackPage {
    /// `Enter` on the Gen page, on the setting `control`: the shape panel on the generator, in
    /// the phrase view, as `gl` opens it there.
    pub(super) fn gen_panel(control: Control, services: &mut Services) -> Outcome {
        let Some((phrase, generator, _)) = gen_of(control) else {
            services.status =
                "Enter opens the shape panel on a generator: go to one first".to_owned();
            return Outcome::Done;
        };
        let found = services.song().tracks[services.cursor.track]
            .phrases
            .get(&phrase)
            .and_then(|found| found.generators().get(generator))
            .map(|found| (found.column, found.rows()));
        let Some((column, Some(rows))) = found else {
            return Outcome::Done;
        };
        Outcome::Reopen {
            field: field_of(column),
            step: rows.first,
        }
    }

    /// A column chosen on `Empty`, made live with the command chosen for an empty FX column,
    /// and its device the cursor's. An empty FX column with none chosen lists the commands.
    pub(crate) fn pick_gen_column(
        &mut self,
        field: Field,
        command: Option<FxCommand>,
        view: View,
        services: &mut Services,
    ) -> Outcome {
        if let Err(picker) = services.make_live(field, command, view) {
            return picker.map_or(Outcome::Done, Outcome::Open);
        }
        let Some(phrase) = services.phrase_here() else {
            return Outcome::Done;
        };
        let generators = phrase.generators();
        let Some(made) = generators.iter().position(|found| on_field(found, field)) else {
            return Outcome::Done;
        };
        let track = &services.song().tracks[services.cursor.track];
        let devices = Page::Gen.devices_on(track, services.phrase_id());
        let device = devices.iter().position(|device| {
            matches!(device.title[0], Some(Control::Gen { generator, .. }) if generator == made)
        });
        if let Some(device) = device {
            self.move_to(services.track_here(), device * DEVICE_ROWS, 0);
        }
        Outcome::Done
    }
}
