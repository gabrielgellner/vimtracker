//! Motions, targets and what operators do to the steps they cover. No terminal is needed here.

use crate::condition::Condition;
use crate::constants::BEAT_STEPS;
use crate::scale::Scale;
use crate::step::{
    FxCommand, FxSlot, Gate, MAX_NOTE, MAX_NUDGE, MAX_PULSES, MAX_VELOCITY, MIN_NOTE, Mark, Note,
    Step,
};

/// One editable part of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Note,
    Velocity,
    /// How far the row's note part lands off its place, in the velocity's column group, as both
    /// are how the note is played.
    Nudge,
    /// The gate cell: a gate type and the row's length in pulses.
    Gate,
    /// The gate cell's length: how much of its last pulse the row's note lasts.
    GateLength,
    FxCommand(usize),
    /// An FX value's coarse digits.
    FxValue(usize),
    /// An FX value's fine digits, which only a continuous setting's has.
    FxFine(usize),
}

/// The column group of the velocity and the nudge.
pub const VELOCITY_GROUP: usize = 1;
/// The column group of the gate cell.
pub const GATE_GROUP: usize = 2;
/// The column group of the first FX slot. Each slot after it is one more.
pub const FX_GROUP: usize = 3;
/// Where the first FX slot's command is in `h l` order, after the note, velocity, nudge and the
/// gate cell's two parts. Each slot takes three places: its command, value and fine digits.
pub const FIRST_FX_ORDER: usize = 5;

impl Field {
    /// The column group this field belongs to: note, velocity, gate, then each FX slot.
    pub fn group(self) -> usize {
        match self {
            Self::Note => 0,
            Self::Velocity | Self::Nudge => VELOCITY_GROUP,
            Self::Gate | Self::GateLength => GATE_GROUP,
            Self::FxCommand(slot) | Self::FxValue(slot) | Self::FxFine(slot) => FX_GROUP + slot,
        }
    }

    /// The first field in a column group.
    pub fn of_group(group: usize) -> Self {
        match group {
            0 => Self::Note,
            VELOCITY_GROUP => Self::Velocity,
            GATE_GROUP => Self::Gate,
            slot => Self::FxCommand(slot - FX_GROUP),
        }
    }

    /// This field's position in `h l` order.
    pub fn order(self) -> usize {
        match self {
            Self::Note => 0,
            Self::Velocity => 1,
            Self::Nudge => 2,
            Self::Gate => 3,
            Self::GateLength => 4,
            Self::FxCommand(slot) => FIRST_FX_ORDER + slot * 3,
            Self::FxValue(slot) => FIRST_FX_ORDER + 1 + slot * 3,
            Self::FxFine(slot) => FIRST_FX_ORDER + 2 + slot * 3,
        }
    }

    /// The field at a position in `h l` order.
    pub fn from_order(order: usize) -> Self {
        match order {
            0 => Self::Note,
            1 => Self::Velocity,
            2 => Self::Nudge,
            3 => Self::Gate,
            4 => Self::GateLength,
            n => {
                let (slot, part) = ((n - FIRST_FX_ORDER) / 3, (n - FIRST_FX_ORDER) % 3);
                match part {
                    0 => Self::FxCommand(slot),
                    1 => Self::FxValue(slot),
                    _ => Self::FxFine(slot),
                }
            }
        }
    }
}

/// An inclusive run of steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepRange {
    pub first: usize,
    pub last: usize,
}

impl StepRange {
    /// A range covering two steps, in either order.
    pub fn between(a: usize, b: usize) -> Self {
        Self {
            first: a.min(b),
            last: a.max(b),
        }
    }

    /// How many steps it covers.
    pub fn len(self) -> usize {
        self.last - self.first + 1
    }

    /// Always false, since a range covers at least one step.
    pub fn is_empty(self) -> bool {
        false
    }
}

/// Which column groups an edit covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    pub first: usize,
    pub last: usize,
}

