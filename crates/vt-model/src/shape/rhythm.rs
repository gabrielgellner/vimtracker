//! Rhythms after Mutable Instruments' Marbles' T section, as `Where`s: the rows among those a
//! column could change that play, by Marbles' clusters, Markov, three states and complementary
//! models. Each walks the rows it could change in order, drawing from the seed, so a seed repeats
//! and `Vary` gives each pass its own.
//!
//! - **Clusters** play in runs of two to four rows, with gaps of rests between, so about its
//!   amount of the rows play, bunched. Marbles' clusters model bursts its two outer outputs at
//!   multiples of the clock for a few ticks at a time, which a row has no room for; a run of rows
//!   is that burst at the rows' own rate.
//! - **Markov** is Marbles' own rules for a channel. Its amount is Marbles' Bias: up from a third
//!   it favours repeating what played eight rows before and leaving a row after a hit, and below
//!   a third the reverse, and at a third it's a sparse coin; after more than 24 rests a row is far
//!   likelier to play. The two rules that read Marbles' other channel are left out, as a track has
//!   one. It starts from eight rows drawn at even odds, as the module's history is never empty, so
//!   at full Bias it loops them, and it goes round the rows before it writes, eight rows at
//!   least, as a loop playing round would, so the first rows know what the last ones played.
//! - **Three states** gives each row a gate, a rest or a tie, by Marbles' three states model with
//!   its two outputs as the gate and the tie: from mostly gates at 0%, through mostly rests at 50%,
//!   to mostly ties at 100%. A tie holds the value before it, and on notes of its own, the note
//!   before is held into it with no new attack (a `Hold` gate); with nothing before it to hold, as
//!   after a rest, it rests.
//! - **Complementary** is a coin a row for two tracks, Marbles' complementary Bernoulli: the
//!   first takes the rows it lands under the amount on, as `Chance` would, and the second the
//!   rest, so with the same seed each row goes to one track or the other.
//!
//! The models are adapted from Emilie Gillet's Marbles firmware, `marbles/random/t_generator.cc`:
//!
//! > Copyright 2015 Emilie Gillet.
//! >
//! > Permission is hereby granted, free of charge, to any person obtaining a copy of this software
//! > and associated documentation files (the "Software"), to deal in the Software without
//! > restriction, including without limitation the rights to use, copy, modify, merge, publish,
//! > distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
//! > Software is furnished to do so, subject to the following conditions:
//! >
//! > The above copyright notice and this permission notice shall be included in all copies or
//! > substantial portions of the Software.
//! >
//! > THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
//! > BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! > NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! > DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! > OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use super::mix;
use super::random::unit;
use super::render::Rows;
use super::{GenCell, ShapeColumn};
use crate::step::{Gate, GateKind};

/// What each model's draws are for, so they don't follow another's or a fill's.
const CLUSTER_START: u64 = 0xC105_7E00;
const CLUSTER_RUN: u64 = 0xC105_7E01;
const MARKOV: u64 = 0x3A2C_0F00;
const STATES: u64 = 0x7E57_A7E0;
/// The shortest and longest run of a cluster.
pub const SHORTEST_RUN: u64 = 2;
pub const LONGEST_RUN: u64 = 4;
/// How far back a Markov rhythm looks for what to repeat, Marbles' eight ticks.
pub const MARKOV_PERIOD: u32 = 8;
/// How many rows a Markov rhythm hears before the ones it writes, at least: as far back as it
/// looks.
const LEAD_IN: usize = MARKOV_PERIOD as usize;
/// After more than this many rests in a row, a Markov rhythm's next row is far likelier to play.
pub const MARKOV_STREAK: u32 = 24;

/// The rows of `open` a cluster rhythm plays: at each row not in a run, a run of two to four rows
/// starts with a chance worked out so that `percent` of the rows play, as the runs are three long
/// on average and a gap ends at each row with that chance.
pub(super) fn clusters(open: &Rows, percent: u8, seed: u64) -> Rows {
    let percent = u64::from(percent.min(100));
    let mut hit = Rows::new(open.len());
    // A run starts with the chance `p / (300 - 2p)`, so a run of 3 and a gap of `(300 - 3p) / p`
    // on average share the rows `p` to `100 - p`.
    let odds = 300 - 2 * percent;
    let mut left = 0;
    for offset in open.ones() {
        if left == 0 && mix(seed ^ CLUSTER_START, offset) % odds < percent {
            let runs = LONGEST_RUN - SHORTEST_RUN + 1;
            left = SHORTEST_RUN + mix(seed ^ CLUSTER_RUN, offset) % runs;
        }
        if left > 0 {
            hit.set(offset);
            left -= 1;
        }
    }
    hit
}

