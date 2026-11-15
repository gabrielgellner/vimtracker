//! Phrases: one track's steps, reusable by ID, with their FX columns' lanes.

use serde::{Deserialize, Serialize};

use crate::constants::{FX_SLOTS, MAX_PHRASE_STEPS};
use crate::generator::{Generator, GeneratorFile, Span};
pub use crate::lane::Lane;
use crate::lane::MOST_DIVISION;
use crate::order::{Deal, Order, Round};
use crate::selection::{Extent, StepRange};
use crate::shape::{Shape, ShapeColumn, Source};
use crate::step::{FxCommand, FxSlot, Step};

/// One track's sequence of steps, reusable by ID.
///
/// It stores more than it plays, like a clip: a loop of `length` steps starting at `start`, with
/// anything either side of it kept but silent. Rows on screen count from the loop's start, so the
/// loop's first step is always beat 1. Each FX column can have a lane of its own. Its loop's rows
/// play in its order, while its lanes keep to their own loops, forward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PhraseFile", into = "PhraseFile")]
pub struct Phrase {
    steps: Vec<Step>,
    start: usize,
    length: usize,
    lanes: [Option<Lane>; FX_SLOTS],
    order: Order,
    generators: Vec<Generator>,
}

#[derive(Serialize, Deserialize)]
struct PhraseFile {
    steps: Vec<Step>,
    #[serde(default)]
    start: usize,
    #[serde(default)]
    length: Option<usize>,
    #[serde(default)]
    lanes: [Option<Lane>; FX_SLOTS],
    #[serde(default, skip_serializing_if = "Order::is_forward")]
    order: Order,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    generators: Vec<GeneratorFile>,
}

/// A lane that fits in this many stored steps, or none if it can't.
fn fitted(lane: Option<Lane>, stored: usize) -> Option<Lane> {
    let lane = lane?;
    (lane.start < stored).then(|| Lane {
        start: lane.start,
        length: lane.length.clamp(1, stored - lane.start),
        division: lane.division.clamp(1, MOST_DIVISION),
    })
}

impl From<PhraseFile> for Phrase {
    fn from(file: PhraseFile) -> Self {
        let stored = file.steps.len();
        let start = file.start.min(stored.saturating_sub(1));
        let length = file
            .length
            .unwrap_or(stored)
            .min(stored - start.min(stored));
        let generators = file
            .generators
            .into_iter()
            .filter_map(|generator| generator.generator(&file.steps))
            .collect();
        Self {
            lanes: file.lanes.map(|lane| fitted(lane, stored)),
            steps: file.steps,
            start,
            length,
            order: file.order,
            generators,
        }
    }
}

impl From<Phrase> for PhraseFile {
    fn from(phrase: Phrase) -> Self {
        Self {
            steps: phrase.steps,
            start: phrase.start,
            length: Some(phrase.length),
            lanes: phrase.lanes,
            order: phrase.order,
            generators: phrase.generators.iter().map(GeneratorFile::of).collect(),
        }
    }
}

impl Phrase {
    /// A phrase of empty steps. A phrase always has at least one.
    pub fn new(length: usize) -> Self {
        let length = length.max(1);
        Self {
            steps: vec![Step::default(); length],
            start: 0,
            length,
            lanes: [None; FX_SLOTS],
            order: Order::Forward,
            generators: Vec::new(),
        }
    }

    /// A phrase holding these steps, looping over all of them.
    pub fn from_steps(steps: Vec<Step>) -> Self {
        let length = steps.len();
        Self {
            steps,
            start: 0,
            length,
            lanes: [None; FX_SLOTS],
            order: Order::Forward,
            generators: Vec::new(),
        }
    }

    /// Every stored step, in or out of the loop.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The steps that play.
    pub fn loop_steps(&self) -> &[Step] {
        &self.steps[self.start..self.start + self.length]
    }

    /// The order its loop's rows play in.
    pub fn order(&self) -> Order {
        self.order
    }

    /// Play its loop's rows in another order.
    pub fn set_order(&mut self, order: Order) {
        self.order = order;
    }

