//! A track's sequencing: its own clock, the steps and parts of them waiting to land, and its
//! retriggers, arpeggios and accumulators.

use vt_model::accumulator::{Accumulation, Moves, Walk};
use vt_model::arpeggio::{ArpMode, Cycle};
use vt_model::groove::Groove;
use vt_model::modulation::MOD_SLOTS;
use vt_model::speed::Speed;

use crate::rng::Pcg32;
use crate::voice::NoteOn;

use super::TrackState;

/// A note waiting for its delayed start.
#[derive(Debug, Clone, Copy)]
pub struct DelayedNote {
    pub at: u64,
    pub note: NoteOn,
}

/// Which of a step's parts lands. A nudge moves the row's note part off the grid, while the FX
/// columns with lanes of their own stay on it, so the two land apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// All of it at once, as every step lands without a nudge.
    Whole,
    /// The lanes' values, where the groove lands the step.
    Lanes,
    /// The note, its own FX columns, and what times the note from any column: `DEL`, `KIL`,
    /// `OFF`, `RET` and `CHA`.
    Row,
}

impl Part {
    /// Whether it plays the row's note part.
    pub fn plays_row(self) -> bool {
        self != Self::Lanes
    }

    /// Whether it runs the lanes' values.
    pub fn runs_lanes(self) -> bool {
        self != Self::Row
    }
}

/// How many parts of steps a track can have waiting. At one speed it's two steps' lanes and rows,
/// as a nudged row can land after the next step is worked out, but never after the one after. A
/// slow track's early groove works every track's steps out further ahead, so a fast track can
/// have several steps worked out before the one heard has landed: at most 28 of a 32nd's steps
/// against a whole note's earliest groove and nudge, 3190 ticks, each with two parts.
pub const PENDING_PARTS: usize = 64;

/// How many of the steps a track has worked out it keeps, to find the one heard while it works
/// some out ahead: as many as a 32nd's steps worked out ahead of a whole note's earliest groove
/// and nudge, and the one heard.
pub(super) const WORKED_KEPT: usize = 32;

/// Where a track's own steps fall on the grid. Each track counts its own steps, so one at an
/// eighth counts its eighths, and its groove swings them.
#[derive(Debug, Clone, Copy, Default)]
pub struct Clock {
    /// Where its next step falls on the grid, in ticks on the engine's count of steps.
    pub beat: u64,
    /// The running count at that step: the track's own steps since the song's start or the last
    /// reset.
    pub count: i64,
    /// Where its steps are counted from on the grid: where it last lined up, at a reset or as its
    /// chain started. A step at a new speed lasts until the next of that speed's steps from here,
    /// so a track keeps to its speed's grid however it changes.
    pub anchor: i64,
}

impl Clock {
    /// Have the next step fall here, and count the steps from here.
    pub fn line_up(&mut self, at: u64) {
        self.beat = at;
        self.anchor = at as i64;
    }

    /// Where the step after the next falls if the next is this many ticks long: the next of its
    /// speed's steps from the anchor, so a step that doesn't start on one is cut short.
    pub fn after(&self, ticks: u64) -> u64 {
        let ticks = ticks.max(1) as i64;
        let from = self.beat as i64 - self.anchor;
        (self.anchor + (from.div_euclid(ticks) + 1) * ticks) as u64
    }

    /// Move on to the step after the next, which is this many ticks long.
    pub fn advance(&mut self, ticks: u64) {
        self.beat = self.after(ticks);
        self.count += 1;
    }

    /// Count on, as if it had played steps this many ticks long, to the first at or after a place.
    pub fn skip_to(&mut self, at: u64, ticks: u64) {
        if at <= self.beat {
            return;
        }
        let steps = (at - self.beat).div_ceil(ticks.max(1));
        self.beat += steps * ticks;
        self.count += steps as i64;
    }

    /// Move it along the grid, as working out a place to play from does once it's known where it
    /// starts.
    pub fn shift(&mut self, by: i64) {
        self.beat = (self.beat as i64 + by).max(0) as u64;
        self.anchor += by;
    }
}

/// A step a track has worked out, for where it's heard: where it falls on the grid, its running
/// count, how long it lasts and the speed it plays at, which a step cut short by a change of speed
/// doesn't last the whole of.
#[derive(Debug, Clone, Copy)]
pub struct Worked {
    pub grid: u64,
    pub count: i64,
    pub ticks: u64,
    pub speed: Speed,
}

