//! A shape's render into a buffer made ahead, allocating nothing, so the edit thread and the engine
//! run the same code and can't differ. Each row of the span gets a `GenCell`, what the shape leaves
//! in its column there; [`written_from`](super::written_from) and a live generator lay those over
//! the steps.
//!
//! The rows chosen are a bitset of the span, at most `MAX_PHRASE_STEPS` of them, and Euclid's
//! pattern is worked out in bitsets too. A transform deals the values there among the cells
//! themselves before it writes them, so it needs nothing more.

use super::mix;
use super::random::{Odds, unit};
use super::{
    FillKind, Ladder, Shape, ShapeColumn, WaveRate, Where, acid, bits, curved, follow, grids,
    variation, walk,
};
use crate::constants::{FX_SLOTS, MAX_PHRASE_STEPS, TICKS_PER_STEP};
use crate::lfo::wave;
use crate::scale::Scale;
use crate::selection::StepRange;
use crate::step::{FxSlot, Gate, GateKind, MAX_NOTE, MIN_NOTE, Mark, Note, Step};

/// What a shape leaves in its column on a row: a small value of its own, never a `Step`, whose FX
/// are a `Vec`, so a buffer of them can be made ahead and filled on the audio thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GenCell {
    /// Nothing: the row stays as it is.
    #[default]
    Under,
    /// A new pitch for the note there, which keeps its velocity and gate.
    Pitch(u8),
    /// A note with its velocity and gate: notes of its own, `Acid`'s and `Follow`'s.
    Note {
        note: Note,
        velocity: Option<u8>,
        gate: Option<Gate>,
    },
    /// The same with the row's FX slots, for a `Follow` copying them.
    NoteFx {
        note: Note,
        velocity: Option<u8>,
        gate: Option<Gate>,
        fx: [FxSlot; FX_SLOTS],
    },
    Velocity(u8),
    /// A nudge, in 24ths of a sixteenth either way.
    Nudge(i8),
    /// A whole gate cell, with the length written into it.
    Gate(Gate),
    /// The column's FX slot.
    Fx(FxSlot),
}

impl GenCell {
    /// Write it over a step, in the column it was rendered for. `Under` leaves the step alone.
    pub fn apply(self, column: ShapeColumn, step: &mut Step) {
        match self {
            Self::Under => {}
            Self::Pitch(note) => step.note = Note::On(note),
            Self::Note {
                note,
                velocity,
                gate,
            } => {
                step.note = note;
                step.velocity = velocity;
                step.gate = gate;
            }
            Self::NoteFx {
                note,
                velocity,
                gate,
                fx,
            } => {
                step.note = note;
                step.velocity = velocity;
                step.gate = gate;
                for (slot, cell) in step.fx.iter_mut().enumerate() {
                    *cell = fx.get(slot).copied().unwrap_or_default();
                }
            }
            Self::Velocity(velocity) => step.velocity = Some(velocity),
            Self::Nudge(nudge) => step.nudge = nudge,
            Self::Gate(gate) => step.gate = Some(gate),
            Self::Fx(fx) => {
                if let ShapeColumn::Fx { slot, .. } = column
                    && let Some(cell) = step.fx.get_mut(slot)
                {
                    *cell = fx;
                }
            }
        }
    }
}

/// A row as it's heard: a step's cells, copied out of it so a generator's cells can be laid over
/// them where they're read, without a `Step` to write into. The engine reads every row through
/// one, and what's laid over one is what [`GenCell::apply`] writes into a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowView {
    pub note: Note,
    pub velocity: Option<u8>,
    pub nudge: i8,
    pub gate: Option<Gate>,
    /// Each FX slot the step has.
    fx: [Option<FxSlot>; FX_SLOTS],
}

impl RowView {
    /// A step as it's written.
    pub fn of(step: &Step) -> Self {
        Self {
            note: step.note,
            velocity: step.velocity,
            nudge: step.nudge,
            gate: step.gate,
            fx: std::array::from_fn(|slot| step.fx.get(slot).copied()),
        }
    }

    /// An FX slot, if the step has it.
    pub fn fx(&self, slot: usize) -> Option<&FxSlot> {
        self.fx.get(slot)?.as_ref()
    }

