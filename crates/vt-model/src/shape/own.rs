//! A rhythm of your own as a `Where` (#1091, decision 461): a line typed in Tidal's mini-notation,
//! `x . [. x] x(3,8)`, whose text is kept on its generator and compiled there into [`Beat`]s, so
//! the `Copy` shape holds only its `Step` and what the engine must know of it.
//!
//! It's stepwise, not cyclic: a top-level token is `Step` rows, so a token more makes the line
//! longer, and the line goes round to fill its span. Inside a token, `[ ]`, `*` and a Euclid's
//! numbers split its rows, each hit landing on the row nearest it within its token, and two hits
//! on one row keep the first. `<a b>` takes the next choice each lap, each time the line comes
//! round, counted on from pass to pass while the line has one; `?` draws from the seed, the token
//! and the row, the same each pass until `Vary` draws a new seed. On the Note column a hit `*n`
//! finer than a row is a `RET` ratchet on its row.
//!
//! The render is a walk of the beats into a bitset of the rows it counts over, as `Euclid`'s is,
//! allocating nothing, so the engine runs it unchanged.

use serde::{Deserialize, Serialize};

use super::mix;
use super::render::Rows;
use super::{Atom, FillKind, GenCell, Shape, ShapeColumn, Texts, Where};
use crate::constants::{GROOVE_UNIT_TICKS, MAX_PHRASE_STEPS, TICKS_PER_STEP};
use crate::step::{FxCommand, FxSlot, Step};

/// What a `?`'s draws are for, so they don't follow a `Chance`'s.
const CHANCE: u64 = 0x0A11_0000;
/// The most rows a top-level token is.
pub const MAX_STEP: u8 = 16;
/// The furthest a `RET` puts its repeats apart, in units, its value's low digit.
const MOST_APART: u16 = 0xF;
/// `RET`'s units in a row.
const UNITS_PER_STEP: u64 = TICKS_PER_STEP / GROOVE_UNIT_TICKS;

/// A typed rhythm's settings in its `Where`: its `Step`, and what its generator works out from
/// its text for the shape to know, which isn't saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Own {
    /// How many rows a top-level token is, from 1 to [`MAX_STEP`].
    pub step: u8,
    /// Whether the line has a `< >`, so its laps count on from pass to pass and the engine
    /// renders each pass.
    #[serde(skip)]
    pub laps: bool,
    /// Whether it has a hit `*n`, which on the Note column may be a `RET` on its row, so its
    /// generator owns the row's FX.
    #[serde(skip)]
    pub ratchets: bool,
    /// Whether it has a `?`, which draws from the seed, so it has `Vary`.
    #[serde(skip)]
    pub chances: bool,
    /// The pass it plays, counted from the first, which [`Shape::on_pass`](super::Shape::on_pass)
    /// sets while it [`laps`](Own::laps).
    #[serde(skip)]
    pub pass: u32,
}

impl Default for Own {
    fn default() -> Self {
        Self {
            step: 1,
            laps: false,
            ratchets: false,
            chances: false,
            pass: 0,
        }
    }
}

impl Own {
    /// Whether it's as it starts, its `Step` a row, which is all that's saved.
    pub(crate) fn is_default(&self) -> bool {
        self.step == 1
    }

    /// The same with what a rhythm's beats say of it.
    #[must_use]
    pub fn of(self, beats: &[Beat]) -> Self {
        Self {
            laps: beats.iter().any(|beat| beat.what == What::Choice),
            ratchets: beats
                .iter()
                .any(|beat| beat.fast > 1 && matches!(beat.what, What::Hit(_))),
            chances: beats.iter().any(|beat| beat.chance < 100),
            ..self
        }
    }
}

/// What a beat is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum What {
    /// `x`, by its place among the line's hits, which lights its rows.
    Hit(u8),
    /// `.` or `~`.
    Rest,
    /// `[ ]`, or the line itself: its beats share its rows, by their weights.
    Group,
    /// `< >`: one of its beats each lap.
    Choice,
}

/// One token of a typed rhythm, compiled: what it is with its modifiers, and how many beats its
/// group holds. A line's beats are in reading order, each group's after it, the first the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Beat {
    pub what: What,
    /// The beats it is and holds, itself counted.
    pub size: u16,
    /// `@n`: how many places long it is.
    pub weight: u16,
    /// `!n`: how many places of its own it takes, one after another.
    pub repeat: u16,
    /// `*n`: how many times it plays in its place.
    pub fast: u16,
    /// `?`: the percent of its hits that play, 100 for every one.
    pub chance: u8,
    /// `(k,n,r)`: `k` hits over `n` parts of its place, moved `r` parts later, worked out as it's
    /// read, so the render only reads it.
    pub euclid: Option<Euclid>,
}

