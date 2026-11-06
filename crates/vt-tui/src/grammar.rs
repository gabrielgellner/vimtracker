//! Counts, registers, operators, Visual modes and repeat, on top of the binding table.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, FieldValue, Fill};
use vt_model::label::is_named_slot;
use vt_model::selection::{Block, Field, Motion, Scope, Shift, StepRange, Target};
use vt_model::step::{Note, Step};

use crate::app::{App, Entry, Selection};
use crate::input::bindings::{Cmd, Op, Resolved};
use crate::input::keys::Key;
use crate::input::mode::Mode;

/// The register that holds the last copy. A cut doesn't touch it.
const YANK_REGISTER: char = '0';

/// What `.` repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Operator {
        op: Op,
        cover: Cover,
    },
    Increment {
        by: i32,
        cover: Cover,
    },
    ClearField,
    Paste {
        register: Option<char>,
        times: usize,
        merge: bool,
        /// Land just after what was pasted.
        advance: bool,
    },
    /// A field replaced, with the step that holds its new value.
    Replace {
        field: Field,
        value: Box<Step>,
        /// More than one if it filled a selection.
        steps: usize,
    },
}

/// What a repeated change covers, relative to the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cover {
    /// The field under the cursor.
    Cursor,
    /// A vertical or horizontal motion from the cursor.
    Motion(Motion, Option<usize>),
    /// A target at the cursor.
    Target(Target, Option<usize>),
    /// `count` whole steps from the cursor.
    Steps(usize),
    /// A selection's size, from the cursor, in the columns it covered.
    Sized { steps: usize, scope: Scope },
}

impl App {
    /// Handle a key in Normal, operator-pending or Visual mode.
    pub(crate) fn command_key_normal(&mut self, key: Key, now: Instant) {
        if key.code == KeyCode::Esc {
            self.reset_pending();
            if self.mode.is_visual() || self.mode == Mode::OperatorPending {
                self.mode = Mode::Normal;
            }
            return;
        }
        if self.awaiting_register {
            self.awaiting_register = false;
            if let KeyCode::Char(c) = key.code
                && (c.is_ascii_lowercase() || c == YANK_REGISTER)
            {
                self.register = Some(c);
            } else {
                self.status = "Registers are a to z, and 0 for the last copy".to_owned();
            }
            return;
        }
        if self.pending.is_empty()
            && let KeyCode::Char(c) = key.code
            && !key.ctrl
        {
            if c == '"' {
                self.awaiting_register = true;
                return;
            }
            if let Some(digit) = c.to_digit(10)
                && (digit > 0 || self.count.is_some())
            {
                let count = self.count.unwrap_or(0).saturating_mul(10);
                self.count = Some(count.saturating_add(digit as usize));
                return;
            }
        }
        if self.pending.is_empty() {
            self.pending_since = Some(now);
        }
        self.pending.push(key);
        match self.table.resolve(self.key_scope(), &self.pending) {
            Resolved::Prefix => {}
            Resolved::Unbound => {
                let keys: Vec<String> = self.pending.iter().map(Key::to_string).collect();
                self.status = format!("{} isn't bound here", keys.join(" "));
                self.reset_pending();
                if self.mode == Mode::OperatorPending {
                    self.mode = Mode::Normal;
                }
            }
            Resolved::Command(cmd) => {
                self.pending.clear();
                self.pending_since = None;
                self.status.clear();
                self.run(cmd, key, now);
            }
        }
    }

    pub(crate) fn reset_pending(&mut self) {
        self.pending.clear();
        self.pending_since = None;
        self.count = None;
        self.operator = None;
        self.register = None;
        self.awaiting_register = false;
    }

    /// Run a resolved command that belongs to the grammar. Returns false if it isn't one.
    pub(crate) fn run_grammar(&mut self, cmd: Cmd) -> bool {
        match cmd {
            Cmd::Move(motion) => self.motion(motion),
            Cmd::Operator(op) => self.operator_key(op),
            Cmd::WholeStep => {
                let steps = self.take_count().unwrap_or(1).max(1);
                self.finish_operator(Cover::Steps(steps));
            }
            Cmd::On(target) => self.target(target),
            Cmd::ClearField => {
                self.do_change(&Change::ClearField);
                self.last_change = Some(Change::ClearField);
                self.count = None;
            }
            Cmd::Paste | Cmd::PasteAfter | Cmd::PasteMerge => {
                let change = Change::Paste {
                    register: self.register,
                    times: self.take_count().unwrap_or(1).max(1),
                    merge: cmd == Cmd::PasteMerge,
                    advance: cmd == Cmd::PasteAfter,
                };
                self.do_change(&change);
                self.last_change = Some(change);
                self.reset_pending();
            }
            Cmd::Increase => self.increment(1),
            Cmd::Decrease => self.increment(-1),
            Cmd::Repeat => self.repeat(),
            Cmd::VisualField => self.enter_visual(Mode::VisualField),
            Cmd::VisualStep => self.enter_visual(Mode::VisualStep),
            Cmd::Reselect => self.reselect(),
            _ => return false,
        }
        true
    }

