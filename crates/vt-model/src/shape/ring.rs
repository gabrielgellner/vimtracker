//! `Ring`: a noise ring after Grant Richter's Wiard 1210 Noise Ring, in Eurorack as Malekko's
//! Richter Noise Ring, of the Buchla 266's line. One shift register drives the rhythm and the
//! pitch together: clocked once a row, it takes a bit in, and a row plays if that newest bit, bit
//! 0, is a 1, at a pitch from the register's lowest eight bits read as a byte, through the scale,
//! so the melody and the rhythm move together.
//!
//! The bit taken in is the one going out, `Length` rows ago, round the loop again, unless
//! `Change`'s die replaces it with a new one, a 1 by `Chance`'s: at `0%` the loop locks, at `100%`
//! every bit is new, and `Chance` is the density either way, as a new register starts with bits
//! drawn by it too. `Length` is the Turing Machine's: how many bits go round, 8 as the module's.
//!
//! Live, the register runs on from pass to pass, so each pass plays where the last one left it.
//! Each pass is worked out from the seed and its number alone, so playing from anywhere hears what
//! playing to there would: a bit is the one that came round `Length`, `2 × Length` rows before, as
//! far back as the first that `Change`'s die replaced, so each of the 16 the register holds as a
//! pass starts takes a look back along its own place in the loop, a round at a time, about `100 /
//! Change` rounds of `Length` rows however long the song has played, rather than every row since
//! the top.

use serde::{Deserialize, Serialize};

use super::{GenCell, Ladder, ShapeColumn, mix};
use crate::constants::DEFAULT_NOTE_VELOCITY;
use crate::selection::StepRange;
use crate::step::{Note, Step};

/// The longest loop, and how many bits the register holds.
pub const MAX_RING: u8 = 16;
/// The dice of the bits taken in, so they don't follow another fill's.
const DICE: u64 = 0x2B1A_0598;
/// The dice of the register a ring starts with.
const START: u64 = 0x5EED_0598;
/// How many rounds of its loop a bit is looked back along before it's taken as the bit it started
/// with. A bit `Change` at its least, `1%`, leaves alone that long comes once in about 10^18.
const LOOK_BACK: i64 = 4096;

/// A `Ring` fill's settings, saved with a live generator's shape and left out while they're as
/// they start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ring {
    /// How likely a bit coming round is replaced by a new one, in percent: `0` locks the loop.
    pub change: u8,
    /// How likely a new bit is a 1, in percent: the density.
    pub chance: u8,
    /// How many bits go round the loop, from 1 to [`MAX_RING`].
    pub length: u8,
    /// The register as `Space g a` kept it, if it did: the ring starts there rather than from its
    /// seed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept: Option<Kept>,
    /// The pass it plays, counted from the first, which [`Shape::on_pass`](super::Shape::on_pass)
    /// sets as the engine plays it. The written ring plays the first, and this is never saved.
    #[serde(skip)]
    pub pass: u32,
}

impl Default for Ring {
    /// The module's eight bits, a coin's chance of a 1, and a little change, so it drifts.
    fn default() -> Self {
        Self {
            change: 10,
            chance: 50,
            length: 8,
            kept: None,
            pass: 0,
        }
    }
}

/// A register kept, and the pass it was kept on, counted from the ring's first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
    /// The bits as the pass started, the newest in bit 0.
    pub bits: u16,
    /// Which pass, so the dice after it are the ones heard then.
    pub pass: u32,
}

impl Ring {
    pub fn is_default(&self) -> bool {
        Self { pass: 0, ..*self } == Self::default()
    }

    /// Its loop's length, kept within the register.
    fn len(&self) -> i64 {
        i64::from(self.length.clamp(1, MAX_RING))
    }

    /// The register before the ring's first row: the one kept, or one from the seed, a loop of
    /// `Length` bits each a 1 by `Chance`, going round.
    fn start(&self, seed: u64) -> u16 {
        if let Some(kept) = self.kept {
            return kept.bits;
        }
        let len = self.len() as usize;
        (0..usize::from(MAX_RING))
            .filter(|bit| below(mix(seed ^ START, bit % len), self.chance))
            .fold(0, |bits, bit| bits | 1 << bit)
    }

