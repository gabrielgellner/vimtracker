//! `Mini`, a fill whose line holds its rhythm and its values together (#1092, decision 462): a
//! line in Tidal's mini-notation whose atoms are the column's own values, `C-3 [. 3] <5 6>*2 8?`
//! on the Note column, `7F 40 [60 40]` on the Velocity column, `40 _ 80` on the Length column.
//! It has no `Where`: the line says where and what.
//!
//! It reads as an `Own` rhythm does (`own.rs`), stepwise, each top-level token `Step` rows, and
//! is walked by the same code: a hit lands on the row nearest it within its place, two on a row
//! keep the first, `< >` takes the next choice each lap and counts on from pass to pass, and `?`
//! draws from the seed, the hit and the row. What's its own is the value each hit writes, kept
//! beside the beats as an [`Atom`] a hit, and on the Note column a hit that falls between rows
//! nudged off its row, as `Divisions` nudges its triplets, so `[C-3 D-3 E-3]` in a beat is a true
//! triplet.
//!
//! The text is kept on its generator and compiled there, on the edit thread, into the beats and
//! atoms, a few hundred bytes; the render reads them, allocating nothing, so the engine runs it
//! unchanged.

use serde::{Deserialize, Serialize};

use super::own::{self, Lands, Own};
use super::render::{Out, Rows, Shaped};
use super::{GenCell, Shape, ShapeColumn, Texts};
use crate::constants::{NUDGE_TICKS, TICKS_PER_STEP};
use crate::scale::Scale;
use crate::selection::StepRange;
use crate::step::{Gate, GateKind, MAX_NOTE, MIN_NOTE, Note, Step, most_nudge};

/// The semitones in an octave.
const OCTAVE: u16 = 12;
/// A row's nudges, in 24ths of a sixteenth, at a sixteenth.
const ROW_NUDGES: f64 = (TICKS_PER_STEP / NUDGE_TICKS) as f64;

/// What one of a `Mini`'s hits writes, in its column's units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Atom {
    /// A value as the column holds it: a note, a velocity, a nudge, a length, an FX value.
    Value(u8),
    /// On the Note column, a degree of the scale, counted from 0 at the root in From's octave,
    /// typed from 1, as `Weight 1` is the root.
    Degree(u8),
    /// On the Length column, `_`: the note tied into the next row's, `HLD`.
    Tie,
    /// On a kit line, a hit naming its own track: the generator's note, `From`, at the velocity
    /// notes are typed with (decision 464).
    Own,
    /// On a kit line, a hit naming another track, which writes nothing on this one.
    Other,
}

/// A kit line's mark on each of its generators (decision 464): the group whose tracks it spans,
/// and which of the group's lines it is, so an edit to it on one track is made on the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KitLine {
    pub group: u8,
    pub line: u32,
}

/// The note a degree is, counted from 0 at the scale's root in the octave `from` is in: the
/// scale's degrees while it's on, and semitones while it's off.
pub fn degree_note(scale: Scale, from: u8, degree: u8) -> u8 {
    let octave = u16::from(from) / OCTAVE * OCTAVE + u16::from(scale.root) % OCTAVE;
    let degree = usize::from(degree);
    let (count, above) = if scale.on {
        let intervals = scale.kind.intervals();
        (
            intervals.len(),
            u16::from(intervals[degree % intervals.len()]),
        )
    } else {
        (usize::from(OCTAVE), (degree % usize::from(OCTAVE)) as u16)
    };
    let note = octave + (degree / count) as u16 * OCTAVE + above;
    note.clamp(u16::from(MIN_NOTE), u16::from(MAX_NOTE)) as u8
}

/// Every row of a span counted, as a `Mini` counts its line over all of them.
fn every_row(len: usize) -> Rows {
    let mut rows = Rows::new(len);
    for at in 0..len {
        rows.set(at);
    }
    rows
}

/// A note's cell nudged `nudge` off its row, its velocity and gate kept.
fn timed(cell: GenCell, step: &Step, nudge: i8) -> GenCell {
    let (note, velocity, gate) = match cell {
        GenCell::Pitch(note) => (Note::On(note), step.velocity, step.gate),
        GenCell::Note {
            note,
            velocity,
            gate,
        } => (note, velocity, gate),
        other => return other,
    };
    GenCell::Timed {
        note,
        velocity,
        gate,
        nudge,
    }
}

