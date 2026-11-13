//! Live generators: the shape panel kept live on a column of a phrase. One owns the cells it fills
//! and keeps the values from under them, so it can be reshaped as often as wanted, until a hand
//! edit to one of its cells applies it and leaves plain values. A `Follow` reads another phrase,
//! usually on another track, and writes it again whenever that phrase changes.

use serde::{Deserialize, Serialize};

use crate::constants::FX_SLOTS;
use crate::song::Song;

use crate::scale::Scale;
use crate::selection::StepRange;
use crate::shape::{
    Context, FillKind, GenCell, RowView, Shape, ShapeColumn, Source, copy_fx, is_empty, open_rows,
    render_into,
};
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
    /// Its shape as its track's macros hold it by hand, which it renders from, if they hold any of
    /// its settings: `shape` with those put where the macros' turns put them, `shape` itself
    /// keeping its own underneath (decision 294). It's worked out again from the track as each
    /// edit is made, so it isn't saved.
    held: Option<Shape>,
}

/// Whether a column's rows are the notes, so a note coming or going is an edit to what it reads,
/// whatever it does to the column's own cell.
fn on_notes(column: ShapeColumn) -> bool {
    matches!(
        column,
        ShapeColumn::Velocity | ShapeColumn::Nudge { .. } | ShapeColumn::Length
    )
}

/// How much of each of its rows a generator on the Note column owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owns {
    /// Its column's cell alone.
    Cell,
    /// A note's velocity and gate as well as its pitch, as `Acid` writes its accents and slides
    /// there, and `Follow` copies them.
    Note,
    /// Those and the row's FX, for a `Follow` copying them too.
    NoteFx,
}

/// What a shape owns of each row.
fn owns(shape: &Shape) -> Owns {
    match shape.kind {
        FillKind::Acid => Owns::Note,
        FillKind::Follow if shape.follow.fx => Owns::NoteFx,
        FillKind::Follow => Owns::Note,
        // A note it adds or clears takes or leaves its velocity and gate.
        FillKind::Multiply => Owns::Note,
        // Its accents are velocities, and a note it adds or clears takes or leaves its gate.
        FillKind::Grids => Owns::Note,
        // Each row is a note learned with its accent and slide, or a rest, so it writes all three.
        FillKind::Variation => Owns::Note,
        _ => Owns::Cell,
    }
}

/// Copy a column's cell from one step to another: a note, with its velocity for notes of its own,
/// and its velocity and gate, and perhaps the row's FX, for whole notes; a velocity, a nudge, a
/// whole gate cell, or an FX slot.
fn copy(column: ShapeColumn, owns: Owns, from: &Step, to: &mut Step) {
    let whole = owns != Owns::Cell;
    match column {
        ShapeColumn::Note { writes, .. } => {
            to.note = from.note;
            if writes.is_some() || whole {
                to.velocity = from.velocity;
            }
            if whole {
                to.gate = from.gate;
            }
            if owns == Owns::NoteFx {
                copy_fx(from, to);
            }
        }
        ShapeColumn::Velocity => to.velocity = from.velocity,
        ShapeColumn::Nudge { .. } => to.nudge = from.nudge,
        ShapeColumn::Length => to.gate = from.gate,
        ShapeColumn::Fx { slot, .. } => {
            if let Some(cell) = to.fx.get_mut(slot) {
                *cell = from.fx.get(slot).copied().unwrap_or_default();
            }
        }
    }
}