    fn take_count(&mut self) -> Option<usize> {
        let count = self.count.take();
        match (self.operator_count.take(), count) {
            (Some(a), Some(b)) => Some(a.saturating_mul(b)),
            (a, b) => a.or(b),
        }
    }

    fn motion(&mut self, motion: Motion) {
        if self.mode == Mode::OperatorPending {
            let count = self.take_count();
            self.finish_operator(Cover::Motion(motion, count));
            return;
        }
        let count = self.count.take();
        if motion.is_horizontal() {
            for _ in 0..count.unwrap_or(1) {
                self.move_horizontal(motion);
            }
        } else {
            self.cursor.step = motion.step(self.cursor.step, count, self.extent());
        }
    }

    fn operator_key(&mut self, op: Op) {
        if self.mode.is_visual() {
            let selection = self.current_selection();
            self.count = None;
            self.apply_operator(op, selection.range, selection.scope);
            self.last_change = Some(Change::Operator {
                op,
                cover: Cover::Sized {
                    steps: selection.range.len(),
                    scope: selection.scope,
                },
            });
            self.leave_visual(selection);
            return;
        }
        self.operator = Some(op);
        self.operator_count = self.count.take();
        self.mode = Mode::OperatorPending;
    }

    fn target(&mut self, target: Target) {
        if self.mode.is_visual() {
            let count = self.count.take();
            let (range, scope) = self.target_range(target, count);
            self.anchor = Some((range.first, scope));
            self.cursor.step = range.last;
            self.mode = if scope.is_whole() {
                Mode::VisualStep
            } else {
                Mode::VisualField
            };
            self.selection_scope = Some(scope);
            return;
        }
        let count = self.take_count();
        self.finish_operator(Cover::Target(target, count));
    }

    fn target_range(&self, target: Target, count: Option<usize>) -> (StepRange, Scope) {
        target.resolve(
            self.cursor.step,
            self.cursor.field.group(),
            count,
            self.extent(),
        )
    }

    /// The steps and columns a cover means at the cursor.
    fn resolve_cover(&self, cover: Cover) -> (StepRange, Scope) {
        let step = self.cursor.step;
        let extent = self.extent();
        let column = Scope::column(self.cursor.field.group());
        match cover {
            Cover::Cursor => (StepRange::between(step, step), column),
            Cover::Motion(Motion::RowStart, _) => (
                StepRange::between(step, step),
                Scope {
                    first: 0,
                    last: self.cursor.field.group(),
                },
            ),
            Cover::Motion(Motion::RowEnd, _) => (
                StepRange::between(step, step),
                Scope {
                    first: self.cursor.field.group(),
                    last: 2 + self.slots(),
                },
            ),
            Cover::Motion(motion, count) => (
                StepRange::between(step, motion.step(step, count, extent)),
                column,
            ),
            Cover::Target(target, count) => self.target_range(target, count),
            Cover::Steps(steps) => (
                StepRange::between(step, (step + steps - 1).min(extent.last_stored())),
                Scope::WHOLE,
            ),
            Cover::Sized { steps, scope } => (
                StepRange::between(step, (step + steps - 1).min(extent.last_stored())),
                scope,
            ),
        }
    }

    fn finish_operator(&mut self, cover: Cover) {
        let Some(op) = self.operator.take() else {
            return;
        };
        self.mode = Mode::Normal;
        self.count = None;
        self.operator_count = None;
        let (range, scope) = self.resolve_cover(cover);
        self.apply_operator(op, range, scope);
        self.register = None;
        if op != Op::Copy {
            self.last_change = Some(Change::Operator { op, cover });
        }
    }

