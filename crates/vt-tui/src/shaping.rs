//! The shape panel: fill a selected column with a ramp, random values, a wave, two values in turn
//! or a short list of them, heard while it's adjusted.

use std::sync::Arc;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, edit};
use vt_model::lfo::{Rate, Shape as Wave, note_rates};
use vt_model::selection::{Field, StepRange};
use vt_model::shape::{
    DEFAULT_RATE, FillKind, MAX_BEND, MAX_VALUES, Shape, ShapeColumn, Values, WAVES, Where,
    default_ends,
};
use vt_model::song::Song;

use crate::app::{App, Selection};
use crate::input::bindings::Cmd;
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::input::picker::{Item, Pick, Picker};

/// How far a small and a large step move a curve.
const BEND_NUDGE: i16 = 5;
const BEND_JUMP: i16 = 25;
/// How many small steps a large one is for a value in hex.
const HEX_JUMP: i16 = 0x10;
/// How far a large step moves an `Every` or a `Chance`.
const EVERY_JUMP: i16 = 4;
const CHANCE_JUMP: i16 = 10;
/// How far a small and a large step move a phase, in degrees, as on an LFO.
const PHASE_NUDGE: i16 = 15;
const PHASE_JUMP: i16 = 90;
/// The longest `Run`, and how far a large step moves it.
const MAX_RUN: i16 = 256;
const RUN_JUMP: i16 = 4;
/// The longest rate that can be typed, like `64 bars`.
const RATE_TEXT: usize = 8;

/// A row of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeRow {
    From,
    To,
    Curve,
    S,
    Fill,
    Wave,
    Rate,
    Phase,
    Values,
    Run,
    Where,
    Amount,
}

impl ShapeRow {
    /// The row's name on screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::From => "From",
            Self::To => "To",
            Self::Curve => "Curve",
            Self::S => "S",
            Self::Fill => "Fill",
            Self::Wave => "Shape",
            Self::Rate => "Rate",
            Self::Phase => "Phase",
            Self::Values => "Values",
            Self::Run => "Run",
            Self::Where => "Where",
            Self::Amount => "Amount",
        }
    }
}

/// The open panel. The song underneath it is a preview until `Enter` keeps it.
#[derive(Debug, Clone)]
pub struct ShapePanel {
    pub selection: Selection,
    pub track: usize,
    pub phrase: u32,
    pub column: ShapeColumn,
    pub shape: Shape,
    pub row: ShapeRow,
    /// What's being typed into the current row.
    pub typing: Option<String>,
    /// The song from before the panel opened.
    base: Arc<Song>,
}

impl ShapePanel {
    /// The rows on show. `Fill` comes first, as it decides the rest: a wave and `Alternate` have
    /// their own settings in place of a curve, a `List` has its values in place of the two ends,
    /// and `Amount` only means something for some choices of rows.
    pub fn rows(&self) -> Vec<ShapeRow> {
        let mut rows = vec![ShapeRow::Fill];
        if self.shape.kind != FillKind::List {
            rows.extend_from_slice(&[ShapeRow::From, ShapeRow::To]);
        }
        rows.extend_from_slice(match self.shape.kind {
            FillKind::Ramp | FillKind::Random => &[ShapeRow::Curve, ShapeRow::S],
            FillKind::Wave => &[ShapeRow::Wave, ShapeRow::Rate, ShapeRow::Phase],
            FillKind::Alternate => &[ShapeRow::Run],
            FillKind::List => &[ShapeRow::Values, ShapeRow::Run],
        });
        rows.push(ShapeRow::Where);
        if matches!(self.shape.place, Where::Every(_) | Where::Chance(_)) {
            rows.push(ShapeRow::Amount);
        }
        rows
    }

    /// The steps underneath, as they were when the panel opened.
    pub fn base_steps(&self) -> &[vt_model::step::Step] {
        self.base
            .tracks
            .get(self.track)
            .and_then(|track| track.phrases.get(&self.phrase))
            .map_or(&[], |phrase| phrase.steps())
    }
}