/// Lay what's under a live generator's cell on a row over a view of the row, as its render puts
/// it back before writing: the column's cell of the step under it, `under`, and with it a note's
/// velocity and gate, and the row's FX, as much as the generator owns. The engine lays this first
/// wherever it plays a render of its own, so a row the render leaves as [`GenCell::Under`] plays
/// what's under the generator, as it would were the render written, not what the phrase has
/// written there (#622).
pub fn lay_under(generator: &Generator, under: &Step, view: &mut RowView) {
    let owns = owns(&generator.shape);
    let whole = owns != Owns::Cell;
    let fx = |slot: usize| under.fx.get(slot).copied().unwrap_or_default();
    match generator.column {
        ShapeColumn::Note { writes, .. } => {
            view.note = under.note;
            if writes.is_some() || whole {
                view.velocity = under.velocity;
            }
            if whole {
                view.gate = under.gate;
            }
            if owns == Owns::NoteFx {
                for slot in 0..FX_SLOTS {
                    view.set_fx(slot, fx(slot));
                }
            }
        }
        ShapeColumn::Velocity => view.velocity = under.velocity,
        ShapeColumn::Nudge { .. } => view.nudge = under.nudge,
        ShapeColumn::Length => view.gate = under.gate,
        ShapeColumn::Fx { slot, .. } => view.set_fx(slot, fx(slot)),
    }
}

/// A column's cell on a step, alone on a step of its own.
fn cell(column: ShapeColumn, owns: Owns, step: &Step) -> Step {
    let mut cell = Step::default();
    if let ShapeColumn::Fx { slot, .. } = column {
        cell.fx = vec![FxSlot::default(); slot + 1];
    }
    if owns == Owns::NoteFx {
        cell.fx = vec![FxSlot::default(); step.fx.len()];
    }
    copy(column, owns, step, &mut cell);
    cell
}

