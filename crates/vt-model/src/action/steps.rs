//! Edits to what is on a phrase's steps.

use crate::selection::{StepRange, clear_step, mark_step, merge_step, paste_step, shift_step};
use crate::shape::{OwnRhythm, written_from};
use crate::song::Song;
use crate::step::{FxCommand, FxSlot, Gate, Note, Step};

use super::generators::followed;
use super::{Action, EditError, FieldValue, Fill, phrase_mut, track_mut};

/// Apply an edit to what is on a phrase's steps.
pub(super) fn edit(next: &mut Song, action: &Action) -> Result<(), EditError> {
    match action {
        Action::SetNote {
            track,
            phrase,
            step,
            note,
        } => {
            step_mut(next, *track, *phrase, *step)?.note = *note;
        }
        Action::SetStep {
            track,
            phrase,
            step,
            value,
        } => {
            *step_mut(next, *track, *phrase, *step)? = value.clone();
        }
        Action::SetField {
            track,
            phrase,
            step,
            value,
        } => {
            let most = most_nudge(next, *track)?;
            let target = step_mut(next, *track, *phrase, *step)?;
            match value {
                FieldValue::Velocity(velocity) => target.velocity = *velocity,
                FieldValue::Nudge(nudge) => target.nudge = (*nudge).clamp(-most, most),
                FieldValue::Gate(gate) => target.gate = *gate,
                FieldValue::Slot(slot, fx) => {
                    *target.fx.get_mut(*slot).ok_or(EditError::ColumnMismatch)? = snapped(*fx);
                }
            }
        }
        Action::Shape {
            track,
            phrase,
            range,
            column,
            shape,
            list,
            rhythm,
        } => {
            let found = track_mut(next, *track)?
                .phrases
                .get(phrase)
                .cloned()
                .ok_or(EditError::NoPhrase(*phrase))?;
            // Once, as plain notes, a phrase may copy its own loop.
            let source = followed(next, shape)?;
            let source = source.as_ref().map(|source| source.loop_steps());
            let mut shape = *shape;
            // A kit line's track plays the hits of its own name.
            let name = next.tracks[*track].label(*track);
            let rhythm = OwnRhythm::kept_as(&shape, *column, rhythm, &name);
            rhythm.settle(&mut shape);
            let texts = rhythm.texts(list);
            for (row, step) in written_from(found.steps(), *range, *column, &shape, texts, source) {
                *step_mut(next, *track, *phrase, row)? = step;
            }
        }
        Action::Fill {
            track,
            phrase,
            range,
            fill,
        } => {
            fill_range(next, *track, *phrase, *range, fill)?;
        }
        Action::SetMark {
            track,
            phrase,
            range,
            scope,
            mark,
        } => {
            for index in range.first..=range.last {
                mark_step(step_mut(next, *track, *phrase, index)?, *scope, *mark);
            }
        }
        Action::Clear {
            track,
            phrase,
            range,
            scope,
        } => {
            for index in range.first..=range.last {
                clear_step(step_mut(next, *track, *phrase, index)?, *scope);
            }
        }
        Action::Shift {
            track,
            phrase,
            range,
            scope,
            by,
            kind,
            scale,
        } => {
            // An empty length moves from the track's `Gate`, as that's what it plays at.
            let owner = next.tracks.get(*track).ok_or(EditError::NoTrack(*track))?;
            let (gate, most) = (owner.gate_length(), owner.speed.most_nudge());
            for index in range.first..=range.last {
                shift_step(
                    step_mut(next, *track, *phrase, index)?,
                    *scope,
                    *by,
                    *kind,
                    *scale,
                    gate,
                    most,
                );
            }
        }
        Action::FitToScale {
            track,
            phrase,
            scale,
        } => {
            let stored = next
                .tracks
                .get(*track)
                .and_then(|state| state.phrases.get(phrase))
                .map_or(0, |found| found.steps().len());
            for index in 0..stored {
                let step = step_mut(next, *track, *phrase, index)?;
                if let Note::On(midi) = step.note {
                    step.note = Note::On(scale.fitted(midi));
                }
            }
        }
        Action::Paste {
            track,
            phrase,
            at,
            group,
            block,
            times,
            merge,
        } => {
            let target = phrase_mut(next, *track, *phrase)?;
            let write = if *merge { merge_step } else { paste_step };
            let total = block.steps.len().saturating_mul(*times);
            target.grow_to(at.saturating_add(total));
            let sources = block.steps.iter().cycle().take(total);
            for (offset, source) in sources.enumerate() {
                let Some(step) = target.step_mut(at + offset) else {
                    break;
                };
                write(step, source, block.scope, *group).map_err(|_| EditError::ColumnMismatch)?;
            }
        }
        _ => unreachable!("not an edit to what is on a phrase's steps"),
    }
    Ok(())
}

