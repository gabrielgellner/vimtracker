//! Generators held from their track's macros, and live `Follow`s written again, as part of every
//! edit.

use std::sync::Arc;

use super::EditError;
use crate::generator::{Generator, reads};
use crate::shape::{FillKind, Shape, Source};
use crate::song::{Phrase, Song};
use crate::step::Step;

/// A generator to hold, by its place in its phrase, as this shape, or to let go with none.
type Hold = (usize, Option<Shape>);

/// Have every live generator of a track whose macros hold any of its settings written from what
/// they give, and every one they've let go of from its own again, as part of the edit: so a turn of
/// a macro, a mapping or a reshape of the generator is heard at once, as an edit, until the engine
/// renders generators itself. A phrase nothing changes for is left as it is, shared with the
/// version before. A song read from its file is held here too, as it loads (`crate::project`), so
/// its generators are heard as held, and the engine left at rest, before the first edit.
pub fn hold_generators(song: &mut Song) {
    for index in 0..song.tracks.len() {
        let track = &song.tracks[index];
        let mapped = track.has_gen_maps();
        let held = || {
            track
                .phrases
                .values()
                .any(|phrase| phrase.generators().iter().any(Generator::is_held))
        };
        if !mapped && !held() {
            continue;
        }
        let due: Vec<(u32, Vec<Hold>)> = track
            .phrases
            .iter()
            .filter_map(|(id, phrase)| {
                let sources = sources_of(song, phrase);
                let changes: Vec<Hold> = phrase
                    .generators()
                    .iter()
                    .enumerate()
                    .filter(|(_, generator)| mapped || generator.is_held())
                    .filter_map(|(at, generator)| {
                        let source = generator
                            .source()
                            .and_then(|source| looked_up(&sources, source));
                        let held = track
                            .held_shape(generator, phrase.steps(), source)
                            .filter(|held| *held != generator.shape);
                        (held != generator.held()).then_some((at, held))
                    })
                    .collect();
                (!changes.is_empty()).then_some((*id, changes))
            })
            .collect();
        for (id, changes) in due {
            let Some(found) = song.tracks[index].phrases.get(&id) else {
                continue;
            };
            let sources = sources_of(song, found);
            let looked: Vec<Option<Vec<Step>>> = changes
                .iter()
                .map(|(at, _)| {
                    found.generators()[*at]
                        .source()
                        .and_then(|source| looked_up(&sources, source))
                        .map(<[Step]>::to_vec)
                })
                .collect();
            let phrase = Arc::make_mut(
                song.tracks[index]
                    .phrases
                    .get_mut(&id)
                    .expect("found above"),
            );
            for ((at, held), source) in changes.into_iter().zip(looked) {
                phrase.hold_generator(at, held, source.as_deref());
            }
        }
    }
}

/// The phrase a `Follow` shape reads, to copy it once, or none for any other shape.
pub(super) fn followed(song: &Song, shape: &Shape) -> Result<Option<Arc<Phrase>>, EditError> {
    if shape.kind != FillKind::Follow {
        return Ok(None);
    }
    let source = shape.follow.source;
    let found = song
        .source_phrase(source)
        .ok_or(EditError::NoSource(source))?;
    Ok(Some(Arc::clone(found)))
}

/// The phrase a live `Follow` on a phrase reads, refused if it's that phrase or reads it, itself
/// or through what it follows, so no edit goes round a cycle.
pub(super) fn followed_live(
    song: &Song,
    shape: &Shape,
    track: usize,
    phrase: u32,
) -> Result<Option<Arc<Phrase>>, EditError> {
    let found = followed(song, shape)?;
    let here = Source { track, phrase };
    let source = shape.follow.source;
    if found.is_some() && source == here {
        return Err(EditError::FollowsItself);
    }
    if found.is_some() && reads(song, source, here) {
        return Err(EditError::FollowCycle(source));
    }
    Ok(found)
}

