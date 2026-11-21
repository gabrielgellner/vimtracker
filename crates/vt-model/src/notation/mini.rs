//! `Mini`'s grammar (#1092, decision 462): an `Own` rhythm's line, Tidal's mini-notation read
//! stepwise, whose hits are the column's own values rather than `x`: a note by name, `C-3` or
//! `c3`, or a scale degree from 1 on the Note column, both in one line; hex on Velocity, Length
//! and an FX column, `_` on Length tying into the next row, `HLD`; a signed nudge on Nudge; and
//! on an FX column whose values `K` names, a `CND`'s conditions and an `ACU`'s amounts, by name.
//! The FX command is the column's, chosen with `K` before, and never typed.
//!
//! The reading is the rhythm's own (`rhythm.rs`, which credits Tidal's `ParseBP.hs`), so `[ ]`,
//! `< >`, `*`, `!`, `@`, `?` and `(k,n,r)` mean what they mean there; what's new is a hit's text
//! read as a value, in the same walk. A stack, `,`, puts one line over another: one track's
//! Note column holds one, so a line with a stack is a layer a track, across the tracks of a `W`
//! selection or of `S` (the study's "Mono"), and [`compile`] reads one layer.

use super::rhythm::{self, Atoms, bad, changes, ends, modifies, numbers, offer_of, read_line};
use super::{
    About, Around, Bad, Bracket, Grammar, Kind, Offer, Spot, Token, as_it_is, degrees, groups,
    is_rest, notes_round, nth_of, values,
};
use crate::accumulator::{MOST_DEGREES, amount, amount_value};
use crate::condition::Condition;
use crate::selection::StepRange;
use crate::shape::{Atom, Beat, ShapeColumn, degree_count};
use crate::step::{FxCommand, Step};

/// The longest line a `Mini` takes, in bytes, its stack's layers and all.
pub const MAX_MINI: usize = 256;
/// The most rows a `Mini` starts its line from.
pub(crate) const START_ROWS: usize = 16;
/// The most a degree is, from 1.
const MOST_DEGREE: u8 = 64;
/// A semitone and an octave, as a note turns with the scale off.
const OCTAVE: i16 = 12;

/// What's said of a stack on one track, or inside a group.
pub const STACKED: &str =
    "A stack , is a layer a track: select tracks with W, or S across them, for one each";

pub(super) static MINI: Grammar = Grammar {
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
    read: |text, column, _| read(text, column),
    meaning: |kind| match kind {
        Kind::Hit => {
            "the value it writes on the row it lands on, in the {}'s units: on the Note column a \
             note, C-3, or a degree from 1, the root"
        }
        Kind::Rest => "a rest: nothing on its rows",
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
        _ => "what isn't a value of the column, a rest, a group or a change",
    },
    about: |kind, _, column| {
        let name = match kind {
            Kind::Hit => column_name(column),
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
            subject: column_word(column),
        }
    },
    turn: |kind, text, sign, large, column, _| turn(kind, text, sign, large, column),
    offer,
    // Its table holds each row's hit, by its place among the hits, as a rhythm's does.
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

/// A hit's name on the status line, by its column.
fn column_name(column: ShapeColumn) -> &'static str {
    match column {
        ShapeColumn::Note { .. } => "Note",
        ShapeColumn::Velocity => "Velocity",
        ShapeColumn::Nudge { .. } => "Nudge",
        ShapeColumn::Length => "Length",
        ShapeColumn::Fx { .. } => "Value",
    }
}

/// The column as a meaning names it.
fn column_word(column: ShapeColumn) -> String {
    match column {
        ShapeColumn::Fx { command, .. } => command.name(),
        other => column_name(other).to_lowercase(),
    }
}

/// Brackets, rests and modifiers as a rhythm's are; a word, up to the next space, comma,
/// bracket, rest or modifier, a hit; and inside a Euclid's `( )`, its numbers.
pub(super) fn split(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut round = 0usize;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if c.is_whitespace() || c == ',' {
            continue;
        }
        if let Some((bracket, opens)) = Bracket::of(c) {
            if bracket == Bracket::Round {
                round = if opens {
                    round + 1
                } else {
                    round.saturating_sub(1)
                };
            }
            let kind = if opens {
                Kind::Open(bracket)
            } else {
                Kind::Close(bracket)
            };
            tokens.push(Token::new(kind, at..at + 1));
            continue;
        }
        if is_rest(&text[at..at + c.len_utf8()]) {
            tokens.push(Token::new(Kind::Rest, at..at + 1));
            continue;
        }
        let digits = modifies(c);
        let mut end = at + c.len_utf8();
        while let Some((next, more)) = chars.next_if(|(_, more)| {
            if digits {
                more.is_ascii_digit()
            } else {
                !ends(*more) && !modifies(*more) && !matches!(more, '.' | '~')
            }
        }) {
            end = next + more.len_utf8();
        }
        let word = &text[at..end];
        let kind = if digits {
            Kind::Modifier
        } else if round > 0 && word.bytes().all(|byte| byte.is_ascii_digit()) {
            Kind::Value
        } else if round > 0 {
            Kind::Unread
        } else {
            Kind::Hit
        };
        tokens.push(Token::new(kind, at..end));
    }
    tokens
}