/// A `Mini`'s cells for `range`, one for each row, the first in `out.cells[0]`: each hit's value
/// on the row it lands on, where the column can take one, and every other row left alone. On the
/// Note column a hit between rows is nudged off its row, and a hit `*n` finer than a row is a
/// `RET` on its note, as an `Own` rhythm's is.
pub(super) fn render<F: FnMut(usize, &Shaped, Option<usize>)>(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    shape: &Shape,
    texts: Texts,
    out: &mut Out<'_, F>,
) {
    let line = shape.mini;
    let len = out.cells.len();
    let most = most_nudge(TICKS_PER_STEP);
    // A kit line's track plays only where its name is: every other row rests.
    if shape.kit.is_some() && matches!(column, ShapeColumn::Note { .. }) {
        for (offset, cell) in out.cells.iter_mut().enumerate() {
            if steps
                .get(range.first + offset)
                .is_some_and(|step| step.note != Note::Off)
            {
                *cell = GenCell::Timed {
                    note: Note::Empty,
                    velocity: None,
                    gate: None,
                    nudge: 0,
                };
            }
        }
    }
    own::walk(texts.rhythm, line, shape.seed, len, |lands: Lands| {
        let row = range.first + lands.nth;
        let (Some(step), Some(atom)) = (steps.get(row), texts.atoms.get(usize::from(lands.hit)))
        else {
            return;
        };
        if !column.can_take(step) {
            return;
        }
        let (cell, value) = match (*atom, column) {
            (Atom::Tie, ShapeColumn::Length) => {
                let gate = Gate {
                    kind: GateKind::Hold,
                    ..step.gate.unwrap_or_default()
                };
                (GenCell::Gate(gate), 0)
            }
            (Atom::Degree(degree), ShapeColumn::Note { scale, .. }) => {
                let note = degree_note(scale, shape.from, degree);
                (column.cell(step, u16::from(note) << 8), note)
            }
            (Atom::Own, ShapeColumn::Note { .. }) => {
                (column.cell(step, u16::from(shape.from) << 8), shape.from)
            }
            (Atom::Tie | Atom::Degree(_) | Atom::Own | Atom::Other, _) => return,
            (Atom::Value(value), _) => (column.cell(step, u16::from(value) << 8), value),
        };
        let nudge = (lands.off * ROW_NUDGES)
            .round()
            .clamp(-f64::from(most), f64::from(most));
        let cell = match column {
            ShapeColumn::Note { .. } if nudge != 0.0 => timed(cell, step, nudge as i8),
            _ => cell,
        };
        out.cells[lands.nth] = cell;
        (out.seen)(row, &Shaped::note(value), Some(usize::from(lands.hit)));
    });
    if line.ratchets && matches!(column, ShapeColumn::Note { .. }) {
        let counted = every_row(len);
        own::ratchet(
            texts.rhythm,
            line,
            shape.seed,
            &counted,
            (steps, range.first),
            out.cells,
        );
    }
}

/// How many of a line's hits land on a row another took first over `count` rows, and so are
/// dropped, at its `Step`.
pub fn dropped(beats: &[crate::shape::Beat], line: Own, seed: u64, count: usize) -> usize {
    let mut dropped = 0;
    own::walk_dropping(beats, line, seed, count, |_| {}, || dropped += 1);
    dropped
}

