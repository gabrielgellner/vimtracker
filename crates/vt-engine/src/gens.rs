//! Live generators as the engine plays them.
//!
//! At rest, what's heard is what's written: a phrase holds what each of its generators renders, so
//! the engine reads it as it reads any other. A generator whose settings the engine holds at other
//! numbers than its `Shape`'s is moving, and so is one whose `Vary` gives a pass a seed of its own
//! ([`Shape::on_pass`]): the engine renders its span again from the shape as heard, with the edit
//! thread's own [`render_into`], and lays its cells over each row it reads through a [`RowView`].
//! What it renders is kept beside the inputs it rendered from, so a row read with the same ones
//! takes its cell from what's kept, and only a change renders the span again.
//!
//! Nothing here allocates or locks on the audio thread: each version comes to the engine with a
//! `GenScratch` made ahead on the edit thread with room for every track's runs and cells, in a
//! [`Handover`](crate::published::Handover), and the engine gives back the one it had with the
//! version it had.

use std::collections::BTreeMap;
use std::sync::Arc;

use vt_model::constants::{FX_SLOTS, MAX_PHRASE_STEPS};
use vt_model::generator::{Span, lay_under};
use vt_model::macros::GenColumn;
use vt_model::scale::Scale;
use vt_model::selection::StepRange;
use vt_model::shape::{
    Context, FillKind, GenCell, GenParam, Kept, RowView, Shape, ShapeColumn, counted, is_empty,
    line_steps, render_into,
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
/// set the registry's settings.
#[derive(Debug, Clone)]
pub(crate) struct GenHolds {
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
    pub(crate) fn hold(&mut self, column: ShapeColumn, param: GenParam, number: Option<i32>) {
        if let Some(at) = param_place(param) {
            self.held[column_place(column)][at] = number;
            self.count();
        }
    }

    /// Hold a column's setting where a macro puts it, the column as a mapping names it.
    pub(crate) fn hold_by_macro(&mut self, column: GenColumn, param: GenParam, number: i32) {
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
    pub(crate) fn forget_macros(&mut self) {
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
    pub(crate) fn any(&self) -> bool {
        self.any
    }

    /// How many times a macro has moved a setting, counting round.
    pub(crate) fn changes(&self) -> u32 {
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
                && param.exists_for(&heard, context.column, generates)
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

/// The most lines a `Mini` typed again and again keeps waiting, the latest first.
const MOST_WAITING: usize = 4;

/// A live `Mini` whose line was typed again (decision 462): what it played before, which, with
/// its track's `Cue` on, it plays on until its line next comes round, so a retyped line is never
/// heard half one and half the other. Made on the edit thread with each version, its cells
/// rendered there, so the engine only reads them.
#[derive(Debug, Clone)]
pub(crate) struct Wait {
    /// The track, the phrase and the generator's place in its list.
    key: (usize, u32, usize),
    column: ShapeColumn,
    rows: StepRange,
    /// Whether this version's line is new, rather than the last version's wait carried on.
    fresh: bool,
    /// The lines it may play on, the last version's first, then those that one was waiting on,
    /// each with its lap in rows and its cells for its rows.
    lines: Box<[(usize, Arc<[GenCell]>)]>,
}

/// Every live `Mini` of a song whose line isn't the last version's, with what that one played,
/// and every one the last version was waiting on whose line is the same, carried on.
pub(crate) fn waits(song: &Song, last: Option<(&Song, &Unders, &[Wait])>) -> Box<[Wait]> {
    let Some((old_song, old_unders, old_waits)) = last else {
        return Box::default();
    };
    let mut waits = Vec::new();
    for (track, owner) in song.tracks.iter().enumerate() {
        for (id, phrase) in &owner.phrases {
            for (index, generator) in phrase.generators().iter().enumerate() {
                if generator.shape.kind != FillKind::Mini {
                    continue;
                }
                let (Some(rows), key) = (generator.rows(), (track, *id, index)) else {
                    continue;
                };
                let Some(old) = old_song
                    .tracks
                    .get(track)
                    .and_then(|owner| owner.phrases.get(id))
                    .and_then(|phrase| phrase.generators().get(index))
                    .filter(|old| {
                        old.shape.kind == FillKind::Mini
                            && old.column == generator.column
                            && old.rows() == Some(rows)
                    })
                else {
                    continue;
                };
                let carried = old_waits.iter().find(|wait| wait.key == key);
                if old.rhythm() == generator.rhythm() {
                    if let Some(wait) = carried.filter(|wait| wait.rows == rows) {
                        waits.push(Wait {
                            fresh: false,
                            ..wait.clone()
                        });
                    }
                    continue;
                }
                let heard = old.heard();
                let line = line_steps(old.texts().rhythm) as usize * usize::from(heard.mini.step);
                let length = rows.len().min(MAX_PHRASE_STEPS);
                let mut cells = vec![GenCell::Under; length];
                let under = old_unders.get(track, *id, index).unwrap_or_default();
                render_into(
                    under,
                    rows,
                    old.column,
                    &heard,
                    old.texts(),
                    None,
                    &mut cells,
                );
                let mut lines = vec![(line.max(1), Arc::from(cells))];
                if let Some(wait) = carried {
                    lines.extend(wait.lines.iter().cloned());
                }
                lines.truncate(MOST_WAITING);
                waits.push(Wait {
                    key,
                    column: generator.column,
                    rows,
                    fresh: true,
                    lines: lines.into(),
                });
            }
        }
    }
    waits.into()
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
    /// The hash of the values its `List` goes through, from its [`Heard`].
    list: u64,
    /// The shape it last rendered, as heard, the column it rendered on, a Note column with the
    /// scale it went through, and its list's hash, if it has since the version came.
    rendered: Option<(Shape, ShapeColumn, u64)>,
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
    /// Where what it was last heard as is kept across passes, for `Mutate` (decision 430).
    kept: Option<usize>,
    /// The variation it last rendered while it mutates, which its cells are.
    variation: Option<i64>,
}

/// What a live generator was last heard as, kept across its passes, phrases and versions, so a
/// new variation can draw only `Mutate`'s share of its rows again and keep the rest as they were
/// heard (decision 430): the cells of the variation it last rendered, and those of the one before,
/// which that one kept its rows from. Room for every live generator's rows is made ahead.
#[derive(Debug, Clone, Copy)]
struct Heard {
    /// The track, the phrase and the generator's place in its list.
    key: (usize, u32, usize),
    /// Where its cells start in `now` and `was`, and how many there are.
    cells: usize,
    length: usize,
    /// The variation whose cells are in `now`, and the one before it, whose are in `was`.
    now: Option<i64>,
    was: Option<i64>,
    /// The hash of the values its `List` goes through and of its `Own` rhythm's text, worked out
    /// on the edit thread, which its run's render key holds, so neither is ever copied to the
    /// audio thread.
    list: u64,
}

/// A `List`'s values and an `Own` rhythm's text hashed, FNV-1a, each with its count, for a run's
/// render key.
fn list_hash(list: &[u8], rhythm: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for bytes in [list, rhythm.as_bytes()] {
        let count = bytes.len() as u64;
        for byte in count.to_le_bytes().iter().chain(bytes) {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// Every live generator's [`Heard`], by key.
#[derive(Debug, Default)]
struct Memory {
    kept: Box<[Heard]>,
    now: Box<[GenCell]>,
    was: Box<[GenCell]>,
}

impl Memory {
    /// Room for every live generator of every phrase of the song, by key.
    fn for_song(song: &Song) -> Self {
        let mut kept = Vec::new();
        let mut cells = 0;
        for (track, owner) in song.tracks.iter().enumerate() {
            for (id, phrase) in &owner.phrases {
                for (generator, live) in phrase.generators().iter().enumerate() {
                    let Some(rows) = live.rows() else {
                        continue;
                    };
                    let length = rows.len().min(MAX_PHRASE_STEPS);
                    kept.push(Heard {
                        key: (track, *id, generator),
                        cells,
                        length,
                        now: None,
                        was: None,
                        list: list_hash(live.list(), live.rhythm()),
                    });
                    cells += length;
                }
            }
        }
        Self {
            kept: kept.into(),
            now: vec![GenCell::Under; cells].into(),
            was: vec![GenCell::Under; cells].into(),
        }
    }

    fn find(&self, key: (usize, u32, usize)) -> Option<usize> {
        self.kept.binary_search_by_key(&key, |kept| kept.key).ok()
    }

    /// Forget everything heard, as playing starts.
    fn forget(&mut self) {
        for kept in &mut self.kept {
            kept.now = None;
            kept.was = None;
        }
    }

    /// Take what another version's memory heard of each generator both have, with as many rows.
    fn keep_from(&mut self, old: &Memory) {
        let mut others = old.kept.iter().filter(|kept| kept.now.is_some()).peekable();
        for kept in &mut self.kept {
            while others.next_if(|other| other.key < kept.key).is_some() {}
            let Some(other) = others.next_if(|other| other.key == kept.key) else {
                continue;
            };
            if other.length != kept.length {
                continue;
            }
            let (to, from) = (kept.cells..kept.cells + kept.length, other.cells);
            self.now[to.clone()].copy_from_slice(&old.now[from..from + other.length]);
            self.was[to].copy_from_slice(&old.was[from..from + other.length]);
            kept.now = other.now;
            kept.was = other.was;
        }
    }
}

/// Render a variation of a generator that mutates into `cells`: a new variation moves what was
/// heard of the last one to `was` and draws `Mutate`'s share of its rows again, keeping the rest
/// from it; the same one again, as a setting moves, renders it again over the same rows kept; and
/// the one before, read again ahead of its own last rows, is as it was heard.
fn render_mutating(
    kept: &mut Heard,
    now: &mut [GenCell],
    was: &mut [GenCell],
    cells: &mut [GenCell],
    (shape, variation): (&Shape, i64),
    render: impl FnOnce(&mut [GenCell]),
) {
    if kept.now != Some(variation) {
        if kept.was == Some(variation) {
            cells.copy_from_slice(was);
            return;
        }
        if kept.now.is_some() {
            was.copy_from_slice(now);
        }
        kept.was = kept.now;
        kept.now = Some(variation);
    }
    render(cells);
    if kept.was.is_some() {
        for (at, (cell, last)) in cells.iter_mut().zip(was.iter()).enumerate() {
            if !shape.draws_again(variation, at) {
                *cell = *last;
            }
        }
    }
    now.copy_from_slice(cells);
}

/// Which pass a generator that varies was heard on, and the seeds it played it with, as its row
/// first played on that pass: what `Space g a` on the Gen page keeps. It's reported for each live
/// generator whose `Vary` is on, whether that pass is the written one or not, on a queue of its own
/// (`GEN_HEARD_QUEUE`), as `NotePitch` is.
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
    /// A `Ring`'s register as the pass started, and which pass that is from its first, which
    /// `Space g a` keeps.
    pub ring: Option<Kept>,
    /// The count an `Acid` with loops started the pass at, which `Space g a` keeps so its loops
    /// go on from there.
    pub acid_at: Option<u64>,
}

/// A row of a live generator as it was heard: the cell it played there, if it played one of its
/// own rather than what's written, and on which pass, for the playhead's row, the overview and
/// the status line to show, and `Space g a` and `Space g A` to keep. Reported for each row each
/// live generator of a track plays while something reaches its generators.
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
/// phrase of the most rows). It goes to the engine with its version, and comes back with the old
/// one on the queue that returns old versions, to be dropped on the edit thread.
#[derive(Debug, Default)]
pub(crate) struct GenScratch {
    rooms: Box<[Room]>,
    runs: Box<[Option<GenRun>]>,
    cells: Box<[GenCell]>,
    /// What each live generator was last heard as, for `Mutate`.
    memory: Memory,
    /// What's waiting for the callback to take, by track, made ahead with room for
    /// [`REPORTS_PER_RUN`] a run, and never grown.
    reports: Vec<(usize, GenReport)>,
    /// For each of its version's [`Wait`]s, which of its lines plays on, or none once it's let
    /// go and the new line plays.
    waiting: Box<[Option<u8>]>,
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
    pub(crate) fn for_song(song: &Song, waits: &[Wait]) -> Self {
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
            memory: Memory::for_song(song),
            reports: Vec::with_capacity((runs * REPORTS_PER_RUN).max(rooms_len)),
            waiting: waits.iter().map(|_| Some(0)).collect(),
        }
    }

    /// Carry on what the last version's scratch was waiting on: a line typed again while one
    /// still waits plays on what that one played, and a wait carried on plays on as it was, or
    /// stays let go. Allocates nothing.
    pub(crate) fn keep_waits_from(&mut self, old: &GenScratch, was: &[Wait], waits: &[Wait]) {
        for (wait, held) in waits.iter().zip(self.waiting.iter_mut()) {
            let before = was
                .iter()
                .zip(old.waiting.iter())
                .find(|(other, _)| other.key == wait.key)
                .and_then(|(_, held)| *held);
            let most = wait.lines.len().saturating_sub(1) as u8;
            *held = match (wait.fresh, before) {
                (true, Some(at)) => Some(at.saturating_add(1).min(most)),
                (true, None) => Some(0),
                (false, before) => before.map(|at| at.min(most)),
            };
        }
    }

    /// Whether a track has a `Mini` playing on its last line, waiting for its lap.
    fn waits_on(&self, track: usize, waits: &[Wait]) -> bool {
        waits
            .iter()
            .zip(self.waiting.iter())
            .any(|(wait, held)| wait.key.0 == track && held.is_some())
    }

    /// Forget what every generator was heard as, as playing starts, so a generator that mutates
    /// starts from a whole variation, as Steps' `Mutate` does.
    pub(crate) fn forget_heard(&mut self) {
        self.memory.forget();
        for run in self.runs.iter_mut().flatten() {
            run.variation = None;
            run.rendered = None;
        }
    }

    /// Keep what the last version's scratch heard of each generator this one has, so an edit
    /// doesn't start a generator that mutates again. Copies, allocating nothing.
    pub(crate) fn keep_heard_from(&mut self, old: &GenScratch) {
        self.memory.keep_from(&old.memory);
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
            let kept = self.memory.find((at.track, at.phrase, index));
            *run = Some(GenRun {
                generator: index,
                column: generator.column,
                rows,
                cells,
                list: kept.map_or(0, |at| self.memory.kept[at].list),
                rendered: None,
                pass: 0,
                renders: 0,
                generates: is_empty(under, rows, generator.column),
                told: None,
                report: None,
                kept,
                variation: None,
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
    /// it, or that nothing reaches them now. In a section with a `scale` of its own, a generator
    /// on the Note column written in another scale is moving too: it renders through the
    /// section's (decision 426).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn view(
        &mut self,
        (song, waits, cues): (&Song, &[Wait], bool),
        unders: &Unders,
        holds: &GenHolds,
        at: RowAt,
        step: &Step,
        passes: impl FnOnce() -> Passes,
        played: Played,
        scale: Option<Scale>,
    ) -> RowView {
        let mut view = RowView::of(step);
        let phrase_here = || {
            song.tracks
                .get(at.track)
                .and_then(|track| track.phrases.get(&at.phrase))
        };
        // Only a scale some Note generator of the phrase isn't written in moves anything.
        let rescaled = scale.filter(|scale| {
            phrase_here().is_some_and(|phrase| {
                phrase
                    .generators()
                    .iter()
                    .any(|generator| generator.scale().is_some_and(|own| own != *scale))
            })
        });
        let waiting = self.waits_on(at.track, waits);
        if rescaled.is_none() && !holds.any() && !self.varies(at.track) && !waiting {
            if played != Played::Ahead {
                self.quieten(at);
            }
            return view;
        }
        let Some(phrase) = phrase_here() else {
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
            // What the phrase holds: the shape as its macros hold it by hand, if they do.
            let written = &generator.heard();
            let source = (written.kind == FillKind::Follow)
                .then(|| song.source_phrase(written.follow.source))
                .flatten()
                .map(|phrase| phrase.loop_steps());
            // A Note column renders through the section's own scale where it has one.
            let column = match (run.column, rescaled) {
                (ShapeColumn::Note { writes, .. }, Some(scale)) => {
                    ShapeColumn::Note { scale, writes }
                }
                (column, _) => column,
            };
            let context = Context {
                column,
                rows: counted(under, run.rows, column, written),
                generates: run.generates,
                sources: &[],
                source,
                list: generator.list(),
                rhythm: generator.rhythm(),
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
            // A `Mini` typed again plays on its last line, with `Cue` on, until a lap starts.
            if waiting
                && let Some(held) = waits
                    .iter()
                    .position(|wait| wait.key == (at.track, at.phrase, run.generator))
                && let Some(line) = self.waiting[held]
            {
                let wait = &waits[held];
                let offset = at.row - run.rows.first;
                let (lap, cells) = &wait.lines[usize::from(line).min(wait.lines.len() - 1)];
                if !cues || plays && offset % *lap == 0 || wait.column != run.column {
                    self.waiting[held] = None;
                } else if let Some(cell) = cells.get(offset) {
                    if let Some(under) = under.get(at.row) {
                        lay_under(generator, under, &mut view);
                    }
                    view.lay(run.column, *cell);
                    if plays {
                        self.rooms[at.track].quiet = None;
                        let row = GenRow {
                            phrase: at.phrase,
                            generator: run.generator,
                            column: run.column,
                            row: at.row,
                            pass: run.pass,
                            cell: Some(*cell),
                        };
                        tell(&mut self.reports, at.track, GenReport::Row(row));
                    }
                    continue;
                }
            }
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
                    ring: (heard.kind == FillKind::Ring)
                        .then(|| heard.ring.kept_on(heard.seed, run.rows.len())),
                    acid_at: (heard.kind == FillKind::Acid && heard.acid.loops())
                        .then(|| heard.acid.kept_at(run.rows.len())),
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
            // Drawing only some rows again, it's rendered on every variation, the written one too,
            // so the next has its rows to keep (decision 430).
            let kept = run.kept.filter(|_| held.mutates());
            let variation = kept.and_then(|_| held.variation_on(run.pass));
            if kept.is_none() && heard == *written && column == run.column {
                run.rendered = None;
                if plays {
                    tell(&mut self.reports, at.track, GenReport::Row(row));
                }
                continue;
            }
            let length = run.rows.len().min(MAX_PHRASE_STEPS);
            let cells = &mut self.cells[run.cells..run.cells + length];
            let key = (heard, column, run.list);
            if run.rendered != Some(key) || run.variation != variation {
                // A `Follow` whose source has gone leaves what it last wrote, as the edit thread's.
                if heard.kind == FillKind::Follow && source.is_none() {
                    continue;
                }
                let render = |cells: &mut [GenCell]| {
                    render_into(
                        under,
                        run.rows,
                        column,
                        &heard,
                        generator.texts(),
                        source,
                        cells,
                    );
                };
                match (kept.map(|at| &mut self.memory.kept[at]), variation) {
                    (Some(kept), Some(variation)) if kept.length == length => {
                        let span = kept.cells..kept.cells + length;
                        render_mutating(
                            kept,
                            &mut self.memory.now[span.clone()],
                            &mut self.memory.was[span],
                            cells,
                            (&held, variation),
                            render,
                        );
                    }
                    _ => render(cells),
                }
                run.rendered = Some(key);
                run.variation = variation;
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use vt_model::action::{Action, edit};
    use vt_model::generator::Span;

    use vt_model::samples::SamplePool;
    use vt_model::settings::SettingId;
    use vt_model::shape::Where;
    use vt_model::song::{Lane, Machine, SynthVoice};
    use vt_model::step::FxCommand;

    use super::*;
    use crate::engine::Engine;
    use crate::published::{Handover, Published};
    use crate::rt_guard::forbidden;

    const RATE: u32 = 48_000;
    const BLOCK: usize = 256;

    fn shape(kind: FillKind, from: u8, to: u8, place: Where) -> Shape {
        Shape {
            from,
            to,
            kind,
            place,
            seed: 3,
            ..Shape::default()
        }
    }

    fn generate(song: Song, track: usize, column: ShapeColumn, span: Span, shape: Shape) -> Song {
        let action = Action::Generate {
            track,
            phrase: 0,
            column,
            span,
            shape,
            list: Box::default(),
            rhythm: Box::default(),
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
            scale: song.saved_scale,
            writes: Some(0x64),
        }
    }

    /// One synth track playing a phrase of `rows` with an `Acid` line over all of them, and a
    /// ramp of `Cut` over a lane in FX column 1 and Euclid's velocities on a second track.
    fn song(rows: usize) -> Song {
        let mut song = Song::new(2);
        song.millibpm = vt_model::constants::MAX_MILLIBPM;
        for track in song.tracks.iter_mut().take(2) {
            track.machine = Machine::Plaits(SynthVoice::Acid);
        }
        let notes = Phrase::from_steps(vec!["C-3 64 --- --- ---".parse().unwrap(); 16]);
        let mut rests = Phrase::new(rows);
        rests.set_lane(0, Some(Lane::new(0, 7)));
        song.tracks[0].phrases.insert(0, Arc::new(rests));
        song.tracks[1].phrases.insert(0, Arc::new(notes));
        let span = Span::rows(StepRange::between(0, rows - 1));
        let acid = shape(FillKind::Acid, 36, 48, vt_model::shape::BEATS);
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
            let next = (round % 8 == 0).then(|| Handover::new(version(&song(16 + round % 48))));
            let number = (round * 7 % 101) as i32;
            let ((), count) = forbidden(|| {
                if let Some(next) = next {
                    old.push(engine.hand_over(next));
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

    /// The scratch a version came with goes back with it as the next comes, to be dropped where
    /// the next was made, and the engine renders into the next one's.
    #[test]
    fn a_versions_scratch_goes_back_with_it() {
        let first = version(&song(64));
        let made = GenScratch::for_song(&first.song, &first.waits).cells.len();
        assert!(made > 0, "there's room for the generators");
        let mut engine = Engine::new(RATE, Arc::clone(&first));
        engine.play_from(0);
        engine.process(&mut vec![0.0; BLOCK * 2]);
        let next = Handover::new(version(&song(16)));
        let room = next.scratch.cells.len();
        let back = engine.hand_over(next);
        assert!(Arc::ptr_eq(&back.version, &first));
        assert_eq!(back.scratch.cells.len(), made);
        let next = Handover::new(version(&song(32)));
        assert_eq!(engine.hand_over(next).scratch.cells.len(), room);
    }

    /// The song with macro 1 holding the `Acid`'s density and the `Cut` ramp's end, an LFO on it,
    /// a `MAC1` lock every third row, and macro 2 on the Euclid's hits, turned to `turn`, each
    /// mapping bent.
    fn macro_song(rows: usize, turn: u16) -> Song {
        use vt_model::lfo::{Lfo, Rate};
        use vt_model::macros::{GenColumn, MacroMap, MacroTarget};
        use vt_model::modulation::ModSlot;

        let mut song = song(rows);
        let target = |column, param| MacroTarget::Gen { column, param };
        let track = &mut song.tracks[0];
        track.macros[0].maps = vec![
            MacroMap::numbers(target(GenColumn::Note, GenParam::Density), (0, 100)).bent(60, 0),
            MacroMap::numbers(target(GenColumn::Fx(0), GenParam::To), (0x20, 0xF0)).bent(-40, 70),
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
        other.macros[1].maps = vec![MacroMap::numbers(hits, (1, 16)).bent(0, -50)];
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
            let next = (round % 16 == 0).then(|| Handover::new(version(&macro_song(32, turn))));
            let ((), count) = forbidden(|| {
                if let Some(next) = next {
                    old.push(engine.hand_over(next));
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
            list: None,
            rhythm: None,
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
            let next =
                (round % 100 == 50).then(|| Handover::new(version(&varying(16 + round % 3))));
            let ((), count) = forbidden(|| {
                if let Some(next) = next {
                    old.push(engine.hand_over(next));
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

    /// One synth track playing a phrase of 16 notes, a random fill on them playing a rhythm of
    /// its own, `text`, kept on the generator (decision 461).
    fn own(text: &str) -> Song {
        let mut song = Song::new(1);
        song.millibpm = vt_model::constants::MAX_MILLIBPM;
        song.tracks[0].machine = Machine::Plaits(SynthVoice::Acid);
        let notes = Phrase::from_steps(vec!["C-3 64 --- --- ---".parse().unwrap(); 16]);
        song.tracks[0].phrases.insert(0, Arc::new(notes));
        let place = Where::Own(vt_model::shape::Own {
            step: 2,
            ..Default::default()
        });
        let action = Action::Generate {
            track: 0,
            phrase: 0,
            column: acid_column(&song),
            span: Span::rows(StepRange::between(0, 15)),
            shape: shape(FillKind::Random, 36, 60, place),
            list: Box::default(),
            rhythm: text.into(),
        };
        Arc::unwrap_or_clone(edit(&Arc::new(song), &action).expect("the edit applies"))
    }

    /// An `Own` rhythm whose `< >` runs on from pass to pass renders each pass, its `Step` held
    /// by hand and its `x*3` a `RET`, and a new version with another text is heard: none of it
    /// allocates or frees in the callback.
    #[test]
    fn an_own_rhythm_renders_each_pass_without_allocating() {
        let texts = ["x <. x . [x x]> x*3 x?", "<x . .> [x x](3,8) x!2"];
        let first = own(texts[0]);
        let generator = &first.tracks[0].phrases[&0].generators()[0];
        assert!(generator.heard().varies(), "its laps run on");
        let column = generator.column;
        let mut engine = Engine::new(RATE, version(&first));
        engine.play_from(0);
        let mut out = vec![0.0; BLOCK * 2];
        let (mut violations, mut reported) = (0, 0);
        let mut old = Vec::with_capacity(64);
        for round in 0..600 {
            let next = (round % 100 == 50).then(|| Handover::new(version(&own(texts[round % 2]))));
            let ((), count) = forbidden(|| {
                if let Some(next) = next {
                    old.push(engine.hand_over(next));
                }
                let step = (round / 40 % 4 + 1) as i32;
                engine.hold_generator(0, column, GenParam::OwnStep, Some(step));
                engine.process(&mut out);
                engine.take_gen_reports(|_, report| {
                    reported += usize::from(matches!(report, GenReport::Pass(_)));
                });
            });
            violations += count;
        }
        assert_eq!(violations, 0);
        assert!(reported > 2, "{reported} passes reported");
        let heard = engine.generators_heard(0).next();
        assert!(heard.is_some_and(|heard| heard.renders > 0));
    }

    /// One synth track playing a phrase of 16 notes, a `Mini` on them playing `text`, a token
    /// a row, its `Cue` as given (decision 462).
    fn mini(text: &str, cue: bool) -> Song {
        let mut song = Song::new(1);
        song.millibpm = vt_model::constants::MAX_MILLIBPM;
        song.tracks[0].machine = Machine::Plaits(SynthVoice::Acid);
        song.tracks[0].cue = u8::from(cue);
        let notes = Phrase::from_steps(vec!["C-3 64 --- --- ---".parse().unwrap(); 16]);
        song.tracks[0].phrases.insert(0, Arc::new(notes));
        let action = Action::Generate {
            track: 0,
            phrase: 0,
            column: acid_column(&song),
            span: Span::rows(StepRange::between(0, 15)),
            shape: Shape {
                kind: FillKind::Mini,
                ..Shape::default()
            },
            list: Box::default(),
            rhythm: text.into(),
        };
        Arc::unwrap_or_clone(edit(&Arc::new(song), &action).expect("the edit applies"))
    }

    /// The pitch each row was heard at after a line is typed again a row into a lap, by row,
    /// in the order they played: none of it allocating in the callback.
    fn retyped(cue: bool) -> Vec<(usize, u8)> {
        // Four rows a lap, the choice making it render each pass, so every row reports.
        let (old, new) = ("<C-3 D-3> E-3 F-3 G-3", "<C-4 D-4> E-4 F-4 G-4");
        let first = Arc::new(Published::new(
            Arc::new(mini(old, cue)),
            Arc::new(SamplePool::default()),
        ));
        let next = Published::after(
            Arc::new(mini(new, cue)),
            Arc::new(SamplePool::default()),
            Some(&first),
        );
        assert_eq!(next.waits.len(), 1, "the line is waited on");
        let mut next = Some(Handover::new(Arc::new(next)));
        let mut engine = Engine::new(RATE, Arc::clone(&first));
        engine.play_from(0);
        let mut out = vec![0.0; BLOCK * 2];
        let (mut heard, mut violations, mut gone) = (Vec::with_capacity(256), 0, None);
        for _ in 0..400 {
            let ((), count) = forbidden(|| {
                engine.process(&mut out);
                let mut handed = false;
                engine.take_gen_reports(|_, report| {
                    let GenReport::Row(GenRow {
                        row,
                        cell: Some(GenCell::Pitch(note)),
                        ..
                    }) = report
                    else {
                        return;
                    };
                    if next.is_none() && heard.len() < heard.capacity() {
                        heard.push((row, note));
                    }
                    handed |= row == 1;
                });
                if handed && let Some(next) = next.take() {
                    gone = Some(engine.hand_over(next));
                }
            });
            violations += count;
        }
        assert_eq!(violations, 0);
        drop(gone);
        heard
    }

    /// Typed again a row into a lap, with `Cue` on the old line plays on to the lap's end and
    /// the new one from the next lap; with it off, the new one plays at once.
    #[test]
    fn a_retyped_line_waits_for_its_lap_with_cue_on() {
        let cued = retyped(true);
        let lap = cued.iter().position(|(row, _)| row % 4 == 0).unwrap();
        assert!(lap > 0, "rows of the old lap still to play: {cued:?}");
        assert!(cued[..lap].iter().all(|(_, note)| *note < 72), "{cued:?}");
        assert!(cued[lap..].iter().all(|(_, note)| *note >= 72), "{cued:?}");
        let at_once = retyped(false);
        assert!(!at_once.is_empty());
        assert!(at_once.iter().all(|(_, note)| *note >= 72), "{at_once:?}");
    }

    /// One synth track playing a phrase of `rows` notes, a `List` of these values on its
    /// velocities, kept on the generator, each value for a run of 2 rows.
    fn listed(rows: usize, list: &[u8]) -> Song {
        let mut song = Song::new(1);
        song.millibpm = vt_model::constants::MAX_MILLIBPM;
        song.tracks[0].machine = Machine::Plaits(SynthVoice::Acid);
        let notes = Phrase::from_steps(vec!["C-3 64 --- --- ---".parse().unwrap(); rows]);
        song.tracks[0].phrases.insert(0, Arc::new(notes));
        let action = Action::Generate {
            track: 0,
            phrase: 0,
            column: ShapeColumn::Velocity,
            span: Span::rows(StepRange::between(0, rows - 1)),
            shape: Shape {
                run: 2,
                ..shape(FillKind::List, 0, 0, Where::AllRows)
            },
            list: list.into(),
            rhythm: Box::default(),
        };
        Arc::unwrap_or_clone(edit(&Arc::new(song), &action).expect("the edit applies"))
    }

    /// A `List` of 256 values, moving as its `Run` is held at 1, plays a value a row from its
    /// generator's list, borrowed from the version's song, and a phrase of 300 rows goes round
    /// it. A new version with another list, its hash worked out on the edit thread, is heard from
    /// the next row; rendering, swapping and reporting allocate and free nothing.
    #[test]
    fn a_list_edit_is_heard_from_the_next_row_without_allocating() {
        use vt_model::shape::MAX_LIST;
        let rows = 300;
        let first: Vec<u8> = (0..MAX_LIST).map(|at| (at % 0x80) as u8).collect();
        let second: Vec<u8> = first.iter().map(|value| 0x7F - value).collect();
        let mut edited = Some(Handover::new(version(&listed(rows, &second))));
        let hash = |handover: &Handover| handover.scratch.memory.kept[0].list;
        let unedited = Handover::new(version(&listed(rows, &first)));
        assert_ne!(hash(edited.as_ref().unwrap()), hash(&unedited));
        let mut engine = Engine::new(RATE, version(&listed(rows, &first)));
        engine.play_from(0);
        engine.hold_generator(0, ShapeColumn::Velocity, GenParam::Run, Some(1));
        let mut out = vec![0.0; BLOCK * 2];
        let mut heard: Vec<(usize, Option<GenCell>)> = Vec::with_capacity(4 * rows);
        let (mut violations, mut old) = (0, Vec::with_capacity(4));
        // Edited as the fifth row plays.
        let edit_after = 4;
        while heard.last().is_none_or(|(row, _)| *row < rows - 1) {
            let swap = heard.last().is_some_and(|(row, _)| *row == edit_after);
            let next = swap.then(|| edited.take()).flatten();
            let ((), count) = forbidden(|| {
                if let Some(next) = next {
                    old.push(engine.hand_over(next));
                }
                engine.process(&mut out);
                engine.take_gen_reports(|_, report| {
                    if let GenReport::Row(row) = report {
                        heard.push((row.row, row.cell));
                    }
                });
            });
            violations += count;
        }
        assert_eq!(violations, 0);
        assert!(edited.is_none(), "the edit came");
        let velocity = |list: &[u8], row: usize| Some(GenCell::Velocity(list[row % MAX_LIST]));
        let at = |row: usize| {
            heard
                .iter()
                .find(|(at, _)| *at == row)
                .and_then(|(_, cell)| *cell)
        };
        for row in 0..=edit_after {
            assert_eq!(at(row), velocity(&first, row), "row {row}, before the edit");
        }
        for row in edit_after + 1..rows {
            assert_eq!(at(row), velocity(&second, row), "row {row}, after it");
        }
        assert_eq!(
            at(MAX_LIST + 1),
            Some(GenCell::Velocity(second[1])),
            "round"
        );
    }

    /// The time in the middle of some, by length: what one of them takes when nothing else has the
    /// machine, as a mean isn't once another test or a build takes the core for one of them.
    fn middle(mut times: Vec<Duration>) -> Duration {
        times.sort();
        times[times.len() / 2]
    }

    /// Play a song's first `rows` rows a step at a time, holding the `Acid`'s density at another
    /// number before each if `moving`: how long the slowest step and the middle one took, how many
    /// renders there were and how many allocations.
    fn steps_timed(song: &Song, rows: usize, moving: bool) -> (Duration, Duration, u32, usize) {
        let acid = acid_column(song);
        let mut engine = Engine::new(RATE, version(song));
        engine.play_from(0);
        let mut out = vec![0.0; step_frames(song) * 2];
        let (mut times, mut violations) = (Vec::with_capacity(rows), 0);
        for row in 0..rows {
            // Never the written 50, so each row's is new.
            let density = if row % 2 == 0 { 10 } else { 90 } + (row % 7) as i32;
            if moving {
                engine.hold_generator(0, acid, GenParam::Density, Some(density));
            }
            let started = Instant::now();
            let ((), count) = forbidden(|| engine.process(&mut out));
            times.push(started.elapsed());
            violations += count;
        }
        let renders = engine
            .generators_heard(0)
            .find(|heard| heard.column == acid)
            .map_or(0, |heard| heard.renders);
        let worst = times.iter().copied().max().unwrap_or_default();
        (worst, middle(times), renders, violations)
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
        let (rest_worst, rest, rest_renders, _) = steps_timed(&song, rows, false);
        let (worst, step, renders, violations) = steps_timed(&song, rows, true);
        // The render alone, as the engine runs it, over the same span.
        let phrase = &song.tracks[0].phrases[&0];
        let generator = &phrase.generators()[0];
        let under = generator.under(phrase.steps());
        let span = generator.rows().unwrap();
        let mut cells = vec![GenCell::Under; span.len()];
        let mut shape = generator.shape;
        let mut renders_timed = Vec::with_capacity(rows);
        for density in 0..rows as u8 {
            shape.place = Where::Beats(density);
            let started = Instant::now();
            render_into(
                &under,
                span,
                generator.column,
                &shape,
                vt_model::shape::Texts::list(generator.list()),
                None,
                &mut cells,
            );
            renders_timed.push(started.elapsed());
        }
        let render_worst = renders_timed.iter().copied().max().unwrap_or_default();
        let render = middle(renders_timed);
        let frames = step_frames(&song);
        let real = Duration::from_secs_f64(frames as f64 / f64::from(RATE));
        println!(
            "a {}-row Acid, {rows} rows, steps of {frames} frames ({real:?} of sound), the middle \
             time of each: at rest {rest:?} a step, {rest_worst:?} at worst; rendered every row \
             ({renders} renders) {step:?} a step, {worst:?} at worst; the render alone \
             {render:?}, {render_worst:?} at worst",
            span.len(),
        );
        assert_eq!(violations, 0);
        assert_eq!(rest_renders, 0);
        assert!(renders as usize >= rows - 1, "{renders} renders");
        // In a release build the render is a sliver of a step, and a step that renders keeps well
        // up with what it plays. It's the middle time of the 64 that's held to it, not their mean:
        // other tests and builds share the machine, and one step left waiting for a core can cost
        // more than the other 63 together, which a mean fails on and the middle doesn't notice,
        // while a render or a step grown slow is slow in the middle too. A quiet machine has the
        // render about 40 times inside its limit and the step about 10 times.
        if !cfg!(debug_assertions) {
            assert!(
                render < real / 20,
                "{render:?} a render, for {real:?} of sound"
            );
            assert!(step < real / 2, "{step:?} a step to make {real:?}");
        }
    }

    /// The most a `Cells` render steps: on a column of 4,096 rows, 255 generations of 4,096 cells,
    /// and across tracks on 4,095 rows, 8,190 generations of 64 cells, the pass's first row just
    /// short of going round. Nothing allocates, and in a release build each takes a sliver of the
    /// shortest step, at 400 BPM.
    #[test]
    fn the_longest_cells_render_keeps_up() {
        use vt_model::shape::{Across, COLUMN_GENERATIONS, CellStart, Cells};
        let song = song(MAX_PHRASE_STEPS);
        let frames = step_frames(&song);
        let real = Duration::from_secs_f64(frames as f64 / f64::from(RATE));
        let steps = vec![Step::default(); MAX_PHRASE_STEPS];
        let column = ShapeColumn::Note {
            scale: song.saved_scale,
            writes: Some(0x64),
        };
        let mut cells = vec![GenCell::Under; MAX_PHRASE_STEPS];
        let column_cells = Cells {
            rule: 30,
            start: CellStart::Seed,
            across: None,
            stepped: (COLUMN_GENERATIONS - 1) as u16,
        };
        let across_cells = Cells {
            across: Some(Across {
                cell: 31,
                cells: 64,
                row: 0x9E37_79B9_7F4A_7C15,
            }),
            stepped: 1,
            ..column_cells
        };
        let cases = [
            (StepRange::between(0, MAX_PHRASE_STEPS - 1), column_cells),
            (StepRange::between(0, MAX_PHRASE_STEPS - 2), across_cells),
        ];
        for (range, chosen) in cases {
            let ramp = shape(FillKind::Ramp, 36, 48, Where::Cells(chosen));
            let times = 9;
            let ((), count) = forbidden(|| {
                render_into(
                    &steps,
                    range,
                    column,
                    &ramp,
                    vt_model::shape::Texts::NONE,
                    None,
                    &mut cells[..range.len()],
                );
            });
            assert_eq!(count, 0);
            // Each timed on its own and the middle one held to the limit, as the `Acid`'s are, so
            // one left waiting for a core on a busy machine doesn't count.
            let timed = (0..times)
                .map(|_| {
                    let started = Instant::now();
                    render_into(
                        &steps,
                        range,
                        column,
                        &ramp,
                        vt_model::shape::Texts::NONE,
                        None,
                        &mut cells[..range.len()],
                    );
                    started.elapsed()
                })
                .collect();
            let render = middle(timed);
            println!("{chosen:?} over {} rows: {render:?} a render", range.len());
            assert!(
                cells[..range.len()]
                    .iter()
                    .any(|cell| *cell != GenCell::Under)
            );
            if !cfg!(debug_assertions) {
                assert!(
                    render < real / 20,
                    "{render:?} a render, for {real:?} of sound"
                );
            }
        }
    }
}
