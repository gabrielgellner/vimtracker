//! Counts, registers, operators, Visual modes and repeat, on top of the binding table.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_model::action::{Action, FieldValue, Fill};
use vt_model::selection::{
    Block, Field, LARGE_NUDGE, Motion, Scope, Shift, StepRange, Target, first_value,
};
use vt_model::shape::{Shape, ShapeColumn};
use vt_model::song::Song;
use vt_model::step::{FxCommand, Note, Step};

use crate::app::{App, Cursor, Entry, Selection, TrackSpan};
use crate::input::bindings::{Cmd, Op, Resolved};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::phrase_view::PhraseView;
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
    /// A column shaped, whose ends are chosen again from what's there. The shape is boxed, as it's
    /// far larger than any other change.
    Shape {
        column: ShapeColumn,
        shape: Box<Shape>,
        steps: usize,
        tracks: usize,
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

/// What a change kept on a selection moves its values by is counted in, for the running total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Semitone,
    /// A step along the scale, which is a different number of semitones from note to note.
    Degree,
    Step,
    LargeStep,
    /// A nudge's 24ths of a sixteenth.
    Nudge,
}

impl Unit {
    /// In the order the total names them.
    const ALL: [Self; 5] = [
        Self::Semitone,
        Self::Degree,
        Self::LargeStep,
        Self::Step,
        Self::Nudge,
    ];

    /// What a change of this kind moves the selection's first column by, as a unit and how many
    /// of it each step is. A note is counted in semitones, an octave being 12, or in degrees of
    /// the scale; any other value in the small and large steps of its own column.
    fn of(kind: Shift, scope: Scope, scale_on: bool) -> (Self, i32) {
        match kind {
            Shift::Nudge => (Self::Nudge, 1),
            Shift::NudgeLarge => (Self::Nudge, LARGE_NUDGE),
            Shift::Large if scope.first == 0 => (Self::Semitone, 12),
            Shift::Chromatic if scope.first == 0 => (Self::Semitone, 1),
            Shift::Increment | Shift::Operator if scope.first == 0 && scale_on => (Self::Degree, 1),
            Shift::Increment | Shift::Operator if scope.first == 0 => (Self::Semitone, 1),
            Shift::Large | Shift::FineLarge | Shift::LengthLarge => (Self::LargeStep, 1),
            _ => (Self::Step, 1),
        }
    }

    fn name(self, plural: bool) -> &'static str {
        match (self, plural) {
            (Self::Semitone, false) => "semitone",
            (Self::Semitone, true) => "semitones",
            (Self::Degree, false) => "degree",
            (Self::Degree, true) => "degrees",
            (Self::Step, false) => "step",
            (Self::Step, true) => "steps",
            (Self::LargeStep, false) => "large step",
            (Self::LargeStep, true) => "large steps",
            (Self::Nudge, false) => "24th of a sixteenth",
            (Self::Nudge, true) => "24ths of a sixteenth",
        }
    }
}

/// One change kept on a selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Adjustment {
    Moved(Unit, i32),
    /// `r` wrote one value across it, so what moved before doesn't add to what moves after.
    Filled,
}

/// The changes made on a selection that stays until `Esc`, so the status line can say what they
/// add up to (decision 148). Each is its own undo step, and undo and redo while it's selected take
/// them back and bring them again.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Adjustments {
    /// Each undo step's changes, in order: one each, or several for a wheel's gesture.
    done: Vec<Vec<Adjustment>>,
    /// What undo took back, the latest last, with nothing for an undo step from before the
    /// selection, so redo brings them back in step with the history.
    undone: Vec<Option<Vec<Adjustment>>>,
}

impl Adjustments {
    fn record(&mut self, adjustment: Adjustment) {
        self.done.push(vec![adjustment]);
        self.undone.clear();
    }

    fn undo(&mut self) {
        let step = self.done.pop();
        self.undone.push(step);
    }

    fn redo(&mut self) {
        if let Some(Some(step)) = self.undone.pop() {
            self.done.push(step);
        }
    }

