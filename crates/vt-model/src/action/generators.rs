//! Generators held from their track's macros, and live `Follow`s written again, as part of every
//! edit.

use std::sync::Arc;

use super::EditError;
use crate::generator::{Generator, Span, reads};
use crate::notation::{is_name, kit_rename};
use crate::scale::Scale;
use crate::shape::{FillKind, KitLine, Own, Shape, Source};
use crate::song::{Phrase, Song, Track};
use crate::step::{Note, Step};

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
        let mapped = song.has_gen_maps(index);
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
                        let held = song
                            .held_shape(index, generator, phrase.steps(), source)
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

/// One of a kit line's generators: its track, its phrase, its place in the phrase's list and
/// its line.
#[derive(Debug, Clone, Copy)]
struct Member {
    track: usize,
    phrase: u32,
    at: usize,
    kit: KitLine,
}

/// What a kit line keeps the same on every track: its text, its `Step`, its seed, its `Vary` and
/// its `Mutate`, which decide where its hits land. Each track keeps its own note, `From`.
fn line_of(generator: &Generator) -> (&str, u8, u64, u8, u8) {
    let shape = &generator.shape;
    (
        generator.rhythm(),
        shape.mini.step,
        shape.seed,
        shape.vary,
        shape.mutate,
    )
}

/// The generator a member is, in a song.
fn member_of(song: &Song, member: Member) -> Option<&Generator> {
    song.tracks
        .get(member.track)?
        .phrases
        .get(&member.phrase)?
        .generators()
        .get(member.at)
}

/// Every kit line's generators in a song, by track, phrase and place.
fn members(song: &Song) -> Vec<Member> {
    let mut members = Vec::new();
    for (track, sound) in song.tracks.iter().enumerate() {
        for (id, phrase) in &sound.phrases {
            for (at, generator) in phrase.generators().iter().enumerate() {
                if let (Some(kit), FillKind::Mini) = (generator.shape.kit, generator.shape.kind) {
                    members.push(Member {
                        track,
                        phrase: *id,
                        at,
                        kit,
                    });
                }
            }
        }
    }
    members
}

/// Whether a member's track is a sound track of its line's group.
fn in_group(song: &Song, member: &Member) -> bool {
    let track = &song.tracks[member.track];
    track.is_sound() && track.group == Some(member.kit.group)
}

/// Whether a track has a generator on this kit line.
fn on_line(track: &Track, kit: KitLine) -> bool {
    track.phrases.values().any(|phrase| {
        phrase
            .generators()
            .iter()
            .any(|generator| generator.shape.kit == Some(kit))
    })
}

/// A track's own name, or why a kit line can't play it: it has none (decision 472).
fn named(song: &Song, track: usize) -> Result<(), EditError> {
    if song.tracks[track].name.is_empty() {
        return Err(EditError::Unnamed(format!("track {:02}", track + 1)));
    }
    Ok(())
}

/// Refuse a kit line made on a track with no name, and have a track that joins a group with a
/// kit line join the line, as part of the edit (decision 472). The track joining gets the line
/// over the same rows of the phrase it plays where each of the line's does, at the first note in
/// those rows or else the line's `From`, and plays where its name is typed, nothing until then;
/// one with no name is refused, as its number can't be a token. A track already in the group
/// with no generator on the line, as `dd` left it, is left out.
pub(super) fn join_kit_lines(before: &Song, after: &mut Song) -> Result<(), EditError> {
    let members = members(after);
    if members.is_empty() {
        return Ok(());
    }
    for member in members.iter().filter(|member| in_group(after, member)) {
        let had = before
            .tracks
            .get(member.track)
            .is_some_and(|was| on_line(was, member.kit));
        if !had {
            named(after, member.track)?;
        }
    }
    let mut lines: Vec<KitLine> = members.iter().map(|member| member.kit).collect();
    lines.sort_by_key(|kit| (kit.group, kit.line));
    lines.dedup();
    for kit in lines {
        let line: Vec<Member> = members
            .iter()
            .filter(|member| member.kit == kit && in_group(after, member))
            .copied()
            .collect();
        let joining: Vec<usize> = (0..after.tracks.len())
            .filter(|&index| {
                let track = &after.tracks[index];
                let was_in = before
                    .tracks
                    .get(index)
                    .is_some_and(|was| was.is_sound() && was.group == Some(kit.group));
                track.is_sound()
                    && track.group == Some(kit.group)
                    && !was_in
                    && !on_line(track, kit)
            })
            .collect();
        if line.is_empty() {
            continue;
        }
        for track in joining {
            named(after, track)?;
            join(after, &line, track);
        }
    }
    Ok(())
}

