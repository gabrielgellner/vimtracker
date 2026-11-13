//! `Variation`: the Note column writing something like what's there, from a Markov chain learned
//! from the notes under it and nothing else. Each row is a state: a note (in the scale's degrees
//! while it's on), whether it's accented and whether it slides, or a rest or an `OFF`, so the
//! rhythm is learned with the notes. The chain counts which state follows each run of `order`
//! states, going round the rows as a loop does, and writes a line of the same length, each row
//! drawn from what followed the rows before it, the likelier more often, `temperature` deciding
//! how much more.
//!
//! The counts are kept in arrays sized by the rows learned from rather than by the states there
//! could be: a dense table of 514 states to the power of the order, times the states that can
//! follow, is far too large at order 3, whereas a source of so many rows has at most that many
//! runs before a row and that many pairs of a run and what follows it. Learning from at most
//! [`MAX_LEARNED`] rows keeps them a few kilobytes on the stack, so a render allocates nothing and
//! the engine can run it.
//!
//! The idea is aicd's, a Max for Live device that writes acid lines from n-grams of many 303
//! patterns with a temperature dial; here the only patterns are the phrase's own.

use serde::{Deserialize, Serialize};

use super::{GenCell, ShapeColumn, mix};
use crate::scale::Scale;
use crate::selection::StepRange;
use crate::step::{ACCENT_VELOCITY, GateKind, Note, Step};

/// The longest run of rows a state can hang on: 1 is loose, 3 copies closely.
pub const MAX_ORDER: u8 = 3;
/// The fewest notes there must be to learn from.
pub const MIN_NOTES: usize = 4;
/// The most rows learned from, the first of the span's: 16 bars of sixteenths.
pub const MAX_LEARNED: usize = 256;

/// What a `Variation` learns and how far it strays. Saved with a live generator's shape, and left
/// out while it's as it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Variation {
    /// How many rows before each row its state hangs on, from 1 to [`MAX_ORDER`].
    pub order: u8,
    /// How far from the likeliest state it may stray, in percent: at 0 always the likeliest, at
    /// 50 each as often as the rows there have it, and at 100 any that ever followed equally.
    pub temperature: u8,
}

impl Default for Variation {
    fn default() -> Self {
        Self {
            order: 1,
            temperature: 50,
        }
    }
}