/// An FX slot with its value within what its command takes, and a fine part only on what has
/// one, or on an `SLI` locking a slice by its letter.
fn snapped(slot: FxSlot) -> FxSlot {
    if let Some(letter) = slot.slice_mark() {
        return FxSlot::slice_by_mark(letter, slot.mark).with_valid_mark();
    }
    let highest = slot.command.map_or(0xFF00, FxCommand::highest_value);
    let value = if slot.command.is_some_and(FxCommand::has_fine) {
        slot.value
    } else {
        slot.value & 0xFF00
    };
    FxSlot {
        value: value.min(highest),
        ..slot
    }
    .with_valid_mark()
}

/// Write one value across a selection of a phrase's rows.
fn fill_range(
    next: &mut Song,
    track: usize,
    phrase: u32,
    range: StepRange,
    fill: &Fill,
) -> Result<(), EditError> {
    let most = most_nudge(next, track)?;
    for index in range.first..=range.last {
        let target = step_mut(next, track, phrase, index)?;
        let has_note = matches!(target.note, Note::On(_));
        match fill {
            Fill::Note { note, velocity } => {
                target.note = *note;
                if matches!(note, Note::On(_)) {
                    target.velocity.get_or_insert(*velocity);
                }
            }
            Fill::Velocity(velocity) if has_note => target.velocity = *velocity,
            Fill::Gate(gate) => target.gate = *gate,
            Fill::GateLength(length) if has_note => {
                if length.is_some() || target.gate.is_some() {
                    target.gate = Some(Gate {
                        length: *length,
                        ..target.gate.unwrap_or_default()
                    });
                }
            }
            Fill::Nudge(nudge) if has_note || *nudge == 0 => {
                target.nudge = (*nudge).clamp(-most, most);
            }
            // A velocity, a gate length or a nudge is left off a row without a note.
            Fill::Velocity(_) | Fill::GateLength(_) | Fill::Nudge(_) => {}
            Fill::Slot(slot, value) => {
                *target.fx.get_mut(*slot).ok_or(EditError::ColumnMismatch)? = snapped(*value);
            }
            Fill::SlotFine(slot, fine) => {
                let slot = target.fx.get_mut(*slot).ok_or(EditError::ColumnMismatch)?;
                if slot.command.is_some_and(FxCommand::has_fine) {
                    *slot = snapped(FxSlot {
                        value: slot.value & 0xFF00 | u16::from(*fine),
                        ..*slot
                    });
                }
            }
            // Typing coarse digits keeps the fine part, and over an `SLI` by a letter
            // locks the slice by its number instead.
            Fill::SlotValue(slot, value) => {
                let slot = target.fx.get_mut(*slot).ok_or(EditError::ColumnMismatch)?;
                if let Some(command) = slot.command {
                    let fine = if command.has_fine() { slot.fine() } else { 0 };
                    *slot = snapped(FxSlot {
                        value: u16::from(*value) << 8 | u16::from(fine),
                        ..*slot
                    });
                }
            }
        }
    }
    Ok(())
}

/// The furthest a track's nudges go at its speed, which a nudge written to it is kept within.
fn most_nudge(song: &Song, track: usize) -> Result<i8, EditError> {
    let owner = song.tracks.get(track).ok_or(EditError::NoTrack(track))?;
    Ok(owner.speed.most_nudge())
}

fn step_mut(
    song: &mut Song,
    track: usize,
    phrase: u32,
    step: usize,
) -> Result<&mut Step, EditError> {
    phrase_mut(song, track, phrase)?
        .step_mut(step)
        .ok_or(EditError::NoStep { phrase, step })
}
