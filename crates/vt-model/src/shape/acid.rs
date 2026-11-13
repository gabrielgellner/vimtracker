//! `Acid`: a 303 line on the Note column, its notes, accents and slides written together. On an
//! empty column it writes a riff of its own, its rhythm weighted toward the strong beats; on notes
//! already there it keeps their rhythm and gives them the riff's pitches, or with `keep` their own,
//! and new accents, slides and octaves.
//!
//! The rhythm's weights, the root counted twice in the riff's set, and the starting accent chance
//! are adapted from David Whiting's Endless Acid Banger (`src/pattern.ts`,
//! <https://github.com/vitling/acid-banger>, CC BY 4.0). Its `ThreeOhGen` plays a step of a bar
//! of 16 with a chance of 0.6 on each beat, 0.5 on every third step, 0.3 on the other even steps
//! and 0.1 on the rest, and gives a note an accent 30% of the time and a slide 10%.

use serde::{Deserialize, Serialize};

use super::render::{GenCell, Out, Shaped, chosen};
use super::{Ladder, Shape, ShapeColumn, mix};
use crate::constants::{DEFAULT_NOTE_VELOCITY, TICKS_PER_QUARTER, TICKS_PER_STEP};
use crate::selection::StepRange;
use crate::step::{ACCENT_VELOCITY, GateKind, MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Note, Step, slid};

/// The most notes a riff's set holds.
pub const MAX_ACID_NOTES: u8 = 8;
/// The rows in a bar, which the rhythm's weights repeat over.
const BAR: usize = (4 * TICKS_PER_QUARTER / TICKS_PER_STEP) as usize;
/// The mean of the weights over a bar: the share of rows Vitling's plays.
const MEAN_WEIGHT: f64 = 5.8 / 16.0;

/// What an `Acid` fill writes besides its pitches, each chance in percent. Saved with a live
/// generator's shape, and left out while it's as it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Acid {
    /// How many notes the riff's set holds, its root among them.
    pub notes: u8,
    /// The chance a note goes an octave up or down.
    pub octave: u8,
    /// The share of rows that play on an empty column, on average, the strong beats more often.
    pub density: u8,
    pub accent: u8,
    pub slide: u8,
    /// Keeps the pitches, so a roll changes only the accents, slides and octaves: on notes already
    /// there, their own pitches, and on an empty column, the riff's.
    pub keep: bool,
    /// Picks the accents, slides and octaves with the shape's seed, so a roll with `keep` changes
    /// this alone, and the pitches and rhythm stay.
    pub roll: u64,
}

impl Default for Acid {
    fn default() -> Self {
        Self {
            notes: 4,
            octave: 20,
            density: 50,
            accent: 30,
            slide: 20,
            keep: false,
            roll: 0,
        }
    }
}

impl Acid {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// What each draw is for, so one draw doesn't follow another.
const PLAYS: u64 = 0xAC1D_0001;
const PICK: u64 = 0xAC1D_0002;
const SET: u64 = 0xAC1D_0003;
const FLAGS: u64 = 0xAC1D_0004;
const OCTAVE: u64 = 0xAC1D_0005;
const UP: u64 = 0xAC1D_0006;
const ACCENT: u64 = 0xAC1D_0007;
const SLIDE: u64 = 0xAC1D_0008;

/// How strongly a row leans toward playing, by where it falls in the bar counting from the first
/// row selected: Vitling's 0.6 on each beat, 0.5 on every third row, 0.3 on the other even rows
/// and 0.1 on the rest.
pub fn weight(offset: usize) -> f64 {
    let at = offset % BAR;
    if at.is_multiple_of(4) {
        0.6
    } else if at.is_multiple_of(3) {
        0.5
    } else if at.is_multiple_of(2) {
        0.3
    } else {
        0.1
    }
}

/// The chance a row plays at a density from 0 to 1. Up to the density Vitling's plays at, each
/// weight is scaled to it, and above, each is moved the same share of the way to certain, so on
/// average that share of a bar plays, and a stronger beat never plays less often than a weaker.
pub fn chance(offset: usize, density: f64) -> f64 {
    let weight = weight(offset);
    if density <= MEAN_WEIGHT {
        weight * density / MEAN_WEIGHT
    } else {
        weight + (1.0 - weight) * (density - MEAN_WEIGHT) / (1.0 - MEAN_WEIGHT)
    }
}

/// A draw from 0 to 1 for a row.
fn unit(seed: u64, offset: usize) -> f64 {
    (mix(seed, offset) >> 11) as f64 / (1u64 << 53) as f64
}

/// Whether a row's draw falls within a percent chance.
fn within(seed: u64, offset: usize, percent: u8) -> bool {
    mix(seed, offset) % 100 < u64::from(percent)
}

/// A riff's set of notes, held in place: the root twice, unless it's alone, and up to
/// `MAX_ACID_NOTES - 1` others.
pub(super) struct Riff {
    notes: [u8; MAX_ACID_NOTES as usize + 1],
    len: usize,
}

impl Riff {
    pub(super) fn notes(&self) -> &[u8] {
        &self.notes[..self.len]
    }

