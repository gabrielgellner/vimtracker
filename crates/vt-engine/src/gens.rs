//! Live generators as the engine plays them (decisions 288 and 293).
//!
//! At rest, what's heard is what's written: a phrase holds what each of its generators renders, so
//! the engine reads it as it reads any other. A generator whose settings the engine holds at other
//! numbers than its `Shape`'s is moving, and so is one whose `Vary` gives a pass a seed of its own
//! ([`Shape::on_pass`]): the engine renders its span again from the shape as heard, with the edit
//! thread's own [`render_into`], and lays its cells over each row it reads through a [`RowView`].
//! What it renders is kept beside the inputs it rendered from, so a row read with the same ones
//! takes its cell from what's kept, and only a change renders the span again.
//!
//! Nothing here allocates on the audio thread: each version brings a [`GenScratch`] made ahead on
//! the edit thread with room for every track's runs and cells, which the engine takes as it takes
//! the version and gives back with it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use vt_model::constants::{FX_SLOTS, MAX_PHRASE_STEPS};
use vt_model::generator::{Span, lay_under};
use vt_model::macros::GenColumn;
use vt_model::selection::StepRange;
use vt_model::shape::{
    Context, FillKind, GenCell, GenParam, RowView, Shape, ShapeColumn, is_empty, open_rows,
    render_into,
};
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;

/// How many columns a generator can be on: Note, Velocity, Nudge, Length and each FX column.
const COLUMNS: usize = 4 + FX_SLOTS;

/// How many settings a generator has, every fill's together.
const PARAMS: usize = GenParam::ALL.len();

/// Where a column's settings are kept: an FX column's by its slot, whatever its command.
fn column_place(column: ShapeColumn) -> usize {
    place_of(GenColumn::of(column))
}

/// Where a column's settings are kept, as a macro's mapping names the column.
fn place_of(column: GenColumn) -> usize {
    match column {
        GenColumn::Note => 0,
        GenColumn::Velocity => 1,
        GenColumn::Nudge => 2,
        GenColumn::Length => 3,
        GenColumn::Fx(slot) => 4 + slot.min(FX_SLOTS - 1),
    }
}

/// The numbers a track's generator settings are held at by the engine rather than by their
/// `Shape`s, by column: what a generator on that column of any of the track's phrases plays with,
/// as an absolute macro holds them. None is the shape's own. A number held directly
/// ([`GenHolds::hold`]) wins over one a macro puts there, which the macros set each block as they
/// set the registry's settings (decision 300).
#[derive(Debug, Clone)]
pub struct GenHolds {
    held: [[Option<i32>; PARAMS]; COLUMNS],
    /// Where the track's macros put them, from each one's turn, a step's `MAC` lock or glide, and
    /// what modulation adds.
    by_macros: [[Option<i32>; PARAMS]; COLUMNS],
    /// Whether any is held, without which no row needs a generator's cells laid over it.
    any: bool,
    /// How many times a macro has moved one, counting round, to tell whether one just did.
    changes: u32,
}

impl Default for GenHolds {
    fn default() -> Self {
        Self {
            held: [[None; PARAMS]; COLUMNS],
            by_macros: [[None; PARAMS]; COLUMNS],
            any: false,
            changes: 0,
        }
    }
}

impl GenHolds {
    /// Hold a column's setting at a number, or give it back to the shape with none. A list
    /// setting is left to the shape.
    pub fn hold(&mut self, column: ShapeColumn, param: GenParam, number: Option<i32>) {
        if let Some(at) = param_place(param) {
            self.held[column_place(column)][at] = number;
            self.count();
        }
    }

    /// Hold a column's setting where a macro puts it, the column as a mapping names it.
    pub fn hold_by_macro(&mut self, column: GenColumn, param: GenParam, number: i32) {
        if let Some(at) = param_place(param) {
            let held = &mut self.by_macros[place_of(column)][at];
            if *held != Some(number) {
                *held = Some(number);
                self.any = true;
                self.changes = self.changes.wrapping_add(1);
            }
        }
    }

    /// Let go of every setting the macros hold, as they're about to be put again.
    pub fn forget_macros(&mut self) {
        self.by_macros = [[None; PARAMS]; COLUMNS];
        self.count();
    }

    fn count(&mut self) {
        self.any = self
            .held
            .iter()
            .chain(&self.by_macros)
            .flatten()
            .any(Option::is_some);
    }

    /// Whether any setting is held.
    pub fn any(&self) -> bool {
        self.any
    }