/// Marbles' Markov rules for one channel: the chance, as a logit, of a row playing after
/// `history`, whose lowest bit is the row before and whose eighth is eight rows before, and
/// `streak` rests in a row, at a `bias` of 0 to 100.
pub fn markov_logit(bias: u8, history: u32, streak: u32) -> f64 {
    let b = 1.5 * f64::from(bias.min(100)) / 100.0 - 0.5;
    let periodic = history >> (MARKOV_PERIOD - 1) & 1 == 1;
    let dense = history & 1 == 1;
    let mut logit = -1.5;
    if streak > MARKOV_STREAK {
        logit += 10.0;
    }
    logit += 8.0 * b.abs() * if periodic { b } else { -b };
    if dense {
        logit -= b;
    }
    logit.clamp(-10.0, 10.0)
}

/// The chance a logit is.
pub fn logistic(logit: f64) -> f64 {
    1.0 / (1.0 + exp(-logit))
}

/// `e` to the power of `x`, for `x` from -10 to 10, as `(1 + x / n)^n` for a large `n`: plain
/// multiplication, so it's the same on every machine, close to a part in a thousand.
fn exp(x: f64) -> f64 {
    let mut power = 1.0 + x / 65_536.0;
    for _ in 0..16 {
        power *= power;
    }
    power
}

/// The rows of `open` a Markov rhythm plays. Eight rows drawn at even odds have played before it,
/// as on the module, whose history is never empty, and it goes round the rows first, enough
/// times to hear at least eight, with draws of their own, so the first rows written know what
/// played before them; then it writes the next time round.
pub(super) fn markov(open: &Rows, bias: u8, seed: u64) -> Rows {
    let mut hit = Rows::new(open.len());
    let count = open.count();
    if count == 0 {
        return hit;
    }
    let laps = LEAD_IN.div_ceil(count) + 1;
    let mut history = (mix(seed ^ MARKOV, usize::MAX) & 0xFF) as u32;
    let mut streak = 0;
    for lap in 0..laps {
        let last = lap + 1 == laps;
        let key = if last {
            seed ^ MARKOV
        } else {
            seed ^ MARKOV ^ mix(MARKOV, laps - lap)
        };
        for offset in open.ones() {
            let plays = unit(mix(key, offset)) < logistic(markov_logit(bias, history, streak));
            history = history << 1 | u32::from(plays);
            streak = if plays { 0 } else { streak + 1 };
            if last && plays {
                hit.set(offset);
            }
        }
    }
    hit
}

/// What a row is in a three-state rhythm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Gate,
    Rest,
    Tie,
}

/// Marbles' three states at a `bias` of 0 to 100, for a draw `u` from 0 to 1: a rest below
/// `0.75 - |bias - 0.5|`, the gate above a threshold past that, and the tie between.
pub fn state(bias: u8, u: f64) -> State {
    let bias = f64::from(bias.min(100)) / 100.0;
    let none = 0.75 - (bias - 0.5).abs();
    let threshold = none + (1.0 - none) * (0.25 + bias * 0.5);
    if u <= none {
        State::Rest
    } else if u > threshold {
        State::Gate
    } else {
        State::Tie
    }
}

/// The gates and the ties among the rows of `open`, a three-state rhythm at `bias`. A tie with
/// nothing before it to hold, the first row or after a rest, rests.
pub(super) fn states(open: &Rows, bias: u8, seed: u64) -> (Rows, Rows) {
    let (mut gates, mut ties) = (Rows::new(open.len()), Rows::new(open.len()));
    let mut held = false;
    for offset in open.ones() {
        match state(bias, unit(mix(seed ^ STATES, offset))) {
            State::Gate => {
                gates.set(offset);
                held = true;
            }
            State::Tie if held => ties.set(offset),
            _ => held = false,
        }
    }
    (gates, ties)
}

