//! Edits to the song's sections and the chain each track plays in them.

use crate::song::Song;

use super::{Action, EditError, section_mut, track_mut};

/// Apply an edit to the song's sections and the chain each track plays in them.
pub(super) fn edit(next: &mut Song, action: &Action) -> Result<(), EditError> {
    match action {
        Action::SetReset { section, reset } => {
            if *reset == Some(0) {
                return Err(EditError::Reset);
            }
            section_mut(next, *section)?.reset = *reset;
        }
        Action::AddSection { at } => next.insert_section(*at),
        Action::CloneSection { at } => {
            let copy = section_mut(next, *at)?.clone();
            next.sections.insert(*at + 1, copy);
        }
        Action::RemoveSection { at } => {
            if next.sections.len() <= 1 {
                return Err(EditError::LastSection);
            }
            section_mut(next, *at)?;
            next.sections.remove(*at);
        }
        Action::SetSectionLength { section, length } => {
            if *length == Some(0) {
                return Err(EditError::Reset);
            }
            section_mut(next, *section)?.length = *length;
        }
        Action::SetSectionRepeat { section, repeat } => {
            section_mut(next, *section)?.repeat = *repeat;
        }
        Action::SetSectionStart { section, start } => {
            section_mut(next, *section)?.start = *start;
        }
        Action::SetSectionScale { section, scale } => {
            section_mut(next, *section)?.scale = *scale;
        }
        Action::SetSectionChain {
            section,
            track,
            chain,
        } => {
            let owner = track_mut(next, *track)?;
            if let Some(chain) = chain
                && !owner.chains.contains_key(chain)
            {
                owner.add_chain(*chain);
            }
            *section_mut(next, *section)?
                .chains
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))? = *chain;
        }
        Action::SetTrackChain {
            section,
            track,
            chain,
        } => {
            let owner = track_mut(next, *track)?;
            if !owner.chains.contains_key(chain) {
                owner.add_chain(*chain);
            }
            *section_mut(next, *section)?
                .chains
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))? = Some(*chain);
        }
        _ => unreachable!(
            "not an edit to the song's sections and the chain each track plays in them"
        ),
    }
    Ok(())
}