    /// The rows of its loop one round of its order visits, in turn, with this deal if it's dealt.
    pub fn round(&self, deal: Deal) -> Round<'_> {
        Round::new(self.loop_steps(), self.order, deal)
    }

    /// The next value with this command in a slot, for a glide: looking on from a visit of a
    /// round, in the order the rows play and wrapping round the round, with how many steps there
    /// are from the start of that visit's row to the row it's on. The row itself doesn't count.
    pub fn glide_to(
        &self,
        visit: usize,
        deal: Deal,
        slot: usize,
        command: FxCommand,
    ) -> Option<(usize, FxSlot)> {
        let steps = self.loop_steps();
        let visits = self.round(deal).count();
        let mut time = 0;
        for (distance, row) in self
            .round(deal)
            .cycle()
            .skip(visit)
            .take(visits)
            .enumerate()
        {
            if distance > 0 {
                let fx = steps[row].fx.get(slot)?;
                if fx.command == Some(command) {
                    return Some((time, *fx));
                }
            }
            time += steps[row].pulses();
        }
        None
    }

    /// Its live generators, each on a column of its own rows or of a lane.
    pub fn generators(&self) -> &[Generator] {
        &self.generators
    }

    /// The rows a generator would cover now: its own, as many as are stored, or its FX column's
    /// lane, or none if that column has no lane.
    pub fn span_rows(&self, generator: &Generator) -> Option<StepRange> {
        let stored = self.steps.len();
        match generator.span {
            Span::Rows { first, last } => {
                (first < stored).then(|| StepRange::between(first, last.min(stored - 1)))
            }
            Span::Lane => match generator.column {
                ShapeColumn::Fx { slot, .. } => self.lane(slot).map(|lane| {
                    StepRange::between(lane.start, lane.start + lane.length.max(1) - 1)
                }),
                _ => None,
            },
        }
    }

    /// Make a generator live, from the values on its rows now. One on the same column whose rows
    /// overlap is applied first, so this one starts from the values it left. A `Follow` writes
    /// from `source`, the loop of the phrase it follows. Returns false if it has no rows here.
    pub fn add_generator(&mut self, mut generator: Generator, source: Option<&[Step]>) -> bool {
        let Some(range) = self.span_rows(&generator) else {
            return false;
        };
        self.generators.retain(|other| {
            !other.shares(generator.column)
                || other
                    .rows()
                    .is_none_or(|rows| rows.last < range.first || rows.first > range.last)
        });
        generator.start(&mut self.steps, range, source);
        self.generators.push(generator);
        true
    }

    /// Give a live generator a new shape, and write it, a `Follow` from `source`. Returns false if
    /// there's no such one.
    pub fn reshape_generator(
        &mut self,
        index: usize,
        shape: Shape,
        source: Option<&[Step]>,
    ) -> bool {
        let Some(generator) = self.generators.get_mut(index) else {
            return false;
        };
        generator.reshape(&mut self.steps, shape, source);
        true
    }

    /// Have a live generator held by its track's macros as this shape, or let go with none,
    /// writing its column again if that changes what's heard, a `Follow` from `source`.
    pub fn hold_generator(
        &mut self,
        index: usize,
        held: Option<Shape>,
        source: Option<&[Step]>,
    ) -> bool {
        let steps = &mut self.steps;
        self.generators
            .get_mut(index)
            .is_some_and(|generator| generator.hold(steps, held, source))
    }

    /// Write a live `Follow` again from its source's loop, which an edit changed: true if that
    /// changed its rows.
    pub fn refollow(&mut self, index: usize, source: &[Step]) -> bool {
        let steps = &mut self.steps;
        self.generators
            .get_mut(index)
            .is_some_and(|generator| generator.refollow(steps, source))
    }

    /// Apply a live generator: its values stay, as plain ones.
    pub fn apply_generator(&mut self, index: usize) -> Option<Generator> {
        (index < self.generators.len()).then(|| self.generators.remove(index))
    }

    /// Take a live generator away, and put back what was under its cells.
    pub fn remove_generator(&mut self, index: usize) -> Option<Generator> {
        let generator = self.apply_generator(index)?;
        generator.restore(&mut self.steps);
        Some(generator)
    }

    /// Follow an edit to the phrase: a live generator whose cells the edit changed by hand is
    /// applied, and the rest rerender where the edit changed what they read or moved their lane.
    /// A `Follow` rerenders from the loop `sources` gives for what it follows.
    pub fn follow_generators<'a>(&mut self, sources: impl Fn(Source) -> Option<&'a [Step]>) {
        let ranges: Vec<Option<StepRange>> = self
            .generators
            .iter()
            .map(|generator| self.span_rows(generator))
            .collect();
        let steps = &mut self.steps;
        let mut ranges = ranges.into_iter();
        self.generators.retain_mut(|generator| {
            let source = generator.source().and_then(&sources);
            generator.follow(steps, ranges.next().flatten(), source)
        });
    }

    /// An FX column's own lane, if it has one. One without follows the phrase's loop.
    pub fn lane(&self, slot: usize) -> Option<Lane> {
        self.lanes.get(slot).copied().flatten()
    }

    /// Give an FX column a lane of its own, or none to follow the phrase's loop again. Steps are
    /// stored for all of it, up to the most a phrase keeps.
    pub fn set_lane(&mut self, slot: usize, lane: Option<Lane>) {
        if slot >= FX_SLOTS {
            return;
        }
        if let Some(lane) = lane {
            self.store_up_to((lane.start + lane.length.max(1)).min(MAX_PHRASE_STEPS));
        }
        self.lanes[slot] = fitted(lane, self.steps.len());
    }

    /// The next value with this command in a lane, looking forward from one of its steps and
    /// wrapping within it, with how many steps away it is.
    pub fn next_in_lane(
        &self,
        slot: usize,
        step: usize,
        command: FxCommand,
    ) -> Option<(usize, FxSlot)> {
        let lane = self.lane(slot)?;
        let at = step.checked_sub(lane.start)?;
        (1..lane.length).find_map(|distance| {
            let index = lane.start + (at + distance) % lane.length;
            let fx = self.steps.get(index)?.fx.get(slot)?;
            (fx.command == Some(command)).then_some((distance, *fx))
        })
    }

    /// How many steps a pass of its loop lasts, whatever its order: each row's pulses, so a skipped
    /// row takes none.
    pub fn time(&self) -> usize {
        self.loop_steps().iter().map(Step::pulses).sum()
    }

    /// How many steps into a pass a row of the loop starts, as written, so where it plays forward.
    pub fn time_to(&self, row: usize) -> usize {
        self.loop_steps().iter().take(row).map(Step::pulses).sum()
    }

    /// How many steps a round of its order lasts before its walk repeats: a pass for an order that
    /// visits each row once, and for a dealt one, whose round is a pass, and longer for one that
    /// visits some rows twice.
    pub fn round_time(&self) -> usize {
        if self.order.is_dealt() {
            return self.time();
        }
        let steps = self.loop_steps();
        self.round(Deal::default())
            .map(|row| steps[row].pulses())
            .sum()
    }

    /// Where a pass starts in the round of its order, for a pass this many since its chain started.
    fn pass_start(&self, pass: usize, round: usize) -> usize {
        pass % round * (self.time() % round) % round
    }

    /// The visit of its order's walk playing this many steps into a pass, this many passes since
    /// its chain started, with the pass's deal if the order is dealt: its row, which of the row's
    /// pulses, and which visit of the round it is. The walk carries on from one pass to the next,
    /// so a row whose pulses run past the end of a pass plays the rest of them at the start of the
    /// next. Skipped rows are passed over, and a loop that takes no time has none.
    pub fn walk_at(&self, pass: usize, time: usize, deal: Deal) -> Option<Visit> {
        let round = self.round_time();
        if time >= self.time() || round == 0 {
            return None;
        }
        let steps = self.loop_steps();
        let mut left = (self.pass_start(pass, round) + time) % round;
        for (visit, row) in self.round(deal).enumerate() {
            let pulses = steps[row].pulses();
            if left < pulses {
                return Some(Visit {
                    row,
                    pulse: left,
                    visit,
                });
            }
            left -= pulses;
        }
        None
    }

    /// Where in a pass, this many since its chain started, the walk plays a row at one of its
    /// pulses, or its last if it has fewer: the place nearest to `near`, if the pass plays the row
    /// at all.
    pub fn place_in_pass(
        &self,
        row: usize,
        pulse: usize,
        pass: usize,
        near: usize,
        deal: Deal,
    ) -> Option<usize> {
        let (length, round) = (self.time() as i64, self.round_time() as i64);
        let steps = self.loop_steps();
        let pulses = steps.get(row)?.pulses();
        if pulses == 0 || round == 0 {
            return None;
        }
        let pulse = pulse.min(pulses - 1) as i64;
        let mut best: Option<i64> = None;
        // Where each round that plays in the pass starts, from the pass's start.
        let mut started = -(self.pass_start(pass, round as usize) as i64);
        while started < length {
            let mut time = started;
            for visited in self.round(deal) {
                let place = time + pulse;
                let closer = |best: i64| place.abs_diff(near as i64) < best.abs_diff(near as i64);
                if visited == row && (0..length).contains(&place) && best.is_none_or(closer) {
                    best = Some(place);
                }
                time += steps[visited].pulses() as i64;
            }
            started += round;
        }
        best.map(|place| place as usize)
    }

    /// The number of rows that play.
    pub fn len(&self) -> usize {
        self.length
    }

    /// Always false, since a phrase has at least one step.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Where the loop starts among the stored steps.
    pub fn start(&self) -> usize {
        self.start
    }

    /// Where the loop sits, for working out rows and beats.
    pub fn extent(&self) -> Extent {
        Extent {
            start: self.start,
            length: self.length,
            stored: self.steps.len(),
        }
    }

    /// One stored step, to change.
    pub fn step_mut(&mut self, index: usize) -> Option<&mut Step> {
        self.steps.get_mut(index)
    }

    /// Set the loop's length, keeping every step. A length of 0 becomes 1.
    pub fn set_len(&mut self, length: usize) {
        self.length = length.max(1);
        self.store_up_to(self.start + self.length);
    }

    /// Store at least this many steps, up to the most a phrase keeps. The loop stays where it is.
    pub fn grow_to(&mut self, stored: usize) {
        self.store_up_to(stored.min(MAX_PHRASE_STEPS));
    }

    fn store_up_to(&mut self, stored: usize) {
        if stored > self.steps.len() {
            self.steps.resize(stored, Step::default());
        }
    }

    /// Move the loop without changing its length, adding empty steps if it runs off either end.
    pub fn slide(&mut self, by: isize) {
        let room = MAX_PHRASE_STEPS.saturating_sub(self.steps.len());
        let to = (self.start as isize).saturating_add(by);
        // Where the loop started, counted in the steps stored after any are added in front.
        let mut was = self.start;
        if to < 0 {
            let added = to.unsigned_abs().min(room);
            self.steps
                .splice(0..0, std::iter::repeat_n(Step::default(), added));
            self.start = 0;
            was += added;
            for lane in self.lanes.iter_mut().flatten() {
                lane.start += added;
            }
            for generator in &mut self.generators {
                generator.shift(added);
            }
        } else {
            let highest = MAX_PHRASE_STEPS
                .max(self.steps.len())
                .saturating_sub(self.length);
            self.start = (to as usize).min(highest);
        }
        self.store_up_to(self.start + self.length);
        // Lanes move with the loop, as they're set where it is. One that would go above the first
        // step stops there.
        let moved = self.start as isize - was as isize;
        let mut lanes = self.lanes;
        for lane in lanes.iter_mut().flatten() {
            lane.start = lane.start.saturating_add_signed(moved);
        }
        for (slot, lane) in lanes.into_iter().enumerate() {
            self.set_lane(slot, lane);
        }
    }

    /// Move the loop's steps later, wrapping round within it. Negative moves them earlier, and steps
    /// outside the loop stay where they are. All the way round moves nothing, and changes nothing.
    pub fn rotate(&mut self, by: isize) {
        let (start, length) = (self.start, self.length);
        let by = by.rem_euclid(length.max(1) as isize) as usize;
        if by == 0 {
            return;
        }
        // A live generator on the rows it moves is applied, as its values move with them.
        let end = start + length;
        self.generators.retain(|generator| {
            generator
                .rows()
                .is_none_or(|rows| rows.last < start || rows.first >= end)
        });
        self.steps[start..end].rotate_right(by);
    }

    /// Double the loop by repeating it. Steps after it move down rather than being overwritten.
    pub fn double(&mut self) {
        // Its live generators are applied, as the rows after the loop move.
        self.generators.clear();
        let copy = self.loop_steps().to_vec();
        let end = self.start + self.length;
        let added = copy.len();
        self.steps.splice(end..end, copy);
        self.length *= 2;
        // Lanes after the loop move down with their steps.
        for lane in self.lanes.iter_mut().flatten() {
            if lane.start >= end {
                lane.start += added;
            }
        }
    }
}