    /// Do what an operator means, on a resolved range.
    fn apply_operator(&mut self, op: Op, range: StepRange, scope: Scope) {
        let Some(phrase) = self.phrase_id() else {
            return;
        };
        match op {
            Op::Clear => {
                self.copy_block(phrase, range, scope, false);
                self.apply(&Action::Clear {
                    phrase,
                    range,
                    scope,
                });
            }
            Op::Copy => self.copy_block(phrase, range, scope, true),
            Op::ShiftUp | Op::ShiftDown => {
                let by = if op == Op::ShiftUp { 1 } else { -1 };
                self.apply(&Action::Shift {
                    phrase,
                    range,
                    scope,
                    by,
                    kind: Shift::Operator,
                });
            }
        }
        self.register = None;
    }

    /// Fill the unnamed register, and a named one if chosen. Only a copy reaches register `0`, so
    /// a cut can never overwrite it.
    fn copy_block(&mut self, phrase: u32, range: StepRange, scope: Scope, copying: bool) {
        let Some(source) = self.song().phrases.get(&phrase).cloned() else {
            return;
        };
        let steps = source.steps()[range.first..=range.last].to_vec();
        let block = Block { scope, steps };
        if let Some(name) = self.register.filter(|name| *name != YANK_REGISTER) {
            self.registers.insert(name, block.clone());
        }
        if copying {
            self.registers.insert(YANK_REGISTER, block.clone());
        }
        self.registers.insert('"', block);
    }

    fn increment(&mut self, sign: i32) {
        let amount = self.count.take().unwrap_or(1) as i32;
        let by = sign.saturating_mul(amount);
        if self.mode.is_visual() {
            let selection = self.current_selection();
            self.apply_increment(by, selection.range, selection.scope);
            self.last_change = Some(Change::Increment {
                by,
                cover: Cover::Sized {
                    steps: selection.range.len(),
                    scope: selection.scope,
                },
            });
            self.leave_visual(selection);
            return;
        }
        // As on the M8, editing an empty cell starts from the value last used there.
        if self.cursor.field == Field::Instrument && self.step_instrument().is_none() {
            if let Some(phrase) = self.phrase_id() {
                self.apply(&Action::SetField {
                    phrase,
                    step: self.cursor.step,
                    value: FieldValue::Instrument(Some(self.instrument)),
                });
            }
            return;
        }
        if self.cursor.field == Field::Instrument
            && let Some(current) = self.step_instrument().filter(|id| is_named_slot(*id))
            && let Some(phrase) = self.phrase_id()
        {
            let mut id = current;
            for _ in 0..amount {
                id = self.song().next_instrument(id, sign > 0);
            }
            self.apply(&Action::SetField {
                phrase,
                step: self.cursor.step,
                value: FieldValue::Instrument(Some(id)),
            });
            self.instrument = id;
            return;
        }
        let change = Change::Increment {
            by,
            cover: Cover::Cursor,
        };
        self.do_change(&change);
        self.last_change = Some(change);
        if self.cursor.field == Field::Instrument
            && let Some(instrument) = self.step_instrument()
        {
            self.instrument = instrument;
        }
    }

    /// Wait for what replaces the field under the cursor, as Vim's `r` does: no move, and back
    /// to Normal once it's written.
    pub(crate) fn begin_replace(&mut self) {
        self.count = None;
        if self.mode.is_visual() {
            self.begin_selection_replace();
            return;
        }
        if let Field::FxValue(slot) = self.cursor.field
            && self.current_slot(slot).and_then(|s| s.command).is_none()
        {
            self.status = "Type an FX command first".to_owned();
            return;
        }
        self.entry = Entry::default();
        self.replacing = Some(self.cursor.field);
        self.mode = Mode::Replace;
    }

    /// Replace across a selection: the value is typed as usual on its first step that can take it,
    /// then goes on the rest. A velocity or instrument needs a note, and an FX value a command.
    fn begin_selection_replace(&mut self) {
        let selection = self.current_selection();
        let field = self.cursor.field;
        let Some(phrase) = self
            .phrase_id()
            .and_then(|id| self.song().phrases.get(&id).cloned())
        else {
            return;
        };
        let takes = |step: &usize| {
            let Some(step) = phrase.steps().get(*step) else {
                return false;
            };
            match field {
                Field::Velocity | Field::Instrument => matches!(step.note, Note::On(_)),
                Field::FxValue(slot) => step.fx.get(slot).is_some_and(|s| s.command.is_some()),
                Field::Note | Field::FxCommand(_) => true,
            }
        };
        let Some(first) = (selection.range.first..=selection.range.last).find(takes) else {
            self.status = match field {
                Field::FxValue(_) => "Type an FX command first",
                _ => "There are no notes in the selection",
            }
            .to_owned();
            return;
        };
        self.leave_visual(selection);
        self.cursor.step = first;
        self.cursor.field = field;
        self.entry = Entry::default();
        self.replacing = Some(field);
        self.replacing_selection = Some((selection, self.history.undo_depth()));
        self.mode = Mode::Replace;
    }

