//! A kit line's grammar (#1093, decision 464): a `Mini`'s line across a group's drum tracks,
//! `bd*2 [. sn] hh?`, whose hits are the names of the tracks it spans, matched without case. It
//! reads as a `Mini`'s does, the rhythm's own reading (`rhythm.rs`, which credits Tidal's
//! `ParseBP.hs`), so `[ ]`, `< >`, `*`, `!`, `@`, `?` and `(k,n,r)` mean what they mean there.
//!
//! Each track's generator holds the whole line and plays the rows where its own name is, so a
//! stack, `bd*4, hh*8`, is natural: each layer is other tracks'. A name in two layers is refused,
//! as one track plays one layer. A name no track of the line has doesn't read where the line is
//! edited, but plays nothing rather than stopping the rest, so a track that leaves the line
//! leaves its tokens in the text, marked.

use super::mini::{START_ROWS, layers, split, stacked};
use super::rhythm::{self, Atoms, bad, changes, numbers, offer_of, read_line};
use super::{
    About, Around, Bad, Bracket, Grammar, Kind, MAX_MINI, Offer, STACKED, Spot, Token, as_it_is,
    groups, nth_of,
};
use crate::shape::{Atom, Beat, ShapeColumn, What};

pub(super) static KIT: Grammar = Grammar {
    kinds: &[
        Kind::Hit,
        Kind::Rest,
        Kind::Modifier,
        Kind::Value,
        Kind::Open(Bracket::Square),
        Kind::Close(Bracket::Square),
        Kind::Open(Bracket::Angle),
        Kind::Close(Bracket::Angle),
        Kind::Open(Bracket::Round),
        Kind::Close(Bracket::Round),
        Kind::Unread,
    ],
    split,
    read: |text, _, names| read(text, names),
    meaning: |kind| match kind {
        Kind::Hit => {
            "a track of the kit, by its name: it plays its note on the row the name lands on, at \
             the velocity notes are typed with"
        }
        Kind::Rest => "a rest: no track plays on its rows",
        Kind::Modifier => {
            "a change to the token before it: *n plays it n times in its place, !n gives it n \
             places, @n makes it n places long, ? plays it half the time and ?30 30% of it"
        }
        Kind::Value => "one of a Euclid's numbers: its hits, its parts, then how many parts later",
        Kind::Open(Bracket::Square) | Kind::Close(Bracket::Square) => {
            "a group: its tokens share its place"
        }
        Kind::Open(Bracket::Angle) | Kind::Close(Bracket::Angle) => {
            "a choice: one of its tokens each lap, the next each time the line comes round"
        }
        Kind::Open(Bracket::Round) | Kind::Close(Bracket::Round) => {
            "a Euclid's numbers: (3,8) spreads 3 hits over 8 parts of the token's place, and \
             (3,8,2) moves them 2 parts later"
        }
        _ => "what isn't a track of the line, a rest, a group or a change",
    },
    about: |kind, _, _| {
        let name = match kind {
            Kind::Hit => "Track",
            Kind::Rest => "Rest",
            Kind::Modifier => "Change",
            Kind::Value => "Euclid's number",
            Kind::Open(Bracket::Square) | Kind::Close(Bracket::Square) => "Group",
            Kind::Open(Bracket::Angle) | Kind::Close(Bracket::Angle) => "Choice",
            Kind::Open(Bracket::Round) | Kind::Close(Bracket::Round) => "Euclid",
            _ => "Unread",
        };
        About {
            name: name.to_owned(),
            subject: String::new(),
        }
    },
    turn,
    offer,
    // A track's table holds the line's hits by their place in the whole line, its own layer's
    // counted on from the layers before it, so a field of the whole line lights its own.
    rows: |tokens, at| nth_of(tokens, at, Kind::Hit),
    fixed: |_| None,
    blank: ".",
    rest: ".",
    most: None,
    width: None,
    between: " ",
    types: |_| false,
    from_setting: as_it_is,
    to_setting: as_it_is,
};

/// Whether a hit's word is a track's name, without case.
pub fn is_name(word: &str, name: &str) -> bool {
    !name.is_empty() && word.to_lowercase() == name.to_lowercase()
}