impl Scope {
    /// Whole steps.
    pub const WHOLE: Self = Self {
        first: 0,
        last: usize::MAX,
    };

    /// One column group.
    pub fn column(group: usize) -> Self {
        Self {
            first: group,
            last: group,
        }
    }

    /// Whether this covers whole steps.
    pub fn is_whole(self) -> bool {
        self == Self::WHOLE
    }

    fn covers(self, group: usize) -> bool {
        (self.first..=self.last).contains(&group)
    }
}

/// Where a phrase's loop sits among its stored steps. Rows count from the loop's start, so `0` is
/// always beat 1 and rows above it are negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent {
    pub start: usize,
    pub length: usize,
    pub stored: usize,
}

impl Extent {
    /// A loop over everything stored.
    pub fn whole(length: usize) -> Self {
        Self {
            start: 0,
            length,
            stored: length,
        }
    }

    /// The row a stored step shows as.
    pub fn row(self, step: usize) -> isize {
        step as isize - self.start as isize
    }

    /// Whether a stored step plays.
    pub fn plays(self, step: usize) -> bool {
        (self.start..self.start + self.length).contains(&step)
    }

    /// The stored step at a row, or the nearest one.
    pub fn step_of(self, row: isize) -> usize {
        (self.start as isize)
            .saturating_add(row)
            .clamp(0, self.last_stored() as isize) as usize
    }

    /// The last stored step.
    pub fn last_stored(self) -> usize {
        self.stored.saturating_sub(1)
    }

    /// The last step of the loop.
    fn last_played(self) -> usize {
        (self.start + self.length)
            .saturating_sub(1)
            .min(self.last_stored())
    }

    /// A number of beats, limited to what could matter here so that it can be multiplied.
    fn beats(self, count: usize) -> isize {
        count.min(self.stored + 1) as isize
    }
}

/// A cursor movement, which can also be an operator's range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Down,
    Up,
    NextGroup,
    PreviousGroup,
    NextBeat,
    PreviousBeat,
    First,
    Last,
    RowStart,
    RowEnd,
}

impl Motion {
    /// Where a vertical motion lands, or the same step for a horizontal one. Motions stop at the
    /// edge of what is stored, with no wrapping. Beats and `gg` and `G` go by the loop.
    pub fn step(self, from: usize, count: Option<usize>, extent: Extent) -> usize {
        let times = count.unwrap_or(1);
        let beat = BEAT_STEPS as isize;
        let row = extent.row(from);
        match self {
            Self::Down => from.saturating_add(times).min(extent.last_stored()),
            Self::Up => from.saturating_sub(times),
            Self::NextBeat => extent.step_of((row.div_euclid(beat) + extent.beats(times)) * beat),
            Self::PreviousBeat => {
                let on_beat = row.rem_euclid(beat) == 0;
                let this = row.div_euclid(beat) + isize::from(!on_beat);
                extent.step_of((this - extent.beats(times)) * beat)
            }
            Self::First => count
                .map_or(extent.start, |n| {
                    extent.start.saturating_add(n.saturating_sub(1))
                })
                .min(extent.last_stored()),
            Self::Last => count
                .map_or(extent.last_played(), |n| {
                    extent.start.saturating_add(n.saturating_sub(1))
                })
                .min(extent.last_stored()),
            Self::Left
            | Self::Right
            | Self::NextGroup
            | Self::PreviousGroup
            | Self::RowStart
            | Self::RowEnd => from,
        }
    }

    /// Whether this moves between fields rather than steps.
    pub fn is_horizontal(self) -> bool {
        matches!(
            self,
            Self::Left
                | Self::Right
                | Self::NextGroup
                | Self::PreviousGroup
                | Self::RowStart
                | Self::RowEnd
        )
    }
}

/// A text-object-style target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The whole loop, in the current column.
    Column,
    /// Whole steps in this beat, and `count - 1` beats after it.
    Beat,
    /// Every step in the loop.
    Phrase,
}