    /// How many times a macro has moved a setting, counting round.
    pub fn changes(&self) -> u32 {
        self.changes
    }

    /// A shape as it's heard on a column: its settings held here set, each within its bounds. A
    /// setting the shape doesn't have is left alone, as the edit thread's macros leave it, so
    /// holding it never moves a generator it means nothing to.
    fn heard(&self, written: &Shape, context: &Context, generates: bool) -> Shape {
        let mut heard = *written;
        let place = column_place(context.column);
        let held = self.held[place].iter().zip(&self.by_macros[place]);
        for (at, (own, by_macro)) in held.enumerate() {
            let param = GenParam::ALL[at];
            if let Some(number) = own.or(*by_macro)
                && param.exists_for(&heard, generates)
            {
                param.set_number(&mut heard, context, number);
            }
        }
        heard
    }
}

/// Where a setting is kept among a column's, if the engine can hold it.
fn param_place(param: GenParam) -> Option<usize> {
    let at = GenParam::ALL.iter().position(|each| *each == param)?;
    param.held_in_engine().then_some(at)
}

/// The steps under each live generator, by track and phrase, one list for each of the phrase's
/// generators in turn: its phrase's steps as they'd be without it, which it renders from as the
/// edit thread does. Made on the edit thread, and kept from the last version for a phrase that
/// hasn't changed.
#[derive(Debug, Default)]
pub(crate) struct Unders {
    phrases: BTreeMap<(usize, u32), Arc<[Vec<Step>]>>,
}

impl Unders {
    /// Every live generator's steps, the last version's kept where its phrase is the same one.
    pub(crate) fn of(song: &Song, last: Option<(&Song, &Unders)>) -> Self {
        let mut phrases = BTreeMap::new();
        for (track, owner) in song.tracks.iter().enumerate() {
            for (id, phrase) in &owner.phrases {
                if phrase.generators().is_empty() {
                    continue;
                }
                let kept = last.and_then(|(song, unders)| {
                    let was = song.tracks.get(track)?.phrases.get(id)?;
                    let under = unders.phrases.get(&(track, *id))?;
                    Arc::ptr_eq(was, phrase).then(|| Arc::clone(under))
                });
                let under = kept.unwrap_or_else(|| {
                    let steps = phrase.steps();
                    phrase
                        .generators()
                        .iter()
                        .map(|generator| generator.under(steps))
                        .collect()
                });
                phrases.insert((track, *id), under);
            }
        }
        Self { phrases }
    }

    fn get(&self, track: usize, phrase: u32, generator: usize) -> Option<&[Step]> {
        let under = self.phrases.get(&(track, phrase))?;
        under.get(generator).map(Vec::as_slice)
    }
}

/// One live generator of the phrase a track is playing, as the engine renders it.
#[derive(Debug, Clone, Copy)]
struct GenRun {
    /// Its place in its phrase's list.
    generator: usize,
    column: ShapeColumn,
    /// The stored rows it covers.
    rows: StepRange,
    /// Where its cells start in the scratch's.
    cells: usize,
    /// How many rows its column could change, which Euclid's `Hits` are counted among.
    open: usize,
    /// The shape it last rendered, as heard, if it has since the version came.
    rendered: Option<Shape>,
    /// Which pass of its phrase, or of its lane, it was last read on.
    pass: i64,
    /// How many times it has rendered since the version came.
    renders: u32,
    /// Whether its column is empty without it, which some settings are only there for.
    generates: bool,
    /// The pass it was last heard on while it varies, and what it played then, if that's waiting
    /// to be reported: only the latest, so a long block never loses it among the rows.
    told: Option<i64>,
    report: Option<GenPass>,
}

/// Which pass a generator that varies was heard on, and the seeds it played it with, as its row
/// first played on that pass: what `Space g a` on the Gen page keeps (decision 293). It's reported
/// for each live generator whose `Vary` is on, whether that pass is the written one or not, on a
/// queue of its own (`GEN_HEARD_QUEUE`), as `NotePitch` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenPass {
    pub phrase: u32,
    /// Its place in its phrase's list.
    pub generator: usize,
    pub column: ShapeColumn,
    /// Which pass of its phrase, or of its lane's loop, counted from 0.
    pub pass: i64,
    /// The seed it played the pass with, and `Acid`'s second die.
    pub seed: u64,
    pub roll: u64,
}