/// A line's layers, each with where it starts: the text between its stack's commas, outside any
/// bracket, or the whole for a line with no stack.
pub fn layers(text: &str) -> Vec<(usize, &str)> {
    let mut layers = Vec::new();
    let (mut depth, mut from) = (0usize, 0);
    for (at, c) in text.char_indices() {
        match Bracket::of(c) {
            Some((_, true)) => depth += 1,
            Some((_, false)) => depth = depth.saturating_sub(1),
            None if c == ',' && depth == 0 => {
                layers.push((from, &text[from..at]));
                from = at + 1;
            }
            None => {}
        }
    }
    layers.push((from, &text[from..]));
    layers
}

/// The layer of a line the `nth` of the tracks it's given plays, going round its layers, and
/// trimmed: the whole line where it has no stack.
pub fn layer(text: &str, nth: usize) -> &str {
    let layers = layers(text);
    layers[nth % layers.len()].1.trim()
}

/// The line a `Mini` starts from on a column: what's in its rows, a token a row, a rest where
/// there's nothing, up to [`START_ROWS`] and without the rests at the end; or on a column with
/// nothing in them, a line of the column's own, as a `List` starts from its two ends.
pub fn start(steps: &[Step], range: StepRange, column: ShapeColumn) -> String {
    let mut words: Vec<String> = (range.first..=range.last)
        .take(START_ROWS)
        .map(|row| {
            steps
                .get(row)
                .and_then(|step| column.value(step))
                .map_or_else(|| ".".to_owned(), |value| shown(value, column))
        })
        .collect();
    while words.last().is_some_and(|word| word == ".") {
        words.pop();
    }
    if !words.is_empty() {
        return words.join(" ");
    }
    match column {
        ShapeColumn::Note { .. } => "1 . 3 5 . 3 [5 8] .".to_owned(),
        ShapeColumn::Velocity => "7F 40 60 40".to_owned(),
        ShapeColumn::Nudge { .. } => "0 +4 0 -4".to_owned(),
        ShapeColumn::Length => "40 _ 80 20".to_owned(),
        ShapeColumn::Fx { .. } => {
            let named = values(column);
            let pick = |at: usize| named[at * (named.len() - 1) / 3].text.clone();
            format!("{} [{} {}] {}", pick(0), pick(1), pick(2), pick(3))
        }
    }
}

/// Where a comma stacks one line over another, other than in a Euclid's `( )`, if one does.
pub(super) fn stacked(text: &str) -> Option<usize> {
    let mut round = 0usize;
    for (at, c) in text.char_indices() {
        match c {
            '(' => round += 1,
            ')' => round = round.saturating_sub(1),
            ',' if round == 0 => return Some(at),
            _ => {}
        }
    }
    None
}

/// Whether a line stacks layers at its top, a layer for each of the tracks it's given.
pub fn stacks(text: &str) -> bool {
    layers(text).len() > 1
}

/// Every layer of a line reads, on a column, or where and why one doesn't: a stack in a group is
/// refused, as only a line's own layers go a track each.
fn read(text: &str, column: ShapeColumn) -> Result<(), Bad> {
    if text.len() > MAX_MINI {
        return Err(bad(
            MAX_MINI,
            format!("A line is at most {MAX_MINI} characters"),
        ));
    }
    for (from, layer) in layers(text) {
        compile(layer, column).map_err(|wrong| Bad {
            at: from + wrong.at,
            why: wrong.why,
        })?;
    }
    Ok(())
}