/// Where a place in a pass of a phrase lands, on its order's walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visit {
    /// The row of the phrase's loop.
    pub row: usize,
    /// Which of the row's pulses, from 0 for its first.
    pub pulse: usize,
    /// Which of the round's visits, which is the row in a phrase played forward.
    pub visit: usize,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn a_lane_saves_fits_its_steps_and_follows_them() {
        let mut phrase = Phrase::new(16);
        phrase.set_lane(
            1,
            Some(Lane {
                division: 2,
                ..Lane::new(4, 3)
            }),
        );
        assert_eq!(
            phrase.lane(1),
            Some(Lane {
                division: 2,
                ..Lane::new(4, 3)
            })
        );
        assert_eq!(phrase.lane(0), None, "others follow the phrase");
        let text = ron::to_string(&phrase).unwrap();
        assert_eq!(ron::from_str::<Phrase>(&text).unwrap(), phrase);

        // A lane past the end stores the steps it needs.
        phrase.set_lane(2, Some(Lane::new(14, 6)));
        assert_eq!(phrase.steps().len(), 20);
        // Sliding the loop moves it with the loop, whether steps are added in front or not.
        phrase.slide(-2);
        assert_eq!(
            phrase.lane(1).unwrap().start,
            4,
            "on the same step of the loop"
        );
        assert_eq!(phrase.start(), 0);
        phrase.slide(3);
        assert_eq!(phrase.lane(1).unwrap().start, 7);
        // Doubling the loop moves one after it down with its steps.
        let mut doubled = Phrase::new(4);
        doubled.grow_to(8);
        doubled.set_lane(0, Some(Lane::new(5, 2)));
        doubled.double();
        assert_eq!(doubled.lane(0).unwrap().start, 9);
    }

    #[test]
    fn a_saved_phrase_without_lanes_has_none_and_a_bad_one_is_fitted() {
        let mut old = ron::to_string(&Phrase::new(2)).unwrap();
        old = old.replace(",lanes:(None,None,None)", "");
        assert!(!old.contains("lanes"), "{old}");
        let phrase: Phrase = ron::from_str(&old).unwrap();
        assert_eq!(phrase.lane(0), None);
        let file = PhraseFile {
            steps: vec![Step::default(); 2],
            start: 0,
            length: Some(2),
            order: Order::Forward,
            generators: Vec::new(),
            lanes: [
                Some(Lane {
                    division: 40,
                    ..Lane::new(1, 9)
                }),
                Some(Lane::new(5, 1)),
                None,
            ],
        };
        let phrase = Phrase::from(file);
        assert_eq!(
            phrase.lane(0),
            Some(Lane {
                division: MOST_DIVISION,
                ..Lane::new(1, 1)
            })
        );
        assert_eq!(phrase.lane(1), None, "it starts past the steps");
    }

    #[test]
    fn a_lane_reads_its_rows_and_a_glide_looks_within_it() {
        let lane = Lane::new(2, 3);
        let read: Vec<usize> = (0..7).map(|row| lane.stored(row)).collect();
        assert_eq!(read, [2, 3, 4, 2, 3, 4, 2]);
        let steps = [
            "--- -- cut:10 --- ---",
            "--- -- --- --- ---",
            "--- -- cut:20 --- ---",
            "--- -- --- --- ---",
            "--- -- cut:30 --- ---",
        ];
        let mut phrase = Phrase::from_steps(steps.iter().map(|t| t.parse().unwrap()).collect());
        phrase.set_lane(0, Some(lane));
        let next = |step| {
            phrase
                .next_in_lane(0, step, FxCommand::CUT)
                .map(|(distance, fx)| (distance, fx.coarse()))
        };
        assert_eq!(
            next(4),
            Some((1, 0x20)),
            "wraps to the lane's start, not row 0"
        );
        assert_eq!(next(2), Some((2, 0x30)));
        assert_eq!(next(0), None, "not in the lane");
    }

    #[test]
    fn next_value_looks_forward_in_the_slot_and_wraps() {
        let steps = [
            "--- -- cut:10 ---",
            "--- -- res:20 ---",
            "--- -- cut:30 cut:90",
            "--- -- --- ---",
        ];
        let steps = steps.iter().map(|text| text.parse().unwrap()).collect();
        let phrase = Phrase::from_steps(steps);
        let found = |step, slot, command| {
            phrase
                .glide_to(step, Deal::default(), slot, command)
                .map(|(distance, fx)| (distance, fx.coarse()))
        };
        assert_eq!(found(0, 0, FxCommand::CUT), Some((2, 0x30)));
        assert_eq!(found(2, 0, FxCommand::CUT), Some((2, 0x10)));
        assert_eq!(found(1, 0, FxCommand::RES), None, "not itself");
        assert_eq!(found(0, 1, FxCommand::CUT), Some((2, 0x90)));
        assert_eq!(found(0, 0, FxCommand::VOL), None);
    }
    pub(crate) fn gated(gates: &[&str]) -> Phrase {
        Phrase::from_steps(
            gates
                .iter()
                .map(|gate| format!("C-3 -- {gate} --- --- ---").parse().unwrap())
                .collect(),
        )
    }

    #[test]
    fn a_row_lasts_its_pulses_and_a_skipped_one_none() {
        let phrase = gated(&["SGL1", "HLD3", "MLT0", "RST2"]);
        assert_eq!(phrase.time(), 6);
        let deal = Deal::default();
        let rows: Vec<(usize, usize)> = (0..6)
            .filter_map(|t| phrase.walk_at(0, t, deal))
            .map(|visit| (visit.row, visit.pulse))
            .collect();
        assert_eq!(rows, [(0, 0), (1, 0), (1, 1), (1, 2), (3, 0), (3, 1)]);
        assert_eq!(phrase.walk_at(0, 6, deal), None);
        assert_eq!(phrase.time_to(3), 4, "the skipped row takes no time");
        let skipped = gated(&["SGL0", "HLD0"]);
        assert_eq!(skipped.time(), 0);
        assert_eq!(
            skipped.walk_at(0, 0, deal),
            None,
            "nothing to play, and no hang"
        );
    }

    /// The rows a phrase's walk plays, a step at a time, over this many of its passes.
    fn walked(phrase: &Phrase, passes: usize) -> Vec<usize> {
        (0..passes)
            .flat_map(|pass| {
                (0..phrase.time()).map(move |t| phrase.walk_at(pass, t, Deal::default()).unwrap())
            })
            .map(|visit| visit.row + 1)
            .collect()
    }

    #[test]
    fn an_order_is_a_walk_and_a_pass_keeps_the_phrases_length() {
        // The author's examples: six steps of a phrase of four.
        let mut phrase = Phrase::new(4);
        let six = |phrase: &Phrase| walked(phrase, 2)[..6].to_vec();
        assert_eq!(six(&phrase), [1, 2, 3, 4, 1, 2]);
        phrase.set_order(Order::UpDown);
        assert_eq!(phrase.time(), 4);
        assert_eq!(six(&phrase), [1, 2, 3, 4, 3, 2]);
        phrase.set_order(Order::UpDownInc);
        assert_eq!(six(&phrase), [1, 2, 3, 4, 4, 3]);
        phrase.set_order(Order::Pedal);
        assert_eq!(six(&phrase), [1, 2, 1, 3, 1, 4]);
        // And on, each pass the phrase's length, the walk carrying on across them.
        phrase.set_order(Order::UpDown);
        assert_eq!(walked(&phrase, 3), [1, 2, 3, 4, 3, 2, 1, 2, 3, 4, 3, 2]);
        phrase.set_order(Order::DownUp);
        assert_eq!(walked(&phrase, 3), [4, 3, 2, 1, 2, 3, 4, 3, 2, 1, 2, 3]);
        phrase.set_order(Order::DownUpInc);
        assert_eq!(walked(&phrase, 3), [4, 3, 2, 1, 1, 2, 3, 4, 4, 3, 2, 1]);
        phrase.set_order(Order::Pedal);
        assert_eq!(walked(&phrase, 3), [1, 2, 1, 3, 1, 4, 1, 2, 1, 3, 1, 4]);
        // An order that visits each row once keeps to its pass.
        phrase.set_order(Order::Converge);
        assert_eq!(walked(&phrase, 2), [1, 4, 2, 3, 1, 4, 2, 3]);
    }

    #[test]
    fn a_rows_pulses_stay_together_and_run_on_into_the_next_pass() {
        let mut phrase = gated(&["SGL1", "MLT2", "SGL1"]);
        let deal = Deal::default();
        let rows = |phrase: &Phrase, pass| -> Vec<(usize, usize)> {
            (0..phrase.time())
                .filter_map(|t| phrase.walk_at(pass, t, deal))
                .map(|visit| (visit.row, visit.pulse))
                .collect()
        };
        phrase.set_order(Order::Reverse);
        assert_eq!(phrase.time(), 4);
        assert_eq!(rows(&phrase, 0), [(2, 0), (1, 0), (1, 1), (0, 0)]);
        phrase.set_order(Order::UpDown);
        assert_eq!(phrase.time(), 4, "the pass keeps the rows' length");
        assert_eq!(
            phrase.round_time(),
            6,
            "while a round plays the middle row twice"
        );
        assert_eq!(rows(&phrase, 0), [(0, 0), (1, 0), (1, 1), (2, 0)]);
        // The middle row on the way back, then the first, and the middle row's first pulse ends
        // the pass, so its second starts the next.
        assert_eq!(rows(&phrase, 1), [(1, 0), (1, 1), (0, 0), (1, 0)]);
        assert_eq!(rows(&phrase, 2), [(1, 1), (2, 0), (1, 0), (1, 1)]);
        assert_eq!(
            rows(&phrase, 3),
            rows(&phrase, 0),
            "three passes are two rounds"
        );
    }

    #[test]
    fn a_row_is_found_where_its_pass_plays_it_nearest() {
        let mut phrase = Phrase::new(4);
        phrase.set_order(Order::UpDown);
        let deal = Deal::default();
        // The second pass plays 3 2 1 2: row 2 is at 1 and 3, and row 4 isn't in it.
        assert_eq!(phrase.place_in_pass(1, 0, 1, 0, deal), Some(1));
        assert_eq!(phrase.place_in_pass(1, 0, 1, 3, deal), Some(3));
        assert_eq!(phrase.place_in_pass(3, 0, 1, 0, deal), None);
        // A pulse it no longer has is its last.
        let mut pulses = gated(&["SGL1", "SGL2"]);
        pulses.set_order(Order::Reverse);
        assert_eq!(pulses.place_in_pass(1, 5, 0, 0, deal), Some(1));
    }

    #[test]
    fn a_glide_looks_on_in_the_order_the_rows_play() {
        let steps = [
            "--- -- cut:10 ---",
            "--- -- --- ---",
            "--- -- cut:30 ---",
            "--- -- --- ---",
        ];
        let mut phrase = Phrase::from_steps(steps.iter().map(|t| t.parse().unwrap()).collect());
        phrase.set_order(Order::Reverse);
        let found = |phrase: &Phrase, visit| {
            phrase
                .glide_to(visit, Deal::default(), 0, FxCommand::CUT)
                .map(|(time, fx)| (time, fx.coarse()))
        };
        // Reverse plays rows 3 2 1 0, so from row 2, its visit 1, row 0 is two rows on.
        assert_eq!(found(&phrase, 1), Some((2, 0x10)));
        assert_eq!(found(&phrase, 3), Some((2, 0x30)), "round to row 2 again");
        phrase.set_order(Order::UpDown);
        // Up-down goes round 0 1 2 3 2 1, so from row 2 on the way up, row 2 again is two on.
        assert_eq!(found(&phrase, 2), Some((2, 0x30)));
    }

    #[test]
    fn a_phrase_saves_its_order_only_when_it_has_one() {
        let mut phrase = Phrase::new(2);
        assert!(!ron::to_string(&phrase).unwrap().contains("order"));
        phrase.set_order(Order::DownUpInc);
        let saved = ron::to_string(&phrase).unwrap();
        assert!(saved.contains("order:DownUpInc"), "{saved}");
        let loaded: Phrase = ron::from_str(&saved).unwrap();
        assert_eq!(loaded.order(), Order::DownUpInc);
    }

    #[test]
    fn a_phrase_saves_a_snake_by_name_and_cartesian_with_its_numbers() {
        use crate::order::{Cartesian, Snake};
        let mut phrase = Phrase::new(16);
        for (order, text) in [
            (Order::Snake(Snake::SpiralOut), "order:Snake(SpiralOut)"),
            (
                Order::Cartesian(Cartesian::new(1, 3)),
                "order:Cartesian((x_rate:1,y_rate:3))",
            ),
            (
                Order::Cartesian(Cartesian::new(2, 5).sized(4, 3)),
                "order:Cartesian((x_rate:2,y_rate:5,x_length:4,y_length:3))",
            ),
            (Order::Jump(5), "order:Jump(5)"),
            (
                Order::Arp(crate::order::Arp { notes: 4, step: 2 }),
                "order:Arp((notes:4,step:2))",
            ),
            (Order::QuasiRandom, "order:QuasiRandom"),
        ] {
            phrase.set_order(order);
            let saved = ron::to_string(&phrase).unwrap();
            assert!(saved.contains(text), "{saved}");
            let loaded: Phrase = ron::from_str(&saved).unwrap();
            assert_eq!(loaded.order(), order);
        }
    }

    /// A phrase whose steps can be told apart by velocity, which is the step's stored position.
    pub(crate) fn numbered(length: usize) -> Phrase {
        let steps = (0..length)
            .map(|index| Step {
                velocity: Some(index as u8),
                ..Step::default()
            })
            .collect();
        Phrase::from_steps(steps)
    }

    fn played(phrase: &Phrase) -> Vec<u8> {
        phrase
            .loop_steps()
            .iter()
            .map(|step| step.velocity.unwrap())
            .collect()
    }

    #[test]
    fn steps_outside_the_loop_stay_stored_and_saved() {
        let mut phrase = numbered(4);
        phrase.set_len(2);
        phrase.slide(1);
        assert_eq!(played(&phrase), [1, 2]);
        assert_eq!(phrase.steps().len(), 4);
        let saved = ron::to_string(&phrase).unwrap();
        let loaded: Phrase = ron::from_str(&saved).unwrap();
        assert_eq!(loaded, phrase);
        phrase.set_len(0);
        assert_eq!(phrase.len(), 1);
    }

    #[test]
    fn a_file_from_before_loops_plays_all_its_steps() {
        let file = PhraseFile {
            steps: vec![Step::default(); 3],
            start: 0,
            lanes: [None; FX_SLOTS],
            length: None,
            order: Order::Forward,
            generators: Vec::new(),
        };
        let phrase = Phrase::from(file);
        assert_eq!((phrase.start(), phrase.len()), (0, 3));
    }

    #[test]
    fn a_loop_past_the_end_of_a_file_is_pulled_back() {
        let file = PhraseFile {
            steps: vec![Step::default(); 3],
            start: 9,
            lanes: [None; FX_SLOTS],
            length: Some(9),
            order: Order::Forward,
            generators: Vec::new(),
        };
        let phrase = Phrase::from(file);
        assert_eq!((phrase.start(), phrase.len()), (2, 1));
        assert_eq!(phrase.loop_steps().len(), 1);
    }

    #[test]
    fn sliding_keeps_the_length_and_adds_steps_at_either_end() {
        let mut phrase = numbered(4);
        phrase.set_len(2);
        phrase.slide(3);
        assert_eq!(phrase.start(), 3);
        assert_eq!(phrase.steps().len(), 5, "one more step, so the loop fits");
        phrase.slide(-5);
        assert_eq!(phrase.start(), 0);
        assert_eq!(phrase.len(), 2);
        assert_eq!(phrase.steps().len(), 7, "two more in front");
        assert_eq!(phrase.steps()[2].velocity, Some(0), "what was first moved");
    }

    #[test]
    fn a_huge_slide_stops_at_the_most_a_phrase_keeps() {
        let mut phrase = numbered(4);
        phrase.slide(isize::MAX);
        assert_eq!(phrase.steps().len(), MAX_PHRASE_STEPS);
        assert_eq!(phrase.loop_steps().len(), 4);
        let mut phrase = numbered(4);
        phrase.slide(isize::MIN);
        assert_eq!(phrase.steps().len(), MAX_PHRASE_STEPS);
        assert_eq!(phrase.start(), 0);
    }

    #[test]
    fn doubling_repeats_the_loop_and_keeps_what_is_after_it() {
        let mut phrase = numbered(4);
        phrase.set_len(2);
        phrase.slide(1);
        phrase.double();
        assert_eq!(played(&phrase), [1, 2, 1, 2]);
        assert_eq!(
            phrase.steps()[5].velocity,
            Some(3),
            "moved, not overwritten"
        );
    }

    #[test]
    fn rotating_stays_inside_the_loop() {
        use crate::step::Note;

        let note = |midi| Step {
            note: Note::On(midi),
            ..Step::default()
        };
        let mut phrase = Phrase::from_steps((60..66).map(note).collect());
        phrase.set_len(3);
        phrase.slide(1);
        let notes =
            |phrase: &Phrase| -> Vec<Note> { phrase.steps().iter().map(|s| s.note).collect() };

        phrase.rotate(1);
        let on = |midi: [u8; 6]| midi.map(Note::On).to_vec();
        assert_eq!(notes(&phrase), on([60, 63, 61, 62, 64, 65]));
        phrase.rotate(-1);
        assert_eq!(notes(&phrase), on([60, 61, 62, 63, 64, 65]));
        phrase.rotate(4);
        assert_eq!(notes(&phrase), on([60, 63, 61, 62, 64, 65]));
        assert_eq!((phrase.start(), phrase.len()), (1, 3));
    }
}