/// Put a kit line on a track joining it, on each phrase it plays where one of the line's
/// generators is played.
fn join(song: &mut Song, line: &[Member], track: usize) {
    let mut done: Vec<u32> = Vec::new();
    for member in line {
        let Some(phrase) = phrase_beside(song, *member, track) else {
            continue;
        };
        let Some(generator) = member_of(song, *member) else {
            continue;
        };
        if done.contains(&phrase) {
            continue;
        }
        done.push(phrase);
        let (column, span, shape) = (generator.column, generator.span, generator.shape);
        let text = generator.rhythm().to_owned();
        let Some(found) = song.tracks[track].phrases.get_mut(&phrase) else {
            continue;
        };
        let found = Arc::make_mut(found);
        let rows = match span {
            Span::Rows { first, last } => first..=last,
            Span::Lane => continue,
        };
        let from = rows
            .filter_map(|row| match found.steps().get(row)?.note {
                Note::On(note) => Some(note),
                _ => None,
            })
            .next()
            .unwrap_or(shape.from);
        let joined = Generator::new(column, span, Shape { from, ..shape }).with_rhythm(&text);
        found.add_generator(joined, None);
    }
}

/// The phrase a track plays where a kit line's generator is played: in the first section whose
/// chain on the member's track plays its phrase, the phrase at the same place in the track's own
/// chain there.
fn phrase_beside(song: &Song, member: Member, track: usize) -> Option<u32> {
    let theirs = &song.tracks[member.track];
    let mine = &song.tracks[track];
    song.sections.iter().find_map(|section| {
        let chain = theirs.chains.get(&(*section.chains.get(member.track)?)?)?;
        let place = chain
            .entries
            .iter()
            .position(|entry| entry.phrase == member.phrase)?;
        let own = mine.chains.get(&(*section.chains.get(track)?)?)?;
        let phrase = own.entries.get(place)?.phrase;
        mine.phrases.contains_key(&phrase).then_some(phrase)
    })
}

/// Keep every kit line one line across its tracks, as part of the edit (decision 464): a track
/// that has left the line's group has its generator applied, leaving its rows as they were
/// heard; a track renamed has its name's tokens renamed in the text; and an edit to the line
/// on one track, its text, `Step`, seed, `Vary` or `Mutate`, is made on every track it spans.
/// Each generator is given its track's name, which it plays the hits of. With no `before`, as a
/// song loads, each takes its track's name and nothing else changes.
pub fn keep_kit_lines(before: Option<&Song>, after: &mut Song) {
    let members = members(after);
    if members.is_empty() {
        return;
    }
    let (left, kept): (Vec<Member>, Vec<Member>) = members
        .into_iter()
        .partition(|member| !in_group(after, member));
    // From the last of a phrase's, then the rest found again, as their places may have moved.
    if !left.is_empty() {
        for member in left.iter().rev() {
            if let Some(phrase) = after.tracks[member.track].phrases.get_mut(&member.phrase) {
                Arc::make_mut(phrase).apply_generator(member.at);
            }
        }
        keep_kit_lines(before, after);
        return;
    }
    let mut lines: Vec<KitLine> = kept.iter().map(|member| member.kit).collect();
    lines.sort_by_key(|kit| (kit.group, kit.line));
    lines.dedup();
    for kit in lines {
        let line: Vec<Member> = kept.iter().filter(|m| m.kit == kit).copied().collect();
        keep_line(before, after, &line);
    }
}