/// One layer read into its beats and a value for each hit, on a column, or where and why it
/// doesn't read. A stack is refused here, as one track holds one layer.
pub fn compile(text: &str, column: ShapeColumn) -> Result<(Vec<Beat>, Vec<Atom>), Bad> {
    if text.len() > MAX_MINI {
        return Err(bad(
            MAX_MINI,
            format!("A line is at most {MAX_MINI} characters"),
        ));
    }
    if let Some(at) = stacked(text) {
        return Err(bad(at, STACKED));
    }
    let mut atoms = Vec::new();
    let mut each = |word: &str, at: usize| {
        atoms.push(atom(word, column).map_err(|why| bad(at, why))?);
        Ok(())
    };
    let reading: Atoms = &mut each;
    let beats = read_line(&MINI, text, reading)?;
    Ok((beats, atoms))
}

/// A hit's text as a value of its column, or why it isn't one.
pub fn atom(word: &str, column: ShapeColumn) -> Result<Atom, String> {
    match column {
        ShapeColumn::Note { .. } => {
            if word.bytes().all(|byte| byte.is_ascii_digit()) {
                return match word.parse::<u8>() {
                    Ok(degree @ 1..=MOST_DEGREE) => Ok(Atom::Degree(degree - 1)),
                    _ => Err(format!("A degree goes from 1, the root, to {MOST_DEGREE}")),
                };
            }
            column
                .parse(word)
                .map(Atom::Value)
                .ok_or_else(|| format!("{word} isn't a note, like C-3, or a degree, like 3"))
        }
        ShapeColumn::Length if word == "_" => Ok(Atom::Tie),
        _ if word == "_" => Err("_ ties a note into the next, on the Length column".to_owned()),
        ShapeColumn::Nudge { .. } => column
            .parse(word)
            .map(Atom::Value)
            .ok_or_else(|| format!("{word} isn't a nudge, like -3, 0 or +B")),
        ShapeColumn::Fx {
            command: FxCommand::Cnd,
            ..
        } => Condition::all()
            .find(|condition| condition.label().eq_ignore_ascii_case(word))
            .map(|condition| Atom::Value(condition.value()))
            .ok_or_else(|| format!("{word} isn't a condition: K lists them")),
        ShapeColumn::Fx {
            command: FxCommand::Acu,
            ..
        } => word
            .parse::<i8>()
            .ok()
            .filter(|moved| moved.unsigned_abs() <= MOST_DEGREES && *moved != 0)
            .map(|moved| Atom::Value(amount_value(moved)))
            .ok_or_else(|| format!("{word} isn't an amount, like +3 or -2: K lists them")),
        _ => {
            let value = u8::from_str_radix(word, 16)
                .ok()
                .filter(|value| *value <= column.highest());
            value.map(Atom::Value).ok_or_else(|| {
                format!(
                    "{word} isn't a hex value, from 00 to {:02X}",
                    column.highest()
                )
            })
        }
    }
}

/// A value as its token shows it on a column: a note by name, a nudge with its sign, a
/// condition or an amount by name, anything else in hex.
fn shown(value: u8, column: ShapeColumn) -> String {
    match column {
        ShapeColumn::Fx {
            command: FxCommand::Cnd,
            ..
        } => Condition::from_value(value).map_or_else(|| format!("{value:02X}"), |c| c.label()),
        ShapeColumn::Fx {
            command: FxCommand::Acu,
            ..
        } => amount(value).map_or_else(|| format!("{value:02X}"), |moved| format!("{moved:+}")),
        _ => column.show(value),
    }
}

/// A token turned: a note by a degree of the scale, or a semitone with it off, and by an octave
/// where `large`; a degree by one, or by the scale's length; a named value to the next name; any
/// other value by 1 or `10`; and a change or a Euclid's number as a rhythm's turn.
fn turn(kind: Kind, text: &str, sign: i16, large: bool, column: ShapeColumn) -> Option<String> {
    if kind != Kind::Hit {
        return rhythm::turn(kind, text, sign, large, column);
    }
    match atom(text, column).ok()? {
        Atom::Tie | Atom::Own | Atom::Other => None,
        Atom::Degree(degree) => {
            let ShapeColumn::Note { scale, .. } = column else {
                return None;
            };
            let by = if large { degree_count(scale) as i16 } else { 1 };
            let turned = (i16::from(degree) + sign * by).clamp(0, i16::from(MOST_DEGREE) - 1);
            Some((turned + 1).to_string())
        }
        Atom::Value(value) => {
            let (low, high) = (i16::from(column.lowest()), i16::from(column.highest()));
            let turned = match column {
                ShapeColumn::Note { scale, .. } if scale.on && !large => {
                    scale.moved(value, i32::from(sign))
                }
                ShapeColumn::Note { .. } => {
                    let by = if large { OCTAVE } else { 1 };
                    (i16::from(value) + sign * by).clamp(low, high) as u8
                }
                ShapeColumn::Fx {
                    command: FxCommand::Cnd | FxCommand::Acu,
                    ..
                } => {
                    let named = values(column);
                    let now = named
                        .iter()
                        .position(|offer| offer.text == shown(value, column))?;
                    let next = (now as i16 + sign).clamp(0, named.len() as i16 - 1) as usize;
                    return Some(named[next].text.clone());
                }
                _ => {
                    let by = if large { 0x10 } else { 1 };
                    (i16::from(value) + sign * by).clamp(low, high) as u8
                }
            };
            Some(shown(turned, column))
        }
    }
}