/// A row of a live generator as it was heard: the cell it played there, if it played one of its
/// own rather than what's written, and on which pass, for the playhead's row, the overview and
/// the status line to show, and `Space g a` and `Space g A` to keep (decision 300). Reported for
/// each row each live generator of a track plays while something reaches its generators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenRow {
    pub phrase: u32,
    /// Its place in its phrase's list.
    pub generator: usize,
    pub column: ShapeColumn,
    /// The stored row.
    pub row: usize,
    /// Which pass of its phrase, or of its lane's loop, counted from 0.
    pub pass: i64,
    /// What it played on the row, laid over what's under it as [`GenCell::apply`] writes it, or
    /// none if it played what's written.
    pub cell: Option<GenCell>,
}

/// What the engine says of a track's live generators as it plays them, on a queue of its own
/// (`GEN_HEARD_QUEUE`), as `NotePitch` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenReport {
    /// A generator that varies was heard on a pass, with these seeds.
    Pass(GenPass),
    /// A live generator played a row.
    Row(GenRow),
    /// Nothing reaches a track's generators now, and a row of this phrase played as written: what
    /// was heard of its generators before is gone.
    Rest { phrase: u32 },
}

/// What may wait in a scratch for the callback to take, at most: a few rows for each run, as a
/// block plays a row or two of each track, and what doesn't fit is dropped, as the queue's are.
const REPORTS_PER_RUN: usize = 4;

/// Which read of a row this is: one that plays, the note's row or a lane's, or one that only looks
/// ahead at it, which reports nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Played {
    Ahead,
    /// The first pulse of the row the note reads.
    Row,
    /// The row an FX column's lane reads.
    Lane(usize),
}

/// A track's share of the scratch: room for the most runs and cells any one of its phrases needs,
/// and the phrase its runs are for now.
#[derive(Debug, Clone, Copy, Default)]
struct Room {
    phrase: Option<u32>,
    /// Whether a generator of one of its phrases varies, so it's heard moving with nothing held.
    varies: bool,
    /// The phrase the app was last told is at rest, as nothing moves the track's generators.
    quiet: Option<u32>,
    runs: usize,
    most_runs: usize,
    cells: usize,
    most_cells: usize,
}

/// What the engine renders moving generators into, made ahead on the edit thread for a version's
/// song: a run for each live generator of the phrase each track plays, and cells for each row of
/// their spans, a whole phrase of them at most for each track (32 bytes a cell, so 128 KB for a
/// phrase of the most rows). The engine takes it as it takes the version, and the queue that
/// returns old versions gives it back to be dropped on the edit thread.
#[derive(Debug, Default)]
pub struct GenScratch {
    rooms: Box<[Room]>,
    runs: Box<[Option<GenRun>]>,
    cells: Box<[GenCell]>,
    /// What's waiting for the callback to take, by track, made ahead with room for
    /// [`REPORTS_PER_RUN`] a run, and never grown.
    reports: Vec<(usize, GenReport)>,
}

/// What a moving generator was last heard as, for tests and for what reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenHeard {
    /// Its place in its phrase's list.
    pub generator: usize,
    pub column: ShapeColumn,
    /// Which pass of its phrase, or of its lane's loop, it was last read on.
    pub pass: i64,
    /// How many times it has rendered since the version came.
    pub renders: u32,
    /// Whether it plays something other than what's written.
    pub moving: bool,
}

/// The pass a row is read on, for a generator on the phrase's rows and for one on each FX
/// column's lane.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Passes {
    pub rows: i64,
    pub lanes: [i64; FX_SLOTS],
}

/// A row of a track's phrase being read, by its stored row.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RowAt {
    pub track: usize,
    pub phrase: u32,
    pub row: usize,
}

impl GenScratch {
    /// Room for every track's live generators: for each track, as many runs as the phrase of it
    /// with the most live generators has, and as many cells as the phrase whose generators cover
    /// the most rows between them.
    pub(crate) fn for_song(song: &Song) -> Self {
        let mut rooms = Vec::with_capacity(song.tracks.len());
        let rooms_len = song.tracks.len();
        let (mut runs, mut cells) = (0, 0);
        for track in &song.tracks {
            let varies = track
                .phrases
                .values()
                .any(|phrase| phrase.generators().iter().any(|g| g.heard().varies()));
            let (most_runs, most_cells) = track
                .phrases
                .values()
                .map(|phrase| {
                    let rows = phrase.generators().iter().filter_map(|g| g.rows());
                    let cells = rows.map(|rows| rows.len().min(MAX_PHRASE_STEPS)).sum();
                    (phrase.generators().len(), cells)
                })
                .fold((0, 0), |(runs, cells), (r, c)| (runs.max(r), cells.max(c)));
            rooms.push(Room {
                phrase: None,
                varies,
                quiet: None,
                runs,
                most_runs,
                cells,
                most_cells,
            });
            runs += most_runs;
            cells += most_cells;
        }
        Self {
            rooms: rooms.into(),
            runs: vec![None; runs].into(),
            cells: vec![GenCell::Under; cells].into(),
            reports: Vec::with_capacity((runs * REPORTS_PER_RUN).max(rooms_len)),
        }
    }