    /// How many undo steps the changes kept are.
    pub(crate) fn steps(&self) -> usize {
        self.done.len()
    }

    /// Make the changes from undo step `from` on one step, as the history squashes them, so undo
    /// takes them back together.
    pub(crate) fn join_from(&mut self, from: usize) {
        if from + 1 >= self.done.len() {
            return;
        }
        let joined = self.done.split_off(from).concat();
        self.done.push(joined);
    }

    /// What the moves since the selection started, or since its last fill, add up to, as
    /// "+11 semitones" or "+1 large step, -2 steps"; nothing if none has been made.
    pub fn total(&self) -> Option<String> {
        let done: Vec<Adjustment> = self.done.concat();
        let since = done
            .iter()
            .rposition(|adjustment| *adjustment == Adjustment::Filled)
            .map_or(0, |at| at + 1);
        let moves: Vec<(Unit, i32)> = done[since..]
            .iter()
            .filter_map(|adjustment| match adjustment {
                Adjustment::Moved(unit, by) => Some((*unit, *by)),
                Adjustment::Filled => None,
            })
            .collect();
        let (last, _) = *moves.last()?;
        let sum = |unit: Unit| -> i32 {
            moves
                .iter()
                .filter(|(of, _)| *of == unit)
                .map(|(_, by)| *by)
                .fold(0, i32::saturating_add)
        };
        let parts: Vec<String> = Unit::ALL
            .into_iter()
            .filter_map(|unit| {
                let total = sum(unit);
                (total != 0).then(|| format!("{total:+} {}", unit.name(total.abs() != 1)))
            })
            .collect();
        Some(if parts.is_empty() {
            format!("0 {}", last.name(true))
        } else {
            parts.join(", ")
        })
    }
}

/// A selection as it was, to come back to.
#[derive(Debug, Clone)]
pub struct SelectionPlace {
    mode: Mode,
    cursor: Cursor,
    anchor: Option<(usize, Scope)>,
    anchor_track: Option<usize>,
    scope: Option<Scope>,
    adjustments: Adjustments,
}

impl App {
    /// What the changes kept on the selection add up to, while it's selected.
    pub fn adjusted_total(&self) -> Option<String> {
        self.modes
            .mode
            .is_visual()
            .then(|| self.adjustments.total())
            .flatten()
    }

    /// After a change kept on the selection: count it if it changed the song, and say what the
    /// changes add up to.
    fn adjusted(&mut self, before: &Arc<Song>, adjustment: Adjustment) {
        if !Arc::ptr_eq(before, self.song()) {
            self.adjustments.record(adjustment);
        }
        self.show_adjusted();
    }

    fn show_adjusted(&mut self) {
        if let Some(total) = self.adjustments.total() {
            self.services.status = total;
        }
    }

    /// A move on the selection, counted in its unit.
    fn moved(&self, kind: Shift, by: i32, scope: Scope) -> Adjustment {
        let (unit, size) = Unit::of(kind, scope, self.scale.on);
        Adjustment::Moved(unit, by.saturating_mul(size))
    }

    /// After undo or redo while selecting: the selection stays, and its total goes back or
    /// forward with the change.
    pub(crate) fn undid_while_selecting(&mut self, undo: bool) {
        if undo {
            self.adjustments.undo();
        } else {
            self.adjustments.redo();
        }
        self.show_adjusted();
    }

    fn selection_place(&self) -> SelectionPlace {
        SelectionPlace {
            mode: self.modes.mode,
            cursor: self.services.cursor,
            anchor: self.phrase_view.anchor,
            anchor_track: self.phrase_view.anchor_track,
            scope: self.phrase_view.scope,
            adjustments: self.adjustments.clone(),
        }
    }

    fn return_to(&mut self, place: SelectionPlace) {
        self.modes.mode = place.mode;
        self.services.cursor = place.cursor;
        self.phrase_view.anchor = place.anchor;
        self.phrase_view.anchor_track = place.anchor_track;
        self.phrase_view.scope = place.scope;
        self.adjustments = place.adjustments;
        self.clamp_cursor();
    }