impl Variation {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A row's state with no note: a rest, or an `OFF`. A note's is its pitch, accent and slide in
/// nine bits, below these.
const REST: u16 = 0x200;
const OFF: u16 = 0x201;
/// Marks the end of a list of what followed.
const NONE: u16 = u16::MAX;
/// What the draws are for, so they don't follow another fill's.
const DRAW: u64 = 0x7A21_0001;

/// How many notes a span has to learn from, among the rows it learns from.
pub fn notes(steps: &[Step], range: StepRange) -> usize {
    (range.first..=range.last)
        .take(MAX_LEARNED)
        .filter_map(|row| steps.get(row))
        .filter(|step| matches!(step.note, Note::On(_)))
        .count()
}

/// A note as it's learned and written: in the scale while it's on.
fn pitch(note: u8, scale: Scale) -> u8 {
    if scale.on { scale.fitted(note) } else { note }
}

/// A row's state: its note in the scale, accented or not and sliding or not, or a rest or `OFF`.
/// No velocity is an accent, as it plays at full level.
fn state(step: &Step, scale: Scale) -> u16 {
    match step.note {
        Note::On(note) => {
            let accent = step.velocity.is_none_or(|v| v >= ACCENT_VELOCITY);
            let slide = step.gate.is_some_and(|gate| gate.kind == GateKind::Hold);
            u16::from(pitch(note, scale)) << 2 | u16::from(accent) << 1 | u16::from(slide)
        }
        Note::Off => OFF,
        Note::Empty => REST,
    }
}

/// The chain learned from a span's rows: each row's state, and for each run of `order` states
/// before a row, what followed it and how often. A run is known by the first row it comes before,
/// and what followed it by the first row it did, which is the row a variation copies its velocity
/// and gate from and whose run it goes on from.
struct Chain {
    states: [u16; MAX_LEARNED],
    len: usize,
    /// Each row's run, as the first row with the same run before it.
    run: [u16; MAX_LEARNED],
    /// Each run's first follower, by its first row.
    head: [u16; MAX_LEARNED],
    /// Each follower: the next of the same run, the first row it followed on and how often.
    next: [u16; MAX_LEARNED],
    row: [u16; MAX_LEARNED],
    count: [u16; MAX_LEARNED],
}

impl Chain {
    fn learn(steps: &[Step], range: StepRange, scale: Scale, order: u8) -> Self {
        let rows = steps.len().saturating_sub(range.first);
        let len = range.len().min(rows).min(MAX_LEARNED);
        let mut chain = Self {
            states: [REST; MAX_LEARNED],
            len,
            run: [0; MAX_LEARNED],
            head: [NONE; MAX_LEARNED],
            next: [NONE; MAX_LEARNED],
            row: [0; MAX_LEARNED],
            count: [0; MAX_LEARNED],
        };
        for (at, state) in chain.states[..len].iter_mut().enumerate() {
            *state = self::state(&steps[range.first + at], scale);
        }
        // The `order` states before each row, going round, packed 16 bits a state.
        let mut runs = [0u64; MAX_LEARNED];
        for (at, run) in runs[..len].iter_mut().enumerate() {
            *run = (1..=usize::from(order)).fold(0, |packed, back| {
                let before = (at + len * usize::from(MAX_ORDER) - back) % len;
                packed << 16 | u64::from(chain.states[before])
            });
        }
        let mut followers = 0;
        for at in 0..len {
            let run = (0..=at)
                .find(|first| runs[*first] == runs[at])
                .unwrap_or(at);
            chain.run[at] = run as u16;
            let mut last = NONE;
            let mut follower = chain.head[run];
            while follower != NONE {
                let index = usize::from(follower);
                if chain.states[usize::from(chain.row[index])] == chain.states[at] {
                    break;
                }
                last = follower;
                follower = chain.next[index];
            }
            if follower != NONE {
                chain.count[usize::from(follower)] += 1;
                continue;
            }
            chain.row[followers] = at as u16;
            chain.count[followers] = 1;
            if last == NONE {
                chain.head[run] = followers as u16;
            } else {
                chain.next[usize::from(last)] = followers as u16;
            }
            followers += 1;
        }
        chain
    }

    /// What follows a run, each as an index among the followers.
    fn followers(&self, run: usize) -> impl Iterator<Item = usize> + '_ {
        let mut at = self.head[run];
        std::iter::from_fn(move || {
            (at != NONE).then(|| {
                let index = usize::from(at);
                at = self.next[index];
                index
            })
        })
    }

    /// One of what followed a run, from a draw from 0 to 1: at temperature 0 the likeliest, a tie
    /// broken by the draw, and above it each weighted by how often it followed, raised to a
    /// power that's higher the colder it is, so 50 is each as often as it followed and 100 all
    /// alike.
    fn choose(&self, run: usize, temperature: u8, draw: f64) -> usize {
        let first = self.head[run] as usize;
        if temperature == 0 {
            let most = self.followers(run).map(|at| self.count[at]).max();
            let likeliest = || {
                self.followers(run)
                    .filter(|at| Some(self.count[*at]) == most)
            };
            let ties = likeliest().count();
            let nth = ((draw * ties as f64) as usize).min(ties.saturating_sub(1));
            return likeliest().nth(nth).unwrap_or(first);
        }
        let temperature = f64::from(temperature.min(100));
        let power = if temperature <= 50.0 {
            50.0 / temperature
        } else {
            (100.0 - temperature) / 50.0
        };
        let weight = |at: usize| f64::from(self.count[at]).powf(power);
        let total: f64 = self.followers(run).map(weight).sum();
        let mut left = draw * total;
        let mut chosen = first;
        for at in self.followers(run) {
            chosen = at;
            left -= weight(at);
            if left < 0.0 {
                break;
            }
        }
        chosen
    }
}

/// A draw from 0 to 1 for a row.
fn unit(seed: u64, offset: usize) -> f64 {
    (mix(seed ^ DRAW, offset) >> 11) as f64 / (1u64 << 53) as f64
}