    /// Set a track's runs for a phrase, if they aren't for it already, each with nothing rendered.
    fn enter(&mut self, unders: &Unders, at: RowAt, phrase: &Phrase) -> Option<Room> {
        let room = self.rooms.get_mut(at.track)?;
        if room.phrase == Some(at.phrase) {
            return Some(*room);
        }
        room.phrase = Some(at.phrase);
        let room = *room;
        let mut cells = room.cells;
        let runs = &mut self.runs[room.runs..room.runs + room.most_runs];
        runs.fill(None);
        for ((index, generator), run) in phrase.generators().iter().enumerate().zip(runs) {
            let Some(rows) = generator.rows() else {
                continue;
            };
            let length = rows.len().min(MAX_PHRASE_STEPS);
            if cells + length > room.cells + room.most_cells {
                break;
            }
            let under = unders.get(at.track, at.phrase, index).unwrap_or_default();
            *run = Some(GenRun {
                generator: index,
                column: generator.column,
                rows,
                cells,
                open: open_rows(under, rows, generator.column),
                rendered: None,
                pass: 0,
                renders: 0,
                generates: is_empty(under, rows, generator.column),
                told: None,
                report: None,
            });
            cells += length;
        }
        Some(room)
    }

    /// Whether a track has a generator that varies, so its rows want looking at with nothing held.
    fn varies(&self, track: usize) -> bool {
        self.rooms.get(track).is_some_and(|room| room.varies)
    }

