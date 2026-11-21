//! A `Bits` mask's grammar (#1089, decision 459): its byte as eight bits, the lowest first, `x` for
//! on and `.` for off, so `Keep` reads as what it keeps. `=` and `-` switch the bit under the
//! cursor, where the row's `=` steps the byte as a number, as macros and the Roto still do.

use super::{About, Bad, Grammar, Kind, Offer, Token};

/// A byte's bits.
const COUNT: usize = 8;

pub(super) static BITS: Grammar = Grammar {
    kinds: &[Kind::Bit, Kind::Unread],
    split,
    read: |text, _| read(text),
    meaning: |kind| match kind {
        Kind::Bit => "bit {} of the mask, counted from the lowest: x is on and . off",
        _ => "what isn't a bit, x or ., so the text doesn't read",
    },
    about: |_, at, _| About {
        name: format!("Bit {}", at + 1),
        subject: (at + 1).to_string(),
    },
    turn: |kind, text, _, large, _| match kind {
        Kind::Bit if !large => Some(if on(text) { "." } else { "x" }.to_owned()),
        _ => None,
    },
    offer: |_, _| {
        [("x", "on"), (".", "off")]
            .into_iter()
            .map(|(text, meaning)| Offer {
                text: text.to_owned(),
                meaning: meaning.to_owned(),
            })
            .collect()
    },
    rows: |_, _| None,
    fixed: |_| Some(COUNT),
    blank: ".",
    rest: ".",
    most: None,
    width: Some(1),
    between: " ",
    types: |c| matches!(c, '0' | '1'),
    from_setting: bits_of,
    to_setting: byte_of,
};

/// Whether a bit's text is on: `x`, or `1` as typed.
fn on(text: &str) -> bool {
    matches!(text, "x" | "X" | "1")
}

/// Each character but a space a token, a bit where it's one.
fn split(text: &str) -> Vec<Token> {
    text.char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .map(|(at, c)| {
            let kind = if matches!(c, 'x' | 'X' | '.' | '0' | '1') {
                Kind::Bit
            } else {
                Kind::Unread
            };
            Token::new(kind, at..at + c.len_utf8())
        })
        .collect()
}

/// Eight bits, no more and no fewer.
fn read(text: &str) -> Result<(), Bad> {
    let tokens = split(text);
    if let Some(token) = tokens.iter().find(|token| token.kind == Kind::Unread) {
        return Err(Bad {
            at: token.at.start,
            why: format!("{} isn't a bit: x is on and . off", &text[token.at.clone()]),
        });
    }
    let why = || format!("A byte is 8 bits, and this has {}", tokens.len());
    match tokens.get(COUNT) {
        Some(extra) => Err(Bad {
            at: extra.at.start,
            why: why(),
        }),
        None if tokens.len() < COUNT => Err(Bad {
            at: text.len(),
            why: why(),
        }),
        None => Ok(()),
    }
}

/// A byte's bits as the field shows them, from its hex: `8D` is `x . x x . . . x`.
fn bits_of(hex: &str) -> String {
    let byte = u8::from_str_radix(hex.trim(), 16).unwrap_or(0);
    let bits: Vec<&str> = (0..COUNT)
        .map(|bit| if byte >> bit & 1 == 1 { "x" } else { "." })
        .collect();
    bits.join(" ")
}

/// The byte bits make, in hex as its row types it.
fn byte_of(bits: &str) -> String {
    let byte = split(bits)
        .iter()
        .enumerate()
        .filter(|(_, token)| on(&bits[token.at.clone()]))
        .fold(0u8, |byte, (bit, _)| byte | 1 << bit);
    format!("{byte:02X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mask_is_its_byte_as_eight_bits_the_lowest_first() {
        assert_eq!(bits_of("8D"), "x . x x . . . x");
        assert_eq!(byte_of("x . x x . . . x"), "8D");
        assert_eq!(byte_of("1 0 1 1 . . . X"), "8D");
        assert_eq!(bits_of("F0"), ". . . . x x x x");
        assert_eq!(read("x . x x . . . x"), Ok(()));
        assert_eq!(read("x.xx...x"), Ok(()));
        assert_eq!(
            read("x . x"),
            Err(Bad {
                at: 5,
                why: "A byte is 8 bits, and this has 3".to_owned()
            })
        );
        assert_eq!(read("x . y x . . . x").map_err(|bad| bad.at), Err(4));
        let column = crate::shape::ShapeColumn::Velocity;
        let turn = |text| (BITS.turn)(Kind::Bit, text, 1, false, column);
        assert_eq!(turn("x").as_deref(), Some("."));
        assert_eq!(turn(".").as_deref(), Some("x"));
    }
}