    /// Put an FX slot there, if the step has it.
    pub(crate) fn set_fx(&mut self, slot: usize, fx: FxSlot) {
        if let Some(Some(cell)) = self.fx.get_mut(slot) {
            *cell = fx;
        }
    }

    /// How many steps the row lasts.
    pub fn pulses(&self) -> usize {
        usize::from(self.gate.map_or(1, |gate| gate.pulses))
    }

    /// What its note does over its pulses.
    pub fn gate_kind(&self) -> GateKind {
        self.gate.map_or(GateKind::Single, |gate| gate.kind)
    }

    /// How much of its last pulse its note lasts, if the row says.
    pub fn length(&self) -> Option<u8> {
        self.gate.and_then(|gate| gate.length)
    }

    /// Lay a generator's cell over it, as [`GenCell::apply`] writes one over a step.
    pub fn lay(&mut self, column: ShapeColumn, cell: GenCell) {
        match cell {
            GenCell::Under => {}
            GenCell::Pitch(note) => self.note = Note::On(note),
            GenCell::Note {
                note,
                velocity,
                gate,
            } => {
                self.note = note;
                self.velocity = velocity;
                self.gate = gate;
            }
            GenCell::NoteFx {
                note,
                velocity,
                gate,
                fx,
            } => {
                self.note = note;
                self.velocity = velocity;
                self.gate = gate;
                for (slot, cell) in self.fx.iter_mut().enumerate() {
                    if let Some(cell) = cell {
                        *cell = fx[slot];
                    }
                }
            }
            GenCell::Velocity(velocity) => self.velocity = Some(velocity),
            GenCell::Nudge(nudge) => self.nudge = nudge,
            GenCell::Gate(gate) => self.gate = Some(gate),
            GenCell::Fx(fx) => {
                if let ShapeColumn::Fx { slot, .. } = column
                    && let Some(Some(cell)) = self.fx.get_mut(slot)
                {
                    *cell = fx;
                }
            }
        }
    }
}

impl ShapeColumn {
    /// The cell a value with a fine part leaves on a step, as [`write_exact`](Self::write_exact)
    /// writes it: notes of its own take the column's velocity, and a step without the FX slot
    /// takes nothing.
    pub fn cell(self, step: &Step, value: u16) -> GenCell {
        let whole = (value >> 8) as u8;
        let note = whole.clamp(MIN_NOTE, MAX_NOTE);
        match self {
            Self::Note { writes: None, .. } => GenCell::Pitch(note),
            Self::Note { writes, .. } => GenCell::Note {
                note: Note::On(note),
                velocity: writes,
                gate: step.gate,
            },
            Self::Velocity => GenCell::Velocity(whole),
            Self::Nudge { .. } => GenCell::Nudge(self.nudge_of(whole)),
            Self::Length => GenCell::Gate(Gate {
                length: Some(whole),
                ..step.gate.unwrap_or_default()
            }),
            Self::Fx { slot, command } => step.fx.get(slot).map_or(GenCell::Under, |fx| {
                GenCell::Fx(
                    FxSlot {
                        command: Some(command),
                        value,
                        mark: if fx.is_value() { fx.mark } else { Mark::Lock },
                    }
                    .with_valid_mark(),
                )
            }),
        }
    }
}

/// Write a shape's cells for `range` into `cells`, the first for its first row, allocating
/// nothing. `cells` holds one for each row of the range, up to `MAX_PHRASE_STEPS`; a row it has no
/// room for isn't rendered. Each is what the shape leaves on that row, or `Under` where it leaves
/// the row alone. A `Follow` reads `source`, the loop of the phrase it follows, and with none
/// leaves every row alone.
pub fn render_into(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    shape: &Shape,
    source: Option<&[Step]>,
    cells: &mut [GenCell],
) {
    render(steps, range, column, shape, source, cells, |_, _| {});
}

/// A shape's value on a row: where it falls exactly, and the whole value written without a fine
/// part.
#[derive(Debug, Clone, Copy)]
pub(super) struct Shaped {
    pub(super) exact: f64,
    pub(super) whole: u8,
}