/// A step, or one part of it, worked out but not yet played, as its groove or nudge lands it later
/// than now.
#[derive(Debug, Clone, Copy)]
pub struct PendingStep {
    /// When it lands, on the track's clock.
    pub at: u64,
    /// Its place in the track's running count, which its groove counts.
    pub running: usize,
    /// The groove it was worked out with, which its velocity and length follow.
    pub groove: Groove,
    /// How long one of the track's steps is at its speed, in ticks, which its pushes, nudge,
    /// timing commands and length are shares of.
    pub ticks: u64,
    /// Which part of the step it is.
    pub part: Part,
    /// Which step it is: where it falls on the grid, in ticks, which never wraps.
    pub step: u64,
    /// Whether it's the step's first part to land, which rolls its `CHA` and ends the last note's
    /// locks.
    pub leads: bool,
    /// The chain the track plays it from and the place in it, found when it's worked out, as a
    /// part landing after the next step is worked out mustn't look again.
    pub place: Option<(u32, usize)>,
    /// The count its lanes read at.
    pub count: i64,
    /// What its row's `CHA` and `CND` decided, once the step's first part has decided them.
    pub plays: Option<Decided>,
    /// Which of its track's chain starts it was worked out after, which its accumulators count
    /// from.
    pub start: u64,
}

/// What a row's chance and conditions decided, once, on its first pulse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decided {
    /// Whether its note plays, past its `CHA` and `CND`.
    pub note: bool,
    /// Whether its own FX run, past its `CND`. A failed `CHA` skips only the note.
    pub fx: bool,
}

impl Decided {
    /// A row with nothing against it.
    pub const PLAYS: Self = Self {
        note: true,
        fx: true,
    };
}

/// Retriggering the current note.
#[derive(Debug, Clone, Copy)]
pub struct Retrigger {
    pub next_at: u64,
    pub interval: u64,
    /// Multiplies the level on each retrigger.
    pub factor: f32,
    /// The row's accumulator, where each hit moves its note on.
    pub accumulates: Option<Accumulating>,
}

/// A note's arpeggio, going round its notes at its rate for as long as the note lasts.
#[derive(Debug, Clone, Copy)]
pub struct Arpeggiating {
    pub cycle: Cycle,
    /// Its `ARP`'s value, which a `HLD` carries into the note it ties into, unless that row has
    /// one of its own.
    pub value: u8,
    pub mode: ArpMode,
    /// Whether each note starts the envelopes again, rather than only changing the pitch.
    pub retrig: bool,
    /// Where its first note started, which each later one is counted from, so they don't drift.
    pub from: u64,
    /// How many of its notes have started, the first among them.
    pub played: u64,
    /// Which of the cycle's notes is sounding.
    pub place: usize,
    /// How long each note is, in ticks of a step 240 long, and how long the track's step is, in
    /// ticks, so a note is that share of it.
    pub note_ticks: u64,
    pub step_ticks: u64,
    pub next_at: u64,
}

/// A row whose accumulator moves its note on each time its retrigger hits.
#[derive(Debug, Clone, Copy)]
pub struct Accumulating {
    pub row: RowKey,
    pub amount: i8,
    /// The note as written, which it's moved from.
    pub written: u8,
    /// How far the row's `RND`s moved it on top, in degrees or semitones.
    pub varied: i16,
    /// Its chain's transpose, added after.
    pub transpose: i8,
    pub epoch: Epoch,
}

/// The most rows on a track that keep an accumulator of their own at once. Past it, a row with an
/// `ACU` plays its note as written, though in Track mode it still moves with the track.
pub const ACCUMULATED_ROWS: usize = 64;

/// A row of a phrase, which an accumulator belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowKey {
    pub phrase: u32,
    pub row: usize,
}

/// How far each mod slot moved a row's note on `Pitch` as it started, in semitones, as it was
/// heard, `Pitch in scale`'s snap and a slot's switch included, and how far the row's own `RND`s
/// moved it: what `Space g A` writes into the notes (#569, decision 323). The row is counted from
/// the phrase's first step, not its loop's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotePitch {
    pub row: RowKey,
    pub moves: [f32; MOD_SLOTS],
    /// How far the row's own `RND`s moved its note, in semitones, before the slots.
    pub varied: i8,
}

/// Which start of a track's chain, and which group of loops round it, a track's accumulators are
/// counting in. They start again from the notes as written once it moves on.
pub type Epoch = (u64, i64);

/// One row's accumulator, and whether its row has played since they started again, as its first
/// play is as written.
#[derive(Debug, Clone, Copy)]
struct RowWalk {
    key: RowKey,
    played: bool,
    walk: Walk,
}

/// A track's accumulators: one for each row with an `ACU`, and the track's own for Track mode, in a
/// fixed table so the callback never allocates.
#[derive(Debug, Clone, Copy)]
pub struct Accumulators {
    rows: [Option<RowWalk>; ACCUMULATED_ROWS],
    track: Walk,
    epoch: Epoch,
}

impl Default for Accumulators {
    fn default() -> Self {
        Self {
            rows: [None; ACCUMULATED_ROWS],
            track: Walk::default(),
            epoch: (0, 0),
        }
    }
}

impl Accumulators {
    /// Go back to the notes as written.
    pub fn forget(&mut self) {
        self.rows = [None; ACCUMULATED_ROWS];
        self.track = Walk::default();
    }

    /// Count in this epoch, starting again if it's a new one.
    pub fn enter(&mut self, epoch: Epoch) {
        if epoch != self.epoch {
            self.forget();
            self.epoch = epoch;
        }
    }

