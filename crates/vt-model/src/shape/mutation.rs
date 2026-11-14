//! A `Follow`'s mutations, after Qu-Bit's Bloom: so many changes, drawn from its seed, to what it
//! copies, each one of the kinds it allows: a note moved within the scale, the whole line moved
//! up or down the scale, a few rows played backwards, a row ratcheted with `RET`, or a note
//! dropped or added. A Bloom branch is a `Follow` of the branch before it that mutates, so each
//! strays a little further from the trunk, and an edit to the trunk grows every branch again
//! (decision 307). It works on the cells the copy left, allocating nothing, so the engine can
//! render it as it plays.

use serde::{Deserialize, Serialize};

use super::random::unit;
use super::{GenCell, Ladder, mix};
use crate::step::{FxCommand, FxSlot, Note, Step};

/// The most changes a branch makes.
pub const MAX_MUTATION: u8 = 32;
/// The furthest `Shift` moves the whole line either way, in degrees with the scale on and
/// semitones off.
const MOST_SHIFT: usize = 4;
/// The shortest and longest span `Retro` plays backwards.
const SHORTEST_SPAN: usize = 2;
const LONGEST_SPAN: usize = 4;
/// What a ratchet writes: `RET` with no fade, repeating every 3 units (two hits a step) or every
/// 2 (three hits).
const RATCHETS: [u16; 2] = [0x03, 0x02];
/// Mixed into a mutation's seed, so its draws aren't those of any other fill drawing from it.
const MUTATION_KEY: u64 = 0xB100_0000;

/// Which kinds of change a mutating `Follow` makes. Saved only once it's changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mutations {
    /// A note moved a degree or two, within the scale while it's on.
    pub repitch: bool,
    /// Every note moved together, one to four degrees up or down.
    pub shift: bool,
    /// Two to four rows played backwards.
    pub retro: bool,
    /// A note repeated within its row, by a `RET` in its first empty FX slot.
    pub ratchet: bool,
    /// A note taken away, or one added on an empty row, from the note before it.
    pub drop: bool,
}

/// Every kind but `shift`, which moves the whole line and so changes far more than a row.
impl Default for Mutations {
    fn default() -> Self {
        Self {
            repitch: true,
            shift: false,
            retro: true,
            ratchet: true,
            drop: true,
        }
    }
}

impl Mutations {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether it allows any kind at all.
    pub fn any(&self) -> bool {
        self.repitch || self.shift || self.retro || self.ratchet || self.drop
    }