impl Shaped {
    pub(super) fn note(note: u8) -> Self {
        Self {
            exact: f64::from(note),
            whole: note,
        }
    }

    /// A value moved whole, which keeps its fine part.
    fn kept(value: f64, top: f64) -> Self {
        Self {
            exact: value,
            whole: value.floor().clamp(0.0, top) as u8,
        }
    }
}

/// The value a column writes for a shaped one, with its fine part where the column has one.
pub(super) fn exact(column: ShapeColumn, value: Shaped) -> u16 {
    let fine = matches!(column, ShapeColumn::Fx { command, .. } if command.has_fine());
    if fine {
        let top = f64::from(column.highest()) * 256.0;
        (value.exact * 256.0).round().clamp(0.0, top) as u16
    } else {
        u16::from(value.whole) << 8
    }
}

/// The render, telling `seen` each row's value as it's written, by row, for
/// [`values`](super::values). A transform tells them from the last row back.
pub(super) fn render(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    shape: &Shape,
    source: Option<&[Step]>,
    cells: &mut [GenCell],
    seen: impl FnMut(usize, &Shaped),
) {
    let len = range.len().min(MAX_PHRASE_STEPS).min(cells.len());
    debug_assert_eq!(
        len,
        range.len().min(MAX_PHRASE_STEPS),
        "a cell for each row"
    );
    let cells = &mut cells[..len];
    cells.fill(GenCell::Under);
    let mut out = Out {
        steps,
        first: range.first,
        column,
        cells,
        along: None,
        seen,
    };
    match shape.kind {
        FillKind::Follow => {
            if let Some(source) = source {
                follow::render(steps, range, column, &shape.follow, source, out.cells);
            }
        }
        FillKind::Acid => acid::render(steps, range, column, shape, &mut out),
        FillKind::Multiply => bits::render(steps, range, column, shape.bits, out.cells),
        FillKind::Grids => grids::render(
            steps,
            range,
            column,
            shape.from,
            shape.seed,
            shape.grids,
            out.cells,
        ),
        FillKind::Variation => {
            variation::render(steps, range, column, shape.variation, shape.seed, out.cells)
        }
        _ => {
            let rows = chosen(steps, range, column, shape.place, shape.seed, len);
            match column {
                ShapeColumn::Note { scale, .. } => noted(&rows, range, scale, shape, &mut out),
                _ => {
                    let top = f64::from(column.highest());
                    filled(
                        &rows,
                        range,
                        shape,
                        top,
                        &|exact| f64::from(exact) / 256.0,
                        None,
                        &mut out,
                    );
                }
            }
        }
    }
}

/// Where a render's values go: each into its row's cell, as the column takes it.
pub(super) struct Out<'a, F> {
    pub(super) steps: &'a [Step],
    /// The row the first cell is for.
    pub(super) first: usize,
    pub(super) column: ShapeColumn,
    pub(super) cells: &'a mut [GenCell],
    /// For a Note column's ramp, random fill, wave or transform, the ladder its values are
    /// positions on, and whether a value between two steps stays there rather than going to the
    /// lower.
    along: Option<(&'a Ladder, bool)>,
    pub(super) seen: F,
}

impl<F: FnMut(usize, &Shaped)> Out<'_, F> {
    /// A row's value, by its offset in the span.
    fn put(&mut self, offset: usize, value: Shaped) {
        let value = match self.along {
            Some((ladder, moved)) => {
                let position = if moved {
                    value.exact
                } else {
                    f64::from(value.whole)
                };
                Shaped::note(ladder.note(position))
            }
            None => value,
        };
        let row = self.first + offset;
        let Some(step) = self.steps.get(row) else {
            return;
        };
        self.cells[offset] = self.column.cell(step, exact(self.column, value));
        (self.seen)(row, &value);
    }
}

/// The words of a bitset of a span's rows.
const WORDS: usize = MAX_PHRASE_STEPS / 64;

/// A bitset of up to `MAX_PHRASE_STEPS` bits, 512 bytes: the rows of a span a shape fills, by
/// their offset in it, or a Euclidean pattern.
#[derive(Clone, Copy)]
pub(super) struct Rows {
    words: [u64; WORDS],
    len: usize,
}

