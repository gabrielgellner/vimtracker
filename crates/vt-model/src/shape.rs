//! Filling a column across a selection with a shaped ramp, random values, a wave, or two values in
//! turn.

use crate::constants::{TICKS_PER_QUARTER, TICKS_PER_STEP};
use crate::lfo::{Rate, Shape as Wave, wave};
use crate::selection::StepRange;
use crate::step::{FxCommand, FxSlot, MAX_VELOCITY, Mark, Note, Step};

/// The furthest `curve` and `s` go either way.
pub const MAX_BEND: i8 = 100;
/// The waves a column can follow, in the order `+` goes through them.
pub const WAVES: [Wave; 5] = [Wave::Sine, Wave::Tri, Wave::Saw, Wave::Ramp, Wave::Square];
/// How long a wave's cycle is unless told otherwise: a half note.
pub const DEFAULT_RATE: Rate = Rate::Note(TICKS_PER_QUARTER as u32 * 2);

/// What goes in the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillKind {
    /// From one value to the other, along the curve.
    Ramp,
    /// Anywhere between the two values, new on each row.
    Random,
    /// Swinging between the two values, by where each row falls in the wave's cycle.
    Wave,
    /// One value then the other, on the rows it fills, however they're spaced.
    Alternate,
}

impl FillKind {
    /// Every fill, in the order `+` goes through them.
    pub const ALL: [Self; 4] = [Self::Ramp, Self::Random, Self::Wave, Self::Alternate];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Ramp => "Ramp",
            Self::Random => "Random",
            Self::Wave => "Wave",
            Self::Alternate => "Alternate",
        }
    }

    /// What it does, for a list to choose from.
    pub fn description(self) -> &'static str {
        match self {
            Self::Ramp => "From one value to the other",
            Self::Random => "Anywhere between the two",
            Self::Wave => "Swinging between the two",
            Self::Alternate => "One value, then the other",
        }
    }
}

/// Which rows get a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Where {
    /// Rows with a note.
    Notes,
    /// Rows without a note.
    NoNotes,
    /// Rows already holding this command.
    Commands,
    /// Every row.
    AllRows,
    /// Every so many rows, starting from the first.
    Every(u16),
    /// Each row with this percent chance.
    Chance(u8),
}

impl Where {
    /// The name on screen, without an amount.
    pub fn name(self) -> &'static str {
        match self {
            Self::Notes => "Notes",
            Self::NoNotes => "No notes",
            Self::Commands => "Commands",
            Self::AllRows => "All rows",
            Self::Every(_) => "Every",
            Self::Chance(_) => "Chance",
        }
    }
}

/// The column being filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeColumn {
    Velocity,
    Fx { slot: usize, command: FxCommand },
}

impl ShapeColumn {
    /// The highest value this column takes.
    pub fn highest(self) -> u8 {
        match self {
            Self::Velocity => MAX_VELOCITY,
            Self::Fx { command, .. } => command.highest(),
        }
    }

    /// Which rows it fills unless told otherwise: the ones already holding such a value.
    pub fn default_where(self) -> Where {
        match self {
            Self::Velocity => Where::Notes,
            Self::Fx { .. } => Where::Commands,
        }
    }

    /// The choices of rows. A velocity means nothing without a note, so those only narrow them.
    pub fn wheres(self) -> Vec<Where> {
        match self {
            Self::Velocity => vec![Where::Notes, Where::Every(4), Where::Chance(50)],
            Self::Fx { .. } => vec![
                Where::Commands,
                Where::AllRows,
                Where::Notes,
                Where::NoNotes,
                Where::Every(4),
                Where::Chance(50),
            ],
        }
    }

    /// This column's value on a step, if it has one.
    fn value(self, step: &Step) -> Option<u8> {
        match self {
            Self::Velocity => {
                matches!(step.note, Note::On(_)).then(|| step.velocity.unwrap_or(MAX_VELOCITY))
            }
            Self::Fx { slot, command } => step
                .fx
                .get(slot)
                .filter(|fx| fx.command == Some(command))
                .map(|fx| fx.value),
        }
    }

