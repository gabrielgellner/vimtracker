//! Edits to a phrase as a whole: its length, loop, order and lanes, and its live generators.

use crate::generator::Generator;
use crate::song::Song;

use super::generators::followed_live;
use super::{Action, EditError, phrase_mut};

/// Apply an edit to a phrase as a whole: its length, loop, order and lanes, and its live generators.
pub(super) fn edit(next: &mut Song, action: &Action) -> Result<(), EditError> {
    match action {
        Action::Generate {
            track,
            phrase,
            column,
            span,
            shape,
            list,
        } => {
            let source = followed_live(next, shape, *track, *phrase)?;
            let source = source.as_ref().map(|source| source.loop_steps());
            let generator = Generator::new(*column, *span, *shape).with_list(list);
            if !phrase_mut(next, *track, *phrase)?.add_generator(generator, source) {
                return Err(EditError::NoLane);
            }
        }
        Action::Reshape {
            track,
            phrase,
            generator,
            shape,
            list,
        } => {
            let source = followed_live(next, shape, *track, *phrase)?;
            let source = source.as_ref().map(|source| source.loop_steps());
            let list = list.as_deref();
            let found = phrase_mut(next, *track, *phrase)?;
            if !found.reshape_generator(*generator, *shape, list, source) {
                return Err(EditError::NoGenerator {
                    phrase: *phrase,
                    generator: *generator,
                });
            }
        }
        Action::ApplyGenerator {
            track,
            phrase,
            generator,
        } => {
            phrase_mut(next, *track, *phrase)?
                .apply_generator(*generator)
                .ok_or(EditError::NoGenerator {
                    phrase: *phrase,
                    generator: *generator,
                })?;
        }
        Action::RemoveGenerator {
            track,
            phrase,
            generator,
        } => {
            phrase_mut(next, *track, *phrase)?
                .remove_generator(*generator)
                .ok_or(EditError::NoGenerator {
                    phrase: *phrase,
                    generator: *generator,
                })?;
        }
        Action::Double { track, phrase } => {
            phrase_mut(next, *track, *phrase)?.double();
        }
        Action::SetLength {
            track,
            phrase,
            length,
        } => {
            phrase_mut(next, *track, *phrase)?.set_len(*length);
        }
        Action::SlideLoop { track, phrase, by } => {
            phrase_mut(next, *track, *phrase)?.slide(*by);
        }
        Action::SetOrder {
            track,
            phrase,
            order,
        } => {
            phrase_mut(next, *track, *phrase)?.set_order(*order);
        }
        Action::SetLane {
            track,
            phrase,
            slot,
            lane,
        } => {
            phrase_mut(next, *track, *phrase)?.set_lane(*slot, *lane);
        }
        Action::RotateLoop { track, phrase, by } => {
            phrase_mut(next, *track, *phrase)?.rotate(*by);
        }
        _ => unreachable!(
            "not an edit to a phrase as a whole: its length, loop, order and lanes, and its live generators"
        ),
    }
    Ok(())
}