impl Target {
    /// The steps and columns a target covers.
    pub fn resolve(
        self,
        step: usize,
        group: usize,
        count: Option<usize>,
        extent: Extent,
    ) -> (StepRange, Scope) {
        let whole = StepRange {
            first: extent.start.min(extent.last_stored()),
            last: extent.last_played(),
        };
        match self {
            Self::Column => (whole, Scope::column(group)),
            Self::Phrase => (whole, Scope::WHOLE),
            Self::Beat => {
                let beat = BEAT_STEPS as isize;
                let first = extent.row(step).div_euclid(beat) * beat;
                let beats = extent.beats(count.unwrap_or(1).max(1));
                let range = StepRange {
                    first: extent.step_of(first),
                    last: extent.step_of(first + beats * beat - 1),
                };
                (range, Scope::WHOLE)
            }
        }
    }
}

/// Clear the covered fields of a step. Clearing either part of an FX slot clears the slot.
pub fn clear_step(step: &mut Step, scope: Scope) {
    if scope.covers(0) {
        step.note = Note::Empty;
    }
    if scope.covers(VELOCITY_GROUP) {
        step.velocity = None;
        step.nudge = 0;
    }
    if scope.covers(GATE_GROUP) {
        step.gate = None;
    }
    for (index, slot) in step.fx.iter_mut().enumerate() {
        if scope.covers(FX_GROUP + index) {
            *slot = FxSlot::default();
        }
    }
}

/// The first value a scope covers on a step, with its slot's index.
pub fn first_value(step: &Step, scope: Scope) -> Option<FxSlot> {
    let mut slots = step.fx.iter().enumerate();
    slots
        .find(|(index, slot)| scope.covers(FX_GROUP + index) && slot.is_value())
        .map(|(_, slot)| *slot)
}

/// Set the mark on every value a scope covers on a step.
pub fn mark_step(step: &mut Step, scope: Scope, mark: Mark) {
    for (index, slot) in step.fx.iter_mut().enumerate() {
        if scope.covers(FX_GROUP + index) && slot.is_value() {
            slot.mark = mark;
            *slot = slot.with_valid_mark();
        }
    }
}

/// What a value change does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    /// `>` and `<`. Whole steps move notes only. Empty fields, `OFF`, and FX names
    /// are skipped.
    Operator,
    /// `=` and `-`. The same.
    Increment,
    /// `C-a` and `C-x`. The same, but a note moves by a semitone whatever the scale.
    Chromatic,
    /// `+` and `_`. The same again in large steps: an octave, or the top hex digit.
    Large,
    /// `=` and `-` on a value's fine digits, which carry into its coarse ones past `FF` or below
    /// `00`. Only values with a fine part move.
    Fine,
    /// `+` and `_` on a value's fine digits, by 16.
    FineLarge,
    /// `=` and `-` on a gate's length, by one. Only lengths move, and an empty one starts from the
    /// track's `Gate`, on a row with a note.
    Length,
    /// `+` and `_` on a gate's length, by 16.
    LengthLarge,
    /// `=` and `-` on a nudge, by a 24th of a step. Only nudges move, and a row without one gets
    /// one only if it has a note.
    Nudge,
    /// `+` and `_` on a nudge, by a sixth of a step, `DEL`'s unit.
    NudgeLarge,
}

/// How many 24ths of a step one large step of a nudge is: a sixth, as `DEL` counts.
const LARGE_NUDGE: i32 = 4;

/// How much one large step is for a value in hex.
const LARGE_HEX: i32 = 0x10;
/// How much one large step is for a note.
const LARGE_NOTE: i32 = 12;