    /// Whether a step could take a value at all. A row holding another command keeps it.
    fn can_take(self, step: &Step) -> bool {
        match self {
            Self::Velocity => matches!(step.note, Note::On(_)),
            Self::Fx { slot, command } => step
                .fx
                .get(slot)
                .is_some_and(|fx| fx.command.is_none() || fx.command == Some(command)),
        }
    }

    /// Write a value to a step.
    pub fn write(self, step: &mut Step, value: u8) {
        match self {
            Self::Velocity => step.velocity = Some(value),
            Self::Fx { slot, command } => {
                if let Some(fx) = step.fx.get_mut(slot) {
                    *fx = FxSlot {
                        command: Some(command),
                        value,
                        mark: if fx.is_value() { fx.mark } else { Mark::Lock },
                    }
                    .with_valid_mark();
                }
            }
        }
    }
}

/// How to fill a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub from: u8,
    pub to: u8,
    /// Bends the line: above 0 it moves early, and below 0 late.
    pub curve: i8,
    /// Eases both ends: above 0 an S, and below 0 the reverse.
    pub s: i8,
    pub kind: FillKind,
    /// The wave a `Wave` follows, its length, and where in the cycle the first row selected falls,
    /// in degrees, as an LFO's are.
    pub wave: Wave,
    pub rate: Rate,
    pub phase: u16,
    /// How many rows in a row `Alternate` gives each value.
    pub run: u16,
    pub place: Where,
    /// Picks the random values and the chance rows, so a change elsewhere doesn't reroll them.
    pub seed: u64,
}

/// Bias: bends `t` toward 1 for `a` above 0.5.
fn bias(t: f64, a: f64) -> f64 {
    t / ((1.0 / a - 2.0) * (1.0 - t) + 1.0)
}

/// Schlick's gain built from bias: an S for `g` below 0.5.
fn gain(t: f64, g: f64) -> f64 {
    if t < 0.5 {
        bias(2.0 * t, g) / 2.0
    } else {
        1.0 - bias(2.0 - 2.0 * t, g) / 2.0
    }
}

/// Where a ramp is at `t` from 0 to 1. It always starts at 0 and ends at 1, and 0 and 0 is a line.
pub fn curved(t: f64, curve: i8, s: i8) -> f64 {
    // Just short of the limits, where these curves stop moving at all.
    let amount = |bend: i8| (f64::from(bend) / 200.0).clamp(-0.495, 0.495);
    gain(bias(t, 0.5 + amount(curve)), 0.5 - amount(s))
}