    /// Clear the field under the cursor and start Insert there, as Vim's `s` does. An FX value
    /// keeps its command for the value about to be typed, and the slot does nothing until then.
    /// As with any Insert, `.` doesn't repeat what is typed.
    pub(crate) fn substitute(&mut self) {
        self.count = None;
        let command = match self.cursor.field {
            Field::FxValue(slot) => self.current_slot(slot).and_then(|s| s.command),
            _ => None,
        };
        self.do_change(&Change::ClearField);
        self.entry = Entry {
            command,
            ..Entry::default()
        };
        self.last_entry = None;
        self.mode = Mode::Insert;
    }

    /// Back to where the replace started, and remember it for `.`.
    pub(crate) fn finish_replace(&mut self) {
        self.mode = Mode::Normal;
        let Some(field) = self.replacing.take() else {
            return;
        };
        self.cursor.field = field;
        let value = self
            .phrase_id()
            .and_then(|id| self.song().phrases.get(&id).cloned())
            .and_then(|phrase| phrase.steps().get(self.cursor.step).cloned());
        let Some(value) = value else {
            return;
        };
        let mut steps = 1;
        if let Some((selection, depth)) = self.replacing_selection.take() {
            self.fill(selection.range, field, &value);
            self.history.squash_to(depth);
            self.cursor.step = selection.range.first;
            steps = selection.range.len();
        }
        self.last_change = Some(Change::Replace {
            field,
            value: Box::new(value),
            steps,
        });
    }

    /// Write a replaced field's value across steps.
    fn fill(&mut self, range: StepRange, field: Field, value: &Step) {
        let Some(phrase) = self.phrase_id() else {
            return;
        };
        let fill = match field {
            Field::Note => Fill::Note {
                note: value.note,
                instrument: self.instrument,
                velocity: self.note_velocity,
            },
            Field::Velocity => Fill::Velocity(value.velocity),
            Field::Instrument => Fill::Instrument(value.instrument),
            Field::FxCommand(slot) => Fill::Slot(slot, value.fx[slot]),
            Field::FxValue(slot) => Fill::SlotValue(slot, value.fx[slot].value),
        };
        self.apply(&Action::Fill {
            phrase,
            range,
            fill,
        });
    }

    /// The instrument on the cursor's step.
    pub(crate) fn step_instrument(&self) -> Option<u32> {
        let id = self.phrase_id()?;
        let phrase = self.song().phrases.get(&id)?;
        phrase.steps().get(self.cursor.step)?.instrument
    }

    fn apply_increment(&mut self, by: i32, range: StepRange, scope: Scope) {
        if let Some(phrase) = self.phrase_id() {
            self.apply(&Action::Shift {
                phrase,
                range,
                scope,
                by,
                kind: Shift::Increment,
            });
        }
    }

    /// Run a recorded change at the cursor.
    fn do_change(&mut self, change: &Change) {
        match change {
            Change::Operator { op, cover } => {
                let (range, scope) = self.resolve_cover(*cover);
                self.apply_operator(*op, range, scope);
            }
            Change::Increment { by, cover } => {
                let (range, scope) = self.resolve_cover(*cover);
                self.apply_increment(*by, range, scope);
            }
            Change::ClearField => {
                let (range, scope) = self.resolve_cover(Cover::Cursor);
                if let Some(phrase) = self.phrase_id() {
                    self.apply(&Action::Clear {
                        phrase,
                        range,
                        scope,
                    });
                }
            }
            Change::Replace {
                field,
                value,
                steps,
            } if *steps > 1 => {
                let last = self.extent().last_stored();
                let end = self.cursor.step.saturating_add(*steps - 1).min(last);
                self.fill(StepRange::between(self.cursor.step, end), *field, value);
            }
            Change::Replace { field, value, .. } => {
                let Some(phrase) = self.phrase_id() else {
                    return;
                };
                let step = self.cursor.step;
                let action = match field {
                    Field::Note => Action::SetNote {
                        phrase,
                        step,
                        note: value.note,
                        instrument: self.step_instrument().or(value.instrument),
                    },
                    Field::Velocity => Action::SetField {
                        phrase,
                        step,
                        value: FieldValue::Velocity(value.velocity),
                    },
                    Field::Instrument => Action::SetField {
                        phrase,
                        step,
                        value: FieldValue::Instrument(value.instrument),
                    },
                    Field::FxCommand(slot) | Field::FxValue(slot) => Action::SetField {
                        phrase,
                        step,
                        value: FieldValue::Slot(*slot, value.fx[*slot]),
                    },
                };
                self.apply(&action);
            }
            Change::Paste {
                register,
                times,
                merge,
                advance,
            } => {
                let name = register.unwrap_or('"');
                let Some(block) = self.registers.get(&name).cloned() else {
                    self.status = format!("Nothing in register {name}");
                    return;
                };
                if let Some(phrase) = self.phrase_id() {
                    let length = block.steps.len().saturating_mul(*times);
                    let before = Arc::clone(self.song());
                    self.apply(&Action::Paste {
                        phrase,
                        at: self.cursor.step,
                        group: self.cursor.field.group(),
                        block,
                        times: *times,
                        merge: *merge,
                    });
                    if *advance && !Arc::ptr_eq(&before, self.song()) {
                        self.move_step(length.min(isize::MAX as usize) as isize);
                    }
                }
            }
        }
    }