    /// A row as it's heard: its step as written, with the cells of each moving generator on it
    /// laid over, each rendered first if what it's heard with has changed since it last was. A
    /// row `played`, not only looked at ahead, reports the pass of each generator on it that
    /// varies, the first time it's heard on that pass, and the row as each generator on it played
    /// it, or that nothing reaches them now (decision 300).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn view(
        &mut self,
        song: &Song,
        unders: &Unders,
        holds: &GenHolds,
        at: RowAt,
        step: &Step,
        passes: impl FnOnce() -> Passes,
        played: Played,
    ) -> RowView {
        let mut view = RowView::of(step);
        if !holds.any() && !self.varies(at.track) {
            if played != Played::Ahead {
                self.quieten(at);
            }
            return view;
        }
        let Some(phrase) = song
            .tracks
            .get(at.track)
            .and_then(|track| track.phrases.get(&at.phrase))
        else {
            return view;
        };
        if phrase.generators().is_empty() {
            return view;
        }
        let Some(room) = self.enter(unders, at, phrase) else {
            return view;
        };
        let passes = passes();
        for slot in room.runs..room.runs + room.most_runs {
            let Some(run) = &mut self.runs[slot] else {
                continue;
            };
            if !(run.rows.first..=run.rows.last).contains(&at.row) {
                continue;
            }
            let Some(generator) = phrase.generators().get(run.generator) else {
                continue;
            };
            let under = unders
                .get(at.track, at.phrase, run.generator)
                .unwrap_or_default();
            // What the phrase holds: the shape as its macros hold it by hand, if they do (decision 294).
            let written = &generator.heard();
            let source = (written.kind == FillKind::Follow)
                .then(|| song.source_phrase(written.follow.source))
                .flatten()
                .map(|phrase| phrase.loop_steps());
            let context = Context {
                column: run.column,
                open: run.open,
                generates: run.generates,
                sources: &[],
                source,
            };
            let on_lane = match run.column {
                ShapeColumn::Fx { slot, .. } if generator.span == Span::Lane => Some(slot),
                _ => None,
            };
            run.pass = match on_lane {
                Some(slot) => passes.lanes.get(slot).copied().unwrap_or(0),
                None => passes.rows,
            };
            // Its own row, as the note's row is read for every column and a lane's for its own.
            let plays = match played {
                Played::Ahead => false,
                Played::Row => on_lane.is_none(),
                Played::Lane(slot) => on_lane == Some(slot),
            };
            let held = holds.heard(written, &context, run.generates);
            let heard = held.on_pass(run.pass);
            if played != Played::Ahead && held.varies() && run.told != Some(run.pass) {
                run.told = Some(run.pass);
                let pass = GenPass {
                    phrase: at.phrase,
                    generator: run.generator,
                    column: run.column,
                    pass: run.pass,
                    seed: heard.seed,
                    roll: heard.acid.roll,
                };
                run.report = Some(pass);
            }
            let mut row = GenRow {
                phrase: at.phrase,
                generator: run.generator,
                column: run.column,
                row: at.row,
                pass: run.pass,
                cell: None,
            };
            if plays {
                self.rooms[at.track].quiet = None;
            }
            if heard == *written {
                run.rendered = None;
                if plays {
                    tell(&mut self.reports, at.track, GenReport::Row(row));
                }
                continue;
            }
            let length = run.rows.len().min(MAX_PHRASE_STEPS);
            let cells = &mut self.cells[run.cells..run.cells + length];
            if run.rendered != Some(heard) {
                // A `Follow` whose source has gone leaves what it last wrote, as the edit thread's.
                if heard.kind == FillKind::Follow && source.is_none() {
                    continue;
                }
                render_into(under, run.rows, run.column, &heard, source, cells);
                run.rendered = Some(heard);
                run.renders = run.renders.wrapping_add(1);
            }
            // What's under it first, as the edit thread's render restores it, then the cell.
            if let Some(cell) = cells.get(at.row - run.rows.first) {
                if let Some(under) = under.get(at.row) {
                    lay_under(generator, under, &mut view);
                }
                view.lay(run.column, *cell);
                if plays {
                    row.cell = Some(*cell);
                    tell(&mut self.reports, at.track, GenReport::Row(row));
                }
            }
        }
        view
    }

    /// Tell the app a track's phrase plays as written, once, as a row of it plays with nothing
    /// moving its generators, so what it heard of them before is gone.
    fn quieten(&mut self, at: RowAt) {
        let Some(room) = self.rooms.get_mut(at.track) else {
            return;
        };
        if room.most_runs == 0 || room.quiet == Some(at.phrase) {
            return;
        }
        room.quiet = Some(at.phrase);
        let room = *room;
        for run in self.runs[room.runs..room.runs + room.most_runs]
            .iter_mut()
            .flatten()
        {
            run.rendered = None;
        }
        let rest = GenReport::Rest { phrase: at.phrase };
        tell(&mut self.reports, at.track, rest);
    }

    /// Leave a track's generators at rest, as nothing it holds moves them now.
    pub(crate) fn rest(&mut self, track: usize) {
        let room = self.rooms.get(track).copied().unwrap_or_default();
        for run in self.runs[room.runs..room.runs + room.most_runs]
            .iter_mut()
            .flatten()
        {
            run.rendered = None;
        }
    }

    /// Hand what's been heard of every track's generators since the last call to `report`, by
    /// track, in the order it was heard.
    pub(crate) fn take_reports(&mut self, mut report: impl FnMut(usize, GenReport)) {
        for (track, room) in self.rooms.iter().enumerate() {
            for run in self.runs[room.runs..room.runs + room.most_runs]
                .iter_mut()
                .flatten()
            {
                if let Some(pass) = run.report.take() {
                    report(track, GenReport::Pass(pass));
                }
            }
        }
        for (track, each) in self.reports.drain(..) {
            report(track, each);
        }
    }

    /// A track's generators as they were last read, for the phrase it last read one of.
    pub(crate) fn heard(&self, track: usize) -> impl Iterator<Item = GenHeard> + '_ {
        let room = self.rooms.get(track).copied().unwrap_or_default();
        self.runs[room.runs..room.runs + room.most_runs]
            .iter()
            .flatten()
            .map(|run| GenHeard {
                generator: run.generator,
                column: run.column,
                pass: run.pass,
                renders: run.renders,
                moving: run.rendered.is_some(),
            })
    }
}

/// Keep a report for the callback to take, if there's room made for it; one that doesn't fit is
/// dropped, as the queue's are.
fn tell(reports: &mut Vec<(usize, GenReport)>, track: usize, report: GenReport) {
    if reports.len() < reports.capacity() {
        reports.push((track, report));
    }
}

/// A version's scratch, taken once by the engine that plays it.
#[derive(Debug, Default)]
pub(crate) struct ScratchSlot(Mutex<GenScratch>);

