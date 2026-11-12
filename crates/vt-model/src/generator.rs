//! Live generators: the shape panel kept live on a column of a phrase. One owns the cells it fills
//! and keeps the values from under them, so it can be reshaped as often as wanted, until a hand
//! edit to one of its cells applies it and leaves plain values.

use serde::{Deserialize, Serialize};

use crate::scale::Scale;
use crate::selection::StepRange;
use crate::shape::{Shape, ShapeColumn, exact_values};
use crate::step::{FxCommand, FxSlot, Note, Step};

/// Where a live generator writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Span {
    /// These stored rows of its phrase.
    Rows { first: usize, last: usize },
    /// Its FX column's lane, filled as a loop of its own, whichever rows the lane is on.
    Lane,
}

impl Span {
    /// Rows of a phrase.
    pub fn rows(range: StepRange) -> Self {
        Self::Rows {
            first: range.first,
            last: range.last,
        }
    }
}

/// A shape kept live on a column of a phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generator {
    pub column: ShapeColumn,
    pub span: Span,
    pub shape: Shape,
    /// The first stored row it covers.
    first: usize,
    /// Its column's cell from under each of its rows, on a step holding nothing else: what the
    /// row had before it, or was given since by an edit it follows.
    under: Vec<Step>,
    /// Its rows as it last left them, to tell what an edit changed. It's what's in the phrase, so
    /// it isn't saved.
    seen: Vec<Step>,
}

/// Whether a column's rows are the notes, so a note coming or going is an edit to what it reads,
/// whatever it does to the column's own cell.
fn on_notes(column: ShapeColumn) -> bool {
    matches!(
        column,
        ShapeColumn::Velocity | ShapeColumn::Nudge | ShapeColumn::Length
    )
}

/// Copy a column's cell from one step to another: a note, a velocity, a nudge, a whole gate cell,
/// or an FX slot.
fn copy(column: ShapeColumn, from: &Step, to: &mut Step) {
    match column {
        ShapeColumn::Note { .. } => to.note = from.note,
        ShapeColumn::Velocity => to.velocity = from.velocity,
        ShapeColumn::Nudge => to.nudge = from.nudge,
        ShapeColumn::Length => to.gate = from.gate,
        ShapeColumn::Fx { slot, .. } => {
            if let Some(cell) = to.fx.get_mut(slot) {
                *cell = from.fx.get(slot).copied().unwrap_or_default();
            }
        }
    }
}

/// A column's cell on a step, alone on a step of its own.
fn cell(column: ShapeColumn, step: &Step) -> Step {
    let mut cell = Step::default();
    if let ShapeColumn::Fx { slot, .. } = column {
        cell.fx = vec![FxSlot::default(); slot + 1];
    }
    copy(column, step, &mut cell);
    cell
}

/// Whether two steps hold the same cell in a column.
fn same(column: ShapeColumn, a: &Step, b: &Step) -> bool {
    match column {
        ShapeColumn::Note { .. } => a.note == b.note,
        ShapeColumn::Velocity => a.velocity == b.velocity,
        ShapeColumn::Nudge => a.nudge == b.nudge,
        ShapeColumn::Length => a.gate == b.gate,
        ShapeColumn::Fx { slot, .. } => a.fx.get(slot) == b.fx.get(slot),
    }
}

impl Generator {
    /// A generator not yet on any rows, to make live with
    /// [`Phrase::add_generator`](crate::song::Phrase::add_generator).
    pub fn new(column: ShapeColumn, span: Span, shape: Shape) -> Self {
        Self {
            column,
            span,
            shape,
            first: 0,
            under: Vec::new(),
            seen: Vec::new(),
        }
    }

    /// The stored rows it covers now.
    pub fn rows(&self) -> Option<StepRange> {
        (!self.under.is_empty())
            .then(|| StepRange::between(self.first, self.first + self.under.len() - 1))
    }