    /// Back to the selection a replace across it started from, as `Esc` cancelled it.
    pub(crate) fn cancel_selection_replace(&mut self) {
        if let Some(place) = self.replace_returns_to.take() {
            self.return_to(place);
        }
    }

    /// Handle a key in Normal, operator-pending or Visual mode.
    pub(crate) fn command_key_normal(&mut self, key: Key, now: Instant) {
        if key.code == KeyCode::Esc {
            self.reset_pending();
            self.song_view.anchor = None;
            // `Esc` is what ends a selection kept through its changes, and `gv` brings it back,
            // as in Vim. Its total goes with it.
            if self.modes.mode.is_visual() {
                self.phrase_view.last_selection = Some(self.current_selection());
                if self
                    .adjusted_total()
                    .is_some_and(|total| total == self.services.status)
                {
                    self.services.status.clear();
                }
                self.adjustments = Adjustments::default();
            }
            if self.modes.mode.is_visual() || self.modes.mode == Mode::OperatorPending {
                self.modes.mode = Mode::Normal;
            }
            // On the sample page it also lets go of what the page is playing, ends a selection
            // and leaves the settings row, and what was said in them goes with them.
            if self.modes.view == View::Sample {
                self.stop_sample_audition();
                if self.sample_page.anchor.is_some() || self.sample_page.setting.is_some() {
                    self.services.status.clear();
                }
                self.sample_page.end_selection();
                self.sample_page.setting = None;
            }
            return;
        }
        if let Some(wait) = self.awaiting_mark.take() {
            self.mark_key(wait, key);
            return;
        }
        if self.awaiting_register {
            self.awaiting_register = false;
            if let KeyCode::Char(c) = key.code
                && (c.is_ascii_lowercase() || c == YANK_REGISTER)
            {
                self.register = Some(c);
            } else {
                self.services.status = "Registers are a to z, and 0 for the last copy".to_owned();
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
                && (digit > 0 || self.modes.count.is_some())
            {
                let count = self.modes.count.unwrap_or(0).saturating_mul(10);
                self.modes.count = Some(count.saturating_add(digit as usize));
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
                self.services.status = format!("{} isn't bound here", keys.join(" "));
                self.reset_pending();
                if self.modes.mode == Mode::OperatorPending {
                    self.modes.mode = Mode::Normal;
                }
            }
            Resolved::Command(cmd) => {
                self.pending.clear();
                self.pending_since = None;
                self.services.status.clear();
                self.run(cmd);
            }
        }
    }

    pub(crate) fn reset_pending(&mut self) {
        self.pending.clear();
        self.pending_since = None;
        self.modes.count = None;
        self.operator = None;
        self.register = None;
        self.awaiting_register = false;
        self.awaiting_mark = None;
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
                self.modes.count = None;
            }
            Cmd::CycleMark => self.cycle_mark_key(),
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
        let count = self.modes.count.take();
        match (self.operator_count.take(), count) {
            (Some(a), Some(b)) => Some(a.saturating_mul(b)),
            (a, b) => a.or(b),
        }
    }

    fn motion(&mut self, motion: Motion) {
        if self.modes.mode == Mode::OperatorPending {
            let count = self.take_count();
            self.finish_operator(Cover::Motion(motion, count));
            return;
        }
        let count = self.modes.count.take();
        PhraseView::move_cursor(motion, count, &mut self.services);
    }

