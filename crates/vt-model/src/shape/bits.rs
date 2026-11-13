//! `Multiply`: a rhythm read as a number and multiplied, after Noise Engineering's Numeric
//! Repetitor, whose Prime knob picks the factor that turns one rhythm into a family of related
//! ones. Sixteen rows are sixteen bits, the first row the lowest, so doubling moves the rhythm a
//! row later and an odd factor keeps the downbeat; what carries past the sixteenth row is lost, as
//! the product is kept to sixteen bits. A longer span is worked a block of sixteen rows at a time.
//!
//! It's worked out from the rows alone, without allocating, so a live generator can run it again
//! on every pass.

use serde::{Deserialize, Serialize};

use super::{GenCell, ShapeColumn};
use crate::selection::StepRange;
use crate::step::{FxSlot, Note, Step};

/// The most a rhythm is multiplied by.
pub const MAX_FACTOR: u8 = 16;
/// How many rows are one number.
const BLOCK: usize = 16;
/// What Numeric Repetitor keeps of a rhythm before multiplying it by 2 and by 3, the first row the
/// lowest bit: rows 1 to 4 and 9 to 12, and rows 1, 2 and 13 to 16.
const MASK_TWO: u16 = 0x0F0F;
const MASK_THREE: u16 = 0xF003;

/// What `Multiply` does to the bits. Saved with a live generator's shape, and left out while it's
/// as it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bits {
    /// What the rhythm is multiplied by, from 1, which leaves it as it is, to `MAX_FACTOR`.
    pub factor: u8,
    /// Whether factors 2 and 3 first keep only some rows, as Numeric Repetitor's do.
    pub masks: bool,
}

impl Default for Bits {
    fn default() -> Self {
        Self {
            factor: 3,
            masks: false,
        }
    }
}

impl Bits {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// A rhythm of sixteen rows, the first the lowest bit, multiplied: masked first where
    /// `masks` says, and kept to sixteen bits.
    pub fn product(self, rhythm: u16) -> u16 {
        let masked = match (self.masks, self.factor) {
            (true, 2) => rhythm & MASK_TWO,
            (true, 3) => rhythm & MASK_THREE,
            _ => rhythm,
        };
        (u32::from(masked) * u32::from(self.factor.clamp(1, MAX_FACTOR))) as u16
    }
}

/// Whether a row is a hit of a column's rhythm: a note, or its FX command.
fn hit(column: ShapeColumn, step: &Step) -> bool {
    match column {
        ShapeColumn::Note { .. } => matches!(step.note, Note::On(_)),
        _ => column.value(step).is_some(),
    }
}

/// Whether a new hit can go on a row: one without a note, so an `OFF` stays, or an FX slot without
/// another command.
fn free(column: ShapeColumn, step: &Step) -> bool {
    match column {
        ShapeColumn::Note { .. } => step.note == Note::Empty,
        _ => column.can_take(step),
    }
}

/// A `Multiply`'s cells for `range`, one for each row, the first in `cells[0]`. In each block of
/// sixteen rows, a hit the product keeps stays as it is, a new one takes the note or command last
/// before it in the block, and a hit it drops is cleared: a note with its velocity and gate, or
/// the FX slot. It reworks notes and FX commands; on any other column it leaves the rows alone.
pub(super) fn render(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    bits: Bits,
    cells: &mut [GenCell],
) {
    if !matches!(column, ShapeColumn::Note { .. } | ShapeColumn::Fx { .. }) {
        return;
    }
    let step = |offset: usize| steps.get(range.first + offset);
    for start in (0..cells.len()).step_by(BLOCK) {
        let rows = BLOCK.min(cells.len() - start);
        let mut rhythm = 0u16;
        for at in 0..rows {
            if step(start + at).is_some_and(|step| hit(column, step)) {
                rhythm |= 1 << at;
            }
        }
        let product = bits.product(rhythm);
        for at in 0..rows {
            let (was, now) = (rhythm & 1 << at != 0, product & 1 << at != 0);
            let Some(here) = step(start + at) else {
                continue;
            };
            cells[start + at] = match (was, now) {
                (true, false) => cleared(column),
                (false, true) if free(column, here) => {
                    // Multiplying never sets a bit below the rhythm's lowest, so a new hit always
                    // has one before it.
                    let before = (0..at).rev().find(|other| rhythm & 1 << other != 0);
                    match before.and_then(|other| step(start + other)) {
                        Some(from) => copied(column, from),
                        None => GenCell::Under,
                    }
                }
                _ => GenCell::Under,
            };
        }
    }
}