    /// Add a row's amount as it plays, to its own accumulator or the track's, a `Random` one
    /// rolling on `rng`. Its first play since they started again is as written, and adds nothing.
    pub fn add(&mut self, key: RowKey, amount: i8, settings: Accumulation, rng: &mut Pcg32) {
        let found = self
            .rows
            .iter()
            .position(|row| row.is_some_and(|r| r.key == key));
        let at = match found.or_else(|| self.rows.iter().position(Option::is_none)) {
            Some(at) => at,
            None => return,
        };
        let row = self.rows[at].get_or_insert(RowWalk {
            key,
            played: false,
            walk: Walk::default(),
        });
        if !row.played {
            row.played = true;
            return;
        }
        let roll = &mut || rng.next_u32();
        match settings.moves {
            Moves::Row => row.walk.add(amount, settings, roll),
            Moves::Track => self.track.add(amount, settings, roll),
        }
    }

    /// How far a row's note is moved: by its own accumulator, or the track's in Track mode.
    pub fn offset(&self, key: RowKey, moves: Moves) -> i16 {
        match moves {
            Moves::Track => self.track.offset,
            Moves::Row => self
                .rows
                .iter()
                .flatten()
                .find(|row| row.key == key)
                .map_or(0, |row| row.walk.offset),
        }
    }
}

impl TrackState {
    /// How far either way a groove's humanise moves a step, at random, in ticks.
    pub fn humanise(&mut self, ticks: u8) -> i64 {
        if ticks == 0 {
            return 0;
        }
        let spread = u32::from(ticks) * 2 + 1;
        i64::from(self.groove_rng.next_u32() % spread) - i64::from(ticks)
    }

    /// Take the part that lands first, if it lands by this time. Parts that land together play in
    /// the order their steps were worked out, a step's first part before its other.
    pub fn take_due(&mut self, by: u64) -> Option<PendingStep> {
        if self.soonest > by {
            return None;
        }
        let next = self
            .pending
            .iter()
            .enumerate()
            .filter_map(|(at, part)| part.map(|part| (at, part)))
            .filter(|(_, part)| part.at <= by)
            .min_by_key(|(_, part)| (part.at, part.step, !part.leads))?;
        self.pending[next.0] = None;
        self.find_soonest();
        Some(next.1)
    }

    /// When the soonest part waiting lands, or `u64::MAX` with none.
    pub fn soonest(&self) -> u64 {
        self.soonest
    }

    fn find_soonest(&mut self) {
        self.soonest = self
            .pending
            .iter()
            .flatten()
            .map(|part| part.at)
            .min()
            .unwrap_or(u64::MAX);
    }

    /// Wait for a part to land. With no room, which only a big change of tempo could bring
    /// about, the earliest waiting is given back to be played now.
    pub fn wait_for(&mut self, part: PendingStep) -> Option<PendingStep> {
        let full = self.pending.iter().all(Option::is_some);
        let earliest = if full { self.take_due(u64::MAX) } else { None };
        if let Some(free) = self.pending.iter_mut().find(|part| part.is_none()) {
            *free = Some(part);
            self.soonest = self.soonest.min(part.at);
        }
        earliest
    }

    /// Keep a step it has worked out, for where it's heard.
    pub fn note_worked(&mut self, worked: Worked) {
        self.worked[self.worked_next] = Some(worked);
        self.worked_next = (self.worked_next + 1) % WORKED_KEPT;
    }

    /// The latest step it has worked out that falls by this tick on the grid, the one heard.
    pub fn heard_at(&self, tick: f64) -> Option<Worked> {
        self.worked
            .iter()
            .flatten()
            .filter(|worked| worked.grid as f64 <= tick)
            .max_by_key(|worked| worked.grid)
            .copied()
    }

    /// Forget the steps it has worked out from this tick on the grid, as they'll be worked out
    /// again, or all of them, when playing starts somewhere else.
    pub fn forget_worked(&mut self, from: u64) {
        for worked in &mut self.worked {
            if worked.is_some_and(|worked| worked.grid >= from) {
                *worked = None;
            }
        }
    }

    /// The latest any waiting part that plays a row's note lands, as the next row's can't land
    /// before it.
    pub fn last_row_at(&self) -> Option<u64> {
        self.pending
            .iter()
            .flatten()
            .filter(|part| part.part.plays_row())
            .map(|part| part.at)
            .max()
    }

    /// Tell a step's other part, still waiting, what its row's chance and conditions decided.
    pub fn decide(&mut self, step: u64, plays: Decided) {
        for part in self.pending.iter_mut().flatten() {
            if part.step == step {
                part.plays = Some(plays);
            }
        }
    }

    /// Forget whatever is waiting of the steps from this tick on the grid, as they're to be worked
    /// out again.
    pub fn forget_steps(&mut self, from: u64) {
        for part in &mut self.pending {
            if part.is_some_and(|part| part.step >= from) {
                *part = None;
            }
        }
        self.find_soonest();
        self.forget_worked(from);
    }
}