    /// The clock the ring's first row is at, counted from its first pass, for a span of `rows`: a
    /// kept ring's dice go on from the pass it was kept on.
    fn origin(&self, rows: usize) -> u64 {
        self.kept
            .map_or(0, |kept| u64::from(kept.pass) * rows as u64)
    }

    /// Whether the row at `clock` takes a new bit, and whether it's a 1.
    fn dice(&self, seed: u64, clock: u64) -> (bool, bool) {
        let die = mix(seed ^ DICE, clock as usize);
        (below(die >> 32, self.change), below(die, self.chance))
    }

    /// The bit taken in on the row `at` rows after the ring's first, at `origin`: a new one if
    /// `Change`'s die gave one there, or else the one taken in a loop before, looked back along a
    /// round at a time; before the first row, the start's. Up to 16 rows before it, as no more
    /// are asked for.
    fn taken(&self, seed: u64, start: u16, origin: u64, at: i64) -> bool {
        let len = self.len();
        let mut at = at;
        if self.change > 0 {
            let mut rounds = 0;
            while at >= 0 && rounds < LOOK_BACK {
                let (new, one) = self.dice(seed, origin + at as u64);
                if new {
                    return one;
                }
                at -= len;
                rounds += 1;
            }
        }
        // Locked, or looked back as far as it goes, it's the start's bit in the same place.
        if at >= 0 {
            at = at.rem_euclid(len) - len;
        }
        start >> (-at - 1) & 1 != 0
    }

    /// The register as its pass starts, over a span of `rows`: the bits taken in on the 16 rows
    /// before, the newest in bit 0.
    pub fn register(&self, seed: u64, rows: usize) -> u16 {
        let start = self.start(seed);
        let origin = self.origin(rows);
        let first = i64::from(self.pass) * rows as i64;
        (0..i64::from(MAX_RING))
            .filter(|back| self.taken(seed, start, origin, first - 1 - back))
            .fold(0, |bits, back| bits | 1 << back)
    }

    /// What `Space g a` keeps of the pass it plays, over a span of `rows`: the register as the pass
    /// started, and which pass that is from the ring's first, so the ring kept plays it first and
    /// goes on from there.
    pub fn kept_on(&self, seed: u64, rows: usize) -> Kept {
        Kept {
            bits: self.register(seed, rows),
            pass: self.kept.map_or(0, |kept| kept.pass) + self.pass,
        }
    }

    /// The register after each row of its pass, over a span of `rows`, in order: a row plays if
    /// bit 0 is a 1, and its pitch is the lowest eight bits.
    pub fn rows(&self, seed: u64, rows: usize) -> impl Iterator<Item = u16> + '_ {
        let origin = self.origin(rows) + u64::from(self.pass) * rows as u64;
        let len = self.len() as u32;
        let mut bits = self.register(seed, rows);
        (0..rows as u64).map(move |row| {
            let (new, one) = self.dice(seed, origin + row);
            let round = bits >> (len - 1) & 1 != 0;
            let bit = if new { one } else { round };
            bits = bits << 1 | u16::from(bit);
            bits
        })
    }
}

/// Whether a die falls below a percent.
fn below(die: u64, percent: u8) -> bool {
    (die & 0xFFFF_FFFF) % 100 < u64::from(percent)
}