impl Rows {
    /// `len` bits, all clear.
    fn new(len: usize) -> Self {
        Self {
            words: [0; WORDS],
            len: len.min(MAX_PHRASE_STEPS),
        }
    }

    pub(super) fn has(&self, at: usize) -> bool {
        at < self.len && self.words[at / 64] & (1 << (at % 64)) != 0
    }

    fn set(&mut self, at: usize) {
        if at < self.len {
            self.words[at / 64] |= 1 << (at % 64);
        }
    }

    /// One more bit on the end, if there's room.
    fn push(&mut self, on: bool) {
        if self.len < MAX_PHRASE_STEPS {
            self.len += 1;
            if on {
                self.set(self.len - 1);
            }
        }
    }

    /// Another's bits on the end, as far as there's room.
    fn extend(&mut self, other: &Self) {
        for at in 0..other.len {
            self.push(other.has(at));
        }
    }

    pub(super) fn count(&self) -> usize {
        self.words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    /// The bits set, from the first.
    pub(super) fn ones(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(index, &word)| {
            let mut left = word;
            std::iter::from_fn(move || {
                (left != 0).then(|| {
                    let bit = left.trailing_zeros() as usize;
                    left &= left - 1;
                    index * 64 + bit
                })
            })
        })
    }

    /// The bits set, from the last.
    fn ones_back(&self) -> impl Iterator<Item = usize> + '_ {
        self.words
            .iter()
            .enumerate()
            .rev()
            .flat_map(|(index, &word)| {
                let mut left = word;
                std::iter::from_fn(move || {
                    (left != 0).then(|| {
                        let bit = 63 - left.leading_zeros() as usize;
                        left &= !(1 << bit);
                        index * 64 + bit
                    })
                })
            })
    }
}

/// `hits` spread as evenly as they go over `rows`, by Bjorklund's algorithm, without allocating.
/// It pairs the hits with the rests, then the pairs with what's left over, until one is left
/// over, and every group made at each turn is the same, so it keeps only the two kinds of group
/// and how many of each.
pub(super) fn euclid(hits: usize, rows: usize) -> Rows {
    let rows = rows.min(MAX_PHRASE_STEPS);
    let mut pattern = Rows::new(0);
    if hits >= rows || hits == 0 {
        for _ in 0..rows {
            pattern.push(hits > 0);
        }
        return pattern;
    }
    let (mut group, mut rest) = (Rows::new(0), Rows::new(0));
    group.push(true);
    rest.push(false);
    let (mut groups, mut rests) = (hits, rows - hits);
    while rests > 1 {
        let pairs = groups.min(rests);
        if groups > pairs {
            // The groups left over are the new rests.
            let left = group;
            group.extend(&rest);
            rest = left;
            rests = groups - pairs;
        } else {
            group.extend(&rest);
            rests -= pairs;
        }
        groups = pairs;
    }
    for _ in 0..groups {
        pattern.extend(&group);
    }
    for _ in 0..rests {
        pattern.extend(&rest);
    }
    pattern
}

/// The rows of the span that get a value, by their offset in it, among its first `len`.
pub(super) fn chosen(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    place: Where,
    seed: u64,
    len: usize,
) -> Rows {
    let mut rows = Rows::new(len);
    for offset in 0..rows.len {
        let Some(step) = steps.get(range.first + offset) else {
            continue;
        };
        if !column.can_take(step) {
            continue;
        }
        let has_note = matches!(step.note, Note::On(_));
        let chosen = match place {
            Where::Notes => has_note,
            Where::NoNotes => !has_note,
            Where::Commands => column.value(step).is_some(),
            Where::AllRows | Where::Euclid { .. } => true,
            Where::Every(every) => offset.is_multiple_of(usize::from(every.max(1))),
            Where::Chance(percent) => mix(seed ^ 0x5555, offset) % 100 < u64::from(percent),
        };
        if chosen {
            rows.set(offset);
        }
    }
    let Where::Euclid { hits, rotate } = place else {
        return rows;
    };
    // Counted among the rows it can change, as `Alternate` counts, so on Velocity it's among the
    // notes however they're spaced.
    let open = rows.count();
    let pattern = euclid(usize::from(hits), open);
    let later = usize::from(rotate) % open.max(1);
    let mut hit = Rows::new(len);
    for (index, offset) in rows.ones().enumerate() {
        if pattern.has((index + open - later) % open) {
            hit.set(offset);
        }
    }
    hit
}