/// A Euclid's pattern over a token's place: how many parts, at most 64, and which of them hit,
/// the first in bit 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Euclid {
    pub parts: u16,
    pub hits: u64,
}

impl Euclid {
    /// `hits` spread over `parts` by Bjorklund's algorithm, as `Where`'s `Euclid` spreads them,
    /// then moved `later` parts later, going round.
    pub fn new(hits: u16, parts: u16, later: u16) -> Self {
        let parts = parts.clamp(1, 64);
        let pattern = super::euclid(usize::from(hits), usize::from(parts));
        let later = usize::from(later % parts);
        let count = usize::from(parts);
        let hits = (0..count)
            .filter(|nth| pattern[(nth + count - later) % count])
            .fold(0u64, |bits, nth| bits | 1 << nth);
        Self { parts, hits }
    }
}

impl Beat {
    /// A plain beat, holding nothing, in one place, played once.
    pub fn new(what: What) -> Self {
        Self {
            what,
            size: 1,
            weight: 1,
            repeat: 1,
            fast: 1,
            chance: 100,
            euclid: None,
        }
    }

    /// How many places it takes in its group's line, or laps among a choice's.
    fn places(self) -> u32 {
        u32::from(self.weight) * u32::from(self.repeat)
    }
}

/// The beats a group holds, by their places.
fn children(beats: &[Beat], group: usize) -> impl Iterator<Item = usize> + '_ {
    let end = group + usize::from(beats[group].size);
    let mut at = group + 1;
    std::iter::from_fn(move || {
        (at < end && at < beats.len()).then(|| {
            let child = at;
            at += usize::from(beats[child].size.max(1));
            child
        })
    })
}

/// How many steps the line is: its top-level places, each `Step` rows.
pub fn line_steps(beats: &[Beat]) -> u32 {
    if beats.is_empty() {
        return 0;
    }
    children(beats, 0).map(|child| beats[child].places()).sum()
}

/// A hit the walk lands: where in the lap it starts and where its place ends, in rows, its hit's
/// place, the percent it plays and how many times it plays in a place finer than a row.
#[derive(Debug, Clone, Copy)]
struct Landed {
    at: f64,
    end: f64,
    hit: u8,
    chance: u32,
    ratchet: u16,
}

/// Walk a group over its place, each of its beats taking its share by its weight.
fn group(
    beats: &[Beat],
    at: usize,
    (start, len): (f64, f64),
    lap: u64,
    chance: u32,
    land: &mut impl FnMut(Landed),
) {
    let total: u32 = children(beats, at).map(|child| beats[child].places()).sum();
    if total == 0 {
        return;
    }
    let mut from = start;
    for child in children(beats, at) {
        let beat = beats[child];
        let share = len * f64::from(beat.weight) / f64::from(total);
        for _ in 0..beat.repeat {
            place(beats, child, (from, share), lap, chance, land);
            from += share;
        }
    }
}

/// A beat in its place, with its Euclid's numbers, then its `*`.
fn place(
    beats: &[Beat],
    at: usize,
    (start, len): (f64, f64),
    lap: u64,
    chance: u32,
    land: &mut impl FnMut(Landed),
) {
    let beat = beats[at];
    let chance = chance * u32::from(beat.chance) / 100;
    match beat.euclid {
        Some(Euclid { parts, hits }) => {
            let part = len / f64::from(parts);
            for nth in 0..parts {
                if hits & 1 << nth != 0 {
                    let from = start + f64::from(nth) * part;
                    fast(beats, at, (from, part), lap, chance, land);
                }
            }
        }
        _ => fast(beats, at, (start, len), lap, chance, land),
    }
}

/// A beat played `*n` times in its place: a hit finer than a row lands once, as a ratchet, and
/// anything else plays each time a lap on, as Tidal's `fast` counts its cycles.
fn fast(
    beats: &[Beat],
    at: usize,
    (start, len): (f64, f64),
    lap: u64,
    chance: u32,
    land: &mut impl FnMut(Landed),
) {
    let beat = beats[at];
    let times = beat.fast.max(1);
    if times == 1 {
        return inner(beats, at, (start, len), lap, chance, land);
    }
    if let What::Hit(hit) = beat.what
        && len / f64::from(times) < 1.0
    {
        land(Landed {
            at: start,
            end: start + len,
            hit,
            chance,
            ratchet: times,
        });
        return;
    }
    let part = len / f64::from(times);
    for nth in 0..times {
        let lap = lap
            .wrapping_mul(u64::from(times))
            .wrapping_add(u64::from(nth));
        inner(
            beats,
            at,
            (start + f64::from(nth) * part, part),
            lap,
            chance,
            land,
        );
    }
}