/// The layer a track's name is in, by its place and with the hits of the layers before it, or
/// none where it's in none. A name in two layers is refused, saying where the second is.
fn layer_of<'a>(text: &'a str, name: &str) -> Result<Option<(usize, usize, &'a str)>, Bad> {
    let mut found = None;
    let mut before = 0;
    for (nth, (from, layer)) in layers(text).into_iter().enumerate() {
        let tokens = split(layer);
        let mine = tokens
            .iter()
            .find(|token| token.kind == Kind::Hit && is_name(&layer[token.at.clone()], name));
        if let Some(token) = mine {
            if found.is_some() {
                return Err(bad(
                    from + token.at.start,
                    format!("{name} is in two layers: a track plays one, so it's in one"),
                ));
            }
            found = Some((nth, before, layer));
        }
        before += tokens
            .iter()
            .filter(|token| token.kind == Kind::Hit)
            .count();
    }
    Ok(found)
}

/// Every layer reads, each hit a name of the line's tracks, and no name in two layers, or where
/// and why it doesn't.
fn read(text: &str, names: &[String]) -> Result<(), Bad> {
    if text.len() > MAX_MINI {
        return Err(bad(
            MAX_MINI,
            format!("A line is at most {MAX_MINI} characters"),
        ));
    }
    for (from, layer) in layers(text) {
        if let Some(at) = stacked(layer) {
            return Err(bad(from + at, STACKED));
        }
        let mut each = |word: &str, at: usize| {
            if names.iter().any(|name| is_name(word, name)) {
                Ok(())
            } else {
                Err(bad(
                    at,
                    format!("{word} isn't a track of the line: K lists them"),
                ))
            }
        };
        let reading: Atoms = &mut each;
        read_line(&KIT, layer, reading).map_err(|wrong| Bad {
            at: from + wrong.at,
            why: wrong.why,
        })?;
    }
    for name in names {
        layer_of(text, name)?;
    }
    Ok(())
}

/// A line read for the track named `name`: the beats of the layer its name is in, its hits
/// counted on from the layers before it, and for each hit of the line up to its last, whether
/// it's this track's, [`Atom::Own`], or another's, [`Atom::Other`]. A track in no layer plays
/// nothing; a word that's no track's is another's, so the rest play on.
pub fn compile(text: &str, name: &str) -> Result<(Vec<Beat>, Vec<Atom>), Bad> {
    if text.len() > MAX_MINI {
        return Err(bad(
            MAX_MINI,
            format!("A line is at most {MAX_MINI} characters"),
        ));
    }
    let Some((_, before, layer)) = layer_of(text, name)? else {
        return Ok((Vec::new(), Vec::new()));
    };
    if let Some(at) = stacked(layer) {
        return Err(bad(at, STACKED));
    }
    let mut atoms = vec![Atom::Other; before];
    let mut each = |word: &str, _: usize| {
        atoms.push(if is_name(word, name) {
            Atom::Own
        } else {
            Atom::Other
        });
        Ok(())
    };
    let reading: Atoms = &mut each;
    let mut beats = read_line(&KIT, layer, reading)?;
    let before = u8::try_from(before).unwrap_or(u8::MAX);
    for beat in &mut beats {
        if let What::Hit(hit) = &mut beat.what {
            *hit = hit.saturating_add(before);
        }
    }
    Ok((beats, atoms))
}

/// A line with a track's name changed in it, each of its hits of the old name, without case,
/// the new one, as `:name` renames them.
pub fn rename(text: &str, old: &str, new: &str) -> String {
    let mut renamed = text.to_owned();
    let mut tokens: Vec<Token> = split(text)
        .into_iter()
        .filter(|token| token.kind == Kind::Hit && is_name(&text[token.at.clone()], old))
        .collect();
    tokens.reverse();
    for token in tokens {
        renamed.replace_range(token.at, new);
    }
    renamed
}

