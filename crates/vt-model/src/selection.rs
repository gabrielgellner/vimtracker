//! Motions, targets and what operators do to the steps they cover. No terminal is needed here.

use crate::constants::BEAT_STEPS;
use crate::step::{FxSlot, MAX_NOTE, MIN_NOTE, Note, Step};

/// One editable part of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Note,
    Velocity,
    Instrument,
    FxCommand(usize),
    FxValue(usize),
}

impl Field {
    /// The column group this field belongs to: note, velocity, instrument, then each FX slot.
    pub fn group(self) -> usize {
        match self {
            Self::Note => 0,
            Self::Velocity => 1,
            Self::Instrument => 2,
            Self::FxCommand(slot) | Self::FxValue(slot) => 3 + slot,
        }
    }

    /// The first field in a column group.
    pub fn of_group(group: usize) -> Self {
        match group {
            0 => Self::Note,
            1 => Self::Velocity,
            2 => Self::Instrument,
            slot => Self::FxCommand(slot - 3),
        }
    }

    /// This field's position in `h l` order.
    pub fn order(self) -> usize {
        match self {
            Self::Note => 0,
            Self::Velocity => 1,
            Self::Instrument => 2,
            Self::FxCommand(slot) => 3 + slot * 2,
            Self::FxValue(slot) => 4 + slot * 2,
        }
    }

    /// The field at a position in `h l` order.
    pub fn from_order(order: usize) -> Self {
        match order {
            0 => Self::Note,
            1 => Self::Velocity,
            2 => Self::Instrument,
            n if (n - 3).is_multiple_of(2) => Self::FxCommand((n - 3) / 2),
            n => Self::FxValue((n - 4) / 2),
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
}

impl Motion {
    /// Where a vertical motion lands, or the same step for a horizontal one. Motions stop at the
    /// edge, with no wrapping.
    pub fn step(self, from: usize, count: Option<usize>, length: usize) -> usize {
        let last = length.saturating_sub(1);
        let times = count.unwrap_or(1);
        match self {
            Self::Down => from.saturating_add(times).min(last),
            Self::Up => from.saturating_sub(times),
            Self::NextBeat => {
                let beat = from / BEAT_STEPS;
                beat.saturating_add(times)
                    .saturating_mul(BEAT_STEPS)
                    .min(last)
            }
            Self::PreviousBeat => {
                let on_beat = from.is_multiple_of(BEAT_STEPS);
                let beat = from / BEAT_STEPS + usize::from(!on_beat);
                beat.saturating_sub(times) * BEAT_STEPS
            }
            Self::First => count.map_or(0, |n| n.saturating_sub(1)).min(last),
            Self::Last => count.map_or(last, |n| n.saturating_sub(1)).min(last),
            Self::Left | Self::Right | Self::NextGroup | Self::PreviousGroup => from,
        }
    }

    /// Whether this moves between fields rather than steps.
    pub fn is_horizontal(self) -> bool {
        matches!(
            self,
            Self::Left | Self::Right | Self::NextGroup | Self::PreviousGroup
        )
    }
}

/// A text-object-style target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The whole phrase, in the current column.
    Column,
    /// Whole steps in this beat, and `count - 1` beats after it.
    Beat,
    /// Every step in the phrase.
    Phrase,
}

impl Target {
    /// The steps and columns a target covers.
    pub fn resolve(
        self,
        step: usize,
        group: usize,
        count: Option<usize>,
        length: usize,
    ) -> (StepRange, Scope) {
        let last = length.saturating_sub(1);
        match self {
            Self::Column => (StepRange { first: 0, last }, Scope::column(group)),
            Self::Phrase => (StepRange { first: 0, last }, Scope::WHOLE),
            Self::Beat => {
                let first = step / BEAT_STEPS * BEAT_STEPS;
                let beats = count.unwrap_or(1).max(1);
                let end = first
                    .saturating_add(beats.saturating_mul(BEAT_STEPS))
                    .saturating_sub(1)
                    .min(last);
                (StepRange { first, last: end }, Scope::WHOLE)
            }
        }
    }
}

/// Clear the covered fields of a step. Clearing either part of an FX slot clears the slot.
pub fn clear_step(step: &mut Step, scope: Scope) {
    if scope.covers(0) {
        step.note = Note::Empty;
    }
    if scope.covers(1) {
        step.velocity = None;
    }
    if scope.covers(2) {
        step.instrument = None;
    }
    for (index, slot) in step.fx.iter_mut().enumerate() {
        if scope.covers(3 + index) {
            *slot = FxSlot::default();
        }
    }
}

/// What a value change does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    /// `>` and `<`. Whole steps move notes only. Empty fields, `OFF`, FX names and instrument IDs
    /// are skipped.
    Operator,
    /// `+` and `-`. The same, but instrument IDs change too.
    Increment,
}