/// What a beat is, in its place: a hit, nothing, its group, or its choice for the lap.
fn inner(
    beats: &[Beat],
    at: usize,
    (start, len): (f64, f64),
    lap: u64,
    chance: u32,
    land: &mut impl FnMut(Landed),
) {
    match beats[at].what {
        What::Hit(hit) => land(Landed {
            at: start,
            end: start + len,
            hit,
            chance,
            ratchet: 1,
        }),
        What::Rest => {}
        What::Group => group(beats, at, (start, len), lap, chance, land),
        What::Choice => {
            let laps: u32 = children(beats, at).map(|child| beats[child].places()).sum();
            if laps == 0 {
                return;
            }
            let mut left = lap % u64::from(laps);
            for child in children(beats, at) {
                let places = u64::from(beats[child].places());
                if left < places {
                    // A choice inside takes its own turn each time this one comes round to it.
                    let lap = lap / u64::from(laps);
                    return place(beats, child, (start, len), lap, chance, land);
                }
                left -= places;
            }
        }
    }
}

/// A hit as it lands among the rows counted: the place of the row it's on, its hit's place and
/// how many times it plays there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Lands {
    pub(super) nth: usize,
    pub(super) hit: u8,
    pub(super) ratchet: u16,
    /// Its place's length, in rows, for a ratchet's spacing.
    len: f64,
    /// How far from its row it falls, in rows, early below 0: a third of a row for a triplet's
    /// hit, which a `Mini` on the Note column nudges off its row (decision 462).
    pub(super) off: f64,
}

/// Each hit the line lands over `count` rows counted, on a pass, in order and once a row: the
/// first to land on a row keeps it, and a `?` that doesn't play leaves it for the next.
pub(super) fn walk(beats: &[Beat], own: Own, seed: u64, count: usize, each: impl FnMut(Lands)) {
    walk_dropping(beats, own, seed, count, each, || {});
}

/// The same, telling `dropped` of each hit that lands on a row another took first.
pub(super) fn walk_dropping(
    beats: &[Beat],
    own: Own,
    seed: u64,
    count: usize,
    mut each: impl FnMut(Lands),
    mut dropped: impl FnMut(),
) {
    let step = u32::from(own.step.clamp(1, MAX_STEP));
    let line = (line_steps(beats) * step) as usize;
    if line == 0 || count == 0 {
        return;
    }
    let count = count.min(MAX_PHRASE_STEPS);
    let laps = count.div_ceil(line);
    let mut taken = Rows::new(count);
    // The laps of the passes before, where the line's laps count on from pass to pass.
    let before = if own.laps {
        u64::from(own.pass).wrapping_mul(laps as u64)
    } else {
        0
    };
    for lap in 0..laps {
        let first = lap * line;
        group(
            beats,
            0,
            (0.0, line as f64),
            before.wrapping_add(lap as u64),
            100,
            &mut |landed: Landed| {
                // The row nearest it, within its own place.
                let last = (landed.end - 1e-9).floor().max(landed.at.floor());
                let row = landed.at.round().min(last).max(0.0) as usize;
                let nth = first + row;
                if nth >= count {
                    return;
                }
                if taken.has(nth) {
                    dropped();
                    return;
                }
                let plays = landed.chance >= 100
                    || mix(seed ^ CHANCE ^ u64::from(landed.hit), nth) % 100
                        < u64::from(landed.chance);
                if plays {
                    taken.set(nth);
                    each(Lands {
                        nth,
                        hit: landed.hit,
                        ratchet: landed.ratchet,
                        len: landed.end - landed.at,
                        off: landed.at - row as f64,
                    });
                }
            },
        );
    }
}