/// Move an FX value's coarse digits, within what its command takes, keeping its fine part. `RET`'s
/// two digits mean separate things, a fade and a rate, so each moves alone and never carries into
/// the other, and `CND`'s moves through its conditions.
fn shift_fx(command: FxCommand, slot: FxSlot, by: i32, kind: Shift) -> u16 {
    let large = kind == Shift::Large;
    let coarse = slot.coarse();
    if command == FxCommand::Ret {
        let (fade, rate) = (i32::from(coarse >> 4), i32::from(coarse & 0x0F));
        let (fade, rate) = if large {
            ((fade + by).clamp(0, 0x0F), rate)
        } else {
            (fade, (rate + by).clamp(0, 0x0F))
        };
        return ((fade << 4 | rate) as u16) << 8;
    }
    // A condition moves through the list of them, so it never lands on a value that isn't one.
    if command == FxCommand::Cnd {
        let condition = Condition::from_value(coarse).unwrap_or(Condition::Fill(true));
        return u16::from(condition.moved(by, large).value()) << 8;
    }
    let by = if large { by * LARGE_HEX } else { by };
    let moved = i32::from(coarse) + by;
    // A phase goes round, so past either end it comes back from the other.
    let wraps = command.setting().is_some_and(|id| id.entry().kind.wraps());
    let coarse = if wraps {
        moved.rem_euclid(i32::from(command.highest()) + 1)
    } else {
        moved.clamp(0, i32::from(command.highest()))
    } as u16;
    (coarse << 8 | u16::from(slot.fine())).min(command.highest_value())
}

/// Move the covered values of a step, clamping to each field's range. With a scale, a note moves
/// through it, except by a large step, as an octave stays in key, or a chromatic one. `gate` is the
/// track's `Gate`, where a length that's empty starts.
pub fn shift_step(
    step: &mut Step,
    scope: Scope,
    by: i32,
    kind: Shift,
    scale: Option<Scale>,
    gate: u8,
) {
    let by = by.clamp(-0xFFFF, 0xFFFF);
    if matches!(kind, Shift::Nudge | Shift::NudgeLarge) {
        let by = if kind == Shift::NudgeLarge {
            by * LARGE_NUDGE
        } else {
            by
        };
        let has_note = matches!(step.note, Note::On(_));
        if scope.covers(VELOCITY_GROUP) && (step.nudge != 0 || has_note) {
            let most = i32::from(MAX_NUDGE);
            step.nudge = (i32::from(step.nudge) + by).clamp(-most, most) as i8;
        }
        return;
    }
    if matches!(kind, Shift::Length | Shift::LengthLarge) {
        if !scope.covers(GATE_GROUP) {
            return;
        }
        let by = if kind == Shift::LengthLarge {
            by * LARGE_HEX
        } else {
            by
        };
        // A length belongs to a note, so a row without one gets none, though one it has moves.
        let cell = step.gate.unwrap_or_default();
        let from = match cell.length {
            Some(length) => length,
            None if matches!(step.note, Note::On(_)) => gate,
            None => return,
        };
        step.gate = Some(Gate {
            length: Some((i32::from(from) + by).clamp(0, 0xFF) as u8),
            ..cell
        });
        return;
    }
    if matches!(kind, Shift::Fine | Shift::FineLarge) {
        let by = if kind == Shift::FineLarge {
            by * LARGE_HEX
        } else {
            by
        };
        for (index, slot) in step.fx.iter_mut().enumerate() {
            if scope.covers(FX_GROUP + index)
                && let Some(command) = slot.command.filter(|command| command.has_fine())
            {
                let moved = i32::from(slot.value) + by;
                slot.value = moved.clamp(0, i32::from(command.highest_value())) as u16;
            }
        }
        return;
    }
    let large = kind == Shift::Large;
    if scope.covers(0)
        && let Note::On(midi) = step.note
    {
        step.note = Note::On(match scale.filter(|_| !large && kind != Shift::Chromatic) {
            Some(scale) => scale.moved(midi, by),
            None => {
                let by = if large { by * LARGE_NOTE } else { by };
                (i32::from(midi) + by).clamp(i32::from(MIN_NOTE), i32::from(MAX_NOTE)) as u8
            }
        });
    }
    if scope.is_whole() {
        return;
    }
    let hex_by = if large { by * LARGE_HEX } else { by };
    if scope.covers(1) {
        let current = i32::from(step.velocity.unwrap_or(MAX_VELOCITY));
        let moved = (current + hex_by).clamp(0, i32::from(MAX_VELOCITY));
        if step.velocity.is_some() || hex_by < 0 {
            step.velocity = Some(moved as u8);
        }
    }
    // A gate's pulses move one at a time, as there are only 8. An empty cell is one pulse.
    if scope.covers(GATE_GROUP) {
        let gate = step.gate.unwrap_or_default();
        let pulses = (i32::from(gate.pulses) + by).clamp(0, i32::from(MAX_PULSES)) as u8;
        if step.gate.is_some() || pulses != gate.pulses {
            step.gate = Some(Gate { pulses, ..gate });
        }
    }
    for (index, slot) in step.fx.iter_mut().enumerate() {
        if scope.covers(FX_GROUP + index)
            && let Some(command) = slot.command
        {
            slot.value = shift_fx(command, *slot, by, kind);
        }
    }
}