    fn push(&mut self, note: u8) {
        self.notes[self.len] = note;
        self.len += 1;
    }
}

/// The riff's notes: `from`, its root, then up to `count - 1` others drawn between the two ends,
/// along the ladder, so in key while the scale is on. The root is listed twice, so it's picked
/// more often than the others, as most of Vitling's sets hold theirs two to four times.
pub(super) fn riff(ladder: &Ladder, from: u8, to: u8, count: u8, seed: u64) -> Riff {
    let root = ladder.step(from);
    let other = ladder.step(to);
    let (low, high) = (root.min(other), root.max(other));
    // The ladder's steps other than the root, at most one for each note.
    let mut others = [0u8; MAX_NOTE as usize + 1];
    let mut len = 0;
    for step in (low..=high).filter(|step| *step != root) {
        others[len] = step;
        len += 1;
    }
    // Fisher and Yates, as far as it needs, from the seed.
    let wanted = usize::from(count.clamp(1, MAX_ACID_NOTES) - 1).min(len);
    for index in 0..wanted {
        let left = len - index;
        let pick = index + (mix(seed ^ SET, index) % left as u64) as usize;
        others.swap(index, pick);
    }
    let others = &mut others[..wanted];
    others.sort_unstable();
    let note = |step: u8| ladder.note(f64::from(step));
    let mut set = Riff {
        notes: [0; MAX_ACID_NOTES as usize + 1],
        len: 0,
    };
    set.push(note(root));
    if wanted > 0 {
        set.push(note(root));
    }
    for step in others {
        set.push(note(*step));
    }
    set
}

/// A pitch an octave up or down, the other way if one would leave the notes a step holds.
fn octaved(pitch: u8, up: bool) -> u8 {
    let (higher, lower) = (pitch.checked_add(12), pitch.checked_sub(12));
    let higher = higher.filter(|note| *note <= MAX_NOTE);
    let lower = lower.filter(|note| *note >= MIN_NOTE);
    let moved = if up {
        higher.or(lower)
    } else {
        lower.or(higher)
    };
    moved.unwrap_or(pitch)
}

/// A velocity that isn't an accent, below `70`: the one given, unless it's an accent itself, and
/// then the one a new song writes with, as `'` takes an accent off.
fn plain(velocity: u8) -> u8 {
    if velocity < ACCENT_VELOCITY {
        velocity
    } else {
        DEFAULT_NOTE_VELOCITY
    }
}

/// The cells of the rows an `Acid` fill changes: each a note with its velocity and gate. On a
/// column with notes it plays on every row `Where` chooses; on an empty one, on those the density
/// draws.
pub(super) fn render<F: FnMut(usize, &Shaped)>(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    shape: &Shape,
    out: &mut Out<'_, F>,
) {
    let ShapeColumn::Note { scale, writes } = column else {
        return;
    };
    let acid = shape.acid;
    let ladder = Ladder::new(scale);
    let set = riff(&ladder, shape.from, shape.to, acid.notes, shape.seed);
    let set = set.notes();
    let density = f64::from(acid.density.min(100)) / 100.0;
    // The second die: the accents, slides and octaves, which a roll with `keep` changes alone.
    let flags = mix(shape.seed ^ FLAGS ^ acid.roll.rotate_left(29), 0);
    let rows = chosen(
        steps,
        range,
        column,
        shape.place,
        shape.seed,
        out.cells.len(),
    );
    for offset in rows.ones() {
        let row = range.first + offset;
        let Some(step) = steps.get(row) else {
            continue;
        };
        if writes.is_some() && unit(shape.seed ^ PLAYS, offset) >= chance(offset, density) {
            continue;
        }
        let picked = set[(mix(shape.seed ^ PICK, offset) % set.len() as u64) as usize];
        let mut pitch = match step.note {
            Note::On(note) if acid.keep => note,
            _ => picked,
        };
        if within(flags ^ OCTAVE, offset, acid.octave) {
            pitch = octaved(pitch, mix(flags ^ UP, offset) & 1 == 0);
        }
        let accent = within(flags ^ ACCENT, offset, acid.accent);
        let accented = step.velocity.is_none_or(|v| v >= ACCENT_VELOCITY);
        let velocity = match (writes, accent) {
            (_, true) if accented && writes.is_none() => step.velocity,
            (_, true) => Some(MAX_VELOCITY),
            (Some(velocity), false) => Some(plain(velocity)),
            (None, false) if accented => Some(DEFAULT_NOTE_VELOCITY),
            (None, false) => step.velocity,
        };
        let gate = if within(flags ^ SLIDE, offset, acid.slide) {
            slid(step.gate, true)
        } else if step.gate_kind() == GateKind::Hold {
            slid(step.gate, false)
        } else {
            step.gate
        };
        out.cells[offset] = GenCell::Note {
            note: Note::On(pitch),
            velocity,
            gate,
        };
        (out.seen)(row, &Shaped::note(pitch));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lfo::Shape as Wave;
    use crate::scale::{Scale, ScaleKind};
    use crate::shape::{Bits, DEFAULT_RATE, FillKind, Pink, Values, Walk, Where};

    const C_MINOR: Scale = Scale {
        on: true,
        root: 0,
        kind: ScaleKind::Minor,
    };
    const OFF: Scale = Scale {
        on: false,
        root: 0,
        kind: ScaleKind::Major,
    };

    fn writing(scale: Scale) -> ShapeColumn {
        ShapeColumn::Note {
            scale,
            writes: Some(0x50),
        }
    }

    fn acid(seed: u64, acid: Acid) -> Shape {
        Shape {
            from: 36,
            to: 48,
            curve: 0,
            s: 0,
            kind: FillKind::Acid,
            wave: Wave::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::default(),
            place: Where::AllRows,
            seed,
            acid,
            follow: crate::shape::Follow::default(),
            walk: Walk::default(),
            pink: Pink::default(),
            bits: Bits::default(),
            grids: crate::shape::Grids::default(),
        }
    }

    fn empty(rows: usize) -> Vec<Step> {
        vec![Step::default(); rows]
    }

    fn all(steps: &[Step]) -> StepRange {
        StepRange::between(0, steps.len() - 1)
    }

    fn run(steps: &[Step], column: ShapeColumn, shape: &Shape) -> Vec<(usize, Step)> {
        crate::shape::written(steps, all(steps), column, shape)
    }

    fn pitch(step: &Step) -> u8 {
        match step.note {
            Note::On(note) => note,
            note => panic!("{note:?}"),
        }
    }

    #[test]
    fn the_chance_keeps_the_density_on_average_and_the_beats_strongest() {
        for percent in [0, 10, 36, 50, 80, 100] {
            let density = f64::from(percent) / 100.0;
            let mean: f64 = (0..BAR).map(|at| chance(at, density)).sum::<f64>() / BAR as f64;
            assert!((mean - density).abs() < 1e-9, "{percent}: {mean}");
            for at in 0..BAR {
                for other in 0..BAR {
                    if weight(at) > weight(other) {
                        assert!(chance(at, density) >= chance(other, density));
                    }
                }
            }
        }
        // At the density Vitling's plays at, its own chances.
        assert!((chance(0, MEAN_WEIGHT) - 0.6).abs() < 1e-9);
        assert!((chance(3, MEAN_WEIGHT) - 0.5).abs() < 1e-9);
        assert!((chance(2, MEAN_WEIGHT) - 0.3).abs() < 1e-9);
        assert!((chance(1, MEAN_WEIGHT) - 0.1).abs() < 1e-9);
        assert_eq!(chance(5, 1.0), 1.0);
        assert_eq!(chance(0, 0.0), 0.0);
    }

    #[test]
    fn strong_beats_get_more_notes_across_many_seeds() {
        let steps = empty(16);
        let mut plays = [0usize; 16];
        let seeds = 4000;
        for seed in 0..seeds {
            for (row, _) in run(&steps, writing(OFF), &acid(seed, Acid::default())) {
                plays[row] += 1;
            }
        }
        let share = |rows: &[usize]| {
            rows.iter().map(|row| plays[*row]).sum::<usize>() as f64
                / (rows.len() * seeds as usize) as f64
        };
        let (beats, thirds, evens, rest) = (
            share(&[0, 4, 8, 12]),
            share(&[3, 6, 9, 15]),
            share(&[2, 10, 14]),
            share(&[1, 5, 7, 11, 13]),
        );
        assert!(
            beats > thirds && thirds > evens && evens > rest,
            "{plays:?}"
        );
        // At 50%, each moves the same share of the way from Vitling's toward certain.
        let expected =
            |weight: f64| weight + (1.0 - weight) * (0.5 - MEAN_WEIGHT) / (1.0 - MEAN_WEIGHT);
        for (got, weight) in [(beats, 0.6), (thirds, 0.5), (evens, 0.3), (rest, 0.1)] {
            assert!((got - expected(weight)).abs() < 0.03, "{got} {weight}");
        }
        let total: usize = plays.iter().sum();
        let mean = total as f64 / (16 * seeds as usize) as f64;
        assert!((mean - 0.5).abs() < 0.02, "{mean}");
    }

    #[test]
    fn density_runs_from_nothing_to_every_row() {
        let steps = empty(32);
        let at = |density| {
            let shape = acid(
                3,
                Acid {
                    density,
                    ..Acid::default()
                },
            );
            run(&steps, writing(OFF), &shape).len()
        };
        assert_eq!(at(0), 0);
        assert_eq!(at(100), 32);
    }

    /// The notes of many rows, all playing, with no octaves.
    fn many(scale: Scale, from: u8, to: u8, notes: u8, seed: u64) -> Vec<u8> {
        let steps = empty(400);
        let settings = Acid {
            notes,
            octave: 0,
            density: 100,
            ..Acid::default()
        };
        let shape = Shape {
            from,
            to,
            ..acid(seed, settings)
        };
        run(&steps, writing(scale), &shape)
            .iter()
            .map(|(_, step)| pitch(step))
            .collect()
    }

    #[test]
    fn the_riff_has_as_many_notes_as_asked_the_root_most_of_all() {
        for notes in 1..=MAX_ACID_NOTES {
            for seed in 0..20 {
                let picked = many(OFF, 36, 48, notes, seed);
                let mut set = picked.clone();
                set.sort_unstable();
                set.dedup();
                assert_eq!(set.len(), usize::from(notes), "{notes} {seed}");
                assert!(set.contains(&36), "the root");
                assert!(set.iter().all(|note| (36..=48).contains(note)));
                let count = |note: u8| picked.iter().filter(|n| **n == note).count();
                if notes >= 3 {
                    assert!(set.iter().all(|note| count(36) >= count(*note)));
                }
            }
        }
        // There aren't more notes between the ends than they hold.
        let mut set = many(OFF, 36, 38, 8, 1);
        set.sort_unstable();
        set.dedup();
        assert_eq!(set, [36, 37, 38]);
        // From is the root either way round.
        assert!(many(OFF, 48, 36, 1, 1).iter().all(|note| *note == 48));
    }

    #[test]
    fn with_the_scale_on_every_note_is_in_key_and_off_any_can_come() {
        let mut outside = false;
        for seed in 0..30 {
            let in_key = many(C_MINOR, 36, 60, 8, seed);
            assert!(in_key.iter().all(|note| C_MINOR.contains(*note)));
            outside |= many(OFF, 36, 60, 8, seed)
                .iter()
                .any(|note| !C_MINOR.contains(*note));
        }
        assert!(outside, "with the scale off, the riff is chromatic");
        // A root outside the scale counts from its nearest degree, as a ramp's end does.
        assert!(many(C_MINOR, 37, 48, 1, 1).iter().all(|note| *note == 36));
    }

    #[test]
    fn the_octave_chance_moves_a_note_an_octave_either_way() {
        let steps = empty(1000);
        let notes = |octave| {
            let settings = Acid {
                notes: 1,
                octave,
                density: 100,
                ..Acid::default()
            };
            let shape = Shape {
                from: 48,
                ..acid(5, settings)
            };
            run(&steps, writing(OFF), &shape)
                .iter()
                .map(|(_, step)| pitch(step))
                .collect::<Vec<u8>>()
        };
        assert!(notes(0).iter().all(|note| *note == 48));
        let moved = notes(100);
        assert!(moved.iter().all(|note| *note == 36 || *note == 60));
        assert!(moved.contains(&36) && moved.contains(&60));
        let half = notes(50).iter().filter(|note| **note != 48).count();
        assert!((430..570).contains(&half), "{half}");
        // At the edge of the notes a step holds, it goes the other way.
        assert_eq!(octaved(MAX_NOTE - 3, true), MAX_NOTE - 15);
        assert_eq!(octaved(MIN_NOTE + 3, false), MIN_NOTE + 15);
    }

    #[test]
    fn accents_and_slides_come_at_their_chances_as_the_keys_write_them() {
        let steps = empty(2000);
        let shape = acid(
            9,
            Acid {
                density: 100,
                ..Acid::default()
            },
        );
        let out = run(&steps, writing(OFF), &shape);
        let share = |test: &dyn Fn(&Step) -> bool| {
            out.iter().filter(|(_, step)| test(step)).count() as f64 / out.len() as f64
        };
        let accented = share(&|step| step.velocity == Some(MAX_VELOCITY));
        let plain = share(&|step| step.velocity == Some(0x50));
        let slid = share(&|step| step.gate_kind() == GateKind::Hold);
        assert!((accented - 0.3).abs() < 0.03, "{accented}");
        assert!((accented + plain - 1.0).abs() < 1e-9);
        assert!((slid - 0.2).abs() < 0.03, "{slid}");
        // A slide is `HLD`, as `/` writes it, and a note without one has the track's gate.
        assert!(out.iter().any(|(_, step)| step.to_string().contains("HLD")));
        assert!(
            out.iter()
                .all(|(_, step)| step.gate.is_none() || step.gate_kind() == GateKind::Hold)
        );
        // None and every one.
        let at = |accent, slide| {
            let shape = Shape {
                acid: Acid {
                    accent,
                    slide,
                    ..shape.acid
                },
                ..shape
            };
            run(&steps[..50], writing(OFF), &shape)
        };
        assert!(
            at(0, 0)
                .iter()
                .all(|(_, step)| step.velocity == Some(0x50) && step.gate.is_none())
        );
        assert!(at(100, 100).iter().all(|(_, step)| {
            step.velocity == Some(MAX_VELOCITY) && step.gate_kind() == GateKind::Hold
        }));
        // Notes typed at an accent's velocity leave a plain note at a new song's.
        let loud = ShapeColumn::Note {
            scale: OFF,
            writes: Some(0x74),
        };
        let unaccented = Shape {
            acid: Acid {
                accent: 0,
                ..shape.acid
            },
            ..shape
        };
        assert!(
            run(&steps[..50], loud, &unaccented)
                .iter()
                .all(|(_, step)| step.velocity == Some(DEFAULT_NOTE_VELOCITY))
        );
    }

    fn line(texts: &[&str]) -> Vec<Step> {
        texts.iter().map(|text| text.parse().unwrap()).collect()
    }

    const ON_NOTES: ShapeColumn = ShapeColumn::Note {
        scale: OFF,
        writes: None,
    };

    #[test]
    fn keep_pitches_leaves_every_pitch_and_rerolls_the_rest() {
        let steps = line(&[
            "C-2 40 --- --- ---",
            "--- -- --- --- ---",
            "D#2 7F HLD1 --- ---",
            "OFF -- --- --- ---",
            "G-2 -- MLT2 --- ---",
            "A#2 50 --- --- ---",
            "--- -- --- --- ---",
            "C-3 40 --- --- ---",
        ]);
        let kept = |roll| Shape {
            place: Where::Notes,
            ..acid(
                1,
                Acid {
                    keep: true,
                    octave: 0,
                    accent: 50,
                    slide: 50,
                    roll,
                    ..Acid::default()
                },
            )
        };
        let rows: Vec<usize> = run(&steps, ON_NOTES, &kept(0))
            .iter()
            .map(|(row, _)| *row)
            .collect();
        assert_eq!(rows, [0, 2, 4, 5, 7], "the rhythm stays");
        let mut seen = std::collections::HashSet::new();
        for roll in 0..40 {
            let out = run(&steps, ON_NOTES, &kept(roll));
            for (row, step) in &out {
                assert_eq!(step.note, steps[*row].note, "the pitch stays");
            }
            seen.insert(
                out.iter()
                    .map(|(_, step)| (step.velocity, step.gate_kind() == GateKind::Hold))
                    .collect::<Vec<_>>(),
            );
        }
        assert!(seen.len() > 10, "each roll changes the accents and slides");
        // Taking an accent off leaves a new song's velocity, a slide comes off as `SGL`, and a
        // gate of another kind stays as it is without one.
        let all_off = Shape {
            acid: Acid {
                accent: 0,
                slide: 0,
                ..kept(0).acid
            },
            ..kept(0)
        };
        let texts: Vec<String> = run(&steps, ON_NOTES, &all_off)
            .iter()
            .map(|(_, step)| step.to_string()[..10].to_owned())
            .collect();
        assert_eq!(
            texts,
            [
                "C-2 40 ---",
                "D#2 64 ---",
                "G-2 64 MLT",
                "A#2 50 ---",
                "C-3 40 ---"
            ]
        );
        // An accent there stays as it was.
        let all_on = Shape {
            acid: Acid {
                accent: 100,
                ..all_off.acid
            },
            ..all_off
        };
        let out = run(&steps, ON_NOTES, &all_on);
        assert_eq!(out[2].1.velocity, None, "an empty velocity plays at full");
        assert_eq!(out[0].1.velocity, Some(MAX_VELOCITY));
        // Its octaves are rolled too.
        let octaves = Shape {
            acid: Acid {
                octave: 100,
                ..kept(0).acid
            },
            ..kept(0)
        };
        for (row, step) in run(&steps, ON_NOTES, &octaves) {
            assert_eq!(pitch(&step).abs_diff(pitch(&steps[row])), 12);
        }
    }

    #[test]
    fn without_keep_notes_there_take_the_riffs_pitches_in_their_rhythm() {
        let steps = line(&[
            "C-5 40 --- --- ---",
            "--- -- --- --- ---",
            "D#5 40 --- --- ---",
            "C-5 40 --- --- ---",
        ]);
        let shape = Shape {
            place: Where::Notes,
            ..acid(
                1,
                Acid {
                    octave: 0,
                    ..Acid::default()
                },
            )
        };
        let out = run(&steps, ON_NOTES, &shape);
        let rows: Vec<usize> = out.iter().map(|(row, _)| *row).collect();
        assert_eq!(rows, [0, 2, 3]);
        assert!(out.iter().all(|(_, step)| (36..=48).contains(&pitch(step))));
    }

    #[test]
    fn with_keep_a_roll_keeps_a_generated_riffs_rhythm_and_pitches() {
        let steps = empty(16);
        let shape = |roll| {
            acid(
                4,
                Acid {
                    keep: true,
                    octave: 0,
                    roll,
                    ..Acid::default()
                },
            )
        };
        let notes = |roll| -> Vec<(usize, Note)> {
            run(&steps, writing(OFF), &shape(roll))
                .into_iter()
                .map(|(row, step)| (row, step.note))
                .collect()
        };
        let flags = |roll| -> Vec<(Option<u8>, GateKind)> {
            run(&steps, writing(OFF), &shape(roll))
                .iter()
                .map(|(_, step)| (step.velocity, step.gate_kind()))
                .collect()
        };
        assert!(!notes(0).is_empty());
        assert!((1..20).all(|roll| notes(roll) == notes(0)));
        assert!((1..20).any(|roll| flags(roll) != flags(0)));
        // A new seed is a new riff.
        let other = Shape {
            seed: 5,
            ..shape(0)
        };
        assert_ne!(
            run(&steps, writing(OFF), &other),
            run(&steps, writing(OFF), &shape(0))
        );
    }

    #[test]
    fn the_same_seed_writes_the_same_riff() {
        let steps = empty(16);
        let shape = acid(11, Acid::default());
        assert_eq!(
            run(&steps, writing(C_MINOR), &shape),
            run(&steps, writing(C_MINOR), &shape)
        );
    }
}
