//! Counts, registers, operators, Visual modes and repeat, on top of the binding table.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, FieldValue, Fill};
use vt_model::bank::is_mark;
use vt_model::selection::{
    Block, FIRST_FX_ORDER, FX_GROUP, Field, Motion, Scope, Shift, StepRange, Target, first_value,
};
use vt_model::shape::{Shape, ShapeColumn};
use vt_model::step::{FxCommand, Note, Step};

use crate::app::{App, Entry, Selection, TrackSpan};
use crate::input::bindings::{Cmd, Op, Resolved};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::shaping::ShapePanel;

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
        kind: Shift,
    },
    ClearField,
    /// `~`, cycling marks from the first value covered.
    CycleMark {
        cover: Cover,
    },
    Paste {
        register: Option<char>,
        times: usize,
        merge: bool,
        /// Land just after what was pasted.
        advance: bool,
    },
    /// A column shaped, whose ends are chosen again from what's there.
    Shape {
        column: ShapeColumn,
        shape: Shape,
        steps: usize,
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
    /// A selection's size, from the cursor, in the columns and tracks it covered.
    Sized {
        steps: usize,
        scope: Scope,
        tracks: usize,
    },
}

impl App {
    /// Handle a key in Normal, operator-pending or Visual mode.
    pub(crate) fn command_key_normal(&mut self, key: Key, now: Instant) {
        if key.code == KeyCode::Esc {
            self.reset_pending();
            if self.mode.is_visual() || self.mode == Mode::OperatorPending {
                self.mode = Mode::Normal;
            }
            // On the sample page it also lets go of what the page is playing.
            if self.view == View::Sample {
                self.stop_sample_audition();
            }
            return;
        }
        if self.awaiting_mark {
            self.awaiting_mark = false;
            match key.code {
                KeyCode::Char(c) if is_mark(c) && !key.ctrl && !key.alt => self.jump_to_mark(c),
                _ => self.status = "Marks are a to z: :marks lists them".to_owned(),
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
            && !key.alt
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
                self.run(cmd);
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
        self.awaiting_mark = false;
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
            Cmd::CycleMark => self.cycle_mark_key(),
            Cmd::NextTrack if self.mode.is_visual() => self.reach_track(1),
            Cmd::PreviousTrack if self.mode.is_visual() => self.reach_track(-1),
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
            Cmd::Increase => self.increment(1, Shift::Increment),
            Cmd::Decrease => self.increment(-1, Shift::Increment),
            Cmd::IncreaseChromatic => self.increment(1, Shift::Chromatic),
            Cmd::DecreaseChromatic => self.increment(-1, Shift::Chromatic),
            Cmd::IncreaseLarge => self.increment(1, Shift::Large),
            Cmd::DecreaseLarge => self.increment(-1, Shift::Large),
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
            self.cursor.step = motion.step(self.cursor.step, count, self.motion_extent(motion));
        }
    }

    fn operator_key(&mut self, op: Op) {
        if self.mode.is_visual() {
            let selection = self.current_selection();
            self.count = None;
            self.apply_operator(op, selection.range, selection.scope, selection.tracks);
            self.last_change = Some(Change::Operator {
                op,
                cover: Cover::Sized {
                    steps: selection.range.len(),
                    scope: selection.scope,
                    tracks: selection.tracks.count(),
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
                StepRange::between(step, motion.step(step, count, self.motion_extent(motion))),
                column,
            ),
            Cover::Target(target, count) => self.target_range(target, count),
            Cover::Steps(steps) => (
                StepRange::between(step, (step + steps - 1).min(extent.last_stored())),
                Scope::WHOLE,
            ),
            Cover::Sized { steps, scope, .. } => (
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
        self.apply_operator(op, range, scope, self.span_of(cover));
        self.register = None;
        if op != Op::Copy {
            self.last_change = Some(Change::Operator { op, cover });
        }
    }

    /// The tracks a recorded change covers, from the cursor's.
    fn span_of(&self, cover: Cover) -> TrackSpan {
        let tracks = match cover {
            Cover::Sized { tracks, .. } => tracks,
            _ => 1,
        };
        let last = (self.song().tracks.len()).saturating_sub(1);
        TrackSpan {
            first: self.cursor.track,
            last: (self.cursor.track + tracks - 1).min(last),
        }
    }

    /// The phrase a track is on, level with the cursor, and where its loop starts.
    fn phrase_beside(&self, track: usize) -> Option<(u32, usize)> {
        let song = self.song();
        let id = if track == self.cursor.track {
            self.phrase_id()?
        } else {
            let chain = (*self.section_here()?.chains.get(track)?)?;
            song.tracks[track]
                .chain_position(chain, self.section_step(), self.dealer(track))?
                .phrase
        };
        Some((id, song.tracks.get(track)?.phrases.get(&id)?.start()))
    }

    /// Where these rows of the focused phrase fall on a track. Rows count from each phrase's own
    /// loop start, as they do on screen, and any a phrase doesn't have are left out.
    fn place_on(&self, track: usize, range: StepRange) -> Option<(u32, StepRange)> {
        let (phrase, start) = self.phrase_beside(track)?;
        let focus = self.extent().start as isize;
        let stored = self.song().tracks[track]
            .phrases
            .get(&phrase)?
            .steps()
            .len() as isize;
        let first = (range.first as isize - focus + start as isize).max(0);
        let last = (range.last as isize - focus + start as isize).min(stored - 1);
        (first <= last).then(|| (phrase, StepRange::between(first as usize, last as usize)))
    }

    /// Make the same edit on the phrase each of these tracks is on, as one change.
    fn edit_across(
        &mut self,
        range: StepRange,
        tracks: TrackSpan,
        edit: impl Fn(usize, u32, StepRange) -> Action,
    ) {
        let mut actions = Vec::new();
        for track in tracks.first..=tracks.last {
            if let Some((phrase, range)) = self.place_on(track, range) {
                actions.push(edit(track, phrase, range));
            }
        }
        match actions.len() {
            0 => {}
            1 => self.apply(&actions[0]),
            _ => self.apply(&Action::Batch(actions)),
        }
    }

    /// Do what an operator means, on a resolved range.
    fn apply_operator(&mut self, op: Op, range: StepRange, scope: Scope, tracks: TrackSpan) {
        match op {
            Op::Clear => {
                self.copy_block(range, scope, tracks, false);
                self.edit_across(range, tracks, |track, phrase, range| Action::Clear {
                    track,
                    phrase,
                    range,
                    scope,
                });
            }
            Op::Copy => self.copy_block(range, scope, tracks, true),
            Op::ShiftUp | Op::ShiftDown => {
                let by = if op == Op::ShiftUp { 1 } else { -1 };
                let scale = self.scale.on.then_some(self.scale);
                self.edit_across(range, tracks, |track, phrase, range| Action::Shift {
                    track,
                    phrase,
                    range,
                    scope,
                    by,
                    kind: Shift::Operator,
                    scale,
                });
            }
        }
        self.register = None;
    }

    /// Fill the unnamed register, and a named one if chosen. Only a copy reaches register `0`, so
    /// a cut can never overwrite it.
    fn copy_block(&mut self, range: StepRange, scope: Scope, tracks: TrackSpan, copying: bool) {
        let mut copied: Vec<Vec<Step>> = (tracks.first..=tracks.last)
            .map(|track| {
                self.place_on(track, range)
                    .map_or_else(Vec::new, |(phrase, range)| {
                        self.song().tracks[track].phrases[&phrase].steps()[range.first..=range.last]
                            .to_vec()
                    })
            })
            .collect();
        let steps = copied.remove(0);
        let block = Block {
            scope,
            steps,
            other_tracks: copied,
        };
        if let Some(name) = self.register.filter(|name| *name != YANK_REGISTER) {
            self.registers.insert(name, block.clone());
        }
        if copying {
            self.registers.insert(YANK_REGISTER, block.clone());
        }
        self.registers.insert('"', block);
    }

    fn cycle_mark_key(&mut self) {
        self.count = None;
        if self.mode.is_visual() {
            let selection = self.current_selection();
            self.cycle_mark(selection.range, selection.scope, selection.tracks);
            self.last_change = Some(Change::CycleMark {
                cover: Cover::Sized {
                    steps: selection.range.len(),
                    scope: selection.scope,
                    tracks: selection.tracks.count(),
                },
            });
            self.leave_visual(selection);
            return;
        }
        let change = Change::CycleMark {
            cover: Cover::Cursor,
        };
        self.do_change(&change);
        self.last_change = Some(change);
    }

    /// Move every value covered to the mark after the first one's, so a mix ends up the same.
    fn cycle_mark(&mut self, range: StepRange, scope: Scope, tracks: TrackSpan) {
        let first = (tracks.first..=tracks.last).find_map(|track| {
            let (phrase, range) = self.place_on(track, range)?;
            let steps = self.song().tracks[track].phrases.get(&phrase)?.steps();
            steps[range.first..=range.last]
                .iter()
                .find_map(|step| first_value(step, scope))
        });
        let Some(first) = first else {
            self.status = "No value to mark here, as events have no marks".to_owned();
            return;
        };
        let mark = first.mark.next();
        self.edit_across(range, tracks, |track, phrase, range| Action::SetMark {
            track,
            phrase,
            range,
            scope,
            mark,
        });
    }

    fn increment(&mut self, sign: i32, kind: Shift) {
        // On a value's fine digits, the same keys move its fine part.
        let kind = match (self.cursor.field, kind) {
            (Field::FxFine(_), Shift::Large) => Shift::FineLarge,
            (Field::FxFine(_), Shift::Increment | Shift::Chromatic) => Shift::Fine,
            // On a gate's length, they move the length rather than the pulses.
            (Field::GateLength, Shift::Large) => Shift::LengthLarge,
            (Field::GateLength, Shift::Increment | Shift::Chromatic) => Shift::Length,
            // On a nudge, they move it by 24ths of a step, or `DEL`'s sixths.
            (Field::Nudge, Shift::Large) => Shift::NudgeLarge,
            (Field::Nudge, Shift::Increment | Shift::Chromatic) => Shift::Nudge,
            _ => kind,
        };
        let amount = self.count.take().unwrap_or(1) as i32;
        let by = sign.saturating_mul(amount);
        if self.mode.is_visual() {
            let selection = self.current_selection();
            self.apply_increment(by, selection.range, selection.scope, selection.tracks, kind);
            self.last_change = Some(Change::Increment {
                by,
                cover: Cover::Sized {
                    steps: selection.range.len(),
                    scope: selection.scope,
                    tracks: selection.tracks.count(),
                },
                kind,
            });
            self.leave_visual(selection);
            return;
        }
        let change = Change::Increment {
            by,
            cover: Cover::Cursor,
            kind,
        };
        self.do_change(&change);
        self.last_change = Some(change);
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
        if let Field::FxFine(slot) = self.cursor.field
            && !self
                .current_slot(slot)
                .and_then(|s| s.command)
                .is_some_and(FxCommand::has_fine)
        {
            self.status = "Only a setting that sweeps has a fine part".to_owned();
            return;
        }
        self.entry = Entry::default();
        self.replacing = Some(self.cursor.field);
        self.mode = Mode::Replace;
    }

    /// Replace across a selection: the value is typed as usual on its first step that can take it,
    /// then goes on the rest. A velocity needs a note, and an FX value a command.
    fn begin_selection_replace(&mut self) {
        let selection = self.current_selection();
        let field = self.cursor.field;
        let Some(phrase) = self.phrase_here().cloned() else {
            return;
        };
        let takes = |step: &usize| {
            let Some(step) = phrase.steps().get(*step) else {
                return false;
            };
            match field {
                Field::Velocity | Field::Nudge | Field::GateLength => {
                    matches!(step.note, Note::On(_))
                }
                Field::FxValue(slot) => step.fx.get(slot).is_some_and(|s| s.command.is_some()),
                Field::FxFine(slot) => step
                    .fx
                    .get(slot)
                    .is_some_and(|s| s.command.is_some_and(FxCommand::has_fine)),
                Field::Note | Field::Gate | Field::FxCommand(_) => true,
            }
        };
        let Some(first) = (selection.range.first..=selection.range.last).find(takes) else {
            self.status = match field {
                Field::FxValue(_) => "Type an FX command first",
                Field::FxFine(_) => "Only a setting that sweeps has a fine part",
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
            // The fine part goes back to 00 and is typed again.
            Field::FxFine(_) => {
                self.do_change(&Change::ClearField);
                self.entry = Entry::default();
                self.mode = Mode::Insert;
                return;
            }
            _ => None,
        };
        self.do_change(&Change::ClearField);
        self.entry = Entry {
            command,
            ..Entry::default()
        };
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
            .phrase_here()
            .cloned()
            .and_then(|phrase| phrase.steps().get(self.cursor.step).cloned());
        let Some(value) = value else {
            return;
        };
        let mut steps = 1;
        if let Some((selection, depth)) = self.replacing_selection.take() {
            self.fill(selection.range, selection.tracks, field, &value);
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
    fn fill(&mut self, range: StepRange, tracks: TrackSpan, field: Field, value: &Step) {
        let fill = match field {
            Field::Note => Fill::Note {
                note: value.note,
                velocity: self.note_velocity,
            },
            Field::Velocity => Fill::Velocity(value.velocity),
            Field::Nudge => Fill::Nudge(value.nudge),
            Field::Gate => Fill::Gate(value.gate),
            Field::GateLength => Fill::GateLength(value.length()),
            Field::FxCommand(slot) => Fill::Slot(slot, value.fx[slot]),
            Field::FxValue(slot) => Fill::SlotValue(slot, value.fx[slot].coarse()),
            Field::FxFine(slot) => Fill::SlotFine(slot, value.fx[slot].fine()),
        };
        self.edit_across(range, tracks, |track, phrase, range| Action::Fill {
            track,
            phrase,
            range,
            fill,
        });
    }

    fn apply_increment(
        &mut self,
        by: i32,
        range: StepRange,
        scope: Scope,
        tracks: TrackSpan,
        kind: Shift,
    ) {
        let scale = self.scale.on.then_some(self.scale);
        self.edit_across(range, tracks, |track, phrase, range| Action::Shift {
            track,
            phrase,
            range,
            scope,
            by,
            kind,
            scale,
        });
    }

    /// Run a recorded change at the cursor.
    fn do_change(&mut self, change: &Change) {
        match change {
            Change::Operator { op, cover } => {
                let (range, scope) = self.resolve_cover(*cover);
                self.apply_operator(*op, range, scope, self.span_of(*cover));
            }
            Change::Increment { by, cover, kind } => {
                let (range, scope) = self.resolve_cover(*cover);
                self.apply_increment(*by, range, scope, self.span_of(*cover), *kind);
            }
            Change::CycleMark { cover } => {
                let (range, scope) = self.resolve_cover(*cover);
                self.cycle_mark(range, scope, self.span_of(*cover));
            }
            // `x` on a value's fine digits sets them back to 00, and keeps the value.
            Change::ClearField
                if !self.mode.is_visual() && matches!(self.cursor.field, Field::FxFine(_)) =>
            {
                if let Field::FxFine(slot) = self.cursor.field {
                    self.set_fine(slot, 0);
                }
            }
            // `x` on a gate's length takes the row's own away, and keeps the gate.
            Change::ClearField
                if !self.mode.is_visual() && self.cursor.field == Field::GateLength =>
            {
                self.set_length(None);
            }
            // `x` on a velocity or a nudge clears just that one, as they share a column group.
            Change::ClearField if !self.mode.is_visual() && self.cursor.field == Field::Nudge => {
                self.set_nudge(0);
            }
            Change::ClearField
                if !self.mode.is_visual() && self.cursor.field == Field::Velocity =>
            {
                self.set_field(FieldValue::Velocity(None));
            }
            Change::ClearField => {
                let (range, scope) = self.resolve_cover(Cover::Cursor);
                if let Some(phrase) = self.phrase_id() {
                    self.apply(&Action::Clear {
                        track: self.cursor.track,
                        phrase,
                        range,
                        scope,
                    });
                }
            }
            Change::Shape {
                column,
                shape,
                steps,
            } => {
                let last = self.extent().last_stored();
                let end = self.cursor.step.saturating_add(*steps - 1).min(last);
                self.shape_again(StepRange::between(self.cursor.step, end), *column, *shape);
            }
            Change::Replace {
                field,
                value,
                steps,
            } if *steps > 1 => {
                let last = self.extent().last_stored();
                let end = self.cursor.step.saturating_add(*steps - 1).min(last);
                let one = TrackSpan::between(self.cursor.track, self.cursor.track);
                self.fill(
                    StepRange::between(self.cursor.step, end),
                    one,
                    *field,
                    value,
                );
            }
            // A length goes on the row's own gate cell, which keeps its gate and pulses.
            Change::Replace {
                field: Field::GateLength,
                value,
                ..
            } => self.set_length(value.length()),
            Change::Replace { field, value, .. } => {
                let Some(phrase) = self.phrase_id() else {
                    return;
                };
                let step = self.cursor.step;
                let action = match field {
                    Field::Note => Action::SetNote {
                        track: self.cursor.track,
                        phrase,
                        step,
                        note: value.note,
                    },
                    Field::Velocity => Action::SetField {
                        track: self.cursor.track,
                        phrase,
                        step,
                        value: FieldValue::Velocity(value.velocity),
                    },
                    Field::Nudge => Action::SetField {
                        track: self.cursor.track,
                        phrase,
                        step,
                        value: FieldValue::Nudge(value.nudge),
                    },
                    Field::Gate | Field::GateLength => Action::SetField {
                        track: self.cursor.track,
                        phrase,
                        step,
                        value: FieldValue::Gate(value.gate),
                    },
                    Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => {
                        Action::SetField {
                            track: self.cursor.track,
                            phrase,
                            step,
                            value: FieldValue::Slot(*slot, value.fx[*slot]),
                        }
                    }
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
                    let group = self.cursor.field.group();
                    let row = self.cursor.step as isize - self.extent().start as isize;
                    let mut actions = vec![Action::Paste {
                        track: self.cursor.track,
                        phrase,
                        at: self.cursor.step,
                        group,
                        block: Block {
                            other_tracks: Vec::new(),
                            ..block.clone()
                        },
                        times: *times,
                        merge: *merge,
                    }];
                    // Further tracks take theirs at the same row, and past the last are dropped.
                    for (offset, steps) in block.other_tracks.iter().enumerate() {
                        let track = self.cursor.track + offset + 1;
                        if let Some((phrase, start)) = self.phrase_beside(track)
                            && let Ok(at) = usize::try_from(start as isize + row)
                        {
                            actions.push(Action::Paste {
                                track,
                                phrase,
                                at,
                                group,
                                block: Block {
                                    scope: block.scope,
                                    steps: steps.clone(),
                                    other_tracks: Vec::new(),
                                },
                                times: *times,
                                merge: *merge,
                            });
                        }
                    }
                    if actions.len() == 1 {
                        self.apply(&actions[0]);
                    } else {
                        self.apply(&Action::Batch(actions));
                    }
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
                    self.apply_operator(op, selection.range, selection.scope, selection.tracks);
                }
                Change::Increment { by, kind, .. } => {
                    self.apply_increment(
                        by,
                        selection.range,
                        selection.scope,
                        selection.tracks,
                        kind,
                    );
                }
                Change::ClearField => self.apply_operator(
                    Op::Clear,
                    selection.range,
                    selection.scope,
                    selection.tracks,
                ),
                Change::CycleMark { .. } => {
                    self.cycle_mark(selection.range, selection.scope, selection.tracks)
                }
                Change::Replace { field, value, .. } => {
                    self.fill(selection.range, selection.tracks, field, &value);
                }
                Change::Shape { column, shape, .. } => {
                    self.shape_again(selection.range, column, shape);
                }
                Change::Paste { .. } => {}
            }
            self.leave_visual(selection);
        } else {
            self.do_change(&change);
        }
    }

    /// Reach another track from a selection. The cursor stays where it is, so moving between
    /// tracks of different lengths doesn't change which rows are selected.
    fn reach_track(&mut self, by: isize) {
        let last = self.song().tracks.len() as isize - 1;
        let from = self.anchor_track.unwrap_or(self.cursor.track) as isize;
        self.anchor_track = Some((from + by).clamp(0, last) as usize);
    }

    fn enter_visual(&mut self, mode: Mode) {
        let scope = if mode == Mode::VisualStep {
            Scope::WHOLE
        } else {
            Scope::column(self.cursor.field.group())
        };
        self.anchor = Some((self.cursor.step, scope));
        self.anchor_track = Some(self.cursor.track);
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
        let anchor_track = self.anchor_track.unwrap_or(self.cursor.track);
        Selection {
            range: StepRange::between(start, self.cursor.step),
            scope,
            tracks: TrackSpan::between(anchor_track, self.cursor.track),
        }
    }

    /// Finish selecting, for something that goes on to use the selection.
    pub(crate) fn leave_selection(&mut self, selection: Selection) {
        self.leave_visual(selection);
    }

    /// Remember a kept shape for `.`.
    pub(crate) fn remember_shape(&mut self, panel: &ShapePanel) {
        self.last_change = Some(Change::Shape {
            column: panel.column,
            shape: panel.shape,
            steps: panel.selection.range.len(),
        });
    }

    fn leave_visual(&mut self, selection: Selection) {
        self.last_selection = Some(selection);
        self.cursor.step = selection.range.first;
        self.mode = Mode::Normal;
        self.anchor = None;
        self.anchor_track = None;
        self.move_track(selection.tracks.first as isize - self.cursor.track as isize);
        self.selection_scope = None;
        self.clamp_cursor();
    }

    fn reselect(&mut self) {
        let Some(selection) = self.last_selection else {
            self.status = "No earlier selection".to_owned();
            return;
        };
        self.anchor = Some((selection.range.first, selection.scope));
        self.anchor_track = Some(selection.tracks.last);
        self.selection_scope = Some(selection.scope);
        self.move_track(selection.tracks.first as isize - self.cursor.track as isize);
        self.cursor.step = selection.range.last;
        self.mode = if selection.scope.is_whole() {
            Mode::VisualStep
        } else {
            Mode::VisualField
        };
        self.clamp_cursor();
    }

    /// Move between fields, or between the step's parts: the note with its velocity and gate,
    /// then each FX slot, as the headings over the grid name them.
    fn move_horizontal(&mut self, motion: Motion) {
        let slots = self.slots();
        let last = FIRST_FX_ORDER - 1 + slots * 3;
        let order = self.cursor.field.order();
        let group = self.cursor.field.group();
        // A value with no fine part has no fine digits to land on, so they're passed over.
        let landable = |field: Field| match field {
            Field::FxFine(slot) => self
                .current_slot(slot)
                .and_then(|cell| cell.command)
                .is_some_and(FxCommand::has_fine),
            _ => true,
        };
        let step = |from: usize, forward: bool| {
            let next = if forward {
                (from + 1).min(last)
            } else {
                from.saturating_sub(1)
            };
            let field = Field::from_order(next);
            if landable(field) {
                field
            } else if forward && next < last {
                Field::from_order(next + 1)
            } else {
                Field::from_order(next.saturating_sub(1))
            }
        };
        self.cursor.field = match motion {
            Motion::Right => step(order, true),
            Motion::Left => step(order, false),
            Motion::NextGroup if slots == 0 => self.cursor.field,
            Motion::NextGroup => {
                Field::of_group(group.max(FX_GROUP - 1).saturating_add(1).min(2 + slots))
            }
            Motion::RowStart => Field::Note,
            Motion::RowEnd if landable(Field::from_order(last)) => Field::from_order(last),
            Motion::RowEnd => Field::from_order(last - 1),
            _ if group > FX_GROUP => Field::of_group(group - 1),
            _ => Field::Note,
        };
    }
}