impl ScratchSlot {
    pub(crate) fn new(scratch: GenScratch) -> Self {
        let slot = Self(Mutex::new(scratch));
        // Some platforms' locks make what they need on their first use, so it's used once here,
        // on the edit thread, and the engine's use of it never allocates.
        drop(slot.0.lock());
        slot
    }

    /// Swap what's held for another, leaving that here: the engine takes a new version's this way,
    /// and leaves the one it had in the old version going back.
    pub(crate) fn exchange(&self, other: &mut GenScratch) {
        let mut held = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::swap(&mut *held, other);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use vt_model::action::{Action, edit};
    use vt_model::generator::Span;
    use vt_model::lfo::Shape as Wave;
    use vt_model::samples::SamplePool;
    use vt_model::settings::SettingId;
    use vt_model::shape::{Acid, Bits, DEFAULT_RATE, Follow, Pink, Values, Walk, Where};
    use vt_model::song::{Lane, Machine, SynthVoice};
    use vt_model::step::FxCommand;

    use super::*;
    use crate::engine::Engine;
    use crate::published::Published;
    use crate::rt_guard::forbidden;

    const RATE: u32 = 48_000;
    const BLOCK: usize = 256;

    fn shape(kind: FillKind, from: u8, to: u8, place: Where) -> Shape {
        Shape {
            from,
            to,
            curve: 0,
            s: 0,
            kind,
            wave: Wave::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::default(),
            place,
            seed: 3,
            acid: Acid::default(),
            follow: Follow::default(),
            walk: Walk::default(),
            pink: Pink::default(),
            bits: Bits::default(),
            grids: vt_model::shape::Grids::default(),
            vary: 0,
            variation: vt_model::shape::Variation::default(),
        }
    }

    fn generate(song: Song, track: usize, column: ShapeColumn, span: Span, shape: Shape) -> Song {
        let action = Action::Generate {
            track,
            phrase: 0,
            column,
            span,
            shape,
        };
        Arc::unwrap_or_clone(edit(&Arc::new(song), &action).expect("the edit applies"))
    }

    fn version(song: &Song) -> Arc<Published> {
        Arc::new(Published::new(
            Arc::new(song.clone()),
            Arc::new(SamplePool::default()),
        ))
    }

    fn acid_column(song: &Song) -> ShapeColumn {
        ShapeColumn::Note {
            scale: song.scale,
            writes: Some(0x64),
        }
    }

    /// One synth track playing a phrase of `rows` with an `Acid` line over all of them, and a
    /// ramp of `Cut` over a lane in FX column 1 and Euclid's velocities on a second track.
    fn song(rows: usize) -> Song {
        let mut song = Song::new(2);
        song.millibpm = vt_model::constants::MAX_MILLIBPM;
        for track in &mut song.tracks[..2] {
            track.machine = Machine::Plaits(SynthVoice::Acid);
        }
        let notes = Phrase::from_steps(vec!["C-3 64 --- --- ---".parse().unwrap(); 16]);
        let mut rests = Phrase::new(rows);
        rests.set_lane(
            0,
            Some(Lane {
                start: 0,
                length: 7,
            }),
        );
        song.tracks[0].phrases.insert(0, Arc::new(rests));
        song.tracks[1].phrases.insert(0, Arc::new(notes));
        let span = Span::rows(StepRange::between(0, rows - 1));
        let acid = shape(FillKind::Acid, 36, 48, Where::AllRows);
        let song = generate(song.clone(), 0, acid_column(&song), span, acid);
        let cut = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::Set(SettingId::CUT),
        };
        let ramp = shape(FillKind::Ramp, 0x10, 0xE0, Where::AllRows);
        let song = generate(song, 0, cut, Span::Lane, ramp);
        let euclid = Where::Euclid { hits: 5, rotate: 2 };
        let velocities = shape(FillKind::Ramp, 0x20, 0x7F, euclid);
        let rows16 = Span::rows(StepRange::between(0, 15));
        generate(song, 1, ShapeColumn::Velocity, rows16, velocities)
    }