/// The line a kit starts from, its tracks' names and the rows each has a note on: a token a
/// row, each the name of the track with a note there, or a rest; where two tracks share a row, a
/// layer a track; up to [`START_ROWS`] rows, the rests at the end kept so the line goes round
/// with its rows. With no notes, or a line too long, one of the names' own.
pub fn start(names: &[String], notes: &[Vec<bool>]) -> String {
    let rows = notes
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .min(START_ROWS);
    let has = |track: usize, row: usize| notes[track].get(row).copied().unwrap_or(false);
    let shared = (0..rows).any(|row| (0..notes.len()).filter(|&t| has(t, row)).count() > 1);
    // A layer of nothing but rests is none.
    let trimmed = |words: Vec<&str>| {
        if words.iter().all(|word| *word == ".") {
            String::new()
        } else {
            words.join(" ")
        }
    };
    let line = if shared {
        let layers: Vec<String> = (0..notes.len().min(names.len()))
            .map(|track| {
                trimmed(
                    (0..rows)
                        .map(|row| {
                            if has(track, row) {
                                names[track].as_str()
                            } else {
                                "."
                            }
                        })
                        .collect(),
                )
            })
            .filter(|layer| !layer.is_empty())
            .collect();
        layers.join(", ")
    } else {
        trimmed(
            (0..rows)
                .map(|row| {
                    (0..notes.len().min(names.len()))
                        .find(|&track| has(track, row))
                        .map_or(".", |track| names[track].as_str())
                })
                .collect(),
        )
    };
    if !line.is_empty() && line.len() <= MAX_MINI {
        return line;
    }
    match names {
        [] => String::new(),
        [one] => format!("{one} . {one} ."),
        [a, b] => format!("{a} . {b} . {a} {a} {b} ."),
        [a, b, c, ..] => format!("{a} . {b} . {a} {a} {b} ., {c}!8"),
    }
}

/// A name turned to the next of the line's, or the one before, and any other token as a
/// rhythm's turns.
fn turn(
    kind: Kind,
    text: &str,
    sign: i16,
    large: bool,
    column: ShapeColumn,
    names: &[String],
) -> Option<String> {
    if kind != Kind::Hit {
        return rhythm::turn(kind, text, sign, large, column);
    }
    let last = names.len().checked_sub(1)?;
    let now = names.iter().position(|name| is_name(text, name));
    let next = match now {
        Some(now) => (now as i16 + sign).clamp(0, last as i16) as usize,
        None => 0,
    };
    Some(names[next].clone())
}

