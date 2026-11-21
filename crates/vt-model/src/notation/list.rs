//! A `List`'s grammar (#1089, decision 459): its values with a space between each, a note each on
//! the Note column, a nudge on the Nudge column and a hex value on the rest, up to
//! [`MAX_LIST`] of them, each a token turned in the column's own units.

use super::{About, Bad, Grammar, Kind, Offer, Token, as_it_is};
use crate::scale::Scale;
use crate::shape::{MAX_LIST, ShapeColumn};

/// A semitone and an octave, as a note turns with the scale off.
const OCTAVE: i16 = 12;

pub(super) static LIST: Grammar = Grammar {
    kinds: &[Kind::Value, Kind::Unread],
    split,
    read,
    meaning: |kind| match kind {
        Kind::Value => "the value it writes on the {} of the rows it fills, going round the list",
        _ => "what isn't a value of the column, so the text doesn't read",
    },
    about: |_, at, _| About {
        name: format!("Value {}", at + 1),
        subject: ordinal(at + 1),
    },
    turn,
    offer,
    rows: |_, _, _| Vec::new(),
    fixed: |_| None,
    blank: "",
    rest: "",
    most: Some((MAX_LIST, "A list has up to 256 values: x takes one out")),
    width: None,
    between: " ",
    types: |_| false,
    from_setting: as_it_is,
    to_setting: as_it_is,
};