/// A Note column's values. A ramp, a random fill, a wave and the transforms work along the
/// `Ladder`, so they land on the scale's degrees while it's on; `Alternate` and `List` write their
/// notes as they're given, and `Snap` moves each note to the nearest in the scale.
fn noted<F: FnMut(usize, &Shaped)>(
    rows: &Rows,
    range: StepRange,
    scale: Scale,
    shape: &Shape,
    out: &mut Out<'_, F>,
) {
    let note = |exact: u16| f64::from(exact >> 8);
    match shape.kind {
        FillKind::Alternate | FillKind::List => {
            filled(rows, range, shape, f64::from(MAX_NOTE), &note, None, out);
        }
        FillKind::Snap => {
            for offset in rows.ones() {
                let Some(Note::On(note)) = out.steps.get(range.first + offset).map(|s| s.note)
                else {
                    continue;
                };
                let snapped = if scale.on { scale.fitted(note) } else { note };
                out.put(offset, Shaped::note(snapped));
            }
        }
        kind => {
            let ladder = Ladder::new(scale);
            let along = Shape {
                from: ladder.step(shape.from),
                to: ladder.step(shape.to),
                ..*shape
            };
            // A transform that moves notes about keeps each as it was, in the scale or not.
            let moved = kind.reworks() && kind != FillKind::Scale;
            let mut along_ladder = Out {
                steps: out.steps,
                first: out.first,
                column: out.column,
                cells: &mut *out.cells,
                along: Some((&ladder, moved)),
                seen: &mut out.seen,
            };
            let position = |exact: u16| ladder.position((exact >> 8) as u8);
            // A random fill weighing its notes works out each one's odds once.
            let odds = (kind == FillKind::Random && shape.random.weighs(scale)).then(|| {
                let (low, high) = (along.from.min(along.to), along.from.max(along.to));
                let notes = ladder.notes();
                let high = usize::from(high).min(notes.len() - 1);
                Odds::new(
                    &shape.random,
                    scale,
                    &notes[usize::from(low).min(high)..=high],
                )
            });
            filled(
                rows,
                range,
                &along,
                ladder.top(),
                &position,
                odds.as_ref(),
                &mut along_ladder,
            );
        }
    }
}