/// A column's hit gone: no note, or an empty FX slot.
fn cleared(column: ShapeColumn) -> GenCell {
    match column {
        ShapeColumn::Note { .. } => GenCell::Note {
            note: Note::Empty,
            velocity: None,
            gate: None,
        },
        _ => GenCell::Fx(FxSlot::default()),
    }
}

/// A hit copied from another row: its note with its velocity and gate, or its FX slot.
fn copied(column: ShapeColumn, from: &Step) -> GenCell {
    match column {
        ShapeColumn::Note { .. } => GenCell::Note {
            note: from.note,
            velocity: from.velocity,
            gate: from.gate,
        },
        ShapeColumn::Fx { slot, .. } => GenCell::Fx(from.fx.get(slot).copied().unwrap_or_default()),
        _ => GenCell::Under,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::Scale;
    use crate::shape::written;
    use crate::shape::{Acid, DEFAULT_RATE, FillKind, Follow, Pink, Shape, Values, Walk, Where};
    use crate::step::FxCommand;

    fn multiply(factor: u8, masks: bool) -> Shape {
        Shape {
            from: 0,
            to: 0,
            curve: 0,
            s: 0,
            kind: FillKind::Multiply,
            wave: crate::lfo::Shape::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::default(),
            place: Where::Notes,
            seed: 1,
            acid: Acid::default(),
            follow: Follow::default(),
            walk: Walk::default(),
            pink: Pink::default(),
            bits: Bits { factor, masks },
        }
    }

    fn note() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale::default(),
            writes: None,
        }
    }

    /// A rhythm drawn as `x` for a note and `.` for none, the notes rising a semitone a row so
    /// each can be told from the others.
    fn drawn(rhythm: &str) -> Vec<Step> {
        rhythm
            .chars()
            .enumerate()
            .map(|(row, hit)| {
                let mut step = Step::default();
                if hit == 'x' {
                    step.note = Note::On(48 + row as u8);
                    step.velocity = Some(0x40 + row as u8);
                }
                step
            })
            .collect()
    }

    /// The steps after a shape, drawn back.
    fn after(steps: &[Step], column: ShapeColumn, shape: &Shape) -> Vec<Step> {
        let range = StepRange::between(0, steps.len() - 1);
        let mut out = steps.to_vec();
        for (row, step) in written(steps, range, column, shape) {
            out[row] = step;
        }
        out
    }

    fn rhythm_of(steps: &[Step]) -> String {
        steps
            .iter()
            .map(|step| {
                if matches!(step.note, Note::On(_)) {
                    'x'
                } else {
                    '.'
                }
            })
            .collect()
    }

    #[test]
    fn products_are_the_hand_worked_ones() {
        let plain = |factor| Bits {
            factor,
            masks: false,
        };
        // Four on the floor, rows 1, 5, 9 and 13: 0x1111.
        assert_eq!(plain(3).product(0x1111), 0x3333);
        assert_eq!(plain(5).product(0x1111), 0x5555);
        // Rows 1, 4 and 7, 0x49: times 3 is 0xDB, times 7 is 0x1FF.
        assert_eq!(plain(3).product(0x49), 0xDB);
        assert_eq!(plain(7).product(0x49), 0x1FF);
        // A power of two moves it later, and what goes past the sixteenth row is lost.
        assert_eq!(plain(4).product(0x0001), 0x0004);
        assert_eq!(plain(2).product(0x8001), 0x0002);
        assert_eq!(plain(16).product(0x1111), 0x1110);
        // Numeric Repetitor's masks, for 2 and 3 only.
        let masked = |factor| Bits {
            factor,
            masks: true,
        };
        assert_eq!(masked(2).product(0xFFFF), 0x1E1E);
        assert_eq!(masked(3).product(0xFFFF), (0xF003u32 * 3) as u16);
        assert_eq!(masked(5).product(0x1111), 0x5555);
    }

    #[test]
    fn factor_one_leaves_the_rhythm_as_it_is() {
        for rhythm in [
            "x...x...x...x...",
            "x..x..x...x.x...",
            "................",
            "xxxxx",
        ] {
            let steps = drawn(rhythm);
            let range = StepRange::between(0, steps.len() - 1);
            assert!(written(&steps, range, note(), &multiply(1, false)).is_empty());
            assert!(written(&steps, range, note(), &multiply(1, true)).is_empty());
        }
    }

    #[test]
    fn a_kept_hit_stays_and_a_new_one_takes_the_note_before_it() {
        // Rows 1, 4 and 7, times 3: rows 1, 2, 4, 5, 7 and 8.
        let steps = drawn("x..x..x.........");
        let out = after(&steps, note(), &multiply(3, false));
        assert_eq!(rhythm_of(&out), "xx.xx.xx........");
        for (row, from) in [(0, 0), (1, 0), (3, 3), (4, 3), (6, 6), (7, 6)] {
            assert_eq!(out[row].note, steps[from].note, "row {row}");
            assert_eq!(out[row].velocity, steps[from].velocity, "row {row}");
        }
        // Doubling drops the first row's note, and the new second takes it.
        let out = after(&steps, note(), &multiply(2, false));
        assert_eq!(rhythm_of(&out), ".x..x..x........");
        assert_eq!(out[0].velocity, None);
        assert_eq!(out[1].note, steps[0].note);
    }

    #[test]
    fn a_new_hit_takes_the_nearest_before_it() {
        // Rows 2 and 16, 0x8002, times 3 is 0x18006, kept to 0x8006: rows 2, 3 and 16.
        let steps = drawn(".x.............x");
        let out = after(&steps, note(), &multiply(3, false));
        assert_eq!(rhythm_of(&out), ".xx............x");
        assert_eq!(out[2].note, steps[1].note);
        // Rows 1 and 4 times 5 are rows 1, 3, 4 and 6: row 3 takes row 1's note, row 6 row 4's.
        let steps = drawn("x..x............");
        let out = after(&steps, note(), &multiply(5, false));
        assert_eq!(rhythm_of(&out), "x.xx.x..........");
        assert_eq!(out[2].note, steps[0].note);
        assert_eq!(out[5].note, steps[3].note);
    }

    #[test]
    fn a_longer_span_is_worked_sixteen_rows_at_a_time() {
        let steps = drawn("x...x...x...x...x.......x.......x");
        let out = after(&steps, note(), &multiply(3, false));
        assert_eq!(rhythm_of(&out), "xx..xx..xx..xx..xx......xx......x");
        // The last block's one row times 3 would be rows 1 and 2, but it has only the one.
        assert_eq!(out[32].note, steps[32].note);
    }

    #[test]
    fn an_off_stays_and_another_command_keeps_its_slot() {
        let mut steps = drawn("x...............");
        steps[1].note = Note::Off;
        let out = after(&steps, note(), &multiply(3, false));
        assert_eq!(out[1].note, Note::Off);

        let command = FxCommand::RES;
        let column = ShapeColumn::Fx { slot: 0, command };
        let mut steps = vec![Step::default(); 16];
        for (row, value) in [(0, 0x10), (4, 0x50)] {
            steps[row].fx[0] = FxSlot {
                command: Some(command),
                value: value << 8,
                ..FxSlot::default()
            };
        }
        steps[5].fx[0].command = Some(FxCommand::CUT);
        let out = after(&steps, column, &multiply(3, false));
        assert_eq!(out[1].fx[0], steps[0].fx[0]);
        assert_eq!(out[5].fx[0], steps[5].fx[0], "another command stays");
        assert_eq!(out[4].fx[0], steps[4].fx[0]);
        // Doubled, the first row's command goes, and the second takes it.
        let out = after(&steps, column, &multiply(2, false));
        assert_eq!(out[0].fx[0], FxSlot::default());
        assert_eq!(out[1].fx[0], steps[0].fx[0]);
    }

    #[test]
    fn other_columns_are_left_alone() {
        let steps = drawn("x..x..x.........");
        let range = StepRange::between(0, 15);
        assert!(written(&steps, range, ShapeColumn::Velocity, &multiply(3, false)).is_empty());
    }
}