/// The least `Step` above a line's own at which no hit of a lap shares a row, if one does.
pub fn keeping_step(beats: &[crate::shape::Beat], line: Own) -> Option<u8> {
    (line.step + 1..=own::MAX_STEP).find(|step| {
        let tried = Own {
            step: *step,
            chances: false,
            pass: 0,
            ..line
        };
        let lap = (own::line_steps(beats) * u32::from(*step)) as usize;
        dropped(beats, tried, 0, lap) == 0
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::compile_mini;
    use crate::scale::ScaleKind;
    use crate::shape::{FillKind, render_into};
    use crate::step::FxCommand;

    fn minor() -> Scale {
        Scale {
            on: true,
            kind: ScaleKind::Minor,
            ..Scale::default()
        }
    }

    fn notes(scale: Scale) -> ShapeColumn {
        ShapeColumn::Note {
            scale,
            writes: Some(0x64),
        }
    }

    /// A line rendered on a column of `count` rows, each with a note, at a `Step`.
    fn rendered(text: &str, column: ShapeColumn, step: u8, count: usize) -> Vec<GenCell> {
        let (beats, atoms) = compile_mini(text, column).unwrap();
        let shape = Shape {
            kind: FillKind::Mini,
            from: 48,
            mini: Own {
                step,
                ..Own::default()
            }
            .of(&beats),
            ..Shape::default()
        };
        let mut steps = vec![Step::default(); count];
        for step in &mut steps {
            step.note = Note::On(60);
        }
        let mut cells = vec![GenCell::Under; count];
        let texts = Texts {
            list: &[],
            rhythm: &beats,
            atoms: &atoms,
        };
        let range = StepRange::between(0, count - 1);
        render_into(&steps, range, column, &shape, texts, None, &mut cells);
        cells
    }

    /// The pitch each row is given, or none where it's left alone.
    fn pitches(cells: &[GenCell]) -> Vec<Option<u8>> {
        cells
            .iter()
            .map(|cell| match *cell {
                GenCell::Pitch(note)
                | GenCell::Timed {
                    note: Note::On(note),
                    ..
                } => Some(note),
                _ => None,
            })
            .collect()
    }

    /// Names and degrees in one line, `1` the root in From's octave, the line going round.
    #[test]
    fn notes_by_name_and_degrees_from_1_in_one_line() {
        let cells = rendered("C-3 . 3 c4 8", notes(minor()), 1, 6);
        // C-3 is 60; the 3rd of C minor in From's octave, C-2's, is D#2, 51; the 8th is C-3.
        assert_eq!(
            pitches(&cells),
            [Some(60), None, Some(51), Some(72), Some(60), Some(60)]
        );
        assert_eq!(degree_note(minor(), 48, 0), 48);
        assert_eq!(degree_note(minor(), 50, 7), 60, "the 8th, an octave up");
        assert_eq!(degree_note(Scale::default(), 48, 3), 51, "semitones off");
    }

    /// Each column's atoms: hex velocities, signed nudges, lengths with `_` tying.
    #[test]
    fn each_column_writes_its_own_units() {
        let velocity = rendered("7F [40 20] ~", ShapeColumn::Velocity, 2, 6);
        assert_eq!(velocity[0], GenCell::Velocity(0x7F));
        assert_eq!(velocity[2], GenCell::Velocity(0x40));
        assert_eq!(velocity[3], GenCell::Velocity(0x20));
        assert_eq!(velocity[4], GenCell::Under, "a rest");
        let nudges = rendered("0 +8 -4", ShapeColumn::Nudge { most: 0xB }, 1, 3);
        assert_eq!(nudges[1], GenCell::Nudge(8));
        assert_eq!(nudges[2], GenCell::Nudge(-4));
        let lengths = rendered("40 _ 80", ShapeColumn::Length, 1, 3);
        assert!(matches!(lengths[1], GenCell::Gate(gate) if gate.kind == GateKind::Hold));
        assert!(matches!(lengths[2], GenCell::Gate(gate) if gate.length == Some(0x80)));
        let cut = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::Cha,
        };
        let mut steps = vec![Step::default(); 2];
        for step in &mut steps {
            step.fx = vec![Default::default()];
        }
        let (beats, atoms) = compile_mini("30 60", cut).unwrap();
        let shape = Shape {
            kind: FillKind::Mini,
            mini: Own::default().of(&beats),
            ..Shape::default()
        };
        let mut cells = vec![GenCell::Under; 2];
        let texts = Texts {
            list: &[],
            rhythm: &beats,
            atoms: &atoms,
        };
        render_into(
            &steps,
            StepRange::between(0, 1),
            cut,
            &shape,
            texts,
            None,
            &mut cells,
        );
        assert!(matches!(cells[1], GenCell::Fx(fx) if fx.value >> 8 == 0x60));
    }

    /// `[ ]` in a beat is a true triplet, its hits between rows nudged a third of a row off them.
    #[test]
    fn a_split_between_rows_is_nudged_off_them() {
        let cells = rendered("[C-3 D-3 E-3]", notes(Scale::default()), 4, 4);
        assert_eq!(pitches(&cells), [Some(60), Some(62), None, Some(64)]);
        let nudges: Vec<i8> = cells
            .iter()
            .map(|cell| match *cell {
                GenCell::Timed { nudge, .. } => nudge,
                _ => 0,
            })
            .collect();
        assert_eq!(nudges, [0, 8, 0, -8]);
    }

    /// Two hits on a row keep the first; the rest are counted, and a `Step` found that keeps
    /// them all.
    #[test]
    fn hits_sharing_a_row_are_counted_and_a_step_keeps_them() {
        let column = notes(Scale::default());
        let (beats, _) = compile_mini("[C-3 D-3 E-3] F-3", column).unwrap();
        let line = Own::default().of(&beats);
        assert_eq!(dropped(&beats, line, 0, 4), 4, "two a lap, over two laps");
        assert_eq!(keeping_step(&beats, line), Some(3));
        let (beats, _) = compile_mini("C-3 D-3", column).unwrap();
        assert_eq!(dropped(&beats, Own::default().of(&beats), 0, 8), 0);
    }

    /// `< >` goes round a lap at a time, on into the next pass.
    #[test]
    fn a_choice_turns_each_lap_and_on_into_the_next_pass() {
        let column = notes(Scale::default());
        let cells = rendered("<C-3 D-3 E-3>", column, 2, 8);
        assert_eq!(
            pitches(&cells),
            [
                Some(60),
                None,
                Some(62),
                None,
                Some(64),
                None,
                Some(60),
                None
            ]
        );
        let (beats, atoms) = compile_mini("<C-3 D-3 E-3>", column).unwrap();
        let shape = Shape {
            kind: FillKind::Mini,
            mini: Own {
                step: 2,
                ..Own::default()
            }
            .of(&beats),
            ..Shape::default()
        };
        assert!(shape.varies(), "its laps count on");
        let next = shape.on_pass(1);
        let mut steps = vec![Step::default(); 8];
        for step in &mut steps {
            step.note = Note::On(60);
        }
        let mut cells = vec![GenCell::Under; 8];
        let texts = Texts {
            list: &[],
            rhythm: &beats,
            atoms: &atoms,
        };
        render_into(
            &steps,
            StepRange::between(0, 7),
            column,
            &next,
            texts,
            None,
            &mut cells,
        );
        assert_eq!(pitches(&cells)[0], Some(62), "pass 1 starts on lap 4, D-3");
    }
}