/// The values a hit's place can hold, near `near`, a hit's text: on the Note column the scale's
/// degrees where it's a degree, or its notes round it; on another column, its values.
fn atoms(column: ShapeColumn, near: Option<&str>, around: &Around) -> Vec<Offer> {
    let ShapeColumn::Note { scale, .. } = column else {
        let mut offers = values(column);
        if column == ShapeColumn::Length {
            offers.insert(0, offer_of("_", "tied into the next row's note, HLD"));
        }
        return offers;
    };
    match near.map(|word| atom(word, column)) {
        Some(Ok(Atom::Degree(_))) => degrees(scale),
        Some(Ok(Atom::Value(note))) => notes_round(scale, note),
        _ => notes_round(scale, around.from),
    }
}

/// What can go in a token's place or after one: a value of the column, a rest and a group, and
/// after a token, the changes it can take, as the study's Mockup 2 lists them.
fn offer(spot: Spot, around: &Around) -> Vec<Offer> {
    let column = around.column;
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
            kind: Kind::Hit,
            text,
            ..
        } => atoms(column, Some(text), around),
        Spot::On { .. } | Spot::After(None) => {
            let mut offers = atoms(column, None, around);
            offers.extend(rest_and_groups());
            offers
        }
        Spot::After(Some((kind, text))) => {
            let near = (kind == Kind::Hit).then_some(text);
            let mut offers = atoms(column, near, around);
            offers.extend(rest_and_groups());
            offers.extend(changes());
            offers
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::{Scale, ScaleKind};
    use crate::shape::What;

    fn notes() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale {
                on: true,
                kind: ScaleKind::Minor,
                ..Scale::default()
            },
            writes: None,
        }
    }

    fn kinds(text: &str) -> Vec<(&str, Kind)> {
        super::super::split(&MINI, text)
            .into_iter()
            .map(|token| (&text[token.at.clone()], token.kind))
            .collect()
    }

    #[test]
    fn a_line_splits_into_values_rests_groups_and_changes() {
        assert_eq!(
            kinds("C-2 [~ D#2] <5 c3>*2 7?30(3,8)"),
            [
                ("C-2", Kind::Hit),
                ("[", Kind::Open(Bracket::Square)),
                ("~", Kind::Rest),
                ("D#2", Kind::Hit),
                ("]", Kind::Close(Bracket::Square)),
                ("<", Kind::Open(Bracket::Angle)),
                ("5", Kind::Hit),
                ("c3", Kind::Hit),
                (">", Kind::Close(Bracket::Angle)),
                ("*2", Kind::Modifier),
                ("7", Kind::Hit),
                ("?30", Kind::Modifier),
                ("(", Kind::Open(Bracket::Round)),
                ("3", Kind::Value),
                ("8", Kind::Value),
                (")", Kind::Close(Bracket::Round)),
            ]
        );
        assert_eq!(
            kinds("7F.+8"),
            [("7F", Kind::Hit), (".", Kind::Rest), ("+8", Kind::Hit)]
        );
    }

    /// Notes by name and degrees from 1 in one line, each hit its value in the order it's read.
    #[test]
    fn notes_and_degrees_read_into_a_value_a_hit() {
        let (beats, atoms) = compile("C-3 [. 3] <5 c4>*2 8?", notes()).unwrap();
        assert_eq!(
            atoms,
            [
                Atom::Value(60),
                Atom::Degree(2),
                Atom::Degree(4),
                Atom::Value(72),
                Atom::Degree(7)
            ]
        );
        let hits = beats
            .iter()
            .filter(|beat| matches!(beat.what, What::Hit(_)));
        assert_eq!(hits.count(), atoms.len());
        assert_eq!(beats.last().unwrap().chance, 50);
    }

    #[test]
    fn each_column_reads_its_own_units_and_says_why_not() {
        let read = |text, column| compile(text, column).map(|(_, atoms)| atoms);
        assert_eq!(
            read("7F 40", ShapeColumn::Velocity),
            Ok(vec![Atom::Value(0x7F), Atom::Value(0x40)])
        );
        assert_eq!(
            read("40 _ 80", ShapeColumn::Length),
            Ok(vec![Atom::Value(0x40), Atom::Tie, Atom::Value(0x80)])
        );
        let nudge = ShapeColumn::Nudge { most: 0xB };
        assert_eq!(
            read("0 +8 -4", nudge),
            Ok(vec![Atom::Value(11), Atom::Value(19), Atom::Value(7)])
        );
        assert_eq!(
            read("C-3 H-3", notes()).map_err(|bad| bad.at),
            Err(4),
            "where it stops reading"
        );
        assert_eq!(
            read("C-3 0", notes()),
            Err(bad(4, "A degree goes from 1, the root, to 64"))
        );
        assert!(read("C-3 _", notes()).is_err(), "a tie is a length's");
        assert!(read("80", ShapeColumn::Velocity).is_err(), "past 7F");
        let cnd = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::Cnd,
        };
        let first = Condition::all().next().unwrap();
        assert_eq!(
            read(&first.label(), cnd),
            Ok(vec![Atom::Value(first.value())])
        );
    }

    /// A stack is a layer a track: read as layers, refused within one.
    #[test]
    fn a_stack_is_layers_and_one_layer_refuses_it() {
        let line = "C-2*4, [. G-2]*2";
        assert!(stacks(line));
        assert_eq!(layers(line), [(0, "C-2*4"), (6, " [. G-2]*2")]);
        assert_eq!(read(line, notes()), Ok(()));
        assert_eq!(
            compile(line, notes()).map_err(|bad| bad.why),
            Err(STACKED.to_owned())
        );
        assert!(!stacks("C-3(3,8,2)"), "a Euclid's commas aren't a stack");
        assert!(compile("C-3(3,8,2)", notes()).is_ok());
        assert_eq!(layers("[C-3, D-3]").len(), 1);
        assert_eq!(
            read("[C-3, D-3]", notes()).map_err(|bad| bad.why),
            Err(STACKED.to_owned()),
            "a stack inside a group"
        );
        assert_eq!(
            read("C-3, H-3", notes()).map_err(|bad| bad.at),
            Err(5),
            "the second layer's place in the line"
        );
    }

    /// A note turns by a degree, a degree by one, a value by 1 and a change as a rhythm's.
    #[test]
    fn each_token_turns_in_its_own_units() {
        let turn = |text, sign, large, column| turn(Kind::Hit, text, sign, large, column);
        assert_eq!(turn("D#3", 1, false, notes()).as_deref(), Some("F-3"));
        assert_eq!(turn("D#3", 1, true, notes()).as_deref(), Some("D#4"));
        assert_eq!(turn("3", 1, false, notes()).as_deref(), Some("4"));
        assert_eq!(turn("1", -1, false, notes()).as_deref(), Some("1"));
        assert_eq!(turn("3", 1, true, notes()).as_deref(), Some("10"));
        assert_eq!(
            turn("40", 1, false, ShapeColumn::Velocity).as_deref(),
            Some("41")
        );
        assert_eq!(turn("_", 1, false, ShapeColumn::Length), None);
        assert_eq!(
            super::turn(Kind::Modifier, "*2", 1, false, notes()).as_deref(),
            Some("*3")
        );
    }

    /// `K` after a token lists the column's values, a rest, the groups and the changes.
    #[test]
    fn k_after_a_token_lists_values_rests_groups_and_changes() {
        let around = Around {
            column: notes(),
            from: 48,
            to: 72,
            tracks: &[],
            reached: &[],
        };
        let after = offer(Spot::After(Some((Kind::Hit, "D#2"))), &around);
        let texts: Vec<&str> = after.iter().map(|offer| offer.text.as_str()).collect();
        assert!(texts.contains(&"D#2"));
        for change in [".", "[ ]", "< >", "*2", "!2", "@2", "?", "(3,8)"] {
            assert!(texts.contains(&change), "{change}");
        }
        let on_degree = offer(
            Spot::On {
                kind: Kind::Hit,
                at: 0,
                text: "3",
            },
            &around,
        );
        assert_eq!(on_degree[0].text, "1", "the scale's degrees");
        let lengths = offer(
            Spot::After(None),
            &Around {
                column: ShapeColumn::Length,
                ..around
            },
        );
        assert_eq!(lengths[0].text, "_");
    }
}