/// The next or previous choice, going round.
fn turned<T: Copy + PartialEq>(choices: &[T], current: T, sign: i16) -> T {
    let at = choices.iter().position(|c| *c == current).unwrap_or(0);
    let step = if sign > 0 { 1 } else { choices.len() - 1 };
    choices[(at + step) % choices.len()]
}

/// A character typed into a list of values. Two hex digits make a value, and the space after it
/// comes by itself, so `704055` types `70 40 55`; a space ends a value of one digit sooner.
fn type_list(text: &mut String, c: char) {
    let count = text.split_whitespace().count();
    let last = text.rsplit(' ').next().unwrap_or("");
    if c.is_ascii_hexdigit() {
        if last.len() == 2 {
            if count == MAX_VALUES {
                return;
            }
            text.push(' ');
        }
        text.push(c.to_ascii_uppercase());
    } else if c == ' ' && !last.is_empty() && count < MAX_VALUES {
        text.push(' ');
    }
}

/// A value moved and kept in `0..=top`.
fn moved(value: i16, by: i16, low: i16, high: i16) -> i16 {
    (value + by).clamp(low, high)
}

impl App {
    /// The column a selection can fill, or why it can't.
    fn shape_column(
        &self,
        selection: Selection,
        field: Field,
    ) -> Result<ShapeColumn, &'static str> {
        if selection.range.len() < 2 {
            return Err("Select at least 2 rows to shape");
        }
        if selection.tracks.count() > 1 {
            return Err("Shape works on one track at a time");
        }
        if selection.scope.first != selection.scope.last {
            return Err("Shape works on one column at a time. Select with v");
        }
        let steps = self.phrase_here().map_or(&[][..], |phrase| phrase.steps());
        let rows = || (selection.range.first..=selection.range.last).filter_map(|r| steps.get(r));
        match field {
            Field::Velocity => {
                if rows().any(|step| matches!(step.note, vt_model::step::Note::On(_))) {
                    Ok(ShapeColumn::Velocity)
                } else {
                    Err("There are no notes in the selection")
                }
            }
            Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => rows()
                .find_map(|step| step.fx.get(slot).and_then(|fx| fx.command))
                .map(|command| ShapeColumn::Fx { slot, command })
                .ok_or("Type an FX command on a row of the selection first"),
            Field::Note | Field::Nudge | Field::Gate | Field::GateLength => {
                Err("Shape works on Velocity and FX columns")
            }
        }
    }

    /// Open the panel on the selection, and show its first result.
    pub(crate) fn open_shape(&mut self) {
        let selection = self.current_selection();
        let field = self.cursor.field;
        let column = match self.shape_column(selection, field) {
            Ok(column) => column,
            Err(reason) => {
                self.status = reason.to_owned();
                return;
            }
        };
        let Some(phrase) = self.phrase_id() else {
            return;
        };
        let base = Arc::clone(self.song());
        let steps = base.tracks[self.cursor.track]
            .phrases
            .get(&phrase)
            .map_or(&[][..], |p| p.steps());
        let (from, to) = default_ends(steps, selection.range, column);
        self.shape_seed = self.shape_seed.wrapping_add(1);
        let panel = ShapePanel {
            selection,
            track: self.cursor.track,
            phrase,
            column,
            shape: Shape {
                from,
                to,
                curve: 0,
                s: 0,
                kind: FillKind::Ramp,
                wave: Wave::Sine,
                rate: DEFAULT_RATE,
                phase: 0,
                run: 1,
                values: Values::default(),
                place: column.default_where(),
                seed: base.seed.wrapping_add(self.shape_seed),
            },
            row: ShapeRow::Fill,
            typing: None,
            base,
        };
        self.leave_selection(selection);
        self.cursor.field = field;
        self.shape = Some(panel);
        self.mode = Mode::Shape;
        self.preview_shape();
    }

    /// Show the panel's result, without adding an undo step.
    fn preview_shape(&mut self) {
        let Some(panel) = &self.shape else {
            return;
        };
        let action = Action::Shape {
            track: panel.track,
            phrase: panel.phrase,
            range: panel.selection.range,
            column: panel.column,
            shape: panel.shape,
        };
        if let Ok(next) = edit(&panel.base, &action) {
            self.history.replace_current(next);
            self.publish();
        }
    }

    /// Keep what's showing, as one undo step.
    fn keep_shape(&mut self) {
        let Some(panel) = self.shape.take() else {
            return;
        };
        self.mode = Mode::Normal;
        let result = Arc::clone(self.song());
        self.history.replace_current(Arc::clone(&panel.base));
        if *result != *panel.base {
            self.history.commit(result);
            self.dirty = true;
        }
        self.remember_shape(&panel);
    }

    /// Put back what was there before the panel opened.
    fn cancel_shape(&mut self) {
        let Some(panel) = self.shape.take() else {
            return;
        };
        self.mode = Mode::Normal;
        self.history.replace_current(panel.base);
        self.publish();
    }

    /// The mode a picker goes back to.
    pub(crate) fn mode_after_picker(&self) -> Mode {
        if self.shape.is_some() {
            Mode::Shape
        } else {
            Mode::Normal
        }
    }

    /// Apply a shape to a range. A ramp's or random fill's ends are chosen again from what's there,
    /// a wave or `Alternate` keeps the two values it moves between, and a `List` keeps its list.
    pub(crate) fn shape_again(&mut self, range: StepRange, column: ShapeColumn, shape: Shape) {
        let Some(phrase) = self.phrase_id() else {
            return;
        };
        let song = Arc::clone(self.song());
        let steps = song.tracks[self.cursor.track]
            .phrases
            .get(&phrase)
            .map_or(&[][..], |p| p.steps());
        let (from, to) = match shape.kind {
            FillKind::Ramp | FillKind::Random => default_ends(steps, range, column),
            FillKind::Wave | FillKind::Alternate | FillKind::List => (shape.from, shape.to),
        };
        self.shape_seed = self.shape_seed.wrapping_add(1);
        let seed = song.seed.wrapping_add(self.shape_seed);
        self.apply(&Action::Shape {
            track: self.cursor.track,
            phrase,
            range,
            column,
            shape: Shape {
                from,
                to,
                seed,
                ..shape
            },
        });
    }

    /// Handle a key while the panel is open.
    pub(crate) fn shape_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Tab => return self.run(Cmd::PauseOrResume),
            KeyCode::BackTab => return self.run(Cmd::PlayFromStart),
            _ => {}
        }
        if self.shape.as_ref().is_some_and(|p| p.typing.is_some()) {
            self.shape_typing_key(key.code);
            return;
        }
        match key.code {
            KeyCode::Esc => self.cancel_shape(),
            KeyCode::Enter => self.keep_shape(),
            KeyCode::Char('j') | KeyCode::Down => self.move_shape_row(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_shape_row(-1),
            KeyCode::Char('=') => self.adjust_shape(1, false),
            KeyCode::Char('-') => self.adjust_shape(-1, false),
            KeyCode::Char('+') => self.adjust_shape(1, true),
            KeyCode::Char('_') => self.adjust_shape(-1, true),
            KeyCode::Char('x') => self.reset_shape_row(),
            KeyCode::Char('~') => self.change_shape(|panel| {
                let shape = &mut panel.shape;
                if shape.kind == FillKind::List {
                    shape.values = shape.values.reversed();
                } else {
                    std::mem::swap(&mut shape.from, &mut shape.to);
                }
            }),
            KeyCode::Char('r') => {
                self.shape_seed = self.shape_seed.wrapping_add(1);
                let seed = self.shape_seed;
                self.change_shape(|panel| {
                    panel.shape.seed = panel.base.seed.wrapping_add(seed);
                });
            }
            KeyCode::Char('K') => self.open_shape_picker(),
            KeyCode::Char('i') => self.start_shape_typing(),
            _ => {}
        }
    }

    fn change_shape(&mut self, change: impl FnOnce(&mut ShapePanel)) {
        if let Some(panel) = &mut self.shape {
            change(panel);
            // A list starts as the two ends, so it sounds as `Alternate` did until it's typed.
            let shape = &mut panel.shape;
            if shape.kind == FillKind::List && shape.values.is_empty() {
                shape.values = Values::new(&[shape.from, shape.to]);
            }
        }
        self.preview_shape();
    }

    fn move_shape_row(&mut self, by: isize) {
        if let Some(panel) = &mut self.shape {
            let rows = panel.rows();
            let at = rows.iter().position(|row| *row == panel.row).unwrap_or(0);
            let to = at.saturating_add_signed(by).min(rows.len() - 1);
            panel.row = rows[to];
        }
    }

    fn adjust_shape(&mut self, sign: i16, large: bool) {
        if self
            .shape
            .as_ref()
            .is_some_and(|panel| panel.row == ShapeRow::Values)
        {
            self.status = "Type the values with i, as hex values like 70 40 55".to_owned();
            return;
        }
        self.change_shape(|panel| {
            let top = i16::from(panel.column.highest());
            let shape = &mut panel.shape;
            let hex = sign * if large { HEX_JUMP } else { 1 };
            let bend = sign * if large { BEND_JUMP } else { BEND_NUDGE };
            let most = i16::from(MAX_BEND);
            match panel.row {
                ShapeRow::From => shape.from = moved(i16::from(shape.from), hex, 0, top) as u8,
                ShapeRow::To => shape.to = moved(i16::from(shape.to), hex, 0, top) as u8,
                ShapeRow::Curve => {
                    shape.curve = moved(i16::from(shape.curve), bend, -most, most) as i8;
                }
                ShapeRow::S => shape.s = moved(i16::from(shape.s), bend, -most, most) as i8,
                ShapeRow::Fill => shape.kind = turned(&FillKind::ALL, shape.kind, sign),
                ShapeRow::Wave => shape.wave = turned(&WAVES, shape.wave, sign),
                ShapeRow::Rate if large => shape.rate = shape.rate.jumped(sign > 0),
                ShapeRow::Rate => shape.rate = shape.rate.nudged(sign > 0),
                ShapeRow::Phase => {
                    let by = sign * if large { PHASE_JUMP } else { PHASE_NUDGE };
                    shape.phase = (shape.phase as i16 + by).rem_euclid(360) as u16;
                }
                ShapeRow::Values => {}
                ShapeRow::Run => {
                    let by = sign * if large { RUN_JUMP } else { 1 };
                    shape.run = moved(shape.run as i16, by, 1, MAX_RUN) as u16;
                }
                ShapeRow::Where => {
                    let choices = panel.column.wheres();
                    let same = |choice: &Where| {
                        std::mem::discriminant(choice) == std::mem::discriminant(&shape.place)
                    };
                    let at = choices.iter().position(same).unwrap_or(0);
                    let step = if sign > 0 { 1 } else { choices.len() - 1 };
                    shape.place = choices[(at + step) % choices.len()];
                }
                ShapeRow::Amount => match shape.place {
                    Where::Every(every) => {
                        let by = sign * if large { EVERY_JUMP } else { 1 };
                        shape.place = Where::Every(moved(every as i16, by, 1, 256) as u16);
                    }
                    Where::Chance(percent) => {
                        let by = sign * if large { CHANCE_JUMP } else { 1 };
                        shape.place = Where::Chance(moved(i16::from(percent), by, 0, 100) as u8);
                    }
                    _ => {}
                },
            }
        });
    }

    /// Put the current row back to what it started as.
    fn reset_shape_row(&mut self) {
        self.change_shape(|panel| {
            let (from, to) = default_ends(panel.base_steps(), panel.selection.range, panel.column);
            let default_where = panel.column.default_where();
            let shape = &mut panel.shape;
            match panel.row {
                ShapeRow::From => shape.from = from,
                ShapeRow::To => shape.to = to,
                ShapeRow::Curve => shape.curve = 0,
                ShapeRow::S => shape.s = 0,
                ShapeRow::Fill => shape.kind = FillKind::Ramp,
                ShapeRow::Wave => shape.wave = Wave::Sine,
                ShapeRow::Rate => shape.rate = DEFAULT_RATE,
                ShapeRow::Phase => shape.phase = 0,
                ShapeRow::Values => shape.values = Values::new(&[shape.from, shape.to]),
                ShapeRow::Run => shape.run = 1,
                ShapeRow::Where => shape.place = default_where,
                ShapeRow::Amount => {
                    shape.place = match shape.place {
                        Where::Every(_) => Where::Every(4),
                        Where::Chance(_) => Where::Chance(50),
                        place => place,
                    };
                }
            }
            if !panel.rows().contains(&panel.row) {
                panel.row = ShapeRow::Where;
            }
        });
    }

    fn open_shape_picker(&mut self) {
        let Some(panel) = &self.shape else {
            return;
        };
        let (title, items, position): (&'static str, Vec<Item>, usize) = match panel.row {
            ShapeRow::Fill => (
                "Fill",
                FillKind::ALL
                    .map(|kind| {
                        Item::new(kind.name(), Pick::ShapeFill(kind)).meaning(kind.description())
                    })
                    .to_vec(),
                FillKind::ALL
                    .iter()
                    .position(|kind| *kind == panel.shape.kind)
                    .unwrap_or(0),
            ),
            ShapeRow::Wave => (
                "Shape",
                WAVES
                    .map(|wave| Item::new(wave.name(), Pick::ShapeWave(wave)))
                    .to_vec(),
                WAVES
                    .iter()
                    .position(|wave| *wave == panel.shape.wave)
                    .unwrap_or(0),
            ),
            ShapeRow::Rate => {
                let rates = note_rates();
                let position = rates
                    .iter()
                    .position(|rate| *rate == panel.shape.rate)
                    .unwrap_or(0);
                let items = rates
                    .into_iter()
                    .map(|rate| Item::new(rate.to_string(), Pick::ShapeRate(rate)))
                    .collect();
                ("Rate", items, position)
            }
            ShapeRow::Where => {
                let choices = panel.column.wheres();
                let same = |choice: &Where| {
                    std::mem::discriminant(choice) == std::mem::discriminant(&panel.shape.place)
                };
                let position = choices.iter().position(same).unwrap_or(0);
                let items = choices
                    .into_iter()
                    .map(|place| Item::new(place.name(), Pick::ShapeWhere(place)))
                    .collect();
                ("Where", items, position)
            }
            _ => {
                self.status = "K chooses from a list on Fill, Shape, Rate and Where".to_owned();
                return;
            }
        };
        self.show_picker(Picker::new(title, items).opened_on(Some(position)));
    }

    pub(crate) fn pick_shape_fill(&mut self, kind: FillKind) {
        self.change_shape(|panel| panel.shape.kind = kind);
    }

    pub(crate) fn pick_shape_wave(&mut self, wave: Wave) {
        self.change_shape(|panel| panel.shape.wave = wave);
    }

    pub(crate) fn pick_shape_rate(&mut self, rate: Rate) {
        self.change_shape(|panel| panel.shape.rate = rate);
    }

    pub(crate) fn pick_shape_where(&mut self, place: Where) {
        self.change_shape(|panel| panel.shape.place = place);
    }

    fn start_shape_typing(&mut self) {
        let Some(panel) = &self.shape else {
            return;
        };
        if matches!(panel.row, ShapeRow::Fill | ShapeRow::Wave | ShapeRow::Where) {
            self.open_shape_picker();
        } else if let Some(panel) = &mut self.shape {
            panel.typing = Some(String::new());
        }
    }

    fn shape_typing_key(&mut self, code: KeyCode) {
        let Some(panel) = &mut self.shape else {
            return;
        };
        let hex = matches!(panel.row, ShapeRow::From | ShapeRow::To);
        let signed = matches!(panel.row, ShapeRow::Curve | ShapeRow::S);
        let rate = panel.row == ShapeRow::Rate;
        let list = panel.row == ShapeRow::Values;
        let Some(text) = &mut panel.typing else {
            return;
        };
        match code {
            KeyCode::Esc => panel.typing = None,
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Enter => self.commit_shape_typing(),
            KeyCode::Char(c) if list => type_list(text, c),
            KeyCode::Char(c) if hex && c.is_ascii_hexdigit() => {
                text.push(c.to_ascii_uppercase());
                if text.len() == 2 {
                    self.commit_shape_typing();
                }
            }
            KeyCode::Char(c)
                if rate
                    && text.len() < RATE_TEXT
                    && (c.is_ascii_alphanumeric() || matches!(c, '/' | ' ')) =>
            {
                text.push(c);
            }
            KeyCode::Char(c)
                if !hex
                    && !rate
                    && text.len() < 4
                    && (c.is_ascii_digit()
                        || (signed && text.is_empty() && matches!(c, '-' | '+'))) =>
            {
                text.push(c);
            }
            _ => {}
        }
    }

    fn commit_shape_typing(&mut self) {
        let Some(text) = self.shape.as_mut().and_then(|panel| panel.typing.take()) else {
            return;
        };
        let mut refused = None;
        self.change_shape(|panel| {
            let top = panel.column.highest();
            let shape = &mut panel.shape;
            match panel.row {
                ShapeRow::From | ShapeRow::To => {
                    if let Ok(value) = u8::from_str_radix(&text, 16) {
                        let value = value.min(top);
                        if panel.row == ShapeRow::From {
                            shape.from = value;
                        } else {
                            shape.to = value;
                        }
                    }
                }
                ShapeRow::Curve | ShapeRow::S => match text.parse::<i16>() {
                    Ok(value) => {
                        let most = i16::from(MAX_BEND);
                        let value = value.clamp(-most, most) as i8;
                        if panel.row == ShapeRow::Curve {
                            shape.curve = value;
                        } else {
                            shape.s = value;
                        }
                    }
                    Err(_) => refused = Some("Curve and S run from -100 to 100"),
                },
                ShapeRow::Amount => match (shape.place, text.parse::<u16>()) {
                    (Where::Every(_), Ok(every)) if every > 0 => {
                        shape.place = Where::Every(every);
                    }
                    (Where::Chance(_), Ok(percent)) => {
                        shape.place = Where::Chance(percent.min(100) as u8);
                    }
                    _ => refused = Some("That amount isn't a whole number above 0"),
                },
                // A wave here is laid out by rows, so a rate has to be in time with them.
                ShapeRow::Rate => match text.parse::<Rate>() {
                    Ok(Rate::Hertz(_)) | Err(_) => {
                        refused = Some("A rate here is like 1/4, 1/8d, 1/16t, 2 bars or 7st");
                    }
                    Ok(rate) => shape.rate = rate,
                },
                ShapeRow::Phase => match text.parse::<u16>() {
                    Ok(phase) if phase < 360 => shape.phase = phase,
                    _ => refused = Some("Phase runs from 0 to 359"),
                },
                ShapeRow::Run => match text.parse::<i16>() {
                    Ok(run) if run > 0 => shape.run = run.min(MAX_RUN) as u16,
                    _ => refused = Some("Run is a whole number above 0"),
                },
                ShapeRow::Values => {
                    let typed: Option<Vec<u8>> = text
                        .split_whitespace()
                        .map(|value| u8::from_str_radix(value, 16).ok().map(|v| v.min(top)))
                        .collect();
                    match typed {
                        Some(list) if !list.is_empty() => shape.values = Values::new(&list),
                        _ => refused = Some("Values are hex values like 70 40 55, up to 8 of them"),
                    }
                }
                ShapeRow::Fill | ShapeRow::Wave | ShapeRow::Where => {}
            }
        });
        if let Some(reason) = refused {
            self.status = reason.to_owned();
        }
    }
}