/// Hold the value before each tie into it, over the cells a three-state rhythm's gates were
/// written into: each tie takes the cell before it, and on notes of its own, or `Acid`'s, the note
/// before is held into it. A gate the fill left alone has nothing to hold, so the ties after it
/// rest, as after a rest.
pub(super) fn tie(
    open: &Rows,
    gates: &Rows,
    ties: &Rows,
    column: ShapeColumn,
    cells: &mut [GenCell],
) {
    let notes = matches!(
        column,
        ShapeColumn::Note {
            writes: Some(_),
            ..
        }
    );
    let mut last: Option<usize> = None;
    for offset in open.ones() {
        let Some(&cell) = cells.get(offset) else {
            break;
        };
        if gates.has(offset) {
            last = (cell != GenCell::Under).then_some(offset);
        } else if ties.has(offset)
            && let Some(before) = last
        {
            cells[offset] = cells[before];
            if notes {
                cells[before] = held(cells[before]);
            }
            last = Some(offset);
        } else {
            last = None;
        }
    }
}

/// A note's cell held into the row after it, with no new attack.
fn held(cell: GenCell) -> GenCell {
    match cell {
        GenCell::Note {
            note,
            velocity,
            gate,
        } => GenCell::Note {
            note,
            velocity,
            gate: Some(Gate {
                kind: GateKind::Hold,
                ..gate.unwrap_or_default()
            }),
        },
        cell => cell,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::Scale;
    use crate::selection::StepRange;
    use crate::shape::{FillKind, Follow, Shape, Where, on_track, targets, written};
    use crate::step::{Note, Step};

    const VEL: ShapeColumn = ShapeColumn::Velocity;

    fn notes(count: usize) -> Vec<Step> {
        (0..count)
            .map(|_| "C-3 -- --- --- ---".parse().unwrap())
            .collect()
    }

    fn all(steps: &[Step]) -> StepRange {
        StepRange::between(0, steps.len() - 1)
    }

    /// Every one of `count` rows open.
    fn open(count: usize) -> Rows {
        let mut rows = Rows::new(count);
        for offset in 0..count {
            rows.set(offset);
        }
        rows
    }

    /// A ramp on these rows, from `00` to `7F`.
    fn ramp(place: Where, seed: u64) -> Shape {
        Shape {
            kind: FillKind::Ramp,
            from: 0,
            to: 0x7F,
            place,
            ..Shape::following(Follow::default(), seed)
        }
    }

    /// Which of `count` notes a `Where` chooses, with this seed.
    fn pattern(place: Where, count: usize, seed: u64) -> Vec<bool> {
        let steps = notes(count);
        let mut hit = vec![false; count];
        for row in targets(&steps, all(&steps), VEL, place, seed) {
            hit[row] = true;
        }
        hit
    }

    /// The share of rows played, and of the rows after a played row that play too, over many
    /// seeds of 64 rows.
    fn shares(place: Where) -> (f64, f64) {
        let (mut played, mut rows, mut after, mut followed) = (0, 0, 0, 0);
        for seed in 0..400 {
            let hit = pattern(place, 64, seed);
            played += hit.iter().filter(|hit| **hit).count();
            rows += hit.len();
            for pair in hit.windows(2).filter(|pair| pair[0]) {
                after += 1;
                followed += usize::from(pair[1]);
            }
        }
        (
            played as f64 / rows as f64,
            followed as f64 / after.max(1) as f64,
        )
    }

    #[test]
    fn exp_is_close_to_the_real_one() {
        for tenth in -100..=100 {
            let x = f64::from(tenth) / 10.0;
            let error = (exp(x) - x.exp()).abs() / x.exp();
            assert!(error < 2e-3, "{x}: {error}");
        }
        assert!((logistic(0.0) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn clusters_play_their_amount_of_the_rows_bunched_together() {
        assert!(pattern(Where::Clusters(0), 64, 1).iter().all(|hit| !hit));
        assert!(pattern(Where::Clusters(100), 64, 1).iter().all(|hit| *hit));
        for percent in [20, 35, 50, 70] {
            let (played, followed) = shares(Where::Clusters(percent));
            let wanted = f64::from(percent) / 100.0;
            assert!((played - wanted).abs() < 0.04, "{percent}%: {played}");
            // A hit is followed by another far more often than the share alone, which is how
            // often one of `Chance`'s is.
            assert!(followed > wanted + 0.1, "{percent}%: {followed}");
            let (chance, after_chance) = shares(Where::Chance(percent));
            assert!(
                (after_chance - chance).abs() < 0.05,
                "{percent}%: {after_chance}"
            );
        }
    }

    #[test]
    fn a_cluster_is_two_to_four_rows_long_unless_two_meet() {
        for seed in 0..200 {
            let hit = pattern(Where::Clusters(25), 64, seed);
            let mut run = 0;
            for (row, played) in hit.iter().enumerate() {
                if *played {
                    run += 1;
                    continue;
                }
                // Runs back to back merge, so a bunch is at least two rows.
                assert!(
                    run == 0 || run >= SHORTEST_RUN as usize,
                    "{seed} row {row}: {hit:?}"
                );
                run = 0;
            }
        }
    }

    #[test]
    fn markov_follows_its_transitions() {
        // On 512 rows the lead-in is one lap, and from the ninth row written, each row's history
        // is the rows written before it.
        for bias in [0, 33, 50, 80, 100] {
            // By whether the row eight before played, then whether the row before did.
            let mut counts = [[(0usize, 0usize); 2]; 2];
            for seed in 0..60 {
                let hit = pattern(Where::Markov(bias), 512, seed);
                let mut streak = None;
                for row in 0..hit.len() {
                    if row >= MARKOV_PERIOD as usize
                        && streak.is_some_and(|streak| streak <= MARKOV_STREAK)
                    {
                        let periodic = usize::from(hit[row - MARKOV_PERIOD as usize]);
                        let dense = usize::from(hit[row - 1]);
                        let count = &mut counts[periodic][dense];
                        count.0 += 1;
                        count.1 += usize::from(hit[row]);
                    }
                    streak = if hit[row] {
                        Some(0)
                    } else {
                        streak.map(|streak| streak + 1)
                    };
                }
            }
            for (periodic, by_dense) in counts.iter().enumerate() {
                for (dense, (rows, played)) in by_dense.iter().enumerate() {
                    if *rows < 400 {
                        continue;
                    }
                    let history = (periodic as u32) << (MARKOV_PERIOD - 1) | dense as u32;
                    let chance = logistic(markov_logit(bias, history, 0));
                    let share = *played as f64 / *rows as f64;
                    let room = 4.0 * (chance * (1.0 - chance) / *rows as f64).sqrt() + 0.01;
                    assert!(
                        (share - chance).abs() < room,
                        "Bias {bias}%, periodic {periodic}, dense {dense}: {share} of {rows}, \
                         not {chance}"
                    );
                }
            }
        }
    }

    #[test]
    fn markov_repeats_eight_rows_back_up_the_bias_and_avoids_it_down_it() {
        let repeats = |bias: u8| {
            let (mut same, mut rows) = (0, 0);
            for seed in 0..200 {
                let hit = pattern(Where::Markov(bias), 64, seed);
                for row in 8..hit.len() {
                    same += usize::from(hit[row] == hit[row - 8]);
                    rows += 1;
                }
            }
            same as f64 / rows as f64
        };
        assert!(repeats(100) > 0.95, "{}", repeats(100));
        assert!(repeats(0) < 0.6, "{}", repeats(0));
        // After more than 24 rests, a row is far likelier to play.
        for bias in [0, 50, 100] {
            for history in [0, 1] {
                let (rested, long) = (
                    markov_logit(bias, history, MARKOV_STREAK),
                    markov_logit(bias, history, MARKOV_STREAK + 1),
                );
                assert!(long > rested + 5.0, "{bias} {history}");
            }
        }
    }

    #[test]
    fn three_states_share_their_rows_as_marbles_does() {
        // Rests up to 0.75 - |bias - 0.5|, ties up to the threshold past that, gates above it.
        assert_eq!(state(50, 0.75), State::Rest);
        assert_eq!(state(50, 0.8), State::Tie);
        assert_eq!(state(50, 0.9), State::Gate);
        assert_eq!(state(0, 0.3), State::Tie);
        assert_eq!(state(0, 0.5), State::Gate);
        assert_eq!(state(100, 0.3), State::Tie);
        assert_eq!(state(100, 0.82), State::Gate);
        for bias in [0, 25, 50, 75, 100] {
            let (mut gates, mut ties, mut rows) = (0, 0, 0);
            for seed in 0..300 {
                let (gate, tie) = states(&open(64), bias, seed);
                gates += gate.count();
                ties += tie.count();
                rows += 64;
                for offset in tie.ones() {
                    // A tie always holds a gate or a tie.
                    assert!(offset > 0 && (gate.has(offset - 1) || tie.has(offset - 1)));
                }
            }
            let b = f64::from(bias) / 100.0;
            let none = 0.75 - (b - 0.5).abs();
            let threshold = none + (1.0 - none) * (0.25 + b * 0.5);
            let share = gates as f64 / rows as f64;
            assert!((share - (1.0 - threshold)).abs() < 0.02, "{bias}: {share}");
            // Fewer ties than drawn, as those after a rest rest.
            let tied = ties as f64 / rows as f64;
            assert!(tied <= threshold - none + 0.01, "{bias}: {tied}");
        }
    }

    #[test]
    fn a_tie_holds_the_value_before_it() {
        let steps = notes(64);
        let wrote = written(&steps, all(&steps), VEL, &ramp(Where::States(75), 9));
        let (gates, ties) = states(&open(64), 75, 9);
        assert!(ties.count() > 4, "{}", ties.count());
        let velocity = |row: usize| {
            wrote
                .iter()
                .find(|(at, _)| *at == row)
                .and_then(|(_, step)| step.velocity)
        };
        for offset in ties.ones() {
            assert!(velocity(offset).is_some());
            assert_eq!(velocity(offset), velocity(offset - 1), "row {offset}");
        }
        assert_eq!(wrote.len(), gates.count() + ties.count());
    }

    #[test]
    fn on_notes_of_its_own_a_tie_holds_the_note_before_into_it() {
        let steps: Vec<Step> = (0..64).map(|_| Step::default()).collect();
        let column = ShapeColumn::Note {
            scale: Scale::default(),
            writes: Some(0x64),
        };
        let mut shape = ramp(Where::States(90), 4);
        shape.from = 48;
        shape.to = 72;
        let wrote = written(&steps, all(&steps), column, &shape);
        let (gates, ties) = states(&open(64), 90, 4);
        assert!(ties.count() > 4 && gates.count() > 4);
        let step = |row: usize| {
            wrote
                .iter()
                .find(|(at, _)| *at == row)
                .map(|(_, step)| step)
        };
        for offset in ties.ones() {
            let (before, tied) = (step(offset - 1).unwrap(), step(offset).unwrap());
            assert!(matches!(tied.note, Note::On(_)));
            assert_eq!(before.note, tied.note, "row {offset}");
            assert_eq!(before.gate.map(|gate| gate.kind), Some(GateKind::Hold));
        }
        // A gate with no tie after it keeps the track's gate.
        for offset in gates.ones().filter(|offset| !ties.has(offset + 1)) {
            assert_eq!(step(offset).unwrap().gate, None, "row {offset}");
        }
    }

    #[test]
    fn complementary_gives_each_row_to_one_track_or_the_other() {
        for share in [0, 30, 50, 85, 100] {
            for seed in 0..50 {
                let first = Where::Complementary {
                    share,
                    second: false,
                };
                let shape = ramp(first, seed);
                let (one, two) = (on_track(&shape, 0), on_track(&shape, 1));
                assert_eq!(one.seed, two.seed);
                let a = pattern(one.place, 48, one.seed);
                let b = pattern(two.place, 48, two.seed);
                assert!(a.iter().zip(&b).all(|(a, b)| a != b), "{share}% {seed}");
                // The first track's rows are `Chance`'s.
                assert_eq!(a, pattern(Where::Chance(share), 48, seed));
            }
        }
        // A third and a fourth track are a pair of their own, with a coin of their own.
        let place = Where::Complementary {
            share: 50,
            second: false,
        };
        let shape = ramp(place, 3);
        let (three, four) = (on_track(&shape, 2), on_track(&shape, 3));
        assert_eq!(three.seed, four.seed);
        assert_ne!(three.seed, shape.seed);
        assert_eq!(three.place, place);
        assert_ne!(four.place, place);
    }

    #[test]
    fn each_rhythm_draws_from_its_seed_so_it_varies() {
        for place in [
            Where::Clusters(50),
            Where::Markov(50),
            Where::States(50),
            Where::Complementary {
                share: 50,
                second: true,
            },
        ] {
            assert!(ramp(place, 1).draws(), "{place:?}");
            let one = pattern(place, 64, 1);
            assert_eq!(one, pattern(place, 64, 1));
            assert_ne!(one, pattern(place, 64, 2), "{place:?}");
        }
    }

    #[test]
    fn markov_at_full_bias_loops_the_eight_rows_it_starts_from() {
        for seed in 0..50 {
            let hit = pattern(Where::Markov(100), 32, seed);
            assert!(hit.iter().any(|hit| *hit), "{seed}");
            let same = (8..32).filter(|row| hit[*row] == hit[row - 8]).count();
            assert!(same >= 22, "{seed}: {hit:?}");
        }
    }
}