/// Whether two steps hold the same cell in a column.
fn same(column: ShapeColumn, owns: Owns, a: &Step, b: &Step) -> bool {
    let whole = owns != Owns::Cell;
    match column {
        ShapeColumn::Note { writes, .. } => {
            a.note == b.note
                && ((writes.is_none() && !whole) || a.velocity == b.velocity)
                && (!whole || a.gate == b.gate)
                && (owns != Owns::NoteFx || a.fx == b.fx)
        }
        ShapeColumn::Velocity => a.velocity == b.velocity,
        ShapeColumn::Nudge { .. } => a.nudge == b.nudge,
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
            held: None,
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
            (ShapeColumn::Nudge { .. }, ShapeColumn::Nudge { .. }) => true,
            (mine, other) => mine == other,
        }
    }

    /// A row of its phrase as the engine played it with this cell of a render of its own: the
    /// step as written, with what's under the generator put back in its column and the cell laid
    /// over, as [`lay_under`] and [`RowView::lay`] lay it (decision 300). None past the steps.
    pub fn heard_row(&self, steps: &[Step], row: usize, cell: GenCell) -> Option<Step> {
        let mut step = steps.get(row)?.clone();
        self.lay_heard(row, cell, &mut step);
        Some(step)
    }

    /// Lay a cell it played on a row over that row's step, with what's under it put back first,
    /// as [`Generator::heard_row`] does, so several generators' can be laid on one step.
    pub fn lay_heard(&self, row: usize, cell: GenCell, step: &mut Step) {
        if let Some(under) = row
            .checked_sub(self.first)
            .and_then(|at| self.under.get(at))
        {
            copy(self.column, owns(&self.shape), under, step);
        }
        cell.apply(self.column, step);
    }

    /// Its phrase's steps as they'd be without it.
    pub fn under(&self, steps: &[Step]) -> Vec<Step> {
        let mut steps = steps.to_vec();
        self.restore(&mut steps);
        steps
    }

    /// Put back what's under it.
    pub(crate) fn restore(&self, steps: &mut [Step]) {
        for (offset, cell) in self.under.iter().enumerate() {
            if let Some(step) = steps.get_mut(self.first + offset) {
                copy(self.column, owns(&self.shape), cell, step);
            }
        }
    }

    /// What its settings act on, given its phrase's steps as they'd be without it, `under`: its
    /// column, the rows it could change and whether its column is empty without it, with what a
    /// `Follow` can read and the loop it reads. It has none while it covers no rows.
    pub fn context<'a>(
        &self,
        under: &[Step],
        sources: &'a [Source],
        source: Option<&'a [Step]>,
    ) -> Option<Context<'a>> {
        let rows = self.rows()?;
        Some(Context {
            column: self.column,
            open: open_rows(under, rows, self.column),
            generates: is_empty(under, rows, self.column),
            sources,
            source,
        })
    }

    /// The shape it's heard with: its own, or as its track's macros hold it.
    pub fn heard(&self) -> Shape {
        self.held.unwrap_or(self.shape)
    }

    /// Whether its track's macros hold any of its settings, so its column is written from what
    /// they give rather than its own.
    pub fn is_held(&self) -> bool {
        self.held.is_some()
    }

    /// Its shape as its track's macros hold it, if they hold any of its settings.
    pub fn held(&self) -> Option<Shape> {
        self.held
    }

    /// Be held by its track's macros as this shape, or let go with none, and write its column
    /// again if that changes what it's heard with: true if it did.
    pub(crate) fn hold(
        &mut self,
        steps: &mut [Step],
        held: Option<Shape>,
        source: Option<&[Step]>,
    ) -> bool {
        let held = held.filter(|held| *held != self.shape);
        if held == self.held {
            return false;
        }
        let before = self.heard();
        self.held = held;
        if self.heard() == before {
            return false;
        }
        self.render(steps, source);
        true
    }

    /// The phrase it follows, if it's a `Follow`.
    pub fn source(&self) -> Option<Source> {
        (self.shape.kind == FillKind::Follow).then_some(self.shape.follow.source)
    }

    /// Start on these rows, keeping what's there as what's under it, and write its values. A
    /// `Follow` writes from `source`, the loop of the phrase it follows.
    pub(crate) fn start(&mut self, steps: &mut [Step], range: StepRange, source: Option<&[Step]>) {
        self.first = range.first;
        self.under = (range.first..=range.last)
            .filter_map(|row| steps.get(row))
            .map(|step| cell(self.column, owns(&self.shape), step))
            .collect();
        self.render(steps, source);
    }

    /// Take a new shape and write it. One that owns more of each note, as `Acid` owns its velocity
    /// and gate, takes what's there now as what's under those, and one that owns less puts back
    /// what was under them first.
    pub(crate) fn reshape(&mut self, steps: &mut [Step], shape: Shape, source: Option<&[Step]>) {
        self.restore(steps);
        self.shape = shape;
        let owns = owns(&self.shape);
        for (offset, under) in self.under.iter_mut().enumerate() {
            if let Some(step) = steps.get(self.first + offset) {
                *under = cell(self.column, owns, step);
            }
        }
        self.render(steps, source);
    }

    /// Write its values over what's under it, changing its own cells only: the cells
    /// [`render_into`] leaves, laid over the steps. A `Follow` with nothing to read, as its source
    /// has gone, leaves what it last wrote, for the edit to apply.
    pub(crate) fn render(&mut self, steps: &mut [Step], source: Option<&[Step]>) {
        if self.shape.kind == FillKind::Follow && source.is_none() {
            return;
        }
        self.restore(steps);
        let Some(range) = self.rows() else {
            return;
        };
        let mut cells = vec![GenCell::Under; range.len()];
        let shape = self.heard();
        render_into(steps, range, self.column, &shape, source, &mut cells);
        for (step, cell) in steps.iter_mut().skip(range.first).zip(cells) {
            cell.apply(self.column, step);
        }
        self.seen = steps[range.first..=range.last.min(steps.len() - 1)].to_vec();
    }

    /// Write a `Follow` again from its source, which an edit changed: true if that changed its
    /// rows.
    pub(crate) fn refollow(&mut self, steps: &mut [Step], source: &[Step]) -> bool {
        let before = self.seen.clone();
        self.render(steps, Some(source));
        self.seen != before
    }

    /// Follow an edit to its phrase, now that it should cover `range`: false if the edit changed
    /// one of its cells by hand, which applies it, leaving the cells as the edit left them.
    /// Otherwise it rerenders if the edit changed the notes it reads, or moved its lane, and stays
    /// live. A note that comes or goes on a column whose rows are the notes takes the cell the
    /// edit gave it as what's under it. A `Follow` rerenders from `source`.
    pub(crate) fn follow(
        &mut self,
        steps: &mut [Step],
        range: Option<StepRange>,
        source: Option<&[Step]>,
    ) -> bool {
        let Some(range) = range else {
            return false;
        };
        let whole = owns(&self.shape);
        let mut changed = false;
        for (offset, seen) in self.seen.iter().enumerate() {
            let Some(now) = steps.get(self.first + offset) else {
                return false;
            };
            let came_or_went = matches!(seen.note, Note::On(_)) != matches!(now.note, Note::On(_));
            if !same(self.column, whole, seen, now) {
                if !(on_notes(self.column) && came_or_went) {
                    return false;
                }
                self.under[offset] = cell(self.column, whole, now);
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
            self.render(steps, source);
        }
        true
    }

    /// Move onto other rows, as a lane does: the rows it leaves get back what was under them, and
    /// the rows it takes keep what's there as what's under them.
    fn retarget(&mut self, steps: &mut [Step], range: StepRange) {
        let whole = owns(&self.shape);
        let (old_first, old_under) = (self.first, std::mem::take(&mut self.under));
        let inside = |row: usize| (range.first..=range.last).contains(&row);
        for (offset, cell) in old_under.iter().enumerate() {
            let row = old_first + offset;
            if !inside(row)
                && let Some(step) = steps.get_mut(row)
            {
                copy(self.column, whole, cell, step);
            }
        }
        self.under = (range.first..=range.last)
            .filter_map(|row| {
                let kept = row
                    .checked_sub(old_first)
                    .and_then(|offset| old_under.get(offset));
                Some(match kept {
                    Some(cell) => cell.clone(),
                    None => cell(self.column, whole, steps.get(row)?),
                })
            })
            .collect();
        self.first = range.first;
    }

    /// The steps it keeps of its rows, what's under them and as it last left them, to convert a
    /// song's nudges (`crate::convert`).
    pub(crate) fn kept_steps_mut(&mut self) -> impl Iterator<Item = &mut Step> {
        self.under.iter_mut().chain(self.seen.iter_mut())
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
            held: None,
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
    Note {
        scale: Scale,
        /// The velocity of notes of its own, on a column that had none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        writes: Option<u8>,
    },
    Velocity,
    Nudge {
        /// The furthest its track's nudges went when it was made, in 24ths of a sixteenth.
        most: i8,
    },
    Length,
    Fx {
        slot: usize,
        command: String,
    },
}

impl From<ShapeColumn> for ColumnFile {
    fn from(column: ShapeColumn) -> Self {
        match column {
            ShapeColumn::Note { scale, writes } => Self::Note { scale, writes },
            ShapeColumn::Velocity => Self::Velocity,
            ShapeColumn::Nudge { most } => Self::Nudge { most },
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
            ColumnFile::Note { scale, writes } => Self::Note { scale, writes },
            ColumnFile::Velocity => Self::Velocity,
            ColumnFile::Nudge { most } => Self::Nudge { most },
            ColumnFile::Length => Self::Length,
            // Checked when it's read.
            ColumnFile::Fx { slot, command } => Self::Fx {
                slot,
                command: FxCommand::from_key(&command).unwrap_or(FxCommand::CUT),
            },
        }
    }
}

/// Whether a phrase reads another: it is that phrase, or it follows it, directly or through the
/// phrases it follows. A phrase can't follow one that reads it, as that would go round a cycle.
pub fn reads(song: &Song, phrase: Source, other: Source) -> bool {
    let mut seen: Vec<Source> = Vec::new();
    let mut next = vec![phrase];
    while let Some(here) = next.pop() {
        if here == other {
            return true;
        }
        if seen.contains(&here) {
            continue;
        }
        seen.push(here);
        if let Some(found) = song.source_phrase(here) {
            next.extend(found.generators().iter().filter_map(Generator::source));
        }
    }
    false
}

/// The phrases an edit took away that live `Follow`s read, which it applied, for saying so.
pub fn unfollowed(before: &Song, after: &Song) -> Vec<Source> {
    let mut gone: Vec<Source> = Vec::new();
    for track in before.tracks.iter().take(before.sound_tracks()) {
        for phrase in track.phrases.values() {
            for source in phrase.generators().iter().filter_map(Generator::source) {
                if after.source_phrase(source).is_none() && !gone.contains(&source) {
                    gone.push(source);
                }
            }
        }
    }
    gone
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