/// Move the covered values of a step, clamping to each field's range.
pub fn shift_step(step: &mut Step, scope: Scope, by: i32, kind: Shift) {
    if scope.covers(0)
        && let Note::On(midi) = step.note
    {
        let moved = (i32::from(midi) + by).clamp(i32::from(MIN_NOTE), i32::from(MAX_NOTE));
        step.note = Note::On(moved as u8);
    }
    if scope.is_whole() {
        return;
    }
    if scope.covers(1) {
        let current = i32::from(step.velocity.unwrap_or(0xFF));
        let moved = (current + by).clamp(0, 0xFF);
        if step.velocity.is_some() || by < 0 {
            step.velocity = Some(moved as u8);
        }
    }
    if scope.covers(2)
        && kind == Shift::Increment
        && let Some(instrument) = step.instrument
    {
        step.instrument = Some((i64::from(instrument) + i64::from(by)).max(0) as u32);
    }
    for (index, slot) in step.fx.iter_mut().enumerate() {
        if scope.covers(3 + index) && slot.command.is_some() {
            slot.value = (i32::from(slot.value) + by).clamp(0, 0xFF) as u8;
        }
    }
}

/// Copied steps, and which columns of them count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub scope: Scope,
    pub steps: Vec<Step>,
}

/// Why a paste was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnMismatch;

fn kind(group: usize) -> usize {
    group.min(3)
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
        let to = at_group + (group - scope.first);
        if kind(group) != kind(to) || (to >= 3 && to - 3 >= target.fx.len()) {
            return Err(ColumnMismatch);
        }
        match group {
            0 => target.note = source.note,
            1 => target.velocity = source.velocity,
            2 => target.instrument = source.instrument,
            slot => {
                if let Some(value) = source.fx.get(slot - 3) {
                    target.fx[to - 3] = *value;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_motions_land_on_beats_and_stop_at_edges() {
        assert_eq!(Motion::NextBeat.step(1, None, 16), 4);
        assert_eq!(Motion::NextBeat.step(4, Some(2), 16), 12);
        assert_eq!(Motion::NextBeat.step(13, None, 16), 15);
        assert_eq!(Motion::PreviousBeat.step(5, None, 16), 4);
        assert_eq!(Motion::PreviousBeat.step(4, None, 16), 0);
        assert_eq!(Motion::PreviousBeat.step(1, Some(9), 16), 0);
        assert_eq!(Motion::Down.step(15, Some(4), 16), 15);
        assert_eq!(Motion::Up.step(1, Some(4), 16), 0);
    }

    #[test]
    fn a_count_on_first_and_last_goes_to_that_step() {
        assert_eq!(Motion::Last.step(0, None, 16), 15);
        assert_eq!(Motion::Last.step(0, Some(5), 16), 4);
        assert_eq!(Motion::First.step(9, None, 16), 0);
        assert_eq!(Motion::First.step(9, Some(99), 16), 15);
    }

    #[test]
    fn targets_resolve() {
        let (range, scope) = Target::Beat.resolve(5, 1, None, 16);
        assert_eq!((range.first, range.last), (4, 7));
        assert!(scope.is_whole());
        let (range, _) = Target::Beat.resolve(5, 1, Some(3), 16);
        assert_eq!((range.first, range.last), (4, 15));
        let (range, scope) = Target::Column.resolve(5, 3, None, 16);
        assert_eq!((range.first, range.last), (0, 15));
        assert_eq!(scope, Scope::column(3));
    }

    #[test]
    fn field_order_and_groups_round_trip() {
        for order in 0..9 {
            assert_eq!(Field::from_order(order).order(), order);
        }
        assert_eq!(Field::FxValue(1).group(), 4);
        assert_eq!(Field::of_group(4), Field::FxCommand(1));
    }
}