    /// The kinds it allows, in a list held in place, and how many.
    fn kinds(&self) -> ([Kind; 5], usize) {
        let mut kinds = [Kind::Repitch; 5];
        let mut count = 0;
        for (on, kind) in [
            (self.repitch, Kind::Repitch),
            (self.shift, Kind::Shift),
            (self.retro, Kind::Retro),
            (self.ratchet, Kind::Ratchet),
            (self.drop, Kind::Drop),
        ] {
            if on {
                kinds[count] = kind;
                count += 1;
            }
        }
        (kinds, count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Repitch,
    Shift,
    Retro,
    Ratchet,
    Drop,
}

/// Numbers drawn one after another from a seed, each worked out from the seed and how many came
/// before, so the same seed draws the same.
struct Draws {
    seed: u64,
    drawn: usize,
}

impl Draws {
    /// A whole number below `count`, at least 1.
    fn below(&mut self, count: usize) -> usize {
        let u = unit(mix(self.seed, self.drawn));
        self.drawn += 1;
        ((u * count as f64) as usize).min(count.max(1) - 1)
    }

    /// Up or down.
    fn sign(&mut self) -> f64 {
        if self.below(2) == 0 { -1.0 } else { 1.0 }
    }
}

/// The note a cell writes, if it writes one.
fn note_of(cell: &GenCell) -> Option<Note> {
    match cell {
        GenCell::Note { note, .. } | GenCell::NoteFx { note, .. } => Some(*note),
        _ => None,
    }
}

/// Whether a cell writes a note that plays.
fn plays(cell: &GenCell) -> bool {
    matches!(note_of(cell), Some(Note::On(_)))
}

/// Give a cell's note another pitch, keeping its velocity, gate and FX.
fn set_note(cell: &mut GenCell, to: Note) {
    if let GenCell::Note { note, .. } | GenCell::NoteFx { note, .. } = cell {
        *note = to;
    }
}

/// A note moved `by` steps of the ladder, from the nearest step to where it is, so it lands in
/// the scale while the scale is on.
fn moved(ladder: &Ladder, note: u8, by: f64) -> u8 {
    ladder.note(ladder.position(note).round() + by)
}

/// The first row from `from` on, going round, whose cell plays a note.
fn noted_row(cells: &[GenCell], from: usize) -> Option<usize> {
    let len = cells.len();
    (0..len)
        .map(|ahead| (from + ahead) % len)
        .find(|row| plays(&cells[*row]))
}

/// Change a `Follow`'s cells `mutation` times, each change one of the kinds `mutations` allows,
/// drawn from `seed`. `steps` are its own phrase's, from which a ratchet takes the row's FX when
/// the cells don't copy them, and `first` the stored row of its first cell. Only the rows the
/// copy wrote, those before the first it left as they were, are changed.
pub(super) fn mutate(
    steps: &[Step],
    first: usize,
    ladder: &Ladder,
    mutation: u8,
    mutations: Mutations,
    seed: u64,
    cells: &mut [GenCell],
) {
    let (kinds, count) = mutations.kinds();
    let len = cells
        .iter()
        .take_while(|cell| **cell != GenCell::Under)
        .count();
    if count == 0 || len == 0 {
        return;
    }
    let cells = &mut cells[..len];
    let mut draws = Draws {
        seed: seed ^ MUTATION_KEY,
        drawn: 0,
    };
    for _ in 0..mutation {
        match kinds[draws.below(count)] {
            Kind::Repitch => {
                let from = draws.below(len);
                let by = draws.sign() * (1 + draws.below(2)) as f64;
                if let Some(row) = noted_row(cells, from)
                    && let Some(Note::On(note)) = note_of(&cells[row])
                {
                    set_note(&mut cells[row], Note::On(moved(ladder, note, by)));
                }
            }
            Kind::Shift => {
                let by = draws.sign() * (1 + draws.below(MOST_SHIFT)) as f64;
                for cell in cells.iter_mut() {
                    if let Some(Note::On(note)) = note_of(cell) {
                        set_note(cell, Note::On(moved(ladder, note, by)));
                    }
                }
            }
            Kind::Retro => {
                let span = SHORTEST_SPAN + draws.below(LONGEST_SPAN - SHORTEST_SPAN + 1);
                let start = draws.below(len);
                cells[start..(start + span).min(len)].reverse();
            }
            Kind::Ratchet => {
                let from = draws.below(len);
                let value = RATCHETS[draws.below(RATCHETS.len())] << 8;
                if let Some(row) = noted_row(cells, from) {
                    ratchet(&mut cells[row], steps.get(first + row), value);
                }
            }
            Kind::Drop => {
                let row = draws.below(len);
                let by = (draws.below(3) as f64) - 1.0;
                match note_of(&cells[row]) {
                    Some(Note::On(_)) => drop_note(&mut cells[row]),
                    Some(Note::Empty) => add_note(cells, row, ladder, by),
                    _ => {}
                }
            }
        }
    }
}

/// Repeat a cell's note within its row: a `RET` in the first FX slot that's empty, taking the
/// row's own FX from its step where the cell doesn't copy any. A row with a `RET` already, or no
/// empty slot, stays as it is.
fn ratchet(cell: &mut GenCell, step: Option<&Step>, value: u16) {
    if let GenCell::Note {
        note,
        velocity,
        gate,
    } = *cell
    {
        let own = |slot: usize| step.and_then(|step| step.fx.get(slot)).copied();
        *cell = GenCell::NoteFx {
            note,
            velocity,
            gate,
            fx: std::array::from_fn(|slot| own(slot).unwrap_or_default()),
        };
    }
    let GenCell::NoteFx { fx, .. } = cell else {
        return;
    };
    // Only the slots the step has show and play.
    let slots = step.map_or(fx.len(), |step| step.fx.len().min(fx.len()));
    let fx = &mut fx[..slots];
    if fx.iter().any(|slot| slot.command == Some(FxCommand::Ret)) {
        return;
    }
    if let Some(free) = fx.iter_mut().find(|slot| slot.command.is_none()) {
        *free = FxSlot {
            command: Some(FxCommand::Ret),
            value,
            ..FxSlot::default()
        };
    }
}

/// Take a note away, leaving an empty row with no velocity or gate.
fn drop_note(cell: &mut GenCell) {
    match cell {
        GenCell::Note {
            note,
            velocity,
            gate,
        }
        | GenCell::NoteFx {
            note,
            velocity,
            gate,
            ..
        } => {
            *note = Note::Empty;
            *velocity = None;
            *gate = None;
        }
        _ => {}
    }
}

/// Add a note on an empty row: the nearest note before it, going round, with its velocity and
/// gate, moved `by` steps of the ladder. With no note anywhere, nothing.
fn add_note(cells: &mut [GenCell], row: usize, ladder: &Ladder, by: f64) {
    let len = cells.len();
    let Some(before) = (1..len)
        .map(|back| (row + len - back) % len)
        .find(|at| plays(&cells[*at]))
    else {
        return;
    };
    let (Some(Note::On(pitch)), velocity, gate) = (match cells[before] {
        GenCell::Note {
            note,
            velocity,
            gate,
        }
        | GenCell::NoteFx {
            note,
            velocity,
            gate,
            ..
        } => (Some(note), velocity, gate),
        _ => (None, None, None),
    }) else {
        return;
    };
    let note = Note::On(moved(ladder, pitch, by));
    match &mut cells[row] {
        GenCell::Note {
            note: at,
            velocity: v,
            gate: g,
        }
        | GenCell::NoteFx {
            note: at,
            velocity: v,
            gate: g,
            ..
        } => {
            *at = note;
            *v = velocity;
            *g = gate;
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::{Scale, ScaleKind};
    use crate::selection::StepRange;
    use crate::shape::{Follow, Shape, ShapeColumn, render_into};

    const C_MINOR: Scale = Scale {
        on: true,
        root: 0,
        kind: ScaleKind::Minor,
    };

    fn rows(steps: &[&str]) -> Vec<Step> {
        steps.iter().map(|text| text.parse().unwrap()).collect()
    }

    /// A bar in C minor, with rests to add notes on.
    fn trunk() -> Vec<Step> {
        rows(&[
            "C-3 7F --- --- ---",
            "--- -- --- --- ---",
            "D#3 50 --- --- ---",
            "G-3 64 --- --- ---",
            "--- -- --- --- ---",
            "A#3 50 --- --- ---",
            "C-4 7F --- --- ---",
            "--- -- --- --- ---",
            "G-3 64 --- --- ---",
            "F-3 50 --- --- ---",
            "--- -- --- --- ---",
            "D#3 64 --- --- ---",
            "D-3 50 --- --- ---",
            "--- -- --- --- ---",
            "C-3 64 --- --- ---",
            "G-2 50 --- --- ---",
        ])
    }

    fn only(kind: Kind) -> Mutations {
        Mutations {
            repitch: kind == Kind::Repitch,
            shift: kind == Kind::Shift,
            retro: kind == Kind::Retro,
            ratchet: kind == Kind::Ratchet,
            drop: kind == Kind::Drop,
        }
    }

    /// What a `Follow` of `source` with these mutations writes over as many empty rows.
    fn mutated(
        source: &[Step],
        scale: Scale,
        mutation: u8,
        mutate: Mutations,
        seed: u64,
    ) -> Vec<Step> {
        let own = rows(&vec!["--- -- --- --- ---"; source.len()]);
        mutated_over(own, source, scale, mutation, mutate, seed)
    }

    /// The same, over rows of its own.
    fn mutated_over(
        mut steps: Vec<Step>,
        source: &[Step],
        scale: Scale,
        mutation: u8,
        mutate: Mutations,
        seed: u64,
    ) -> Vec<Step> {
        let range = StepRange::between(0, steps.len() - 1);
        let column = ShapeColumn::Note {
            scale,
            writes: None,
        };
        let follow = Follow {
            mutation,
            mutate,
            ..Follow::default()
        };
        let mut cells = vec![GenCell::Under; range.len()];
        render_into(
            &steps.clone(),
            range,
            column,
            &Shape::following(follow, seed),
            Some(source),
            &mut cells,
        );
        for (step, cell) in steps.iter_mut().zip(cells) {
            cell.apply(column, step);
        }
        steps
    }

    fn differing(a: &[Step], b: &[Step]) -> usize {
        a.iter().zip(b).filter(|(a, b)| a != b).count()
    }

    fn notes(steps: &[Step]) -> usize {
        steps
            .iter()
            .filter(|step| matches!(step.note, Note::On(_)))
            .count()
    }

    #[test]
    fn no_mutation_or_no_kind_copies_it_as_it_is() {
        let trunk = trunk();
        assert_eq!(mutated(&trunk, C_MINOR, 0, Mutations::default(), 7), trunk);
        let none = Mutations {
            repitch: false,
            shift: false,
            retro: false,
            ratchet: false,
            drop: false,
        };
        assert_eq!(mutated(&trunk, C_MINOR, 8, none, 7), trunk);
    }

    #[test]
    fn a_seed_repeats_and_another_mutates_otherwise() {
        let trunk = trunk();
        let mutate = Mutations {
            shift: true,
            ..Mutations::default()
        };
        let once = mutated(&trunk, C_MINOR, 4, mutate, 11);
        assert_eq!(once, mutated(&trunk, C_MINOR, 4, mutate, 11));
        assert_ne!(once, trunk);
        let others = (12..20)
            .filter(|seed| mutated(&trunk, C_MINOR, 4, mutate, *seed) != once)
            .count();
        assert!(others >= 7, "{others} of 8 seeds differ");
    }

    #[test]
    fn every_kind_keeps_the_notes_in_the_scale() {
        let trunk = trunk();
        for kind in [
            Kind::Repitch,
            Kind::Shift,
            Kind::Retro,
            Kind::Ratchet,
            Kind::Drop,
        ] {
            let mut changed = 0;
            for seed in 0..64 {
                let out = mutated(&trunk, C_MINOR, 6, only(kind), seed);
                changed += differing(&out, &trunk);
                for step in &out {
                    if let Note::On(note) = step.note {
                        assert!(C_MINOR.contains(note), "{kind:?} wrote {step} off C minor");
                    }
                }
            }
            assert!(
                changed > 64,
                "{kind:?} changed {changed} rows over 64 seeds"
            );
        }
        // With the scale off, a repitch moves by semitones.
        let off = Scale {
            on: false,
            ..C_MINOR
        };
        assert_ne!(mutated(&trunk, off, 4, only(Kind::Repitch), 3), trunk);
    }

    #[test]
    fn each_kind_changes_what_it_says() {
        let trunk = trunk();
        let sorted = |steps: &[Step]| {
            let mut texts: Vec<String> = steps.iter().map(ToString::to_string).collect();
            texts.sort();
            texts
        };
        for seed in 0..32 {
            // A repitch moves one note's pitch, keeping its velocity and the rhythm.
            let out = mutated(&trunk, C_MINOR, 1, only(Kind::Repitch), seed);
            assert_eq!(differing(&out, &trunk), 1);
            for (now, was) in out.iter().zip(&trunk) {
                assert_eq!(now.velocity, was.velocity);
                assert_eq!(
                    matches!(now.note, Note::On(_)),
                    matches!(was.note, Note::On(_))
                );
            }
            // A shift moves every note together.
            let out = mutated(&trunk, C_MINOR, 1, only(Kind::Shift), seed);
            assert_eq!(differing(&out, &trunk), notes(&trunk));
            // A span backwards: the same rows, at most four of them moved.
            let out = mutated(&trunk, C_MINOR, 1, only(Kind::Retro), seed);
            assert!(differing(&out, &trunk) <= LONGEST_SPAN);
            assert_eq!(sorted(&out), sorted(&trunk));
            // A ratchet: one note with a `RET` of two or three hits in its first slot.
            let out = mutated(&trunk, C_MINOR, 1, only(Kind::Ratchet), seed);
            assert_eq!(differing(&out, &trunk), 1);
            let row = out
                .iter()
                .find(|step| step.fx[0].command.is_some())
                .unwrap();
            assert_eq!(row.fx[0].command, Some(FxCommand::Ret));
            assert!(matches!(row.fx[0].coarse(), 0x02 | 0x03), "{row}");
            // A note dropped or added: one note fewer or more.
            let out = mutated(&trunk, C_MINOR, 1, only(Kind::Drop), seed);
            assert_eq!(differing(&out, &trunk), 1);
            assert_eq!(notes(&out).abs_diff(notes(&trunk)), 1);
        }
    }

    #[test]
    fn a_ratchet_takes_an_empty_slot_and_never_makes_a_second_ret() {
        let source = rows(&["C-3 64 --- --- ---"; 4]);
        // Not copying the source's FX, it keeps the row's own and ratchets in the next slot.
        let own = rows(&["--- -- cut:40 ret:03 ---"; 4]);
        let out = mutated_over(own.clone(), &source, C_MINOR, 8, only(Kind::Ratchet), 5);
        assert!(out.iter().all(|step| step.fx[1..] == own[0].fx[1..]));
        let own = rows(&["--- -- cut:40 --- ---"; 4]);
        let out = mutated_over(own, &source, C_MINOR, 8, only(Kind::Ratchet), 5);
        assert!(
            out.iter()
                .any(|step| step.fx[1].command == Some(FxCommand::Ret))
        );
        assert!(
            out.iter()
                .all(|step| step.fx[0].command == Some(FxCommand::CUT))
        );
    }
}
