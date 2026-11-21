//! A Steps slot's values as a list (decision 463, after decision 459's typed list): the values
//! it plays, a signed hex value each with a space between, `0C 00 -07 -05`, up to its sixteen
//! steps, each a token turned by 1, or by `10` where large, the list as long as the slot plays.

use super::{About, Bad, Grammar, Kind, Token, as_it_is};
use crate::steps::{MAX_STEP, STEPS};

pub(super) static STEPS_LIST: Grammar = Grammar {
    kinds: &[Kind::Value, Kind::Unread],
    split,
    read: |text, column, _| read(text, column),
    meaning: |kind| match kind {
        Kind::Value => "the value step {} plays, signed hex",
        _ => "what isn't a step's value, so the text doesn't read",
    },
    about: |_, at, _| About {
        name: format!("Step {}", at + 1),
        subject: format!("{}", at + 1),
    },
    turn: |kind, text, sign, large, _, _| {
        if kind != Kind::Value {
            return None;
        }
        let by = if large { 0x10 } else { 1 };
        Some(show((value(text)? + sign * by).clamp(-MAX_STEP, MAX_STEP)))
    },
    offer: |_, _| Vec::new(),
    rows: |_, _| None,
    fixed: |_| None,
    blank: "",
    rest: "",
    most: Some((STEPS, "A Steps slot has 16 steps: x takes one out")),
    width: None,
    between: " ",
    types: |_| false,
    from_setting: as_it_is,
    to_setting: as_it_is,
};

/// A step's value as the Mod page shows it: signed hex, two digits.
pub fn show(value: i16) -> String {
    if value < 0 {
        format!("-{:02X}", -value)
    } else {
        format!("{value:02X}")
    }
}

/// A step's value typed, signed hex, where it's one.
pub fn value(text: &str) -> Option<i16> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => (-1, digits),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    if digits.is_empty() || digits.len() > 2 {
        return None;
    }
    let value = i16::from_str_radix(digits, 16).ok()?;
    (value <= MAX_STEP).then_some(sign * value)
}

/// Each word between spaces a value; whether it is one is for [`read`] to say.
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

/// One value or more, up to [`STEPS`], each signed hex up to [`MAX_STEP`] either way.
fn read(text: &str, _: crate::shape::ShapeColumn) -> Result<(), Bad> {
    let tokens = split(text);
    if let Some(token) = tokens
        .iter()
        .find(|token| value(&text[token.at.clone()]).is_none())
    {
        return Err(Bad {
            at: token.at.start,
            why: format!(
                "{} isn't a step's value, signed hex such as -0C",
                &text[token.at.clone()]
            ),
        });
    }
    match tokens.get(STEPS) {
        Some(extra) => Err(Bad {
            at: extra.at.start,
            why: "A Steps slot has 16 steps".to_owned(),
        }),
        None if tokens.is_empty() => Err(Bad {
            at: 0,
            why: "A Steps slot plays a step at least".to_owned(),
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::Field;
    use crate::shape::ShapeColumn;

    #[test]
    fn a_steps_list_reads_signed_hex_and_turns_in_it() {
        let mut field = Field::new(&STEPS_LIST, "0C 00 -07 -05", ShapeColumn::Velocity);
        assert!(field.bad().is_none());
        assert_eq!(field.tokens().len(), 4);
        field.turn(2, 1, true).unwrap();
        assert_eq!(field.text(), "0C 00 09 -05");
        field.turn(0, -1, false).unwrap();
        assert_eq!(field.text(), "0B 00 09 -05");
        let field = Field::new(&STEPS_LIST, "0C g0", ShapeColumn::Velocity);
        assert_eq!(
            field.bad().map(|bad| bad.why.as_str()),
            Some("g0 isn't a step's value, signed hex such as -0C")
        );
        assert_eq!(value("-FF"), Some(-0xFF));
        assert_eq!(value("100"), None);
    }
}