/// Copied steps, and which columns of them count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub scope: Scope,
    pub steps: Vec<Step>,
    /// What came from the tracks after the first, if it covered several.
    pub other_tracks: Vec<Vec<Step>>,
}

/// Why a paste was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnMismatch;

fn kind(group: usize) -> usize {
    group.min(FX_GROUP)
}

/// Overwrite one step from a copied step, starting at a column group. Whole-step blocks replace
/// the whole step.
pub fn paste_step(
    target: &mut Step,
    source: &Step,
    scope: Scope,
    at_group: usize,
) -> Result<(), ColumnMismatch> {
    if scope.is_whole() {
        *target = source.clone();
        return Ok(());
    }
    for group in scope.first..=scope.last {
        let to = column_to(target, group, scope, at_group)?;
        match group {
            0 => target.note = source.note,
            VELOCITY_GROUP => {
                target.velocity = source.velocity;
                target.nudge = source.nudge;
            }
            GATE_GROUP => target.gate = source.gate,
            slot => {
                if let Some(value) = source.fx.get(slot - FX_GROUP) {
                    target.fx[to - FX_GROUP] = *value;
                }
            }
        }
    }
    Ok(())
}

/// Like [`paste_step`], but what is empty in the copy leaves what is on the step alone.
pub fn merge_step(
    target: &mut Step,
    source: &Step,
    scope: Scope,
    at_group: usize,
) -> Result<(), ColumnMismatch> {
    let whole = scope.is_whole();
    let last = if whole {
        FX_GROUP + source.fx.len().min(target.fx.len())
    } else {
        scope.last
    };
    for group in scope.first..=last {
        let to = if whole {
            group
        } else {
            column_to(target, group, scope, at_group)?
        };
        match group {
            0 if source.note != Note::Empty => target.note = source.note,
            VELOCITY_GROUP => {
                if source.velocity.is_some() {
                    target.velocity = source.velocity;
                }
                if source.nudge != 0 {
                    target.nudge = source.nudge;
                }
            }
            GATE_GROUP if source.gate.is_some() => target.gate = source.gate,
            0 | GATE_GROUP => {}
            slot => {
                if let Some(value) = source.fx.get(slot - FX_GROUP)
                    && value.command.is_some()
                {
                    target.fx[to - FX_GROUP] = *value;
                }
            }
        }
    }
    Ok(())
}