/// `1st`, `2nd`, `3rd`, `4th` and on, `11th` to `13th` too.
fn ordinal(nth: usize) -> String {
    let suffix = match (nth % 10, nth % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{nth}{suffix}")
}

/// Each word between spaces a value; whether it is one on the column is for [`read`] to say.
fn split(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (at, c) in text.char_indices().chain([(text.len(), ' ')]) {
        match (c.is_whitespace(), start) {
            (true, Some(from)) => {
                tokens.push(Token::new(Kind::Value, from..at));
                start = None;
            }
            (false, None) => start = Some(at),
            _ => {}
        }
    }
    tokens
}

/// One value or more, up to [`MAX_LIST`], each one the column reads.
fn read(text: &str, column: ShapeColumn) -> Result<(), Bad> {
    let tokens = split(text);
    if let Some(token) = tokens
        .iter()
        .find(|token| column.parse(&text[token.at.clone()]).is_none())
    {
        let what = &text[token.at.clone()];
        let why = match column {
            ShapeColumn::Note { .. } => format!("{what} isn't a note, like C-3 or D#4"),
            ShapeColumn::Nudge { .. } => format!("{what} isn't a nudge, like -3, 0 or +B"),
            _ => format!("{what} isn't a hex value, like 70"),
        };
        return Err(Bad {
            at: token.at.start,
            why,
        });
    }
    match tokens.get(MAX_LIST) {
        Some(extra) => Err(Bad {
            at: extra.at.start,
            why: "A list has up to 256 values".to_owned(),
        }),
        None if tokens.is_empty() => Err(Bad {
            at: 0,
            why: "A list has a value at least".to_owned(),
        }),
        None => Ok(()),
    }
}

/// A value turned in the column's units: a note by a degree of the scale, or a semitone with the
/// scale off, and by an octave where `large`; anything else by 1, or by `10` where `large`, kept
/// to the column's range.
fn turn(kind: Kind, text: &str, sign: i16, large: bool, column: ShapeColumn) -> Option<String> {
    if kind != Kind::Value {
        return None;
    }
    let value = column.parse(text)?;
    let (low, high) = (i16::from(column.lowest()), i16::from(column.highest()));
    let turned = match column {
        ShapeColumn::Note { scale, .. } if scale.on && !large => {
            scale.moved(value, i32::from(sign))
        }
        ShapeColumn::Note { .. } => {
            let by = if large { OCTAVE } else { 1 };
            (i16::from(value) + sign * by).clamp(low, high) as u8
        }
        _ => {
            let by = if large { 0x10 } else { 1 };
            (i16::from(value) + sign * by).clamp(low, high) as u8
        }
    };
    Some(column.show(turned))
}

/// On the Note column, the scale's notes over the octaves round the middle, each named; on the
/// others, nothing to list, as a value is typed or turned.
fn offer(kind: Kind, column: ShapeColumn) -> Vec<Offer> {
    let ShapeColumn::Note { scale, .. } = column else {
        return Vec::new();
    };
    if kind != Kind::Value {
        return Vec::new();
    }
    let scale = Scale { on: true, ..scale };
    let first = scale.degree(0, 2);
    (first..=first + 3 * OCTAVE as u8)
        .filter(|note| scale.contains(*note))
        .map(|note| Offer {
            text: column.show(note),
            meaning: format!("in {} {}", scale.root_name(), scale.kind.name()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleKind;
    use crate::step::FxCommand;

    fn notes(on: bool) -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale {
                on,
                kind: ScaleKind::MinorPentatonic,
                ..Scale::default()
            },
            writes: None,
        }
    }

    #[test]
    fn a_list_reads_a_value_of_the_column_a_word() {
        assert_eq!(read("C-3 D#3 G-3", notes(true)), Ok(()));
        assert_eq!(
            read("C-3 H-3", notes(true)),
            Err(Bad {
                at: 4,
                why: "H-3 isn't a note, like C-3 or D#4".to_owned()
            })
        );
        assert_eq!(read("70 40 55", ShapeColumn::Velocity), Ok(()));
        assert_eq!(
            read("70 4G", ShapeColumn::Velocity).map_err(|bad| bad.why),
            Err("4G isn't a hex value, like 70".to_owned())
        );
        assert!(read("", ShapeColumn::Velocity).is_err());
        let full = vec!["10"; MAX_LIST].join(" ");
        assert_eq!(read(&full, ShapeColumn::Velocity), Ok(()));
        let over = format!("{full} 20");
        assert_eq!(
            read(&over, ShapeColumn::Velocity),
            Err(Bad {
                at: full.len() + 1,
                why: "A list has up to 256 values".to_owned()
            })
        );
        let tokens = split("  70 40  55 ");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[2].at, 9..11);
    }

    /// A note turns by a degree of the song's scale, or a semitone with it off, and an octave
    /// with `+` and `_`; a value by 1 or `10`, kept to the column's range.
    #[test]
    fn a_value_turns_in_the_columns_units() {
        let turn = |text, sign, large, column| turn(Kind::Value, text, sign, large, column);
        assert_eq!(turn("D#3", 1, false, notes(true)).as_deref(), Some("F-3"));
        assert_eq!(turn("C-3", -1, false, notes(true)).as_deref(), Some("A#2"));
        assert_eq!(turn("D#3", 1, false, notes(false)).as_deref(), Some("E-3"));
        assert_eq!(turn("D#3", 1, true, notes(true)).as_deref(), Some("D#4"));
        let velocity = ShapeColumn::Velocity;
        assert_eq!(turn("70", 1, false, velocity).as_deref(), Some("71"));
        assert_eq!(turn("7F", 1, true, velocity).as_deref(), Some("7F"));
        assert_eq!(turn("05", -1, true, velocity).as_deref(), Some("00"));
        let nudge = ShapeColumn::Nudge { most: 8 };
        assert_eq!(turn("0", -1, false, nudge).as_deref(), Some("-1"));
        let fx = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::Cha,
        };
        assert!(turn("40", 1, false, fx).is_some());
        assert_eq!(super::turn(Kind::Unread, "70", 1, false, velocity), None);
    }

    #[test]
    fn a_value_is_named_by_its_place_and_k_lists_the_scale() {
        let about = (LIST.about)(Kind::Value, 1, notes(true));
        assert_eq!(about.name, "Value 2");
        assert_eq!(about.subject, "2nd");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(23), "23rd");
        let offers = offer(Kind::Value, notes(true));
        assert_eq!(offers.len(), 16, "five a octave over three octaves, and the top C");
        assert_eq!(offers[0].text, "C-2");
        assert_eq!(offers[1].text, "D#2");
        assert!(offer(Kind::Value, ShapeColumn::Velocity).is_empty());
    }
}