    /// Whether it fills the same cells as a column would: an FX column is its slot, whatever
    /// its command.
    pub fn shares(&self, column: ShapeColumn) -> bool {
        match (self.column, column) {
            (ShapeColumn::Fx { slot, .. }, ShapeColumn::Fx { slot: other, .. }) => slot == other,
            (ShapeColumn::Note { .. }, ShapeColumn::Note { .. }) => true,
            (mine, other) => mine == other,
        }
    }

    /// Its phrase's steps as they'd be without it.
    pub fn under(&self, steps: &[Step]) -> Vec<Step> {
        let mut steps = steps.to_vec();
        self.restore(&mut steps);
        steps
    }

    /// Put back what's under it.
    fn restore(&self, steps: &mut [Step]) {
        for (offset, cell) in self.under.iter().enumerate() {
            if let Some(step) = steps.get_mut(self.first + offset) {
                copy(self.column, cell, step);
            }
        }
    }

    /// Start on these rows, keeping what's there as what's under it, and write its values.
    pub(crate) fn start(&mut self, steps: &mut [Step], range: StepRange) {
        self.first = range.first;
        self.under = (range.first..=range.last)
            .filter_map(|row| steps.get(row))
            .map(|step| cell(self.column, step))
            .collect();
        self.render(steps);
    }

    /// Write its values over what's under it, changing its own cells only.
    pub(crate) fn render(&mut self, steps: &mut [Step]) {
        self.restore(steps);
        let Some(range) = self.rows() else {
            return;
        };
        for (row, value) in exact_values(steps, range, self.column, &self.shape) {
            if let Some(step) = steps.get_mut(row) {
                self.column.write_exact(step, value);
            }
        }
        self.seen = steps[range.first..=range.last.min(steps.len() - 1)].to_vec();
    }

    /// Follow an edit to its phrase, now that it should cover `range`: false if the edit changed
    /// one of its cells by hand, which applies it, leaving the cells as the edit left them.
    /// Otherwise it rerenders if the edit changed the notes it reads, or moved its lane, and stays
    /// live. A note that comes or goes on a column whose rows are the notes takes the cell the
    /// edit gave it as what's under it.
    pub(crate) fn follow(&mut self, steps: &mut [Step], range: Option<StepRange>) -> bool {
        let Some(range) = range else {
            return false;
        };
        let mut changed = false;
        for (offset, seen) in self.seen.iter().enumerate() {
            let Some(now) = steps.get(self.first + offset) else {
                return false;
            };
            let came_or_went = matches!(seen.note, Note::On(_)) != matches!(now.note, Note::On(_));
            if !same(self.column, seen, now) {
                if !(on_notes(self.column) && came_or_went) {
                    return false;
                }
                self.under[offset] = cell(self.column, now);
                changed = true;
            } else if seen.note != now.note {
                changed = true;
            }
        }
        if self.rows() != Some(range) {
            self.retarget(steps, range);
            changed = true;
        }
        if changed {
            self.render(steps);
        }
        true
    }

    /// Move onto other rows, as a lane does: the rows it leaves get back what was under them, and
    /// the rows it takes keep what's there as what's under them.
    fn retarget(&mut self, steps: &mut [Step], range: StepRange) {
        let (old_first, old_under) = (self.first, std::mem::take(&mut self.under));
        let inside = |row: usize| (range.first..=range.last).contains(&row);
        for (offset, cell) in old_under.iter().enumerate() {
            let row = old_first + offset;
            if !inside(row)
                && let Some(step) = steps.get_mut(row)
            {
                copy(self.column, cell, step);
            }
        }
        self.under = (range.first..=range.last)
            .filter_map(|row| {
                let kept = row
                    .checked_sub(old_first)
                    .and_then(|offset| old_under.get(offset));
                Some(match kept {
                    Some(cell) => cell.clone(),
                    None => cell(self.column, steps.get(row)?),
                })
            })
            .collect();
        self.first = range.first;
    }