    fn operator_key(&mut self, op: Op) {
        if self.modes.mode.is_visual() {
            let selection = self.current_selection();
            self.modes.count = None;
            let before = Arc::clone(self.song());
            self.apply_operator(op, selection.range, selection.scope, selection.tracks);
            self.last_change = Some(Change::Operator {
                op,
                cover: Cover::Sized {
                    steps: selection.range.len(),
                    scope: selection.scope,
                    tracks: selection.tracks.count(),
                },
            });
            // `>` and `<` move the values, so the selection stays for more; `d`, `x` and `y` end
            // it.
            match op {
                Op::ShiftUp | Op::ShiftDown => {
                    let by = if op == Op::ShiftUp { 1 } else { -1 };
                    let moved = self.moved(Shift::Operator, by, selection.scope);
                    self.adjusted(&before, moved);
                }
                Op::Clear | Op::Copy => self.leave_visual(selection),
            }
            return;
        }
        self.operator = Some(op);
        self.operator_count = self.modes.count.take();
        self.modes.mode = Mode::OperatorPending;
    }

    fn target(&mut self, target: Target) {
        if self.modes.mode.is_visual() {
            let count = self.modes.count.take();
            let (range, scope) = self.target_range(target, count);
            self.phrase_view.anchor = Some((range.first, scope));
            self.services.cursor.step = range.last;
            self.modes.mode = if scope.is_whole() {
                Mode::VisualStep
            } else {
                Mode::VisualField
            };
            self.phrase_view.scope = Some(scope);
            return;
        }
        let count = self.take_count();
        self.finish_operator(Cover::Target(target, count));
    }

    fn target_range(&self, target: Target, count: Option<usize>) -> (StepRange, Scope) {
        target.resolve(
            self.services.cursor.step,
            self.services.cursor.field.group(),
            count,
            self.services.extent(),
        )
    }