/// Let the live generators of every phrase an edit changed follow it: one whose cells it changed
/// by hand is applied, and one whose notes it changed rerenders, as part of the same edit. Then
/// every `Follow` of a phrase it changed, on whichever track, writes it again, and so on along
/// phrases that follow those, so a source and all its followers are one edit and one undo; a
/// `Follow` whose source has gone, with its track, is applied.
///
/// Every phrase is passed over by pointer to find the ones the edit changed, so the phrases that
/// follow another are gathered in the same pass rather than kept in an index, which every way of
/// changing a song would have to keep in step.
pub(super) fn follow_generators(before: &Song, after: &mut Song) {
    let mut changed: Vec<Source> = Vec::new();
    let mut followers: Vec<Source> = Vec::new();
    for (index, track) in after.tracks.iter().enumerate() {
        let old = before.tracks.get(index);
        for (id, phrase) in &track.phrases {
            let here = Source {
                track: index,
                phrase: *id,
            };
            let kept = old
                .and_then(|old| old.phrases.get(id))
                .is_some_and(|old| Arc::ptr_eq(old, phrase));
            if !kept {
                changed.push(here);
            }
            if phrase.generators().iter().any(|g| g.source().is_some()) {
                followers.push(here);
            }
        }
    }
    // A track gone takes its phrases with it, which counts as changing them.
    let sound = before.sound_tracks();
    for (index, track) in before.tracks.iter().enumerate().take(sound) {
        for id in track.phrases.keys() {
            let here = Source {
                track: index,
                phrase: *id,
            };
            if after.source_phrase(here).is_none() && !changed.contains(&here) {
                changed.push(here);
            }
        }
    }
    for here in changed.clone() {
        let Some(phrase) = after
            .tracks
            .get(here.track)
            .and_then(|track| track.phrases.get(&here.phrase))
        else {
            continue;
        };
        if phrase.generators().is_empty() {
            continue;
        }
        let sources = sources_of(after, phrase);
        let phrase = after.tracks[here.track]
            .phrases
            .get_mut(&here.phrase)
            .expect("found above");
        Arc::make_mut(phrase).follow_generators(|source| looked_up(&sources, source));
    }
    if followers.is_empty() {
        return;
    }
    // Each follower after the followers it follows, so a change goes all the way along.
    let mut done = vec![false; followers.len()];
    loop {
        let mut moved = false;
        for nth in 0..followers.len() {
            if done[nth] {
                continue;
            }
            let here = followers[nth];
            let reading: Vec<Source> = after.tracks[here.track]
                .phrases
                .get(&here.phrase)
                .map(|phrase| {
                    phrase
                        .generators()
                        .iter()
                        .filter_map(Generator::source)
                        .collect()
                })
                .unwrap_or_default();
            let waiting = followers
                .iter()
                .zip(&done)
                .any(|(other, finished)| !finished && *other != here && reading.contains(other));
            if waiting {
                continue;
            }
            done[nth] = true;
            moved = true;
            if refollow(after, here, &changed) && !changed.contains(&here) {
                changed.push(here);
            }
        }
        if !moved {
            break;
        }
    }
}

/// The phrases a phrase's live `Follow`s read, as they are now.
fn sources_of(song: &Song, phrase: &Phrase) -> Vec<(Source, Arc<Phrase>)> {
    phrase
        .generators()
        .iter()
        .filter_map(Generator::source)
        .filter_map(|source| Some((source, Arc::clone(song.source_phrase(source)?))))
        .collect()
}

fn looked_up(sources: &[(Source, Arc<Phrase>)], source: Source) -> Option<&[Step]> {
    sources
        .iter()
        .find(|(found, _)| *found == source)
        .map(|(_, phrase)| phrase.loop_steps())
}

/// Write a phrase's `Follow`s again where the phrase they follow changed, and apply those whose
/// phrase has gone: true if that changed the phrase.
fn refollow(song: &mut Song, here: Source, changed: &[Source]) -> bool {
    let Some(phrase) = song.tracks[here.track].phrases.get(&here.phrase) else {
        return false;
    };
    let sources = sources_of(song, phrase);
    let due: Vec<(usize, Option<Source>)> = phrase
        .generators()
        .iter()
        .enumerate()
        .filter_map(|(index, generator)| {
            let source = generator.source()?;
            let gone = looked_up(&sources, source).is_none();
            (gone || changed.contains(&source)).then_some((index, (!gone).then_some(source)))
        })
        .collect();
    if due.is_empty() {
        return false;
    }
    let phrase = Arc::make_mut(
        song.tracks[here.track]
            .phrases
            .get_mut(&here.phrase)
            .expect("found above"),
    );
    let mut wrote = false;
    // From the last, so applying one leaves the places of those before it as they were. Applying
    // leaves the steps as they are.
    for (index, source) in due.into_iter().rev() {
        match source.and_then(|source| looked_up(&sources, source)) {
            Some(steps) => wrote |= phrase.refollow(index, steps),
            None => {
                phrase.apply_generator(index);
            }
        }
    }
    wrote
}