/// A stateless number for a row, so a row's result doesn't depend on any other's.
fn mix(seed: u64, row: usize) -> u64 {
    let mut mixed = seed
        .wrapping_add((row as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^ (mixed >> 31)
}

/// The rows that get a value.
pub fn targets(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    place: Where,
    seed: u64,
) -> Vec<usize> {
    (range.first..=range.last)
        .filter(|row| {
            let Some(step) = steps.get(*row) else {
                return false;
            };
            if !column.can_take(step) {
                return false;
            }
            let has_note = matches!(step.note, Note::On(_));
            let offset = row - range.first;
            match place {
                Where::Notes => has_note,
                Where::NoNotes => !has_note,
                Where::Commands => column.value(step).is_some(),
                Where::AllRows => true,
                Where::Every(every) => offset.is_multiple_of(usize::from(every.max(1))),
                Where::Chance(percent) => mix(seed ^ 0x5555, offset) % 100 < u64::from(percent),
            }
        })
        .collect()
}

/// The values a shape writes, by row. A ramp's ends are the first and last rows it fills, and rows
/// keep their spacing along it.
pub fn values(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    shape: &Shape,
) -> Vec<(usize, u8)> {
    let rows = targets(steps, range, column, shape.place, shape.seed);
    let (Some(&first), Some(&last)) = (rows.first(), rows.last()) else {
        return Vec::new();
    };
    let top = f64::from(column.highest());
    let (from, to) = (f64::from(shape.from).min(top), f64::from(shape.to).min(top));
    let (low, high) = (from.min(to) as u64, from.max(to) as u64);
    let across = (last - first).max(1) as f64;
    rows.iter()
        .enumerate()
        .map(|(index, &row)| {
            let value = match shape.kind {
                FillKind::Ramp => {
                    let t = (row - first) as f64 / across;
                    // Rounding the distance travelled means falling mirrors rising exactly.
                    let moved = ((to - from).abs() * curved(t, shape.curve, shape.s) + 0.5).floor();
                    (from + moved.copysign(to - from)).clamp(0.0, top) as u8
                }
                FillKind::Random => {
                    let span = high - low + 1;
                    (low + mix(shape.seed, row - range.first) % span) as u8
                }
                FillKind::Wave => {
                    let ticks = (row - range.first) as u64 * TICKS_PER_STEP;
                    let cycles =
                        shape.rate.cycles(ticks as f64, 0.0) + f64::from(shape.phase) / 360.0;
                    let t = (f64::from(wave(shape.wave, cycles, shape.seed)) + 1.0) / 2.0;
                    (from + (to - from) * t).round().clamp(0.0, top) as u8
                }
                FillKind::Alternate => {
                    let turn = index / usize::from(shape.run.max(1));
                    (if turn.is_multiple_of(2) { from } else { to }) as u8
                }
            };
            (row, value)
        })
        .collect()
}

/// Where a shape starts and ends unless told otherwise: the first and last values in the selection,
/// or if those match, the lowest and highest, or if those match, the column's whole range.
pub fn default_ends(steps: &[Step], range: StepRange, column: ShapeColumn) -> (u8, u8) {
    let present: Vec<u8> = (range.first..=range.last)
        .filter_map(|row| steps.get(row).and_then(|step| column.value(step)))
        .collect();
    let (Some(&first), Some(&last)) = (present.first(), present.last()) else {
        return (0, column.highest());
    };
    if first != last {
        return (first, last);
    }
    let lowest = present.iter().copied().min().unwrap_or(first);
    let highest = present.iter().copied().max().unwrap_or(first);
    if lowest != highest {
        (lowest, highest)
    } else {
        (0, column.highest())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_a_value_keeps_a_mark_already_there_and_otherwise_locks() {
        let column = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::Res,
        };
        let mut step: Step = "--- -- CUT40h".parse().unwrap();
        column.write(&mut step, 0x20);
        assert_eq!(step.to_string(), "--- -- RES20h");
        let mut step: Step = "--- -- ---00".parse().unwrap();
        column.write(&mut step, 0x20);
        assert_eq!(step.to_string(), "--- -- RES20");
        let mut step: Step = "--- -- DEL10".parse().unwrap();
        column.write(&mut step, 0x20);
        assert_eq!(step.to_string(), "--- -- RES20");
    }

    const VEL: ShapeColumn = ShapeColumn::Velocity;
    const CUT: ShapeColumn = ShapeColumn::Fx {
        slot: 0,
        command: FxCommand::Cut,
    };

    fn rows(steps: &[&str]) -> Vec<Step> {
        steps.iter().map(|text| text.parse().unwrap()).collect()
    }

    fn all(steps: &[Step]) -> StepRange {
        StepRange::between(0, steps.len() - 1)
    }

    fn ramp(from: u8, to: u8, curve: i8, s: i8) -> Shape {
        Shape {
            from,
            to,
            curve,
            s,
            kind: FillKind::Ramp,
            wave: Wave::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            place: Where::Notes,
            seed: 1,
        }
    }

    fn notes(count: usize) -> Vec<Step> {
        rows(&vec!["C-4 -- ---00 ---00 ---00"; count])
    }

    fn written(steps: &[Step], shape: Shape) -> Vec<u8> {
        values(steps, all(steps), VEL, &shape)
            .iter()
            .map(|(_, value)| *value)
            .collect()
    }

    #[test]
    fn a_curve_reaches_the_values_the_design_gives() {
        // Over 9 steps from 00 to 7F. Curve +60 is bias 0.8, and S +60 is gain 0.2.
        assert_eq!(
            written(&notes(9), ramp(0, 0x7F, 60, 0)),
            [0x00, 0x2E, 0x49, 0x5A, 0x66, 0x6E, 0x75, 0x7B, 0x7F]
        );
        assert_eq!(
            written(&notes(9), ramp(0, 0x7F, 0, 60)),
            [0x00, 0x05, 0x0D, 0x1B, 0x40, 0x64, 0x72, 0x7A, 0x7F]
        );
    }

    #[test]
    fn with_no_bend_it_is_a_straight_line() {
        assert_eq!(
            written(&notes(5), ramp(0x10, 0x50, 0, 0)),
            [0x10, 0x20, 0x30, 0x40, 0x50]
        );
    }

    #[test]
    fn the_ends_are_exact_and_the_ramp_never_turns_back() {
        for curve in [-100, -60, -1, 0, 1, 60, 100] {
            for s in [-100, -60, 0, 60, 100] {
                for (from, to) in [(0, 0x7F), (0x7F, 0), (0x10, 0x11), (0x40, 0x40)] {
                    let out = written(&notes(33), ramp(from, to, curve, s));
                    assert_eq!(out[0], from, "{curve} {s}");
                    assert_eq!(out[32], to, "{curve} {s}");
                    let rising = out.windows(2).all(|pair| pair[0] <= pair[1]);
                    let falling = out.windows(2).all(|pair| pair[0] >= pair[1]);
                    assert!(rising || falling, "{curve} {s} {out:?}");
                }
            }
        }
    }

    #[test]
    fn falling_mirrors_rising() {
        let rising = written(&notes(9), ramp(0, 0x7F, 0, 0));
        let falling = written(&notes(9), ramp(0x7F, 0, 0, 0));
        let mirrored: Vec<u8> = rising.iter().map(|value| 0x7F - value).collect();
        assert_eq!(falling, mirrored);
    }

    #[test]
    fn rows_keep_their_spacing_along_a_ramp() {
        let steps = rows(&[
            "C-4 -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
        ]);
        let out = values(&steps, all(&steps), VEL, &ramp(0, 0x40, 0, 0));
        assert_eq!(out, [(0, 0x00), (1, 0x10), (4, 0x40)]);
    }

    #[test]
    fn the_ends_are_the_first_and_last_rows_filled() {
        let steps = rows(&[
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
        ]);
        let out = values(&steps, all(&steps), VEL, &ramp(0x10, 0x20, 0, 0));
        assert_eq!(out, [(1, 0x10), (2, 0x20)]);
    }

    #[test]
    fn velocity_only_ever_goes_on_notes() {
        let steps = rows(&[
            "C-4 -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
        ]);
        let picked = |place| targets(&steps, all(&steps), VEL, place, 1);
        assert_eq!(picked(Where::Notes), [0, 2, 4]);
        assert_eq!(picked(Where::Every(2)), [0, 2, 4]);
        assert_eq!(picked(Where::Every(4)), [0, 4]);
    }

    #[test]
    fn fx_rows_can_be_narrowed_and_other_commands_are_kept() {
        let steps = rows(&[
            "C-4 -- CUT10 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "--- -- RES10 ---00 ---00",
            "--- -- CUT30 ---00 ---00",
        ]);
        let picked = |place| targets(&steps, all(&steps), CUT, place, 1);
        assert_eq!(picked(Where::Commands), [0, 4]);
        assert_eq!(picked(Where::AllRows), [0, 1, 2, 4], "RES stays");
        assert_eq!(picked(Where::Notes), [0, 2]);
        assert_eq!(picked(Where::NoNotes), [1, 4]);
        assert_eq!(picked(Where::Every(2)), [0, 2, 4]);
    }

    #[test]
    fn chance_is_about_right_and_stays_put_for_a_seed() {
        let steps = notes(1000);
        let picked =
            |percent, seed| targets(&steps, all(&steps), VEL, Where::Chance(percent), seed);
        let half = picked(50, 1);
        assert!((400..600).contains(&half.len()), "{}", half.len());
        assert_eq!(half, picked(50, 1));
        assert_ne!(half, picked(50, 2));
        assert_eq!(picked(0, 1).len(), 0);
        assert_eq!(picked(100, 1).len(), 1000);
    }

    #[test]
    fn random_stays_in_range_either_way_round_and_reaches_both_ends() {
        let steps = notes(500);
        for (from, to) in [(0x10, 0x20), (0x20, 0x10)] {
            let shape = Shape {
                kind: FillKind::Random,
                ..ramp(from, to, 0, 0)
            };
            let out = written(&steps, shape);
            assert!(out.iter().all(|value| (0x10..=0x20).contains(value)));
            assert!(out.contains(&0x10) && out.contains(&0x20));
            assert_eq!(out, written(&steps, shape));
            assert_ne!(out, written(&steps, Shape { seed: 2, ..shape }));
        }
    }

    fn waving(wave: Wave, from: u8, to: u8) -> Shape {
        Shape {
            kind: FillKind::Wave,
            wave,
            ..ramp(from, to, 0, 0)
        }
    }

    #[test]
    fn a_wave_swings_between_the_values_at_its_rate() {
        // A half note is 8 rows.
        assert_eq!(
            written(&notes(9), waving(Wave::Tri, 0, 0x40)),
            [0x20, 0x30, 0x40, 0x30, 0x20, 0x10, 0, 0x10, 0x20]
        );
        assert_eq!(
            written(&notes(5), waving(Wave::Sine, 0, 0x40)),
            [0x20, 0x37, 0x40, 0x37, 0x20]
        );
        assert_eq!(
            written(&notes(8), waving(Wave::Square, 0x10, 0x70)),
            [0x70, 0x70, 0x70, 0x70, 0x10, 0x10, 0x10, 0x10]
        );
        assert_eq!(
            written(&notes(5), waving(Wave::Ramp, 0, 0x40)),
            [0, 0x08, 0x10, 0x18, 0x20]
        );
        assert_eq!(
            written(&notes(5), waving(Wave::Saw, 0, 0x40)),
            [0x40, 0x38, 0x30, 0x28, 0x20]
        );
    }

    #[test]
    fn a_waves_rate_and_phase_move_it_and_swapped_ends_flip_it() {
        let quarter = Shape {
            rate: "1/4".parse().unwrap(),
            ..waving(Wave::Tri, 0, 0x40)
        };
        assert_eq!(
            written(&notes(5), quarter),
            [0x20, 0x40, 0x20, 0, 0x20],
            "a quarter note is 4 rows"
        );
        let steps = Shape {
            rate: "2st".parse().unwrap(),
            ..waving(Wave::Square, 0, 0x40)
        };
        assert_eq!(written(&notes(4), steps), [0x40, 0, 0x40, 0]);
        let phased = Shape {
            phase: 90,
            ..waving(Wave::Tri, 0, 0x40)
        };
        assert_eq!(written(&notes(3), phased), [0x40, 0x30, 0x20]);
        assert_eq!(
            written(&notes(3), waving(Wave::Tri, 0x40, 0)),
            [0x20, 0x10, 0]
        );
    }

    #[test]
    fn a_wave_goes_by_where_a_row_falls_counting_from_the_first_selected() {
        let steps = rows(&[
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
        ]);
        let out = values(&steps, all(&steps), VEL, &waving(Wave::Tri, 0, 0x40));
        assert_eq!(out, [(1, 0x30), (3, 0x30), (4, 0x20)]);
        let later = StepRange::between(1, 4);
        let out = values(&steps, later, VEL, &waving(Wave::Tri, 0, 0x40));
        assert_eq!(out, [(1, 0x20), (3, 0x40), (4, 0x30)]);
    }

    fn alternating(run: u16, from: u8, to: u8) -> Shape {
        Shape {
            kind: FillKind::Alternate,
            run,
            ..ramp(from, to, 0, 0)
        }
    }

    #[test]
    fn alternate_takes_turns_for_its_run() {
        assert_eq!(
            written(&notes(5), alternating(1, 0x70, 0x40)),
            [0x70, 0x40, 0x70, 0x40, 0x70]
        );
        assert_eq!(
            written(&notes(6), alternating(2, 0x70, 0x40)),
            [0x70, 0x70, 0x40, 0x40, 0x70, 0x70]
        );
    }

    #[test]
    fn alternate_flips_on_every_note_however_they_are_spaced() {
        let steps = rows(&[
            "C-4 -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
        ]);
        let out = values(&steps, all(&steps), VEL, &alternating(1, 0x70, 0x40));
        assert_eq!(out, [(0, 0x70), (3, 0x40), (4, 0x70), (6, 0x40)]);
    }

    #[test]
    fn a_wave_and_alternate_stop_at_the_columns_top() {
        let steps = notes(4);
        assert_eq!(written(&steps, alternating(1, 0xFF, 0)), [0x7F, 0, 0x7F, 0]);
        let out = written(&notes(3), waving(Wave::Tri, 0, 0xFF));
        assert!(out.iter().all(|value| *value <= 0x7F), "{out:?}");
    }

    #[test]
    fn values_stop_at_the_columns_top() {
        let steps = notes(2);
        assert_eq!(written(&steps, ramp(0, 0xFF, 0, 0)), [0, 0x7F]);
        let vol = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::Vol,
        };
        let shape = Shape {
            place: Where::AllRows,
            ..ramp(0, 0xFF, 0, 0)
        };
        let out = values(&steps, all(&steps), vol, &shape);
        assert_eq!(out[1].1, 0xF8);
    }

    #[test]
    fn default_ends_prefer_the_first_and_last_values() {
        let steps = rows(&[
            "C-4 50 ---00 ---00 ---00",
            "C-4 7F ---00 ---00 ---00",
            "C-4 20 ---00 ---00 ---00",
        ]);
        assert_eq!(default_ends(&steps, all(&steps), VEL), (0x50, 0x20));

        let same_ends = rows(&[
            "C-4 40 ---00 ---00 ---00",
            "C-4 7F ---00 ---00 ---00",
            "C-4 40 ---00 ---00 ---00",
        ]);
        assert_eq!(default_ends(&same_ends, all(&same_ends), VEL), (0x40, 0x7F));

        let one_value = rows(&["C-4 40 ---00 ---00 ---00"; 3]);
        assert_eq!(default_ends(&one_value, all(&one_value), VEL), (0, 0x7F));

        let empty = rows(&["--- -- ---00 ---00 ---00"; 2]);
        assert_eq!(default_ends(&empty, all(&empty), VEL), (0, 0x7F));
    }

    #[test]
    fn default_ends_count_a_note_with_no_velocity_as_full() {
        let steps = rows(&["C-4 -- ---00 ---00 ---00", "C-4 20 ---00 ---00 ---00"]);
        assert_eq!(default_ends(&steps, all(&steps), VEL), (0x7F, 0x20));
    }

    #[test]
    fn default_ends_for_fx_use_only_that_command() {
        let steps = rows(&[
            "--- -- RES70 ---00 ---00",
            "--- -- CUT10 ---00 ---00",
            "--- -- CUT30 ---00 ---00",
        ]);
        assert_eq!(default_ends(&steps, all(&steps), CUT), (0x10, 0x30));
    }
}