/// The values on the rows chosen, for a column whose values run from 0 to `top`. A transform reads
/// the values there through `read`, from the column's value with its fine part, and a random fill
/// weighing its notes draws them by their `odds`.
fn filled<F: FnMut(usize, &Shaped)>(
    rows: &Rows,
    range: StepRange,
    shape: &Shape,
    top: f64,
    read: &dyn Fn(u16) -> f64,
    odds: Option<&Odds>,
    out: &mut Out<'_, F>,
) {
    if shape.kind.reworks() {
        return reworked(rows, shape, top, read, out);
    }
    let count = rows.count();
    let Some(first) = rows.ones().next() else {
        return;
    };
    let (from, to) = (f64::from(shape.from).min(top), f64::from(shape.to).min(top));
    let (low, high) = (from.min(to) as u64, from.max(to) as u64);
    let run = usize::from(shape.run.max(1));
    // A ramp, a wave or a random fill holds for a run: each row takes the value of the first row
    // of its run, and a ramp's ends are the first and last runs, so it climbs in even stairs.
    let last = rows.ones().nth((count - 1) / run * run).unwrap_or(first);
    let across = (last - first).max(1) as f64;
    let whole = |value: f64| Shaped {
        exact: value,
        whole: value as u8,
    };
    // A walk and a 1/f fill count turns among the rows they fill, a turn a run.
    let mut walker =
        walk::Walker::new(shape.walk, shape.seed, from as i32, low as i32, high as i32);
    let mut held = first;
    for (index, offset) in rows.ones().enumerate() {
        if index.is_multiple_of(run) {
            held = offset;
        }
        let value = match shape.kind {
            FillKind::Ramp => {
                let t = (held - first) as f64 / across;
                along_line(from, to, curved(t, shape.curve, shape.s), top)
            }
            FillKind::Random => {
                let span = high - low + 1;
                let drawn = mix(shape.seed, held);
                // Drawn evenly as it always was, unless it's shaped or weighed (decision 302).
                let along = match odds {
                    Some(odds) => odds.pick(&shape.random, unit(drawn)) as u64,
                    None if shape.random.shaped() => {
                        shape.random.pick(unit(drawn), span as usize) as u64
                    }
                    None => drawn % span,
                };
                whole((low + along) as f64)
            }
            FillKind::Walk => whole(f64::from(walker.at(index / run))),
            FillKind::Pink => {
                let t = walk::pink(shape.pink, shape.seed, index / run);
                let along = ((t * (high - low + 1) as f64) as u64).min(high - low);
                // From the first value, so `~` turns it upside down.
                whole(if from <= to {
                    low + along
                } else {
                    high - along
                } as f64)
            }
            FillKind::Wave => {
                let cycles = match shape.rate {
                    WaveRate::Time(rate) => rate.cycles((held as u64 * TICKS_PER_STEP) as f64, 0.0),
                    // A cycle is the whole selection, so the next row after it starts again.
                    WaveRate::Cycles(cycles) => {
                        held as f64 * f64::from(cycles) / range.len() as f64
                    }
                } + f64::from(shape.phase) / 360.0;
                let t = (f64::from(wave(shape.wave, cycles, shape.seed)) + 1.0) / 2.0;
                let exact = from + (to - from) * t;
                Shaped {
                    exact,
                    whole: exact.round().clamp(0.0, top) as u8,
                }
            }
            FillKind::Alternate => {
                let turn = index / run;
                whole(if turn.is_multiple_of(2) { from } else { to })
            }
            FillKind::List => {
                let list = shape.values.as_slice();
                // An empty list never comes from the panel, but leaves the rows at nothing rather
                // than failing.
                let value = list.get(index / run % list.len().max(1)).copied();
                whole(f64::from(value.unwrap_or(0)).min(top))
            }
            FillKind::Scale
            | FillKind::Invert
            | FillKind::Shuffle
            | FillKind::Reverse
            | FillKind::Snap
            | FillKind::InvertBits
            | FillKind::MaskBits
            | FillKind::RotateBits => unreachable!("reworked above"),
            FillKind::Multiply => unreachable!("rendered by `bits::render`"),
            FillKind::Grids => unreachable!("rendered by `grids::render`"),
            FillKind::Acid => unreachable!("rendered by `acid::render`"),
            FillKind::Variation => unreachable!("rendered by `variation::render`"),
            FillKind::Follow => unreachable!("read from what it follows"),
        };
        out.put(offset, value);
    }
}