    /// The steps and columns a cover means at the cursor.
    fn resolve_cover(&self, cover: Cover) -> (StepRange, Scope) {
        let step = self.services.cursor.step;
        let extent = self.services.extent();
        let column = Scope::column(self.services.cursor.field.group());
        match cover {
            Cover::Cursor => (StepRange::between(step, step), column),
            Cover::Motion(Motion::RowStart, _) => (
                StepRange::between(step, step),
                Scope {
                    first: 0,
                    last: self.services.cursor.field.group(),
                },
            ),
            Cover::Motion(Motion::RowEnd, _) => (
                StepRange::between(step, step),
                Scope {
                    first: self.services.cursor.field.group(),
                    last: 2 + self.slots(),
                },
            ),
            Cover::Motion(motion, count) => (
                StepRange::between(
                    step,
                    motion.step(
                        step,
                        count,
                        PhraseView::motion_extent(motion, &self.services),
                    ),
                ),
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
        self.modes.mode = Mode::Normal;
        self.modes.count = None;
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
            first: self.services.cursor.track,
            last: (self.services.cursor.track + tracks - 1).min(last),
        }
    }

    /// The phrase a track is on, level with the cursor, and where its loop starts.
    pub(crate) fn phrase_beside(&self, track: usize) -> Option<(u32, usize)> {
        let song = self.song();
        let id = if track == self.services.cursor.track {
            self.services.phrase_id()?
        } else {
            let chain = (*self.services.section_here()?.chains.get(track)?)?;
            song.tracks[track]
                .chain_position(
                    chain,
                    self.services.section_step(self.modes.view),
                    self.services.dealer(track),
                )?
                .phrase
        };
        Some((id, song.tracks.get(track)?.phrases.get(&id)?.start()))
    }

    /// Where these rows of the focused phrase fall on a track. Rows count from each phrase's own
    /// loop start, as they do on screen, and any a phrase doesn't have are left out.
    pub(crate) fn place_on(&self, track: usize, range: StepRange) -> Option<(u32, StepRange)> {
        let (phrase, start) = self.phrase_beside(track)?;
        let focus = self.services.extent().start as isize;
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
            1 => self.services.apply(&actions[0]),
            _ => self.services.apply(&Action::Batch(actions)),
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
        self.modes.count = None;
        if self.modes.mode.is_visual() {
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
            self.services.status = "No value to mark here, as events have no marks".to_owned();
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
        let kind = match (self.services.cursor.field, kind) {
            (Field::FxFine(_), Shift::Large) => Shift::FineLarge,
            (Field::FxFine(_), Shift::Increment | Shift::Chromatic) => Shift::Fine,
            // On a gate's length, they move the length rather than the pulses.
            (Field::GateLength, Shift::Large) => Shift::LengthLarge,
            (Field::GateLength, Shift::Increment | Shift::Chromatic) => Shift::Length,
            // On a nudge, they move it by 24ths of a sixteenth, or sixths of one.
            (Field::Nudge, Shift::Large) => Shift::NudgeLarge,
            (Field::Nudge, Shift::Increment | Shift::Chromatic) => Shift::Nudge,
            _ => kind,
        };
        let amount = self.modes.count.take().unwrap_or(1) as i32;
        let by = sign.saturating_mul(amount);
        // On a selection, it stays selected for the next press, until `Esc`.
        if self.modes.mode.is_visual() {
            let selection = self.current_selection();
            let before = Arc::clone(self.song());
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
            let moved = self.moved(kind, by, selection.scope);
            self.adjusted(&before, moved);
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
        self.modes.count = None;
        if self.modes.mode.is_visual() {
            self.begin_selection_replace();
            return;
        }
        if let Field::FxValue(slot) = self.services.cursor.field
            && self.current_slot(slot).and_then(|s| s.command).is_none()
        {
            self.services.status = "Type an FX command first".to_owned();
            return;
        }
        if let Field::FxFine(slot) = self.services.cursor.field
            && !self
                .current_slot(slot)
                .and_then(|s| s.command)
                .is_some_and(FxCommand::has_fine)
        {
            self.services.status = "Only a setting that sweeps has a fine part".to_owned();
            return;
        }
        self.modes.entry = Entry::default();
        self.replacing = Some(self.services.cursor.field);
        self.modes.mode = Mode::Replace;
    }

    /// Replace across a selection: the value is typed as usual on its first step that can take it,
    /// then goes on the rest. A velocity needs a note, and an FX value a command.
    fn begin_selection_replace(&mut self) {
        let selection = self.current_selection();
        let field = self.services.cursor.field;
        let Some(phrase) = self.services.phrase_here().cloned() else {
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
            self.services.status = match field {
                Field::FxValue(_) => "Type an FX command first",
                Field::FxFine(_) => "Only a setting that sweeps has a fine part",
                _ => "There are no notes in the selection",
            }
            .to_owned();
            return;
        };
        // Once the value is in, or `Esc` cancels it, the selection comes back, for another.
        let place = self.selection_place();
        self.leave_visual(selection);
        self.replace_returns_to = Some(place);
        self.services.cursor.step = first;
        self.services.cursor.field = field;
        self.modes.entry = Entry::default();
        self.replacing = Some(field);
        self.replacing_selection = Some((selection, self.services.history.undo_depth()));
        self.modes.mode = Mode::Replace;
    }

    /// Clear the field under the cursor and start Insert there, as Vim's `s` does. An FX value
    /// keeps its command for the value about to be typed, and the slot does nothing until then.
    /// As with any Insert, `.` doesn't repeat what is typed.
    pub(crate) fn substitute(&mut self) {
        self.modes.count = None;
        let command = match self.services.cursor.field {
            Field::FxValue(slot) => self.current_slot(slot).and_then(|s| s.command),
            // The fine part goes back to 00 and is typed again.
            Field::FxFine(_) => {
                self.do_change(&Change::ClearField);
                self.modes.entry = Entry::default();
                self.modes.mode = Mode::Insert;
                return;
            }
            _ => None,
        };
        self.do_change(&Change::ClearField);
        self.modes.entry = Entry {
            command,
            ..Entry::default()
        };
        self.modes.mode = Mode::Insert;
    }

    /// Back to where the replace started, and remember it for `.`.
    pub(crate) fn finish_replace(&mut self) {
        self.modes.mode = Mode::Normal;
        let Some(field) = self.replacing.take() else {
            return;
        };
        self.services.cursor.field = field;
        let value = self
            .services
            .phrase_here()
            .cloned()
            .and_then(|phrase| phrase.steps().get(self.services.cursor.step).cloned());
        let Some(value) = value else {
            return;
        };
        let mut steps = 1;
        if let Some((selection, depth)) = self.replacing_selection.take() {
            self.fill(selection.range, selection.tracks, field, &value);
            self.services.history.squash_to(depth);
            self.services.cursor.step = selection.range.first;
            steps = selection.range.len();
            if let Some(place) = self.replace_returns_to.take() {
                self.return_to(place);
                self.adjustments.record(Adjustment::Filled);
            }
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
                if !self.modes.mode.is_visual()
                    && matches!(self.services.cursor.field, Field::FxFine(_)) =>
            {
                if let Field::FxFine(slot) = self.services.cursor.field {
                    self.set_fine(slot, 0);
                }
            }
            // `x` on a gate's length takes the row's own away, and keeps the gate.
            Change::ClearField
                if !self.modes.mode.is_visual()
                    && self.services.cursor.field == Field::GateLength =>
            {
                self.set_length(None);
            }
            // `x` on a velocity or a nudge clears just that one, as they share a column group.
            Change::ClearField
                if !self.modes.mode.is_visual() && self.services.cursor.field == Field::Nudge =>
            {
                self.set_nudge(0);
            }
            Change::ClearField
                if !self.modes.mode.is_visual()
                    && self.services.cursor.field == Field::Velocity =>
            {
                self.set_field(FieldValue::Velocity(None));
            }
            Change::ClearField => {
                let (range, scope) = self.resolve_cover(Cover::Cursor);
                if let Some(phrase) = self.services.phrase_id() {
                    self.services.apply(&Action::Clear {
                        track: self.services.cursor.track,
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
                tracks,
            } => {
                let last = self.services.extent().last_stored();
                let end = self
                    .services
                    .cursor
                    .step
                    .saturating_add(*steps - 1)
                    .min(last);
                let tracks = self.span_of(Cover::Sized {
                    steps: *steps,
                    scope: Scope::WHOLE,
                    tracks: *tracks,
                });
                self.shape_again(
                    StepRange::between(self.services.cursor.step, end),
                    tracks,
                    *column,
                    **shape,
                );
            }
            Change::Replace {
                field,
                value,
                steps,
            } if *steps > 1 => {
                let last = self.services.extent().last_stored();
                let end = self
                    .services
                    .cursor
                    .step
                    .saturating_add(*steps - 1)
                    .min(last);
                let one =
                    TrackSpan::between(self.services.cursor.track, self.services.cursor.track);
                self.fill(
                    StepRange::between(self.services.cursor.step, end),
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
                let Some(phrase) = self.services.phrase_id() else {
                    return;
                };
                let step = self.services.cursor.step;
                let action = match field {
                    Field::Note => Action::SetNote {
                        track: self.services.cursor.track,
                        phrase,
                        step,
                        note: value.note,
                    },
                    Field::Velocity => Action::SetField {
                        track: self.services.cursor.track,
                        phrase,
                        step,
                        value: FieldValue::Velocity(value.velocity),
                    },
                    Field::Nudge => Action::SetField {
                        track: self.services.cursor.track,
                        phrase,
                        step,
                        value: FieldValue::Nudge(value.nudge),
                    },
                    Field::Gate | Field::GateLength => Action::SetField {
                        track: self.services.cursor.track,
                        phrase,
                        step,
                        value: FieldValue::Gate(value.gate),
                    },
                    Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => {
                        Action::SetField {
                            track: self.services.cursor.track,
                            phrase,
                            step,
                            value: FieldValue::Slot(*slot, value.fx[*slot]),
                        }
                    }
                };
                self.services.apply(&action);
            }
            Change::Paste {
                register,
                times,
                merge,
                advance,
            } => {
                let name = register.unwrap_or('"');
                let Some(block) = self.registers.get(&name).cloned() else {
                    self.services.status = format!("Nothing in register {name}");
                    return;
                };
                if let Some(phrase) = self.services.phrase_id() {
                    let length = block.steps.len().saturating_mul(*times);
                    let before = Arc::clone(self.song());
                    let group = self.services.cursor.field.group();
                    let row =
                        self.services.cursor.step as isize - self.services.extent().start as isize;
                    let mut actions = vec![Action::Paste {
                        track: self.services.cursor.track,
                        phrase,
                        at: self.services.cursor.step,
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
                        let track = self.services.cursor.track + offset + 1;
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
                        self.services.apply(&actions[0]);
                    } else {
                        self.services.apply(&Action::Batch(actions));
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
            self.services.status = "Nothing to repeat".to_owned();
            return;
        };
        self.modes.count = None;
        if self.modes.mode.is_visual() {
            let selection = self.current_selection();
            let before = Arc::clone(self.song());
            // What moves the values, or fills them, keeps the selection, as it did the first time.
            let kept = match change {
                Change::Increment { by, kind, .. } => Some(self.moved(kind, by, selection.scope)),
                Change::Operator {
                    op: op @ (Op::ShiftUp | Op::ShiftDown),
                    ..
                } => {
                    let by = if op == Op::ShiftUp { 1 } else { -1 };
                    Some(self.moved(Shift::Operator, by, selection.scope))
                }
                Change::Replace { .. } => Some(Adjustment::Filled),
                _ => None,
            };
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
                    self.shape_again(selection.range, selection.tracks, column, *shape);
                }
                Change::Paste { .. } => {}
            }
            match kept {
                Some(adjustment) => self.adjusted(&before, adjustment),
                None => self.leave_visual(selection),
            }
        } else {
            self.do_change(&change);
        }
    }

    fn enter_visual(&mut self, mode: Mode) {
        let scope = if mode == Mode::VisualStep {
            Scope::WHOLE
        } else {
            Scope::column(self.services.cursor.field.group())
        };
        self.phrase_view.anchor = Some((self.services.cursor.step, scope));
        self.phrase_view.anchor_track = Some(self.services.cursor.track);
        self.phrase_view.scope = None;
        self.adjustments = Adjustments::default();
        self.modes.mode = mode;
        self.modes.count = None;
    }

    /// The current selection in a Visual mode.
    pub fn current_selection(&self) -> Selection {
        self.phrase_view
            .selection(self.modes.mode, self.services.cursor)
    }

    /// Finish selecting, for something that goes on to use the selection.
    pub(crate) fn leave_selection(&mut self, selection: Selection) {
        self.leave_visual(selection);
    }

    /// Remember a kept shape for `.`.
    pub(crate) fn remember_shape(&mut self, panel: &ShapePanel) {
        self.last_change = Some(Change::Shape {
            column: panel.column,
            shape: Box::new(panel.shape),
            steps: panel.selection.range.len(),
            tracks: panel.selection.tracks.count(),
        });
    }

    fn leave_visual(&mut self, selection: Selection) {
        self.phrase_view.last_selection = Some(selection);
        self.adjustments = Adjustments::default();
        self.services.cursor.step = selection.range.first;
        self.modes.mode = Mode::Normal;
        self.phrase_view.anchor = None;
        self.phrase_view.anchor_track = None;
        self.services
            .go_to_track_at(selection.tracks.first, self.modes.view);
        self.phrase_view.scope = None;
        self.clamp_cursor();
    }

    fn reselect(&mut self) {
        let Some(selection) = self.phrase_view.last_selection else {
            self.services.status = "No earlier selection".to_owned();
            return;
        };
        self.phrase_view.anchor = Some((selection.range.first, selection.scope));
        self.phrase_view.anchor_track = Some(selection.tracks.last);
        self.phrase_view.scope = Some(selection.scope);
        self.adjustments = Adjustments::default();
        self.services
            .go_to_track_at(selection.tracks.first, self.modes.view);
        self.services.cursor.step = selection.range.last;
        self.modes.mode = if selection.scope.is_whole() {
            Mode::VisualStep
        } else {
            Mode::VisualField
        };
        self.clamp_cursor();
    }
}
