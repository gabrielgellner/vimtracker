//! A `Tonnetz`'s moves (#1089, decision 459): `P`, `L` and `R`, each a token, spaces between as
//! wanted, up to [`MAX_MOVES`]; `=` and `-` go round the three.

use super::{About, Bad, Grammar, Kind, Offer, Token, as_it_is};
use crate::shape::MAX_MOVES;

/// The moves in the order `=` goes round them.
const LETTERS: [char; 3] = ['P', 'L', 'R'];

pub(super) static MOVES: Grammar = Grammar {
    kinds: &[Kind::Move, Kind::Unread],
    split,
    read,
    meaning: |kind| match kind {
        Kind::Move => {
            "move {} of the walk: P keeps the root and turns major and minor round, L and R \
             each move one note, by a semitone or a tone"
        }
        _ => "what isn't a move, P, L or R, so the text doesn't read",
    },
    about: |_, at, _| About {
        name: format!("Move {}", at + 1),
        subject: (at + 1).to_string(),
    },
    turn: |kind, text, sign, _, _| {
        let at = LETTERS.iter().position(|each| text.eq_ignore_ascii_case(&each.to_string()));
        let at = at.filter(|_| kind == Kind::Move)?;
        let turned = (at as i16 + sign).rem_euclid(LETTERS.len() as i16) as usize;
        Some(LETTERS[turned].to_string())
    },
    offer: |_, _| {
        [
            ("P", "parallel: the same root, major and minor turned round"),
            ("L", "leading-tone: a major's root down a semitone, a minor's fifth up one"),
            ("R", "relative: a major's fifth up a tone, a minor's root down one"),
        ]
        .into_iter()
        .map(|(text, meaning)| Offer {
            text: text.to_owned(),
            meaning: meaning.to_owned(),
        })
        .collect()
    },
    rows: |_, _, _| Vec::new(),
    fixed: |_| None,
    blank: "",
    rest: "",
    most: Some((MAX_MOVES, "A walk has up to 8 moves: x takes one out")),
    width: Some(1),
    between: " ",
    types: |_| false,
    from_setting: as_it_is,
    to_setting: as_it_is,
};

/// Each character but a space a token, a move where it's one.
fn split(text: &str) -> Vec<Token> {
    text.char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .map(|(at, c)| {
            let kind = if LETTERS.contains(&c.to_ascii_uppercase()) {
                Kind::Move
            } else {
                Kind::Unread
            };
            Token::new(kind, at..at + c.len_utf8())
        })
        .collect()
}

/// One move or more, up to [`MAX_MOVES`].
fn read(text: &str, _: crate::shape::ShapeColumn) -> Result<(), Bad> {
    let tokens = split(text);
    if let Some(token) = tokens.iter().find(|token| token.kind == Kind::Unread) {
        return Err(Bad {
            at: token.at.start,
            why: format!("{} isn't a move: P, L or R", &text[token.at.clone()]),
        });
    }
    match tokens.get(MAX_MOVES) {
        Some(extra) => Err(Bad {
            at: extra.at.start,
            why: "A walk has up to 8 moves".to_owned(),
        }),
        None if tokens.is_empty() => Err(Bad {
            at: 0,
            why: "A walk has a move at least".to_owned(),
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::ShapeColumn;

    #[test]
    fn moves_read_up_to_eight_and_turn_round_p_l_and_r() {
        let column = ShapeColumn::Velocity;
        assert_eq!(read("P L R L", column), Ok(()));
        assert_eq!(read("plrl", column), Ok(()));
        assert_eq!(
            read("P X", column),
            Err(Bad {
                at: 2,
                why: "X isn't a move: P, L or R".to_owned()
            })
        );
        assert_eq!(read("PLRLPLRLP", column).map_err(|bad| bad.at), Err(8));
        assert!(read(" ", column).is_err());
        let turn = |text, sign| (MOVES.turn)(Kind::Move, text, sign, false, column);
        assert_eq!(turn("P", 1).as_deref(), Some("L"));
        assert_eq!(turn("R", 1).as_deref(), Some("P"));
        assert_eq!(turn("P", -1).as_deref(), Some("R"));
        assert_eq!(turn("l", 1).as_deref(), Some("R"));
        assert_eq!((MOVES.offer)(Kind::Move, column).len(), 3);
        assert_eq!((MOVES.about)(Kind::Move, 1, column).name, "Move 2");
    }
}
