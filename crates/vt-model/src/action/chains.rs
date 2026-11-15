//! Edits to chains and their rows.

use std::sync::Arc;

use crate::bloom::{Bloom, LAST_PHRASE, MAX_BRANCHES};
use crate::constants::{DEFAULT_PHRASE_STEPS, MAX_TRANSPOSE, MIN_TRANSPOSE};
use crate::generator::{Generator, Span};
use crate::scale::Scale;
use crate::selection::StepRange;
use crate::shape::{ShapeColumn, Source};
use crate::song::{Chain, ChainEntry, Phrase, Song, Track, lowest_unused};

use super::{Action, EditError, section_mut, track_mut};

/// Apply an edit to chains and their rows.
pub(super) fn edit(next: &mut Song, action: &Action) -> Result<(), EditError> {
    match action {
        Action::SetChainLabel {
            track,
            chain,
            label,
        } => {
            let owner = track_mut(next, *track)?;
            let old = owner.chains.get(chain).ok_or(EditError::NoChain(*chain))?;
            let mut labelled = Chain::clone(old);
            labelled.label.clone_from(label);
            owner.chains.insert(*chain, Arc::new(labelled));
        }
        Action::SetChainEntry {
            track,
            chain,
            slot,
            entry,
        } => {
            check_transpose(entry)?;
            ensure_phrase(track_mut(next, *track)?, entry.phrase);
            let target = chain_mut(next, *track, *chain)?
                .entries
                .get_mut(*slot)
                .ok_or(EditError::NoRow {
                    chain: *chain,
                    slot: *slot,
                })?;
            *target = *entry;
        }
        Action::InsertChainEntry {
            track,
            chain,
            at,
            entry,
        } => {
            check_transpose(entry)?;
            ensure_phrase(track_mut(next, *track)?, entry.phrase);
            let entries = &mut chain_mut(next, *track, *chain)?.entries;
            let at = (*at).min(entries.len());
            entries.insert(at, *entry);
        }
        Action::RemoveChainEntry { track, chain, at } => {
            let entries = &mut chain_mut(next, *track, *chain)?.entries;
            if entries.len() <= 1 {
                return Err(EditError::LastRow);
            }
            if *at >= entries.len() {
                return Err(EditError::NoRow {
                    chain: *chain,
                    slot: *at,
                });
            }
            entries.remove(*at);
        }
        Action::ClonePhrase {
            track,
            chain,
            slot,
            new,
        } => {
            let entry = chain_mut(next, *track, *chain)?
                .entries
                .get_mut(*slot)
                .ok_or(EditError::NoRow {
                    chain: *chain,
                    slot: *slot,
                })?;
            let old = std::mem::replace(&mut entry.phrase, *new);
            let owner = track_mut(next, *track)?;
            let copy = owner
                .phrases
                .get(&old)
                .cloned()
                .ok_or(EditError::NoPhrase(old))?;
            owner.phrases.insert(*new, copy);
        }
        Action::CloneChain { section, track } => {
            let old = section_mut(next, *section)?
                .chains
                .get(*track)
                .copied()
                .flatten()
                .ok_or(EditError::NoTrack(*track))?;
            let owner = track_mut(next, *track)?;
            let mut copy = Chain::clone(owner.chains.get(&old).ok_or(EditError::NoChain(old))?);
            // A phrase used twice in the chain stays one phrase in the copy.
            let mut made: Vec<(u32, u32)> = Vec::new();
            for entry in &mut copy.entries {
                if let Some((_, new)) = made.iter().find(|(from, _)| *from == entry.phrase) {
                    entry.phrase = *new;
                    continue;
                }
                let phrase = owner
                    .phrases
                    .get(&entry.phrase)
                    .cloned()
                    .ok_or(EditError::NoPhrase(entry.phrase))?;
                let new = lowest_unused(&owner.phrases);
                owner.phrases.insert(new, phrase);
                made.push((entry.phrase, new));
                entry.phrase = new;
            }
            let new = lowest_unused(&owner.chains);
            owner.chains.insert(new, Arc::new(copy));
            next.sections[*section].chains[*track] = Some(new);
        }
        Action::Bloom {
            track,
            chain,
            slot,
            scale,
            bloom,
        } => {
            grow_bloom(next, *track, *chain, *slot, *scale, bloom)?;
        }
        _ => unreachable!("not an edit to chains and their rows"),
    }
    Ok(())
}

/// Grow a chain row's phrase into branches, each a copy following the one before, and lay them
/// along the chain in the bloom's path.
fn grow_bloom(
    next: &mut Song,
    track: usize,
    chain: u32,
    slot: usize,
    scale: Scale,
    bloom: &Bloom,
) -> Result<(), EditError> {
    if !(1..=MAX_BRANCHES).contains(&bloom.branches) {
        return Err(EditError::Branches);
    }
    let entry = *chain_mut(next, track, chain)?
        .entries
        .get(slot)
        .ok_or(EditError::NoRow { chain, slot })?;
    let owner = track_mut(next, track)?;
    let trunk = owner
        .phrases
        .get(&entry.phrase)
        .cloned()
        .ok_or(EditError::NoPhrase(entry.phrase))?;
    let rows = StepRange::between(trunk.start(), trunk.start() + trunk.len() - 1);
    let column = ShapeColumn::Note {
        scale,
        writes: None,
    };
    let mut grown = vec![entry.phrase];
    for nth in 1..=usize::from(bloom.branches) {
        let id = lowest_unused(&owner.phrases);
        if id > LAST_PHRASE {
            return Err(EditError::NoPhrasesLeft(bloom.branches));
        }
        let before = grown[nth - 1];
        let source = Source {
            track,
            phrase: before,
        };
        let generator = Generator::new(column, Span::rows(rows), bloom.branch(source, nth));
        let read = Arc::clone(&owner.phrases[&before]);
        let mut branch = Phrase::clone(&trunk);
        branch.add_generator(generator, Some(read.loop_steps()));
        owner.phrases.insert(id, Arc::new(branch));
        grown.push(id);
    }
    let path = bloom
        .path
        .places(usize::from(bloom.branches), bloom.seed)
        .into_iter()
        .map(|place| ChainEntry {
            phrase: grown[place],
            transpose: entry.transpose,
        });
    chain_mut(next, track, chain)?
        .entries
        .splice(slot..=slot, path);
    Ok(())
}

fn check_transpose(entry: &ChainEntry) -> Result<(), EditError> {
    if (MIN_TRANSPOSE..=MAX_TRANSPOSE).contains(&entry.transpose) {
        Ok(())
    } else {
        Err(EditError::Transpose)
    }
}

fn ensure_phrase(track: &mut Track, phrase: u32) {
    track
        .phrases
        .entry(phrase)
        .or_insert_with(|| Arc::new(Phrase::new(DEFAULT_PHRASE_STEPS)));
}

fn chain_mut(song: &mut Song, track: usize, chain: u32) -> Result<&mut Chain, EditError> {
    let target = track_mut(song, track)?
        .chains
        .get_mut(&chain)
        .ok_or(EditError::NoChain(chain))?;
    Ok(Arc::make_mut(target))
}