    /// `.`: repeat the last change, on the selection in Visual modes.
    fn repeat(&mut self) {
        let Some(change) = self.last_change.clone() else {
            self.status = "Nothing to repeat".to_owned();
            return;
        };
        self.count = None;
        if self.mode.is_visual() {
            let selection = self.current_selection();
            match change {
                Change::Operator { op, .. } => {
                    self.apply_operator(op, selection.range, selection.scope);
                }
                Change::Increment { by, .. } => {
                    self.apply_increment(by, selection.range, selection.scope);
                }
                Change::ClearField => {
                    self.apply_operator(Op::Clear, selection.range, selection.scope)
                }
                Change::Replace { field, value, .. } => {
                    self.fill(selection.range, field, &value);
                }
                Change::Paste { .. } => {}
            }
            self.leave_visual(selection);
        } else {
            self.do_change(&change);
        }
    }

    fn enter_visual(&mut self, mode: Mode) {
        let scope = if mode == Mode::VisualStep {
            Scope::WHOLE
        } else {
            Scope::column(self.cursor.field.group())
        };
        self.anchor = Some((self.cursor.step, scope));
        self.selection_scope = None;
        self.mode = mode;
        self.count = None;
    }

    /// The current selection in a Visual mode.
    pub fn current_selection(&self) -> Selection {
        let (start, start_scope) = self
            .anchor
            .unwrap_or((self.cursor.step, Scope::column(self.cursor.field.group())));
        let scope = match (self.mode, self.selection_scope) {
            (_, Some(fixed)) => fixed,
            (Mode::VisualStep, _) => Scope::WHOLE,
            _ => {
                let here = self.cursor.field.group();
                Scope {
                    first: start_scope.first.min(here),
                    last: start_scope.last.max(here),
                }
            }
        };
        Selection {
            range: StepRange::between(start, self.cursor.step),
            scope,
        }
    }

    fn leave_visual(&mut self, selection: Selection) {
        self.last_selection = Some(selection);
        self.cursor.step = selection.range.first;
        self.mode = Mode::Normal;
        self.anchor = None;
        self.selection_scope = None;
        self.clamp_cursor();
    }

    fn reselect(&mut self) {
        let Some(selection) = self.last_selection else {
            self.status = "No earlier selection".to_owned();
            return;
        };
        self.anchor = Some((selection.range.first, selection.scope));
        self.selection_scope = Some(selection.scope);
        self.cursor.step = selection.range.last;
        self.mode = if selection.scope.is_whole() {
            Mode::VisualStep
        } else {
            Mode::VisualField
        };
        self.clamp_cursor();
    }

    /// Move between fields or column groups, within the step's slots.
    fn move_horizontal(&mut self, motion: Motion) {
        let slots = self.slots();
        let last = 2 + slots * 2;
        let order = self.cursor.field.order();
        self.cursor.field = match motion {
            Motion::Right => Field::from_order((order + 1).min(last)),
            Motion::Left => Field::from_order(order.saturating_sub(1)),
            Motion::NextGroup => Field::of_group((self.cursor.field.group() + 1).min(2 + slots)),
            Motion::RowStart => Field::Note,
            Motion::RowEnd => Field::from_order(last),
            _ => Field::of_group(self.cursor.field.group().saturating_sub(1)),
        };
    }
}