/// The rows of `counted` the rhythm hits, by their offsets in the span.
pub(super) fn chosen(beats: &[Beat], own: Own, seed: u64, counted: &Rows) -> Rows {
    let count = counted.count();
    let mut hits = Rows::new(count);
    // A line with no choice and no chance is the same each lap, so one lap is walked and the
    // rest are its copies.
    let same = !own.laps && !own.chances;
    let line = (line_steps(beats) * u32::from(own.step.clamp(1, MAX_STEP))) as usize;
    if same && line > 0 && line < count {
        walk(beats, own, seed, line, |lands| hits.set(lands.nth));
        for nth in line..count {
            if hits.has(nth % line) {
                hits.set(nth);
            }
        }
    } else {
        walk(beats, own, seed, count, |lands| hits.set(lands.nth));
    }
    let mut rows = Rows::new(counted.len());
    for (nth, offset) in counted.ones().enumerate() {
        if hits.has(nth) {
            rows.set(offset);
        }
    }
    rows
}

/// Which hit lands on each row counted, by its offset in the span: `tokens` gets the hit's place
/// on its row, for a field's rows to be lit (decision 460).
pub(super) fn hits(beats: &[Beat], own: Own, seed: u64, counted: &Rows, tokens: &mut [u8]) {
    let mut nths = counted.ones();
    let mut last = (0, None);
    walk(beats, own, seed, counted.count(), |lands| {
        // The hits land in order, so the offsets are walked once.
        let (mut nth, mut offset) = last;
        while nth <= lands.nth {
            offset = nths.next();
            nth += 1;
        }
        last = (nth, offset);
        if let Some(token) = offset.and_then(|offset| tokens.get_mut(offset)) {
            *token = lands.hit;
        }
    });
}

/// On the Note column, each note a hit `*n` finer than a row lands on gets a `RET` in its row's
/// first empty FX slot, its repeats as far apart as `n` share its place, so `x*3` in a row plays
/// three times there. A row with a `RET` already, or no empty slot, stays as it is.
pub(super) fn ratchet(
    beats: &[Beat],
    own: Own,
    seed: u64,
    counted: &Rows,
    (steps, first): (&[Step], usize),
    cells: &mut [GenCell],
) {
    let mut nths = counted.ones();
    let mut last = (0, None);
    walk(beats, own, seed, counted.count(), |lands| {
        let (mut nth, mut offset) = last;
        while nth <= lands.nth {
            offset = nths.next();
            nth += 1;
        }
        last = (nth, offset);
        let Some(offset) = offset.filter(|_| lands.ratchet > 1) else {
            return;
        };
        let units = UNITS_PER_STEP as f64 * lands.len / f64::from(lands.ratchet);
        let apart = (units.round() as u16).clamp(1, MOST_APART);
        let step = steps.get(first + offset);
        if let Some(cell) = cells.get_mut(offset) {
            ratcheted(cell, step, apart);
        }
    });
}

/// A note's cell with a `RET` of its own, its repeats `apart` units apart.
fn ratcheted(cell: &mut GenCell, step: Option<&Step>, apart: u16) {
    let Some(step) = step else {
        return;
    };
    let (note, velocity, gate) = match *cell {
        GenCell::Pitch(note) => (crate::step::Note::On(note), step.velocity, step.gate),
        GenCell::Note {
            note,
            velocity,
            gate,
        } => (note, velocity, gate),
        GenCell::Under if matches!(step.note, crate::step::Note::On(_)) => {
            (step.note, step.velocity, step.gate)
        }
        _ => return,
    };
    let mut fx = std::array::from_fn(|slot| step.fx.get(slot).copied().unwrap_or_default());
    let slots = step.fx.len().min(fx.len());
    let open = &mut fx[..slots];
    if open
        .iter()
        .any(|slot: &FxSlot| slot.command == Some(FxCommand::Ret))
    {
        return;
    }
    let Some(free) = open.iter_mut().find(|slot| slot.command.is_none()) else {
        return;
    };
    *free = FxSlot {
        command: Some(FxCommand::Ret),
        value: apart << 8,
        ..FxSlot::default()
    };
    *cell = GenCell::NoteFx {
        note,
        velocity,
        gate,
        fx,
    };
}

/// A line typed on a generator, kept beside its shape, as a text of any length can't go in one:
/// an `Own` rhythm's, or a `Mini`'s (decision 462), what was typed and the beats it reads as,
/// with a `Mini`'s values, a hit each, in the column's units.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OwnRhythm {
    text: Box<str>,
    beats: Box<[Beat]>,
    atoms: Box<[Atom]>,
}

impl OwnRhythm {
    /// What a generator of a shape on a column keeps of a text: a `Mini`'s line, an `Own`
    /// rhythm's, or none for any other shape, and the beats of a text that reads, or none, so a
    /// text that doesn't plays nothing.
    pub fn kept(shape: &Shape, column: ShapeColumn, text: &str) -> Self {
        Self::kept_as(shape, column, text, "")
    }