    /// Playing with every generator moving, its settings changed every block and a new version
    /// every few, rendering, laying cells and swapping scratch allocate and free nothing.
    #[test]
    fn moving_generators_never_allocate_in_the_callback() {
        let first = song(64);
        let acid = acid_column(&first);
        let cut = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::Set(SettingId::CUT),
        };
        let mut engine = Engine::new(RATE, version(&first));
        engine.play_from(0);
        let mut out = vec![0.0; BLOCK * 2];
        let mut violations = 0;
        let mut old = Vec::with_capacity(64);
        for round in 0..400 {
            let next = (round % 8 == 0).then(|| version(&song(16 + round % 48)));
            let number = (round * 7 % 101) as i32;
            let ((), count) = forbidden(|| {
                if let Some(next) = next {
                    old.push(engine.swap(next));
                }
                engine.hold_generator(0, acid, GenParam::Density, Some(number));
                engine.hold_generator(0, cut, GenParam::To, Some(number * 2));
                engine.hold_generator(1, ShapeColumn::Velocity, GenParam::Hits, Some(number % 16));
                engine.process(&mut out);
            });
            violations += count;
        }
        assert_eq!(violations, 0);
        let renders: u32 = (0..2)
            .flat_map(|track| engine.generators_heard(track))
            .map(|heard| heard.renders)
            .sum();
        assert!(renders > 0, "the generators rendered");
    }

    /// The song with macro 1 holding the `Acid`'s density and the `Cut` ramp's end, an LFO on it,
    /// a `MAC1` lock every third row, and macro 2 on the Euclid's hits, turned to `turn`.
    fn macro_song(rows: usize, turn: u16) -> Song {
        use vt_model::lfo::{Lfo, Rate};
        use vt_model::macros::{GenColumn, MacroMap, MacroTarget};
        use vt_model::modulation::ModSlot;

        let mut song = song(rows);
        let target = |column, param| MacroTarget::Gen { column, param };
        let track = &mut song.tracks[0];
        track.macros[0].maps = vec![
            MacroMap::numbers(target(GenColumn::Note, GenParam::Density), (0, 100)),
            MacroMap::numbers(target(GenColumn::Fx(0), GenParam::To), (0x20, 0xF0)),
        ];
        track.mods[3] = ModSlot::lfo(
            SettingId::macro_at(0),
            Lfo {
                rate: Rate::Steps(2),
                ..Lfo::default()
            },
        )
        .with_depth(0x7F);
        let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
        for row in (0..rows).step_by(3) {
            if let Some(step) = phrase.step_mut(row) {
                step.fx[1] = "macro1:C0".parse().unwrap();
            }
        }
        let other = &mut song.tracks[1];
        let hits = target(GenColumn::Velocity, GenParam::Hits);
        other.macros[1].maps = vec![MacroMap::numbers(hits, (1, 16))];
        other.macros[1].value = turn;
        song
    }

    /// With macros moving generators by their turns, locks and an LFO, and new versions coming,
    /// putting the macros, rendering and reporting what's heard allocate and free nothing.
    #[test]
    fn macros_moving_generators_never_allocate_in_the_callback() {
        let mut engine = Engine::new(RATE, version(&macro_song(32, 0)));
        engine.play_from(0);
        let mut out = vec![0.0; BLOCK * 2];
        let (mut violations, mut rows) = (0, 0);
        let mut old = Vec::with_capacity(64);
        for round in 0..400 {
            let turn = (round as u32 * 0x0700 % 0xFF00) as u16;
            let next = (round % 16 == 0).then(|| version(&macro_song(32, turn)));
            let ((), count) = forbidden(|| {
                if let Some(next) = next {
                    old.push(engine.swap(next));
                }
                engine.process(&mut out);
                engine.take_gen_reports(|_, report| {
                    rows += usize::from(matches!(report, GenReport::Row(_)));
                });
            });
            violations += count;
        }
        assert_eq!(violations, 0);
        assert!(rows > 0, "rows were heard");
        assert!(engine.generators_heard(0).any(|heard| heard.renders > 0));
    }

    /// The song with its `Acid` line given a new seed each pass, over a phrase of `rows`.
    fn varying(rows: usize) -> Song {
        let song = song(rows);
        let mut shape = song.tracks[0].phrases[&0].generators()[0].shape;
        shape.vary = 1;
        let action = Action::Reshape {
            track: 0,
            phrase: 0,
            generator: 0,
            shape,
        };
        Arc::unwrap_or_clone(edit(&Arc::new(song), &action).expect("the edit applies"))
    }

    /// With `Vary` on and nothing held, rendering a pass, laying its cells, reporting the passes
    /// and swapping versions allocate and free nothing.
    #[test]
    fn a_generator_that_varies_never_allocates_in_the_callback() {
        let mut engine = Engine::new(RATE, version(&varying(16)));
        engine.play_from(0);
        let mut out = vec![0.0; BLOCK * 2];
        let (mut violations, mut reported) = (0, 0);
        let mut old = Vec::with_capacity(64);
        for round in 0..600 {
            let next = (round % 100 == 50).then(|| version(&varying(16 + round % 3)));
            let ((), count) = forbidden(|| {
                if let Some(next) = next {
                    old.push(engine.swap(next));
                }
                engine.process(&mut out);
                engine.take_gen_reports(|_, report| {
                    reported += usize::from(matches!(report, GenReport::Pass(_)));
                });
            });
            violations += count;
        }
        assert_eq!(violations, 0);
        assert!(reported > 2, "{reported} passes reported");
        let acid = engine.generators_heard(0).find(|heard| heard.moving);
        assert!(acid.is_some_and(|heard| heard.renders > 0));
    }

    /// Play a song's first `rows` rows a step at a time, holding the `Acid`'s density at another
    /// number before each if `moving`: how long the slowest step and all of them took, how many
    /// renders there were and how many allocations.
    fn steps_timed(song: &Song, rows: usize, moving: bool) -> (Duration, Duration, u32, usize) {
        let acid = acid_column(song);
        let mut engine = Engine::new(RATE, version(song));
        engine.play_from(0);
        let mut out = vec![0.0; step_frames(song) * 2];
        let (mut worst, mut total, mut violations) = (Duration::ZERO, Duration::ZERO, 0);
        for row in 0..rows {
            // Never the written 50, so each row's is new.
            let density = if row % 2 == 0 { 10 } else { 90 } + (row % 7) as i32;
            if moving {
                engine.hold_generator(0, acid, GenParam::Density, Some(density));
            }
            let started = Instant::now();
            let ((), count) = forbidden(|| engine.process(&mut out));
            let took = started.elapsed();
            (worst, total) = (worst.max(took), total + took);
            violations += count;
        }
        let renders = engine
            .generators_heard(0)
            .find(|heard| heard.column == acid)
            .map_or(0, |heard| heard.renders);
        (worst, total, renders, violations)
    }

    fn step_frames(song: &Song) -> usize {
        let step = vt_model::constants::TICKS_PER_STEP;
        crate::engine::sample_of_tick(step, RATE, song.millibpm) as usize
    }

    /// The worst case §7.7 sets: a 4,096-row `Acid` rendered again on every row, as an input
    /// moving every row would. Nothing allocates, and in a release build the render takes a sliver
    /// of the time a step plays for. `--nocapture` prints the times.
    #[test]
    fn a_4096_row_acid_rendered_every_row_keeps_up() {
        let song = song(MAX_PHRASE_STEPS);
        let rows = 64;
        let (rest_worst, rest_total, rest_renders, _) = steps_timed(&song, rows, false);
        let (worst, total, renders, violations) = steps_timed(&song, rows, true);
        // The render alone, as the engine runs it, over the same span.
        let phrase = &song.tracks[0].phrases[&0];
        let generator = &phrase.generators()[0];
        let under = generator.under(phrase.steps());
        let span = generator.rows().unwrap();
        let mut cells = vec![GenCell::Under; span.len()];
        let mut shape = generator.shape;
        let (mut render_worst, mut render_total) = (Duration::ZERO, Duration::ZERO);
        for density in 0..rows as u8 {
            shape.acid.density = density;
            let started = Instant::now();
            render_into(&under, span, generator.column, &shape, None, &mut cells);
            let took = started.elapsed();
            (render_worst, render_total) = (render_worst.max(took), render_total + took);
        }
        let frames = step_frames(&song);
        let real = Duration::from_secs_f64(frames as f64 / f64::from(RATE));
        let mean = |total: Duration| total / rows as u32;
        println!(
            "a {}-row Acid, {rows} rows, steps of {frames} frames ({real:?} of sound): at rest \
             {:?} a step, {rest_worst:?} at worst; rendered every row ({renders} renders) {:?} a \
             step, {worst:?} at worst; the render alone {:?}, {render_worst:?} at worst",
            span.len(),
            mean(rest_total),
            mean(total),
            mean(render_total),
        );
        assert_eq!(violations, 0);
        assert_eq!(rest_renders, 0);
        assert!(renders as usize >= rows - 1, "{renders} renders");
        // Held loosely, as other tests share the machine: in a release build the render is a
        // sliver of a step, and a step that renders keeps well up with what it plays.
        if !cfg!(debug_assertions) {
            let render = mean(render_total);
            assert!(
                render < real / 20,
                "{render:?} a render, for {real:?} of sound"
            );
            let step = mean(total);
            assert!(step < real / 2, "{step:?} a step to make {real:?}");
        }
    }
}
