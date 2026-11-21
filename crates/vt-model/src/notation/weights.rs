//! `Weights`' grammar: a hex digit for each of the scale's degrees, from the root (decision 388),
//! each digit a token of its own, over the note it's for.

use super::{About, Bad, Grammar, Kind, Offer, Token, as_it_is, nth_of};
use crate::shape::{MAX_WEIGHT, ShapeColumn, degree_count, degree_name};

/// How far `+` and `_` turn a weight, a quarter of its range, as decision 388 has it.
const JUMP: i16 = 4;

pub(super) static WEIGHTS: Grammar = Grammar {
    kinds: &[Kind::Weight, Kind::Unread],
    split,
    read: |text, column, _| read(text, column),
    meaning: |kind| match kind {
        Kind::Weight => "how often {} is chosen, from 0, never, to F, the most",
        _ => "what isn't a weight, a hex digit from 0 to F, so the text doesn't read",
    },
    about: |_, at, column| {
        let note = match column {
            ShapeColumn::Note { scale, .. } if at < degree_count(scale) => degree_name(scale, at),
            _ => "",
        };
        About {
            name: format!("Weight of {note}"),
            subject: note.to_owned(),
        }
    },
    turn: |kind, text, sign, large, _, _| {
        let weight = match kind {
            Kind::Weight => i16::from_str_radix(text, 16).ok()?,
            _ => return None,
        };
        let by = if large { JUMP } else { 1 };
        let turned = (weight + sign * by).clamp(0, i16::from(MAX_WEIGHT));
        Some(format!("{turned:X}"))
    },
    // Any weight goes in a weight's place, or in what doesn't read as one, and a field of fixed
    // length is only ever typed over, so it's the same list at every spot.
    offer: |_, _| {
        (0..=MAX_WEIGHT)
            .map(|weight| Offer {
                text: format!("{weight:X}"),
                meaning: match weight {
                    0 => "never".to_owned(),
                    MAX_WEIGHT => "the most".to_owned(),
                    _ => format!(
                        "{}% as often as F",
                        u32::from(weight) * 100 / u32::from(MAX_WEIGHT)
                    ),
                },
            })
            .collect()
    },
    // A random fill's table holds each note's degree, and a digit is its degree's.
    rows: |tokens, at| nth_of(tokens, at, Kind::Weight),
    fixed: |column| match column {
        ShapeColumn::Note { scale, .. } => Some(degree_count(scale)),
        _ => None,
    },
    blank: "F",
    rest: "0",
    most: None,
    width: Some(1),
    between: "",
    types: |c| c.is_ascii_hexdigit(),
    from_setting: as_it_is,
    to_setting: as_it_is,
};

/// Each character a token, a hex digit a weight; spaces between are let be.
fn split(text: &str) -> Vec<Token> {
    text.char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .map(|(at, c)| {
            let kind = if c.is_ascii_hexdigit() {
                Kind::Weight
            } else {
                Kind::Unread
            };
            Token::new(kind, at..at + c.len_utf8())
        })
        .collect()
}

/// A digit for each degree, no more and no fewer, as the field shows every one.
fn read(text: &str, column: ShapeColumn) -> Result<(), Bad> {
    let ShapeColumn::Note { scale, .. } = column else {
        return Err(Bad {
            at: 0,
            why: "Weights are for the Note column".to_owned(),
        });
    };
    let tokens = split(text);
    if let Some(token) = tokens.iter().find(|token| token.kind == Kind::Unread) {
        return Err(Bad {
            at: token.at.start,
            why: format!(
                "{} isn't a weight, a hex digit from 0 to F",
                &text[token.at.clone()]
            ),
        });
    }
    let degrees = degree_count(scale);
    let why = || {
        format!(
            "{degrees} notes take {degrees} weights, and this has {}",
            tokens.len()
        )
    };
    match tokens.get(degrees) {
        Some(extra) => Err(Bad {
            at: extra.at.start,
            why: why(),
        }),
        None if tokens.len() < degrees => Err(Bad {
            at: text.len(),
            why: why(),
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::{Around, Spot};
    use crate::scale::{Scale, ScaleKind};

    fn pentatonic() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale {
                on: true,
                kind: ScaleKind::MinorPentatonic,
                ..Scale::default()
            },
            writes: None,
        }
    }

    #[test]
    fn a_weight_a_degree_no_more_and_no_fewer() {
        let column = pentatonic();
        assert_eq!(read("F08FF", column), Ok(()));
        assert_eq!(
            read("F08F", column),
            Err(Bad {
                at: 4,
                why: "5 notes take 5 weights, and this has 4".to_owned()
            })
        );
        assert_eq!(read("F08FFF", column).map_err(|bad| bad.at), Err(5));
        assert_eq!(
            read("F0GFF", column),
            Err(Bad {
                at: 2,
                why: "G isn't a weight, a hex digit from 0 to F".to_owned()
            })
        );
        assert!(read("FFFFF", ShapeColumn::Velocity).is_err());
    }

    #[test]
    fn a_weight_turns_by_one_or_four_and_is_named_by_its_note() {
        let turn =
            |kind, text, sign, large| (WEIGHTS.turn)(kind, text, sign, large, pentatonic(), &[]);
        assert_eq!(turn(Kind::Weight, "E", 1, false).as_deref(), Some("F"));
        assert_eq!(turn(Kind::Weight, "F", 1, false).as_deref(), Some("F"));
        assert_eq!(turn(Kind::Weight, "8", -1, true).as_deref(), Some("4"));
        assert_eq!(turn(Kind::Weight, "2", -1, true).as_deref(), Some("0"));
        assert_eq!(turn(Kind::Unread, "G", 1, false), None);
        let about = (WEIGHTS.about)(Kind::Weight, 3, pentatonic());
        assert_eq!(about.name, "Weight of G");
        assert_eq!(about.subject, "G");
        assert_eq!((WEIGHTS.fixed)(pentatonic()), Some(5));
        let around = Around {
            column: pentatonic(),
            from: 0,
            to: 0,
            tracks: &[],
            reached: &[],
        };
        let spot = Spot::On {
            kind: Kind::Weight,
            at: 3,
            text: "F",
        };
        let offers = (WEIGHTS.offer)(spot, &around);
        assert_eq!(offers.len(), 16);
        assert_eq!(offers[0].meaning, "never");
        assert_eq!(offers[15].meaning, "the most");
    }

    /// A digit is its degree's byte of a render's table, its place among the digits.
    #[test]
    fn a_weight_writes_as_its_degree() {
        let tokens = split("F0F8F");
        assert_eq!((WEIGHTS.rows)(&tokens, 0), Some(0));
        assert_eq!((WEIGHTS.rows)(&tokens, 3), Some(3));
        assert_eq!((WEIGHTS.rows)(&split("F0G8F"), 2), None);
    }
}