/// A transform's values: the ones already on the rows chosen, reworked. A row chosen with no value
/// of this column's is left as it is.
///
/// The values are gathered into the first cells, in order, and dealt there, then each goes to its
/// row from the last back: a row is never before its place among them, so none is written over
/// before it's moved. Until then a cell holds its value in an FX slot's, the one with room for a
/// fine part.
fn reworked<F: FnMut(usize, &Shaped)>(
    rows: &Rows,
    shape: &Shape,
    top: f64,
    read: &dyn Fn(u16) -> f64,
    out: &mut Out<'_, F>,
) {
    let mut present = Rows::new(rows.len);
    let mut count = 0;
    let (mut lowest, mut highest) = (f64::MAX, f64::MIN);
    for offset in rows.ones() {
        let Some(value) = out
            .steps
            .get(out.first + offset)
            .and_then(|step| out.column.exact(step))
        else {
            continue;
        };
        lowest = lowest.min(read(value));
        highest = highest.max(read(value));
        out.cells[count] = GenCell::Fx(FxSlot {
            value,
            ..FxSlot::default()
        });
        present.set(offset);
        count += 1;
    }
    let dealt = &mut out.cells[..count];
    match shape.kind {
        FillKind::Reverse => dealt.reverse(),
        FillKind::Shuffle => {
            // Fisher and Yates, from the seed, so the same seed deals the same.
            for index in (1..count).rev() {
                let other = (mix(shape.seed ^ 0xA5A5, index) % (index as u64 + 1)) as usize;
                dealt.swap(index, other);
            }
        }
        _ => {}
    }
    let (from, to) = (f64::from(shape.from).min(top), f64::from(shape.to).min(top));
    for (at, offset) in (0..count).rev().zip(present.ones_back()) {
        let GenCell::Fx(FxSlot { value, .. }) = out.cells[at] else {
            unreachable!("gathered above");
        };
        let value = read(value);
        let shaped = match shape.kind {
            FillKind::Scale => {
                // All one value has no range to scale, so it goes to `from`.
                let t = if highest > lowest {
                    (value - lowest) / (highest - lowest)
                } else {
                    0.0
                };
                along_line(from, to, curved(t, shape.curve, shape.s), top)
            }
            FillKind::Invert => Shaped::kept(lowest + highest - value, top),
            kind if kind.on_bits() => {
                // A value with a fine part is worked on its whole part, and loses the fine.
                let whole = shape
                    .bits
                    .worked(kind, value.clamp(0.0, top) as u8, top as u8);
                Shaped {
                    exact: f64::from(whole),
                    whole,
                }
            }
            _ => Shaped::kept(value, top),
        };
        out.put(offset, shaped);
    }
    for (offset, cell) in out.cells.iter_mut().enumerate() {
        if !present.has(offset) {
            *cell = GenCell::Under;
        }
    }
}

/// Where a line from one value to the other is, `along` of the way. Rounding the distance
/// travelled means falling mirrors rising exactly.
fn along_line(from: f64, to: f64, along: f64, top: f64) -> Shaped {
    let moved = ((to - from).abs() * along + 0.5).floor();
    Shaped {
        exact: from + (to - from) * along,
        whole: (from + moved.copysign(to - from)).clamp(0.0, top) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bjorklund's algorithm as it was written before, with a `Vec` for each group.
    fn grouped(hits: usize, rows: usize) -> Vec<bool> {
        if hits >= rows {
            return vec![true; rows];
        }
        if hits == 0 {
            return vec![false; rows];
        }
        let mut groups: Vec<Vec<bool>> = vec![vec![true]; hits];
        let mut rest: Vec<Vec<bool>> = vec![vec![false]; rows - hits];
        while rest.len() > 1 {
            let pairs = groups.len().min(rest.len());
            let left = if groups.len() > pairs {
                groups.split_off(pairs)
            } else {
                rest.split_off(pairs)
            };
            for (group, tail) in groups.iter_mut().zip(rest) {
                group.extend(tail);
            }
            rest = left;
        }
        groups.into_iter().chain(rest).flatten().collect()
    }

    #[test]
    fn euclid_in_place_spreads_the_hits_as_the_groups_did() {
        for rows in (0..=130).chain([255, 256, 1000, MAX_PHRASE_STEPS]) {
            for hits in
                (0..=rows.min(70) + 1).chain([rows / 3, rows / 2 + 1, rows.saturating_sub(1), rows])
            {
                let pattern = euclid(hits, rows);
                let bits: Vec<bool> = (0..pattern.len).map(|at| pattern.has(at)).collect();
                assert_eq!(bits, grouped(hits, rows), "{hits} in {rows}");
            }
        }
    }

    #[test]
    fn a_bitset_goes_through_its_rows_either_way() {
        let mut rows = Rows::new(MAX_PHRASE_STEPS);
        let set = [0, 1, 63, 64, 65, 127, 1000, 4031, MAX_PHRASE_STEPS - 1];
        for at in set {
            rows.set(at);
        }
        rows.set(MAX_PHRASE_STEPS);
        assert_eq!(rows.count(), set.len());
        assert_eq!(rows.ones().collect::<Vec<_>>(), set);
        let mut back: Vec<usize> = rows.ones_back().collect();
        back.reverse();
        assert_eq!(back, set);
        assert!(rows.has(64) && !rows.has(66) && !rows.has(MAX_PHRASE_STEPS));
    }
}
