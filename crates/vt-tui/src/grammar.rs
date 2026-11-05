//! Counts, registers, operators, Visual modes and repeat, on top of the binding table.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::Action;
use vt_model::selection::{Block, Field, Motion, Scope, Shift, StepRange, Target};

use crate::app::{App, Selection};
use crate::input::bindings::{Cmd, Op, Resolved};
use crate::input::keys::Key;
use crate::input::mode::Mode;

/// What `.` repeats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Operator { op: Op, cover: Cover },
    Increment { by: i32, cover: Cover },
    ClearField,
    Paste { register: Option<char> },
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
                && c.is_ascii_lowercase()
            {
                self.register = Some(c);
            } else {
                self.status = "Registers are a to z".to_owned();
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
        match self.table.resolve(self.mode, &self.pending) {
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
            Cmd::Paste => {
                let change = Change::Paste {
                    register: self.register,
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

    fn phrase_length(&self) -> usize {
        self.phrase_id()
            .and_then(|id| self.song().phrases.get(&id))
            .map_or(0, |p| p.len())
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
            self.cursor.step = motion.step(self.cursor.step, count, self.phrase_length());
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
            self.phrase_length(),
        )
    }

    /// The steps and columns a cover means at the cursor.
    fn resolve_cover(&self, cover: Cover) -> (StepRange, Scope) {
        let step = self.cursor.step;
        let length = self.phrase_length();
        let last = length.saturating_sub(1);
        let column = Scope::column(self.cursor.field.group());
        match cover {
            Cover::Cursor => (StepRange::between(step, step), column),
            Cover::Motion(motion, count) => (
                StepRange::between(step, motion.step(step, count, length)),
                column,
            ),
            Cover::Target(target, count) => self.target_range(target, count),
            Cover::Steps(steps) => (
                StepRange::between(step, (step + steps - 1).min(last)),
                Scope::WHOLE,
            ),
            Cover::Sized { steps, scope } => (
                StepRange::between(step, (step + steps - 1).min(last)),
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
                self.copy_block(phrase, range, scope);
                self.apply(&Action::Clear {
                    phrase,
                    range,
                    scope,
                });
            }
            Op::Copy => self.copy_block(phrase, range, scope),
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

    /// Copy into the unnamed register, and a named one if chosen.
    fn copy_block(&mut self, phrase: u32, range: StepRange, scope: Scope) {
        let Some(source) = self.song().phrases.get(&phrase).cloned() else {
            return;
        };
        let steps = source.steps()[range.first..=range.last].to_vec();
        let block = Block { scope, steps };
        if let Some(name) = self.register {
            self.registers.insert(name, block.clone());
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
        let change = Change::Increment {
            by,
            cover: Cover::Cursor,
        };
        self.do_change(&change);
        self.last_change = Some(change);
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
            Change::Paste { register } => {
                let name = register.unwrap_or('"');
                let Some(block) = self.registers.get(&name).cloned() else {
                    self.status = format!("Nothing in register {name}");
                    return;
                };
                if let Some(phrase) = self.phrase_id() {
                    self.apply(&Action::Paste {
                        phrase,
                        at: self.cursor.step,
                        group: self.cursor.field.group(),
                        block,
                    });
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
            _ => Field::of_group(self.cursor.field.group().saturating_sub(1)),
        };
    }
}