/// One kit line's members made one line, from the one an edit changed, or a new one, or the
/// first.
fn keep_line(before: Option<&Song>, after: &mut Song, line: &[Member]) {
    let was = |member: &Member| {
        before
            .and_then(|before| member_of(before, *member))
            .filter(|generator| generator.shape.kit == Some(member.kit))
    };
    let Some(first) = line.first() else {
        return;
    };
    let changed = line.iter().find(|member| {
        let (Some(now), Some(was)) = (member_of(after, **member), was(member)) else {
            return false;
        };
        line_of(now) != line_of(was)
    });
    let new = || line.iter().find(|member| was(member).is_none());
    let source = changed.or_else(new).unwrap_or(first);
    let Some(generator) = member_of(after, *source) else {
        return;
    };
    let (text, step, seed, vary, mutate) = line_of(generator);
    let mut text = text.to_owned();
    // A track renamed has its tokens renamed with it.
    for member in line {
        let Some(generator) = member_of(after, *member) else {
            continue;
        };
        let label = after.tracks[member.track].label(member.track);
        let old = generator.kit_name();
        if !old.is_empty() && !is_name(old, &label) {
            text = kit_rename(&text, old, &label);
        }
    }
    for member in line {
        let Some(generator) = member_of(after, *member) else {
            continue;
        };
        let label = after.tracks[member.track].label(member.track);
        let shape = Shape {
            seed,
            vary,
            mutate,
            mini: Own {
                step,
                ..generator.shape.mini
            },
            ..generator.shape
        };
        let same = generator.rhythm() == text && shape == generator.shape;
        if same && *generator.kit_name() == *label {
            continue;
        }
        let Some(phrase) = after.tracks[member.track].phrases.get_mut(&member.phrase) else {
            continue;
        };
        let phrase = Arc::make_mut(phrase);
        phrase.name_kit_generator(member.at, &label);
        if !same {
            phrase.reshape_generator(member.at, shape, (None, Some(&text)), None);
        }
    }
}

/// The song with every live generator on a Note column going through this scale, its notes
/// written again, and every `Follow` of a phrase that changes written again after it, or none if
/// they all go through it already. A live generator's notes follow the song's scale as it is now
/// (decision 360), and the scale is changed outside undo, so this isn't an edit: whoever changes
/// the scale puts the result in the place of the version it was made from. A phrase with no such
/// generator is left as it is, shared with the song given.
pub fn rescale_generators(song: &Song, scale: Scale) -> Option<Song> {
    let stale = |phrase: &Phrase| {
        phrase
            .generators()
            .iter()
            .any(|generator| generator.scale().is_some_and(|its| its != scale))
    };
    if !song
        .tracks
        .iter()
        .any(|track| track.phrases.values().any(|phrase| stale(phrase)))
    {
        return None;
    }
    let mut next = song.clone();
    for index in 0..next.tracks.len() {
        let due: Vec<u32> = next.tracks[index]
            .phrases
            .iter()
            .filter(|(_, phrase)| stale(phrase))
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            let Some(found) = next.tracks[index].phrases.get(&id) else {
                continue;
            };
            // Each `Follow` from its source as it is: one whose source changes too is written
            // again below, after it.
            let sources = sources_of(&next, found);
            let looked: Vec<Option<Vec<Step>>> = found
                .generators()
                .iter()
                .map(|generator| {
                    generator
                        .source()
                        .and_then(|source| looked_up(&sources, source))
                        .map(<[Step]>::to_vec)
                })
                .collect();
            let phrase = Arc::make_mut(
                next.tracks[index]
                    .phrases
                    .get_mut(&id)
                    .expect("found above"),
            );
            for (at, source) in looked.into_iter().enumerate() {
                phrase.rescale_generator(at, scale, source.as_deref());
            }
        }
    }
    follow_generators(song, &mut next);
    Some(next)
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
/// Every phrase is passed over to find the ones the edit changed, by its track's pointer and then
/// its own, so the phrases that follow another are gathered in the same pass rather than kept in
/// an index, which every way of changing a song would have to keep in step.
pub(super) fn follow_generators(before: &Song, after: &mut Song) {
    let mut changed: Vec<Source> = Vec::new();
    let mut followers: Vec<Source> = Vec::new();
    for (index, track) in after.tracks.iter().enumerate() {
        let old = before.tracks.get(index);
        // A track the edit left shared has every phrase it had, so none is looked up.
        let shared = after.tracks.same(index, &before.tracks, index);
        for (id, phrase) in &track.phrases {
            let here = Source {
                track: index,
                phrase: *id,
            };
            let kept = shared
                || old
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
    let still = after.sound_tracks();
    for (index, track) in before.tracks.iter().enumerate().take(sound) {
        // One still shared, and still a sound track, has lost none.
        if index < still && after.tracks.same(index, &before.tracks, index) {
            continue;
        }
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
