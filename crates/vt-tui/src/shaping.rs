//! The shape panel: fill a selected column with a ramp or random values, heard while it's adjusted.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, edit};
use vt_model::selection::{Field, StepRange};
use vt_model::shape::{FillKind, MAX_BEND, Shape, ShapeColumn, Where, default_ends};
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

/// A row of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeRow {
    From,
    To,
    Curve,
    S,
    Fill,
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
            Self::Where => "Where",
            Self::Amount => "Amount",
        }
    }
}

/// The open panel. The song underneath it is a preview until `Enter` keeps it.
#[derive(Debug, Clone)]
pub struct ShapePanel {
    pub selection: Selection,
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
    /// The rows on show. `Amount` only means something for some choices of rows.
    pub fn rows(&self) -> Vec<ShapeRow> {
        let mut rows = vec![
            ShapeRow::From,
            ShapeRow::To,
            ShapeRow::Curve,
            ShapeRow::S,
            ShapeRow::Fill,
            ShapeRow::Where,
        ];
        if matches!(self.shape.place, Where::Every(_) | Where::Chance(_)) {
            rows.push(ShapeRow::Amount);
        }
        rows
    }

    /// The steps underneath, as they were when the panel opened.
    pub fn base_steps(&self) -> &[vt_model::step::Step] {
        self.base
            .phrases
            .get(&self.phrase)
            .map_or(&[], |phrase| phrase.steps())
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
        if selection.scope.first != selection.scope.last {
            return Err("Shape works on one column at a time. Select with v");
        }
        let steps = self
            .phrase_id()
            .and_then(|id| self.song().phrases.get(&id))
            .map_or(&[][..], |phrase| phrase.steps());
        let rows = || (selection.range.first..=selection.range.last).filter_map(|r| steps.get(r));
        match field {
            Field::Velocity => {
                if rows().any(|step| matches!(step.note, vt_model::step::Note::On(_))) {
                    Ok(ShapeColumn::Velocity)
                } else {
                    Err("There are no notes in the selection")
                }
            }
            Field::FxCommand(slot) | Field::FxValue(slot) => rows()
                .find_map(|step| step.fx.get(slot).and_then(|fx| fx.command))
                .map(|command| ShapeColumn::Fx { slot, command })
                .ok_or("Type an FX command on a row of the selection first"),
            Field::Note => Err("Shape works on Velocity and FX columns"),
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
        let steps = base.phrases.get(&phrase).map_or(&[][..], |p| p.steps());
        let (from, to) = default_ends(steps, selection.range, column);
        self.shape_seed = self.shape_seed.wrapping_add(1);
        let panel = ShapePanel {
            selection,
            phrase,
            column,
            shape: Shape {
                from,
                to,
                curve: 0,
                s: 0,
                kind: FillKind::Ramp,
                place: column.default_where(),
                seed: base.seed.wrapping_add(self.shape_seed),
            },
            row: ShapeRow::From,
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

    /// Apply a shape to a range, with its ends chosen again from what's there.
    pub(crate) fn shape_again(&mut self, range: StepRange, column: ShapeColumn, shape: Shape) {
        let Some(phrase) = self.phrase_id() else {
            return;
        };
        let song = Arc::clone(self.song());
        let steps = song.phrases.get(&phrase).map_or(&[][..], |p| p.steps());
        let (from, to) = default_ends(steps, range, column);
        self.shape_seed = self.shape_seed.wrapping_add(1);
        let seed = song.seed.wrapping_add(self.shape_seed);
        self.apply(&Action::Shape {
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
    pub(crate) fn shape_key(&mut self, key: Key, now: Instant) {
        match key.code {
            KeyCode::Tab => return self.run(Cmd::PauseOrResume, key, now),
            KeyCode::BackTab => return self.run(Cmd::PlayFromStart, key, now),
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
                std::mem::swap(&mut panel.shape.from, &mut panel.shape.to);
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
                ShapeRow::Fill => {
                    shape.kind = match shape.kind {
                        FillKind::Ramp => FillKind::Random,
                        FillKind::Random => FillKind::Ramp,
                    };
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
                [FillKind::Ramp, FillKind::Random]
                    .map(|kind| Item {
                        label: match kind {
                            FillKind::Ramp => "Ramp  From one value to the other".to_owned(),
                            FillKind::Random => "Random  Anywhere between the two".to_owned(),
                        },
                        pick: Pick::ShapeFill(kind),
                    })
                    .to_vec(),
                usize::from(panel.shape.kind == FillKind::Random),
            ),
            ShapeRow::Where => {
                let choices = panel.column.wheres();
                let same = |choice: &Where| {
                    std::mem::discriminant(choice) == std::mem::discriminant(&panel.shape.place)
                };
                let position = choices.iter().position(same).unwrap_or(0);
                let items = choices
                    .into_iter()
                    .map(|place| Item {
                        label: place.name().to_owned(),
                        pick: Pick::ShapeWhere(place),
                    })
                    .collect();
                ("Where", items, position)
            }
            _ => {
                self.status = "K chooses from a list on Fill and Where".to_owned();
                return;
            }
        };
        let mut picker = Picker::new(title, items);
        picker.move_highlight(position as isize);
        self.picker = Some(picker);
        self.mode = Mode::Picker;
    }

    pub(crate) fn pick_shape_fill(&mut self, kind: FillKind) {
        self.change_shape(|panel| panel.shape.kind = kind);
    }

    pub(crate) fn pick_shape_where(&mut self, place: Where) {
        self.change_shape(|panel| panel.shape.place = place);
    }

    fn start_shape_typing(&mut self) {
        let Some(panel) = &self.shape else {
            return;
        };
        if matches!(panel.row, ShapeRow::Fill | ShapeRow::Where) {
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
        let Some(text) = &mut panel.typing else {
            return;
        };
        match code {
            KeyCode::Esc => panel.typing = None,
            KeyCode::Backspace => {
                text.pop();
            }
            KeyCode::Enter => self.commit_shape_typing(),
            KeyCode::Char(c) if hex && c.is_ascii_hexdigit() => {
                text.push(c.to_ascii_uppercase());
                if text.len() == 2 {
                    self.commit_shape_typing();
                }
            }
            KeyCode::Char(c)
                if !hex
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
                ShapeRow::Fill | ShapeRow::Where => {}
            }
        });
        if let Some(reason) = refused {
            self.status = reason.to_owned();
        }
    }
}