/// The cells of a `Variation`: every row of the span, each a note or a rest with its velocity and
/// gate, copied from the row learned from that the chain chose. It starts as the rows round from
/// the last ones would go on, so the line comes in where the loop does, and a span with fewer
/// than [`MIN_NOTES`] notes to learn from is left as it is.
pub(super) fn render(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    variation: Variation,
    seed: u64,
    cells: &mut [GenCell],
) {
    let ShapeColumn::Note { scale, .. } = column else {
        return;
    };
    if notes(steps, range) < MIN_NOTES {
        return;
    }
    let order = variation.order.clamp(1, MAX_ORDER);
    let chain = Chain::learn(steps, range, scale, order);
    if chain.len == 0 {
        return;
    }
    // The row whose run the line is in: the first, whose run is the last rows'.
    let mut at = 0;
    for (offset, cell) in cells.iter_mut().enumerate() {
        if steps.get(range.first + offset).is_none() {
            break;
        }
        let run = usize::from(chain.run[at]);
        let chosen = chain.choose(run, variation.temperature, unit(seed, offset));
        let row = usize::from(chain.row[chosen]);
        let from = &steps[range.first + row];
        let note = match from.note {
            Note::On(note) => Note::On(pitch(note, scale)),
            other => other,
        };
        *cell = GenCell::Note {
            note,
            velocity: from.velocity,
            gate: from.gate,
        };
        at = (row + 1) % chain.len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleKind;

    const OFF_SCALE: Scale = Scale {
        on: false,
        root: 0,
        kind: ScaleKind::Major,
    };
    const C_MAJOR: Scale = Scale {
        on: true,
        root: 0,
        kind: ScaleKind::Major,
    };

    fn column(scale: Scale) -> ShapeColumn {
        ShapeColumn::Note {
            scale,
            writes: None,
        }
    }

    fn rows(texts: &[&str]) -> Vec<Step> {
        texts.iter().map(|text| text.parse().unwrap()).collect()
    }

    /// What a variation writes over `steps`, each row's text.
    fn varied(steps: &[Step], scale: Scale, variation: Variation, seed: u64) -> Vec<String> {
        let range = StepRange::between(0, steps.len() - 1);
        let mut cells = vec![GenCell::Under; steps.len()];
        render(steps, range, column(scale), variation, seed, &mut cells);
        steps
            .iter()
            .zip(cells)
            .map(|(step, cell)| {
                let mut step = step.clone();
                cell.apply(column(scale), &mut step);
                step.to_string()
            })
            .collect()
    }

    fn texts(steps: &[Step]) -> Vec<String> {
        steps.iter().map(ToString::to_string).collect()
    }

    /// A line of 16 rows with notes, rests, accents, slides and an `OFF`, in which every run of
    /// three rows, going round, is followed by one row only, but a run of one or two isn't.
    fn line() -> Vec<Step> {
        rows(&[
            "C-2 7F --- --- ---",
            "--- -- --- --- ---",
            "C-2 50 --- --- ---",
            "D#2 50 HLD1 --- --- ---",
            "G-2 50 --- --- ---",
            "--- -- --- --- ---",
            "C-2 7F --- --- ---",
            "C-3 50 --- --- ---",
            "--- -- --- --- ---",
            "A#2 50 HLD1 --- --- ---",
            "C-2 50 --- --- ---",
            "OFF -- --- --- ---",
            "C-2 7F --- --- ---",
            "--- -- --- --- ---",
            "G-2 50 --- --- ---",
            "D#2 7F --- --- ---",
        ])
    }

    fn at(order: u8, temperature: u8) -> Variation {
        Variation { order, temperature }
    }

    #[test]
    fn order_3_at_temperature_0_writes_the_line_back() {
        for seed in [0, 1, 7, 0xDEAD] {
            assert_eq!(varied(&line(), OFF_SCALE, at(3, 0), seed), texts(&line()));
        }
    }

    /// Every row after the first `order` follows the `order` rows before it as some row of the
    /// source does, going round it, and at temperature 0 as the likeliest one does.
    #[test]
    fn every_transition_written_is_one_the_line_has() {
        let source: Vec<String> = texts(&line());
        let length = source.len();
        let seen = |run: &[String]| {
            (0..length).any(|start| (0..run.len()).all(|k| source[(start + k) % length] == run[k]))
        };
        for order in 1..=MAX_ORDER {
            for temperature in [0, 30, 50, 100] {
                for seed in 0..40 {
                    let out = varied(&line(), OFF_SCALE, at(order, temperature), seed);
                    let order = usize::from(order);
                    for end in order..out.len() {
                        assert!(
                            seen(&out[end - order..=end]),
                            "order {order} at {temperature}, seed {seed}: {:?}",
                            &out[end - order..=end]
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_seed_repeats_and_another_strays_elsewhere() {
        let loose = at(1, 50);
        let first = varied(&line(), OFF_SCALE, loose, 3);
        assert_eq!(varied(&line(), OFF_SCALE, loose, 3), first);
        let others = (4..20)
            .filter(|seed| varied(&line(), OFF_SCALE, loose, *seed) != first)
            .count();
        assert!(others > 10, "{others} of 16 seeds wrote another line");
        assert_ne!(first, texts(&line()), "order 1 strays from the line");
    }

    #[test]
    fn at_temperature_0_only_a_tie_is_left_to_the_seed() {
        // After C-2 comes D-2 twice and E-2 once, so order 1 at 0 never writes E-2.
        let source = rows(&[
            "C-2 50 --- --- ---",
            "D-2 50 --- --- ---",
            "C-2 50 --- --- ---",
            "D-2 50 --- --- ---",
            "C-2 50 --- --- ---",
            "E-2 50 --- --- ---",
        ]);
        for seed in 0..30 {
            let out = varied(&source, OFF_SCALE, at(1, 0), seed);
            assert!(out.iter().all(|row| !row.starts_with("E-2")), "{out:?}");
        }
        // Hotter, it now and then does.
        let hot = (0..30)
            .flat_map(|seed| varied(&source, OFF_SCALE, at(1, 100), seed))
            .filter(|row| row.starts_with("E-2"))
            .count();
        assert!(hot > 0);
    }

    #[test]
    fn rests_and_rhythm_are_learned_with_the_notes() {
        // A note on every fourth row: the line keeps the rhythm whatever the seed.
        let mut source = Vec::new();
        for note in ["C-3", "E-3", "G-3", "E-3", "C-3", "G-3"] {
            source.extend(rows(&[&format!("{note} 50 --- --- ---")]));
            source.extend(rows(&["--- -- --- --- ---"; 3]));
        }
        // Order 3 knows how many rests have gone by; order 1 wouldn't.
        for seed in 0..20 {
            let out = varied(&source, OFF_SCALE, at(3, 100), seed);
            for (row, text) in out.iter().enumerate() {
                assert_eq!(text.starts_with("---"), row % 4 != 0, "{row}: {out:?}");
            }
        }
    }

    #[test]
    fn with_the_scale_on_a_note_outside_it_is_its_degree() {
        let mut source = line();
        source[7] = "C#3 50 --- --- ---".parse().unwrap();
        let out = varied(&source, C_MAJOR, at(3, 0), 1);
        assert!(out[7].starts_with("C-3"), "{}", out[7]);
        // D# and A# are fitted to D and A in C major.
        assert!(out[3].starts_with("D-2"), "{}", out[3]);
    }

    #[test]
    fn too_few_notes_leave_the_rows_as_they_are() {
        let source = rows(&[
            "C-3 50 --- --- ---",
            "--- -- --- --- ---",
            "E-3 50 --- --- ---",
            "--- -- --- --- ---",
            "G-3 50 --- --- ---",
        ]);
        let range = StepRange::between(0, source.len() - 1);
        assert_eq!(notes(&source, range), 3);
        let mut cells = vec![GenCell::Under; source.len()];
        render(
            &source,
            range,
            column(OFF_SCALE),
            Variation::default(),
            1,
            &mut cells,
        );
        assert!(cells.iter().all(|cell| *cell == GenCell::Under));
    }

    #[test]
    fn a_longer_span_than_it_learns_from_goes_on_round() {
        // 300 rows, the first 256 learned from: order 3 at 0 plays that loop, then again.
        let base = line();
        let source: Vec<Step> = (0..300).map(|row| base[row % 16].clone()).collect();
        let out = varied(&source, OFF_SCALE, at(3, 0), 9);
        assert_eq!(out, texts(&source));
    }
}