    /// The same on the track named `name`, which on a kit line plays the hits of its name
    /// (decision 464).
    pub fn kept_as(shape: &Shape, column: ShapeColumn, text: &str, name: &str) -> Self {
        if text.is_empty() {
            return Self::default();
        }
        if shape.kind == FillKind::Mini && shape.kit.is_some() {
            let (beats, atoms) = crate::notation::compile_kit(text, name).unwrap_or_default();
            return Self {
                text: text.into(),
                beats: beats.into(),
                atoms: atoms.into(),
            };
        }
        if shape.kind == FillKind::Mini {
            let (beats, atoms) = crate::notation::compile_mini(text, column).unwrap_or_default();
            return Self {
                text: text.into(),
                beats: beats.into(),
                atoms: atoms.into(),
            };
        }
        if !matches!(shape.place, Where::Own(_)) {
            return Self::default();
        }
        Self {
            text: text.into(),
            beats: crate::notation::compile_rhythm(text)
                .unwrap_or_default()
                .into(),
            atoms: Box::default(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn beats(&self) -> &[Beat] {
        &self.beats
    }

    /// What a render reads of it, beside a `List`'s values.
    pub fn texts<'a>(&'a self, list: &'a [u8]) -> Texts<'a> {
        Texts {
            list,
            rhythm: &self.beats,
            atoms: &self.atoms,
        }
    }

    /// A shape with what its line's beats say of them, in its `Where` or a `Mini`'s settings:
    /// whether its laps count on, whether it ratchets and whether it draws.
    pub fn settle(&self, shape: &mut Shape) {
        if shape.kind == FillKind::Mini {
            shape.mini = shape.mini.of(&self.beats);
        }
        if let Where::Own(own) = &mut shape.place {
            *own = own.of(&self.beats);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::compile_rhythm;
    use crate::scale::Scale;
    use crate::selection::StepRange;
    use crate::shape::render::chosen;
    use crate::shape::{Over, render_into, rhythm_tokens_into};
    use crate::step::Note;

    fn notes() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale::default(),
            writes: Some(0x64),
        }
    }

    /// The rows a text chooses of `count` empty ones at a `Step`, on a pass, as `x` and `.`.
    fn rows_on(text: &str, step: u8, count: usize, pass: u32, seed: u64) -> String {
        let beats = compile_rhythm(text).unwrap();
        let own = Own {
            step,
            pass,
            ..Own::default()
        }
        .of(&beats);
        let steps = vec![Step::default(); count];
        let range = StepRange::between(0, count - 1);
        let rows = chosen(
            &steps,
            range,
            notes(),
            Where::Own(own),
            Some(Over::AllRows),
            seed,
            count,
            &beats,
        );
        (0..count)
            .map(|row| if rows.has(row) { 'x' } else { '.' })
            .collect()
    }

    fn rows(text: &str, step: u8, count: usize) -> String {
        rows_on(text, step, count, 0, 1)
    }

    /// A top-level token is `Step` rows, a group splits its token's rows, and a Euclid's parts
    /// finer than a row land on the nearest row within their own.
    #[test]
    fn a_token_is_step_rows_and_a_group_splits_them() {
        assert_eq!(rows("x . [. x] x(3,8)", 4, 16), "x.........x.xx.x");
        assert_eq!(
            rows("x . . x . . x . . . x . x . . .", 1, 16),
            "x..x..x...x.x..."
        );
        // The line goes round to fill its span, and a span shorter than it cuts it.
        assert_eq!(rows("x . .", 1, 8), "x..x..x.");
        assert_eq!(rows("x . . . . . . . . x", 1, 4), "x...");
        // A token more makes the line longer, rather than every token shorter.
        assert_eq!(rows("x .", 2, 8), "x...x...");
        assert_eq!(rows("x . .", 2, 8), "x.....x.");
        // A triplet in a beat, each hit on its nearest row.
        assert_eq!(rows("[x x x]", 4, 4), "xx.x");
        // Two hits on one row keep the first; the second is gone.
        assert_eq!(rows("[x x x]", 1, 2), "xx");
        assert_eq!(rows("", 1, 4), "....");
    }

    #[test]
    fn repeats_lengths_and_euclids_rotated_later() {
        assert_eq!(rows("x!3 .", 1, 4), "xxx.");
        assert_eq!(rows("x@3 x", 1, 8), "x..xx..x");
        assert_eq!(rows("x*2 .", 2, 4), "xx..");
        assert_eq!(rows("x(3,8)@8", 1, 8), "x..x..x.");
        // Later, as `Rotate` moves Euclid's hits, not earlier as Tidal's third number does.
        assert_eq!(rows("x(3,8,1)@8", 1, 8), ".x..x..x");
        assert_eq!(rows("[x .](3,4)@4", 2, 8), "x.x.x...");
    }

    /// A choice takes the next each lap, a choice inside it when it comes round to it, and its
    /// laps count on into the next pass.
    #[test]
    fn a_choice_turns_each_lap_and_on_from_pass_to_pass() {
        assert_eq!(rows("<x .>", 4, 16), "x.......x.......");
        assert_eq!(rows("<x <. [x x]>>", 2, 8), "x...x.xx");
        assert_eq!(rows("<x . .>", 4, 16), "x...........x...");
        // Pass 1 starts on lap 4, the choice's second.
        assert_eq!(rows_on("<x . .>", 4, 16, 1, 1), "........x.......");
        // `*` on a choice plays its choices in turn within its place.
        assert_eq!(rows("<x .>*2", 2, 4), "x.x.");
        assert_eq!(rows("<. x>*2", 2, 4), ".x.x");
    }

    #[test]
    fn a_chance_draws_from_the_seed_the_same_each_pass() {
        assert_eq!(rows("x?0", 1, 16), "................");
        assert_eq!(rows("x?100", 1, 16), "xxxxxxxxxxxxxxxx");
        let half = rows("x?", 1, 256);
        let hits = half.matches('x').count();
        assert!((96..160).contains(&hits), "{hits} of 256");
        assert_eq!(
            half,
            rows_on("x?", 1, 256, 3, 1),
            "no laps, so every pass alike"
        );
        assert_ne!(half, rows_on("x?", 1, 256, 0, 2), "another seed");
    }

    fn render_notes(text: &str, step: u8, count: usize) -> Vec<GenCell> {
        let beats = compile_rhythm(text).unwrap();
        let own = Own {
            step,
            ..Own::default()
        };
        let shape = Shape {
            kind: FillKind::Ramp,
            place: Where::Own(own.of(&beats)),
            from: 48,
            to: 48,
            ..Shape::default()
        };
        let mut steps = vec![Step::default(); count];
        for step in &mut steps {
            step.note = Note::On(48);
        }
        let mut cells = vec![GenCell::Under; count];
        let texts = Texts {
            rhythm: &beats,
            ..Texts::NONE
        };
        let range = StepRange::between(0, count - 1);
        render_into(&steps, range, notes(), &shape, texts, None, &mut cells);
        cells
    }

    fn ret(cell: GenCell) -> Option<u16> {
        match cell {
            GenCell::NoteFx { fx, .. } => fx
                .iter()
                .find(|slot| slot.command == Some(FxCommand::Ret))
                .map(|slot| slot.value >> 8),
            _ => None,
        }
    }

    /// On the Note column, `x*3` in a row is a `RET` there, its repeats two units apart; with the
    /// rows to split, it's split rows instead.
    #[test]
    fn a_ratchet_finer_than_a_row_is_a_ret() {
        let cells = render_notes("x*3 .", 1, 4);
        assert_eq!(ret(cells[0]), Some(2));
        assert_eq!(ret(cells[2]), Some(2), "the line goes round");
        assert_eq!(cells[1], GenCell::Under);
        let split = render_notes("x*2", 2, 2);
        assert!(
            split
                .iter()
                .all(|cell| ret(*cell).is_none() && *cell != GenCell::Under)
        );
    }

    /// The table of hits: each row by the hit that landed on it.
    #[test]
    fn each_row_knows_its_hit() {
        let beats = compile_rhythm("x [x x] .").unwrap();
        let own = Own {
            step: 2,
            ..Own::default()
        };
        let shape = Shape {
            place: Where::Own(own),
            over: Some(Over::AllRows),
            ..Shape::default()
        };
        let steps = vec![Step::default(); 8];
        let mut tokens = [0; 8];
        let range = StepRange::between(0, 7);
        rhythm_tokens_into(&steps, range, notes(), &shape, &beats, &mut tokens);
        let none = crate::shape::NO_TOKEN;
        assert_eq!(tokens, [0, none, 1, 2, none, none, 0, none]);
    }
}