/// The column a copied column lands on, if it is the same kind and exists on the step.
fn column_to(
    target: &Step,
    group: usize,
    scope: Scope,
    at_group: usize,
) -> Result<usize, ColumnMismatch> {
    let to = at_group + (group - scope.first);
    if kind(group) != kind(to) || (to >= FX_GROUP && to - FX_GROUP >= target.fx.len()) {
        return Err(ColumnMismatch);
    }
    Ok(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_motions_land_on_beats_and_stop_at_edges() {
        assert_eq!(Motion::NextBeat.step(1, None, Extent::whole(16)), 4);
        assert_eq!(Motion::NextBeat.step(4, Some(2), Extent::whole(16)), 12);
        assert_eq!(Motion::NextBeat.step(13, None, Extent::whole(16)), 15);
        assert_eq!(Motion::PreviousBeat.step(5, None, Extent::whole(16)), 4);
        assert_eq!(Motion::PreviousBeat.step(4, None, Extent::whole(16)), 0);
        assert_eq!(Motion::PreviousBeat.step(1, Some(9), Extent::whole(16)), 0);
        assert_eq!(Motion::Down.step(15, Some(4), Extent::whole(16)), 15);
        assert_eq!(Motion::Up.step(1, Some(4), Extent::whole(16)), 0);
    }

    #[test]
    fn a_count_on_first_and_last_goes_to_that_step() {
        assert_eq!(Motion::Last.step(0, None, Extent::whole(16)), 15);
        assert_eq!(Motion::Last.step(0, Some(5), Extent::whole(16)), 4);
        assert_eq!(Motion::First.step(9, None, Extent::whole(16)), 0);
        assert_eq!(Motion::First.step(9, Some(99), Extent::whole(16)), 15);
    }

    #[test]
    fn targets_resolve() {
        let (range, scope) = Target::Beat.resolve(5, 1, None, Extent::whole(16));
        assert_eq!((range.first, range.last), (4, 7));
        assert!(scope.is_whole());
        let (range, _) = Target::Beat.resolve(5, 1, Some(3), Extent::whole(16));
        assert_eq!((range.first, range.last), (4, 15));
        let (range, scope) = Target::Column.resolve(5, 3, None, Extent::whole(16));
        assert_eq!((range.first, range.last), (0, 15));
        assert_eq!(scope, Scope::column(3));
    }

    /// Sixteen steps looping from stored step 6, in 32 stored.
    const SLID: Extent = Extent {
        start: 6,
        length: 16,
        stored: 32,
    };

    #[test]
    fn beats_count_from_the_loop_start() {
        assert_eq!(SLID.row(6), 0);
        assert_eq!(SLID.row(4), -2);
        assert_eq!(Motion::NextBeat.step(6, None, SLID), 10);
        assert_eq!(Motion::NextBeat.step(7, None, SLID), 10);
        assert_eq!(Motion::PreviousBeat.step(9, None, SLID), 6);
        assert_eq!(
            Motion::PreviousBeat.step(6, None, SLID),
            2,
            "above the loop"
        );
        assert_eq!(Motion::NextBeat.step(3, None, SLID), 6, "up to the loop");
        assert_eq!(
            Motion::PreviousBeat.step(1, None, SLID),
            0,
            "stops at the top"
        );
        assert_eq!(Motion::NextBeat.step(30, Some(usize::MAX), SLID), 31);
    }

    #[test]
    fn first_and_last_go_by_the_loop_and_their_count_by_row() {
        assert_eq!(Motion::First.step(20, None, SLID), 6);
        assert_eq!(Motion::Last.step(0, None, SLID), 21);
        assert_eq!(Motion::First.step(0, Some(3), SLID), 8);
        assert_eq!(Motion::Last.step(0, Some(usize::MAX), SLID), 31);
    }

    fn shifted(text: &str, by: i32, kind: Shift) -> String {
        let mut step: Step = text.parse().unwrap();
        shift_step(&mut step, Scope::column(FX_GROUP), by, kind, None, 0xFF);
        step.to_string()
    }

    fn nudged(text: &str, by: i32, kind: Shift) -> String {
        let mut step: Step = text.parse().unwrap();
        shift_step(
            &mut step,
            Scope::column(VELOCITY_GROUP),
            by,
            kind,
            None,
            0xFF,
        );
        step.to_string()
    }

    #[test]
    fn a_nudge_moves_alone_within_just_under_half_a_step() {
        assert_eq!(nudged("C-4 64", 1, Shift::Nudge), "C-4 64 +1");
        assert_eq!(nudged("C-4 64 +1", -2, Shift::Nudge), "C-4 64 -1");
        assert_eq!(
            nudged("C-4 64 -1", 1, Shift::Nudge),
            "C-4 64",
            "0 is on the grid"
        );
        assert_eq!(nudged("C-4 64", 1, Shift::NudgeLarge), "C-4 64 +4");
        assert_eq!(nudged("C-4 64 +9", 1, Shift::NudgeLarge), "C-4 64 +B");
        assert_eq!(nudged("C-4 64", -9, Shift::NudgeLarge), "C-4 64 -B");
        assert_eq!(
            nudged("--- -- cut:40", 1, Shift::Nudge),
            "--- -- cut:40",
            "a row with no note gets no nudge"
        );
        assert_eq!(
            nudged("--- -- +2 cut:40", 1, Shift::Nudge),
            "--- -- +3 cut:40",
            "but one it has moves"
        );
        // The velocity moves on its own part, and keeps the nudge.
        assert_eq!(nudged("C-4 60 +2", 1, Shift::Increment), "C-4 61 +2");
    }

    #[test]
    fn the_velocity_group_copies_and_clears_the_nudge_with_it() {
        let mut step: Step = "C-4 60 -3 SGL2".parse().unwrap();
        let source: Step = "D-4 70 +5".parse().unwrap();
        let velocity = Scope::column(VELOCITY_GROUP);
        merge_step(&mut step, &"D-4 -- +5".parse().unwrap(), velocity, 1).unwrap();
        assert_eq!(
            step.to_string(),
            "C-4 60 +5 SGL2",
            "a merge keeps what's empty"
        );
        paste_step(&mut step, &source, velocity, 1).unwrap();
        assert_eq!(step.to_string(), "C-4 70 +5 SGL2");
        clear_step(&mut step, velocity);
        assert_eq!(step.to_string(), "C-4 -- SGL2");
    }

    fn lengthened(text: &str, by: i32, kind: Shift, gate: u8) -> String {
        let mut step: Step = text.parse().unwrap();
        shift_step(&mut step, Scope::column(GATE_GROUP), by, kind, None, gate);
        step.to_string()
    }

    #[test]
    fn a_length_moves_alone_and_an_empty_one_starts_from_the_tracks_gate() {
        assert_eq!(
            lengthened("C-4 -- SGL2 80", 1, Shift::Length, 0xFF),
            "C-4 -- SGL2 81"
        );
        assert_eq!(
            lengthened("C-4 -- SGL2 80", -1, Shift::LengthLarge, 0xFF),
            "C-4 -- SGL2 70"
        );
        assert_eq!(
            lengthened("C-4 --", -1, Shift::Length, 0x80),
            "C-4 -- SGL1 7F"
        );
        assert_eq!(
            lengthened("C-4 -- SGL1 FF", 1, Shift::LengthLarge, 0x80),
            "C-4 -- SGL1 FF"
        );
        assert_eq!(
            lengthened("C-4 -- SGL1 05", -1, Shift::LengthLarge, 0x80),
            "C-4 -- SGL1 00"
        );
        assert_eq!(
            lengthened("--- -- RST4", 1, Shift::Length, 0x80),
            "--- -- RST4",
            "a row with no note gets no length"
        );
        // The pulses move on the gate's own part, and keep the length.
        assert_eq!(
            lengthened("C-4 -- SGL2 80", 1, Shift::Increment, 0xFF),
            "C-4 -- SGL3 80"
        );
    }

    #[test]
    fn a_fine_nudge_carries_into_the_coarse_digits_and_a_coarse_one_keeps_the_fine() {
        assert_eq!(shifted("--- -- cut:40FF", 1, Shift::Fine), "--- -- cut:41");
        assert_eq!(
            shifted("--- -- cut:4100", -1, Shift::Fine),
            "--- -- cut:40FF"
        );
        assert_eq!(
            shifted("--- -- cut:4012", 1, Shift::Increment),
            "--- -- cut:4112"
        );
        assert_eq!(
            shifted("--- -- cut:4012", 1, Shift::FineLarge),
            "--- -- cut:4022"
        );
        // At the top the fine part is 00, and at the bottom nothing goes below.
        assert_eq!(shifted("--- -- cut:FF", 1, Shift::Fine), "--- -- cut:FF");
        assert_eq!(
            shifted("--- -- cut:FE80", 1, Shift::Increment),
            "--- -- cut:FF"
        );
        assert_eq!(shifted("--- -- cut:00", -1, Shift::Fine), "--- -- cut:00");
        // A phase goes round.
        assert_eq!(
            shifted("--- -- lfo1.phase:FF", 1, Shift::Increment),
            "--- -- lfo1.phase:00"
        );
        assert_eq!(
            shifted("--- -- lfo1.phase:00", -1, Shift::Increment),
            "--- -- lfo1.phase:FF"
        );
        // An event or a list has no fine part, so a fine nudge leaves it.
        assert_eq!(shifted("--- -- del:10", 1, Shift::Fine), "--- -- del:10");
    }

    #[test]
    fn a_list_can_hold_but_not_glide() {
        let mut step: Step = "--- -- 1.distort.bits:03".parse().unwrap();
        mark_step(&mut step, Scope::column(FX_GROUP), Mark::Glide);
        assert_eq!(step.fx[0].mark, Mark::Lock);
        mark_step(&mut step, Scope::column(FX_GROUP), Mark::Hold);
        assert_eq!(step.fx[0].mark, Mark::Hold);
    }

    #[test]
    fn line_motions_cross_the_loop_edges() {
        assert_eq!(Motion::Down.step(21, None, SLID), 22);
        assert_eq!(Motion::Up.step(6, None, SLID), 5);
        assert_eq!(Motion::Down.step(31, Some(4), SLID), 31);
    }

    #[test]
    fn targets_cover_the_loop_and_beats_from_its_start() {
        let (range, _) = Target::Phrase.resolve(0, 0, None, SLID);
        assert_eq!((range.first, range.last), (6, 21));
        let (range, scope) = Target::Column.resolve(25, 1, None, SLID);
        assert_eq!((range.first, range.last), (6, 21));
        assert_eq!(scope, Scope::column(1));
        let (range, _) = Target::Beat.resolve(7, 0, None, SLID);
        assert_eq!((range.first, range.last), (6, 9));
        let (range, _) = Target::Beat.resolve(4, 0, None, SLID);
        assert_eq!((range.first, range.last), (2, 5), "a beat above the loop");
    }

    #[test]
    fn field_order_and_groups_round_trip() {
        for order in 0..12 {
            assert_eq!(Field::from_order(order).order(), order);
        }
        assert_eq!(Field::from_order(2), Field::Nudge);
        assert_eq!(Field::Nudge.group(), VELOCITY_GROUP);
        assert_eq!(Field::from_order(4), Field::GateLength);
        assert_eq!(Field::GateLength.group(), GATE_GROUP);
        assert_eq!(Field::from_order(7), Field::FxFine(0));
        assert_eq!(Field::from_order(8), Field::FxCommand(1));
        assert_eq!(Field::FxFine(1).group(), 4);
        assert_eq!(Field::FxValue(1).group(), 4);
        assert_eq!(Field::of_group(4), Field::FxCommand(1));
        assert_eq!(Field::of_group(GATE_GROUP), Field::Gate);
        assert_eq!(Field::from_order(3), Field::Gate);
    }
}