/// A `Ring` fill's cells for `range`, one for each row, the first in `cells[0]`. A row whose newest
/// bit is a 1 plays a note at the byte's pitch, from `from` to `to` along the scale's degrees
/// while it's on: over the note there, keeping its velocity and gate, or a new one with the first
/// note's, or the one notes are typed with on a column that had none. A row that doesn't play is
/// cleared, so a render laid over an earlier one leaves nothing of it. An `OFF` stays. It writes
/// notes; on any other column it leaves the rows alone.
pub(super) fn render(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    (from, to): (u8, u8),
    seed: u64,
    ring: Ring,
    cells: &mut [GenCell],
) {
    let ShapeColumn::Note { scale, writes } = column else {
        return;
    };
    let ladder = Ladder::new(scale);
    let (from, to) = (ladder.step(from), ladder.step(to));
    let (low, high) = (u32::from(from.min(to)), u32::from(from.max(to)));
    let rows = steps.iter().skip(range.first).take(cells.len());
    let first = rows.clone().find(|step| matches!(step.note, Note::On(_)));
    let (velocity, gate) = match first {
        Some(step) => (step.velocity, step.gate),
        None => (Some(writes.unwrap_or(DEFAULT_NOTE_VELOCITY)), None),
    };
    let registers = ring.rows(seed, range.len());
    for ((cell, step), bits) in cells.iter_mut().zip(rows).zip(registers) {
        let along = u32::from(bits as u8) * (high - low + 1) / 256;
        let position = if from <= to {
            low + along
        } else {
            high - along
        };
        let pitch = ladder.note(f64::from(position));
        *cell = match (step.note, bits & 1 != 0) {
            (Note::Off, _) => GenCell::Under,
            (Note::On(_), true) => GenCell::Note {
                note: Note::On(pitch),
                velocity: step.velocity,
                gate: step.gate,
            },
            (Note::Empty, true) => GenCell::Note {
                note: Note::On(pitch),
                velocity,
                gate: first.map_or(step.gate, |_| gate),
            },
            (Note::On(_), false) => GenCell::Note {
                note: Note::Empty,
                velocity: None,
                gate: None,
            },
            (Note::Empty, false) => GenCell::Note {
                note: Note::Empty,
                velocity: step.velocity,
                gate: step.gate,
            },
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::{Scale, ScaleKind};
    use crate::shape::{
        Acid, Bits, DEFAULT_RATE, Divisions, FillKind, Follow, Grids, Pink, Shape, Values, Walk,
        Where, written,
    };

    const C_MINOR: Scale = Scale {
        on: true,
        root: 0,
        kind: ScaleKind::Minor,
    };

    fn shape(ring: Ring, seed: u64) -> Shape {
        Shape {
            from: 48,
            to: 72,
            curve: 0,
            s: 0,
            kind: FillKind::Ring,
            wave: crate::lfo::Shape::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::default(),
            place: Where::AllRows,
            seed,
            acid: Acid::default(),
            follow: Follow::default(),
            walk: Walk::default(),
            pink: Pink::default(),
            bits: Bits::default(),
            grids: Grids::default(),
            random: crate::shape::Random::default(),
            vary: 0,
            variation: crate::shape::Variation::default(),
            divisions: Divisions::default(),
            ring,
        }
    }

    fn writing(scale: Scale) -> ShapeColumn {
        ShapeColumn::Note {
            scale,
            writes: Some(0x50),
        }
    }

    fn ring(change: u8, chance: u8, length: u8) -> Ring {
        Ring {
            change,
            chance,
            length,
            ..Ring::default()
        }
    }

    /// The ring on a pass.
    fn on(ring: Ring, pass: u32) -> Ring {
        Ring { pass, ..ring }
    }

    /// The rows a pass plays, by their offsets.
    fn plays(ring: Ring, seed: u64, rows: usize) -> Vec<usize> {
        ring.rows(seed, rows)
            .enumerate()
            .filter(|(_, bits)| bits & 1 != 0)
            .map(|(row, _)| row)
            .collect()
    }

    /// What a pass writes on an empty column of `rows`, row by row.
    fn notes(ring: Ring, seed: u64, rows: usize) -> Vec<Note> {
        let steps = vec![Step::default(); rows];
        let range = StepRange::between(0, rows - 1);
        let mut out = vec![Note::Empty; rows];
        for (row, step) in written(&steps, range, writing(C_MINOR), &shape(ring, seed)) {
            out[row] = step.note;
        }
        out
    }

    /// The register after every row of `passes` passes of `rows`, worked out by playing them
    /// through a row at a time from the start, as the module is clocked.
    fn played_through(ring: Ring, seed: u64, rows: usize, passes: usize) -> Vec<u16> {
        let len = ring.len() as usize;
        let start = ring.start(seed);
        let origin = ring.origin(rows);
        // The bits taken in, oldest first, the start's before the first row.
        let mut taken: Vec<bool> = (0..usize::from(MAX_RING))
            .rev()
            .map(|back| start >> back & 1 != 0)
            .collect();
        let mut registers = Vec::new();
        for clock in 0..(rows * passes) as u64 {
            let (new, one) = ring.dice(seed, origin + clock);
            let bit = if new { one } else { taken[taken.len() - len] };
            taken.push(bit);
            let newest = taken.len() - 1;
            let register = (0..usize::from(MAX_RING))
                .filter(|back| taken[newest - back])
                .fold(0u16, |bits, back| bits | 1 << back);
            registers.push(register);
        }
        registers
    }

    #[test]
    fn at_change_0_the_loop_repeats_exactly() {
        for length in [1, 3, 5, 8, 16] {
            for seed in 0..40 {
                let locked = ring(0, 50, length);
                let rows = usize::from(length) * 4;
                let first: Vec<u16> = locked.rows(seed, rows).collect();
                // Round the loop, every `Length` rows, the same bits and so the same byte.
                for (row, bits) in first.iter().enumerate().skip(usize::from(length)) {
                    assert_eq!(bits & 0xFF, first[row - usize::from(length)] & 0xFF);
                }
                // And every pass the same as the first.
                for pass in 1..12 {
                    let again: Vec<u16> = on(locked, pass).rows(seed, rows).collect();
                    assert_eq!(again, first, "length {length}, seed {seed}, pass {pass}");
                    assert_eq!(
                        notes(on(locked, pass), seed, rows),
                        notes(locked, seed, rows)
                    );
                }
            }
        }
    }

    #[test]
    fn chance_sets_the_density_across_seeds() {
        for change in [0, 30, 100] {
            for chance in [0, 10, 25, 50, 75, 100] {
                let set = ring(change, chance, 8);
                let played: usize = (0..2000).map(|seed| plays(set, seed, 16).len()).sum();
                let share = played as f64 / (2000.0 * 16.0);
                let wanted = f64::from(chance) / 100.0;
                assert!(
                    (share - wanted).abs() < 0.03,
                    "change {change}, chance {chance}: {share}"
                );
                if chance == 0 || chance == 100 {
                    assert_eq!(share, wanted, "none or every row");
                }
            }
        }
    }

    #[test]
    fn raising_chance_only_adds_notes() {
        for seed in 0..200 {
            for pass in [0, 1, 5] {
                let fewer = plays(on(ring(40, 30, 8), pass), seed, 16);
                let more = plays(on(ring(40, 60, 8), pass), seed, 16);
                assert!(fewer.iter().all(|row| more.contains(row)), "seed {seed}");
            }
        }
    }

    #[test]
    fn a_seed_repeats_and_another_differs() {
        let set = ring(20, 50, 8);
        assert_eq!(notes(set, 7, 32), notes(set, 7, 32));
        let differ = (0..20).filter(|seed| notes(set, *seed, 32) != notes(set, 7, 32));
        assert_eq!(differ.count(), 19);
    }

    /// Each pass is what playing every row up to it gives, for spans a loop goes into evenly and
    /// ones it doesn't, so playing from anywhere hears what playing to there would.
    #[test]
    fn each_pass_is_what_playing_up_to_it_gives() {
        for (change, length, rows) in [(10, 8, 16), (1, 8, 16), (50, 5, 12), (100, 16, 7)] {
            for seed in 0..10 {
                let set = ring(change, 40, length);
                let through = played_through(set, seed, rows, 40);
                for pass in 0..40 {
                    let heard: Vec<u16> = on(set, pass).rows(seed, rows).collect();
                    let at = pass as usize * rows;
                    assert_eq!(heard, through[at..at + rows], "{change}% pass {pass}");
                }
            }
        }
    }

    #[test]
    fn a_kept_ring_plays_the_pass_kept_first_and_goes_on_from_there() {
        for seed in 0..20 {
            let set = ring(25, 50, 8);
            let kept = Ring {
                kept: Some(on(set, 7).kept_on(seed, 16)),
                ..set
            };
            assert_eq!(kept.kept.unwrap().pass, 7);
            for pass in 0..10 {
                let heard: Vec<u16> = on(set, 7 + pass).rows(seed, 16).collect();
                let again: Vec<u16> = on(kept, pass).rows(seed, 16).collect();
                assert_eq!(again, heard, "seed {seed}, pass {pass}");
            }
            // Kept again from there, it counts on from the first keep.
            assert_eq!(on(kept, 3).kept_on(seed, 16).pass, 10);
            // Change down to 0, it holds the loop the register had.
            let locked = Ring { change: 0, ..kept };
            let bits = kept.kept.unwrap().bits;
            for (row, register) in locked.rows(seed, 16).enumerate() {
                let round = bits >> (7 - row % 8) & 1 != 0;
                assert_eq!(register & 1 != 0, round, "seed {seed}, row {row}");
            }
        }
    }

    #[test]
    fn its_pitches_are_in_the_scale_between_the_ends_and_tilde_turns_them_over() {
        let (low, high) = (48, 72);
        let ladder = Ladder::new(C_MINOR);
        for seed in 0..100 {
            let set = ring(30, 70, 8);
            let up = notes(set, seed, 32);
            for note in &up {
                if let Note::On(pitch) = *note {
                    assert!((low..=high).contains(&pitch) && C_MINOR.contains(pitch));
                }
            }
            // From above To, the byte reads down from it.
            let mut down = shape(set, seed);
            (down.from, down.to) = (high, low);
            let steps = vec![Step::default(); 32];
            let range = StepRange::between(0, 31);
            let (bottom, top) = (ladder.position(low), ladder.position(high));
            for (row, step) in written(&steps, range, writing(C_MINOR), &down) {
                let (Note::On(was), Note::On(now)) = (up[row], step.note) else {
                    assert_eq!(
                        (up[row], step.note),
                        (Note::Empty, Note::Empty),
                        "row {row}"
                    );
                    continue;
                };
                let flipped = top - (ladder.position(was) - bottom);
                assert_eq!(ladder.position(now), flipped);
            }
        }
        // The byte reads the whole range: the lowest and the highest come.
        let all: Vec<Note> = (0..200)
            .flat_map(|seed| notes(ring(100, 50, 8), seed, 64))
            .collect();
        assert!(all.contains(&Note::On(low)));
        assert!(all.contains(&Note::On(high)));
    }

    #[test]
    fn over_notes_there_it_keeps_their_velocity_clears_the_rest_and_leaves_an_off() {
        let note: Step = "C-3 40 --- --- ---".parse().unwrap();
        let off: Step = "OFF -- --- --- ---".parse().unwrap();
        let mut steps = vec![note.clone(); 16];
        steps[5] = off;
        steps[9] = Step::default();
        let range = StepRange::between(0, 15);
        let column = ShapeColumn::Note {
            scale: C_MINOR,
            writes: None,
        };
        for seed in 0..40 {
            let set = ring(30, 50, 8);
            let played = plays(set, seed, 16);
            for (row, step) in written(&steps, range, column, &shape(set, seed)) {
                assert_ne!(row, 5, "the OFF stays");
                if played.contains(&row) {
                    assert!(matches!(step.note, Note::On(_)));
                    assert_eq!(step.velocity, Some(0x40), "row {row}");
                } else {
                    assert_eq!(step.note, Note::Empty, "row {row} rests");
                }
            }
        }
        // On an empty column, a new note at the velocity notes are typed with.
        let empty = vec![Step::default(); 16];
        let every = shape(ring(0, 100, 8), 1);
        for (_, step) in written(&empty, range, writing(C_MINOR), &every) {
            assert_eq!(step.velocity, Some(0x50));
        }
    }

    /// A pass works out its register by looking back along the loop, not by playing every row
    /// since the top: a ring far into a song at its least `Change` is quick.
    #[test]
    fn a_pass_far_into_a_song_looks_back_only_along_its_loop() {
        let set = ring(1, 50, 16);
        let started = std::time::Instant::now();
        for pass in 0..200 {
            let _ = on(set, 1_000_000 + pass).register(3, 64);
        }
        assert!(started.elapsed().as_millis() < 500);
        // A shape plays a pass as the engine sets it, and has no `Vary` of its own.
        let ringing = shape(set, 3);
        assert!(ringing.varies() && !ringing.draws());
        assert_eq!(ringing.on_pass(4).ring.pass, 4);
        assert_eq!(ringing.on_pass(0), ringing);
    }
}