    /// Move its rows along, for steps added in front of them.
    pub(crate) fn shift(&mut self, by: usize) {
        self.first += by;
        if let Span::Rows { first, last } = &mut self.span {
            *first += by;
            *last += by;
        }
    }

    /// The same generator as saved, reading what it last left from the steps it's on.
    fn loaded(file: GeneratorFile, steps: &[Step]) -> Self {
        let mut generator = Self {
            column: file.column.into(),
            span: file.span,
            shape: file.shape,
            first: file.first,
            under: file.under,
            seen: Vec::new(),
        };
        // What's under it is cut to the rows stored, as a phrase is never shorter than it was.
        let room = steps.len().saturating_sub(generator.first);
        generator.under.truncate(room);
        if let Some(range) = generator.rows() {
            generator.seen = steps[range.first..=range.last].to_vec();
        }
        generator
    }
}

/// How a generator is saved: what it is and where, and what's under it, as steps holding only its
/// column.
#[derive(Serialize, Deserialize)]
pub(crate) struct GeneratorFile {
    column: ColumnFile,
    span: Span,
    shape: Shape,
    first: usize,
    under: Vec<Step>,
}

impl GeneratorFile {
    pub(crate) fn of(generator: &Generator) -> Self {
        Self {
            column: generator.column.into(),
            span: generator.span,
            shape: generator.shape,
            first: generator.first,
            under: generator.under.clone(),
        }
    }

    /// The generator, or none if its FX command isn't known.
    pub(crate) fn generator(self, steps: &[Step]) -> Option<Generator> {
        if let ColumnFile::Fx { command, .. } = &self.column
            && FxCommand::from_key(command).is_none()
        {
            return None;
        }
        Some(Generator::loaded(self, steps))
    }
}

/// A column as saved, with an FX command by its key, as the steps name it.
#[derive(Serialize, Deserialize)]
enum ColumnFile {
    Note { scale: Scale },
    Velocity,
    Nudge,
    Length,
    Fx { slot: usize, command: String },
}

impl From<ShapeColumn> for ColumnFile {
    fn from(column: ShapeColumn) -> Self {
        match column {
            ShapeColumn::Note { scale } => Self::Note { scale },
            ShapeColumn::Velocity => Self::Velocity,
            ShapeColumn::Nudge => Self::Nudge,
            ShapeColumn::Length => Self::Length,
            ShapeColumn::Fx { slot, command } => Self::Fx {
                slot,
                command: command.key(),
            },
        }
    }
}

impl From<ColumnFile> for ShapeColumn {
    fn from(column: ColumnFile) -> Self {
        match column {
            ColumnFile::Note { scale } => Self::Note { scale },
            ColumnFile::Velocity => Self::Velocity,
            ColumnFile::Nudge => Self::Nudge,
            ColumnFile::Length => Self::Length,
            // Checked when it's read.
            ColumnFile::Fx { slot, command } => Self::Fx {
                slot,
                command: FxCommand::from_key(&command).unwrap_or(FxCommand::CUT),
            },
        }
    }
}

/// The columns of the live generators an edit applied, for saying so.
pub fn applied(before: &crate::song::Song, after: &crate::song::Song) -> Vec<ShapeColumn> {
    let mut gone = Vec::new();
    for (index, track) in before.tracks.iter().enumerate() {
        let Some(now) = after.tracks.get(index) else {
            continue;
        };
        for (id, phrase) in &track.phrases {
            if phrase.generators().is_empty() {
                continue;
            }
            let Some(edited) = now.phrases.get(id) else {
                continue;
            };
            if std::sync::Arc::ptr_eq(phrase, edited) {
                continue;
            }
            for generator in phrase.generators() {
                let kept = edited
                    .generators()
                    .iter()
                    .any(|other| other.column == generator.column && other.span == generator.span);
                if !kept {
                    gone.push(generator.column);
                }
            }
        }
    }
    gone
}