/// The line's tracks by name, then a rest and the groups, and after a token the changes it can
/// take, as a `Mini`'s are listed.
fn offer(spot: Spot, around: &Around) -> Vec<Offer> {
    let tracks = || {
        around
            .tracks
            .iter()
            .map(|name| offer_of(name, "this track's note"))
            .collect::<Vec<_>>()
    };
    let rest_and_groups = || {
        let mut offers = vec![offer_of(".", "a rest")];
        offers.extend(groups(".").into_iter().filter(|offer| offer.text != "."));
        offers
    };
    match spot {
        Spot::On {
            kind: Kind::Open(_) | Kind::Close(_),
            ..
        } => groups("."),
        Spot::On {
            kind: Kind::Modifier,
            ..
        } => changes(),
        Spot::On {
            kind: Kind::Value, ..
        } => numbers(),
        Spot::On {
            kind: Kind::Hit, ..
        } => tracks(),
        Spot::On { .. } | Spot::After(None) => {
            let mut offers = tracks();
            offers.extend(rest_and_groups());
            offers
        }
        Spot::After(Some(_)) => {
            let mut offers = tracks();
            offers.extend(rest_and_groups());
            offers.extend(changes());
            offers
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::Scale;

    fn notes() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale::default(),
            writes: Some(0x64),
        }
    }

    fn kit() -> Vec<String> {
        ["BD", "SN", "HH"].map(str::to_owned).to_vec()
    }

    /// The names read without case; a name the line doesn't span doesn't, saying where.
    #[test]
    fn hits_are_the_tracks_names_without_case() {
        assert_eq!(read("bd*2 [. Sn] hh?", &kit()), Ok(()));
        assert_eq!(
            read("bd cp", &kit()),
            Err(bad(3, "cp isn't a track of the line: K lists them"))
        );
        assert!(read("bd cp", &[]).is_err(), "with no tracks, no name reads");
        assert_eq!(
            read("bd [sn, hh]", &kit()).map_err(|bad| bad.why),
            Err(STACKED.to_owned())
        );
    }

    /// Each track plays its own hits; another's are the line's but write nothing.
    #[test]
    fn a_track_plays_the_hits_of_its_name() {
        let (beats, atoms) = compile("bd [. sn] BD hh", "bd").unwrap();
        assert_eq!(atoms, [Atom::Own, Atom::Other, Atom::Own, Atom::Other]);
        let hits = beats
            .iter()
            .filter(|beat| matches!(beat.what, What::Hit(_)))
            .count();
        assert_eq!(hits, atoms.len());
        let (beats, atoms) = compile("bd sn", "cp").unwrap();
        assert!(beats.is_empty() && atoms.is_empty(), "no layer, nothing");
    }

    /// A stack's layers go to their tracks, a later layer's hits counted on from the first's, so
    /// each track's table and the field of the whole line agree.
    #[test]
    fn a_stack_gives_each_track_its_layer() {
        let line = "bd . sn ., hh*4 hh";
        let (beats, atoms) = compile(line, "hh").unwrap();
        assert_eq!(atoms, [Atom::Other, Atom::Other, Atom::Own, Atom::Own]);
        let first = beats.iter().find_map(|beat| match beat.what {
            What::Hit(hit) => Some(hit),
            _ => None,
        });
        assert_eq!(first, Some(2));
        let tokens = super::super::split(&KIT, line);
        let hh = tokens
            .iter()
            .position(|token| &line[token.at.clone()] == "hh")
            .unwrap();
        assert_eq!((KIT.rows)(&tokens, hh), Some(2));
        assert_eq!(
            read("bd hh, hh", &kit()).map_err(|bad| bad.at),
            Err(7),
            "a name in two layers"
        );
    }

    #[test]
    fn a_rename_changes_its_tokens_and_no_other() {
        assert_eq!(
            rename("bd [. Bd*2] sn, bd", "BD", "KK"),
            "KK [. KK*2] sn, KK"
        );
        assert_eq!(rename("bdx bd", "bd", "k"), "bdx k");
    }

    #[test]
    fn a_name_turns_to_the_next_track() {
        let names = kit();
        let turn = |text, sign| turn(Kind::Hit, text, sign, false, notes(), &names);
        assert_eq!(turn("bd", 1).as_deref(), Some("SN"));
        assert_eq!(turn("HH", 1).as_deref(), Some("HH"));
        assert_eq!(turn("bd", -1).as_deref(), Some("BD"));
        assert_eq!(
            super::turn(Kind::Modifier, "*2", 1, false, notes(), &names).as_deref(),
            Some("*3")
        );
    }

    #[test]
    fn k_offers_the_lines_tracks() {
        let names = kit();
        let around = Around {
            column: notes(),
            from: 48,
            to: 72,
            tracks: &names,
            reached: &[],
        };
        let after = offer(Spot::After(Some((Kind::Hit, "BD"))), &around);
        let texts: Vec<&str> = after.iter().map(|offer| offer.text.as_str()).collect();
        assert_eq!(&texts[..3], ["BD", "SN", "HH"]);
        assert!(texts.contains(&"*2") && texts.contains(&"[ ]"));
    }

    #[test]
    fn a_kit_starts_from_its_rows() {
        let names = kit();
        let row = |hits: &[usize]| (0..8).map(|row| hits.contains(&row)).collect::<Vec<_>>();
        assert_eq!(
            start(&names, &[row(&[0, 4]), row(&[2, 6]), row(&[])]),
            "BD . SN . BD . SN ."
        );
        assert_eq!(
            start(&names, &[row(&[0]), row(&[0, 2]), row(&[])]),
            "BD . . . . . . ., SN . SN . . . . ."
        );
        assert_eq!(
            start(&names, &[row(&[]), row(&[]), row(&[])]),
            "BD . SN . BD BD SN ., HH!8"
        );
    }
}
