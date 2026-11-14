//! What's typed into a value after `i`, and the rule it's typed by: one buffer for Insert's text,
//! the shape panel's rows and the mapping view's ends, so two hex digits commit in one place.

use std::fmt;
use std::ops::Deref;

use ratatui::crossterm::event::KeyCode;
use vt_model::shape::params::Typing;
use vt_model::shape::{MAX_VALUES, WEIGHTS};

/// The longest list of notes or nudges that can be typed: eight notes and the spaces between.
const NAMED_LIST_TEXT: usize = 40;
/// The longest rate that can be typed, like `64 bars`.
const RATE_TEXT: usize = 8;
/// The longest list of a `Tonnetz`' moves that can be typed: eight and the spaces between.
const MOVES_TEXT: usize = 15;

/// The text typed so far.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypedValue {
    text: String,
}

/// What a key typed into a value leaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typed {
    /// Still typing.
    Editing,
    /// Done: the text to set, which leaves the value empty.
    Commit(String),
    /// Given up, which leaves the value empty.
    Cancel,
}

/// What a value takes as it's typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// As a shape's setting is.
    Typing(Typing),
    /// Any characters, up to `longest` of them, kept with `Enter`.
    Text { longest: usize },
}

/// Two hex digits, kept as soon as the second is typed.
pub const HEX: Rule = Rule::Typing(Typing::Hex);

impl From<Typing> for Rule {
    fn from(typing: Typing) -> Self {
        Self::Typing(typing)
    }
}

impl TypedValue {
    /// A key typed into it under a rule: two hex digits commit as the second is typed, others on
    /// `Enter`, `Esc` cancels, `Backspace` takes one back, and what a rule doesn't allow is
    /// ignored.
    pub fn key(&mut self, code: KeyCode, rule: Rule) -> Typed {
        match code {
            KeyCode::Esc => {
                self.text.clear();
                Typed::Cancel
            }
            KeyCode::Backspace => {
                self.text.pop();
                Typed::Editing
            }
            KeyCode::Enter if rule == HEX => Typed::Editing,
            KeyCode::Enter => Typed::Commit(self.take()),
            KeyCode::Char(c) => self.char(c, rule),
            _ => Typed::Editing,
        }
    }

    fn char(&mut self, c: char, rule: Rule) -> Typed {
        let text = &mut self.text;
        let typing = match rule {
            Rule::Text { longest } => {
                if text.len() < longest {
                    text.push(c);
                }
                return Typed::Editing;
            }
            Rule::Typing(typing) => typing,
        };
        match typing {
            Typing::Hex if c.is_ascii_hexdigit() => {
                text.push(c.to_ascii_uppercase());
                if text.len() == 2 {
                    return Typed::Commit(self.take());
                }
            }
            // A note's name or a nudge, alone or in a list, ended with Enter.
            Typing::Named { list } => {
                let longest = if list { NAMED_LIST_TEXT } else { 4 };
                if text.len() < longest
                    && (c.is_ascii_alphanumeric()
                        || matches!(c, '#' | '-' | '+' | '_')
                        || (list && c == ' '))
                {
                    text.push(c);
                }
            }
            Typing::HexList => type_list(text, c),
            // A `Tonnetz`' moves, `P`, `L` and `R`, with spaces between as wanted.
            Typing::Moves
                if text.len() < MOVES_TEXT
                    && (matches!(c.to_ascii_uppercase(), 'P' | 'L' | 'R') || c == ' ') =>
            {
                text.push(c.to_ascii_uppercase());
            }
            // A hex digit a degree, at most one for each semitone.
            Typing::Weights if c.is_ascii_hexdigit() && text.len() < WEIGHTS => {
                text.push(c.to_ascii_uppercase());
            }
            Typing::Rate
                if text.len() < RATE_TEXT
                    && (c.is_ascii_alphanumeric() || matches!(c, '/' | ' ')) =>
            {
                text.push(c);
            }
            Typing::Whole { signed }
                if text.len() < 4
                    && (c.is_ascii_digit()
                        || (signed && text.is_empty() && matches!(c, '-' | '+'))) =>
            {
                text.push(c);
            }
            _ => {}
        }
        Typed::Editing
    }

    /// What it shows in place of the value: the text and a `_` where the next character goes.
    pub fn shown(&self) -> String {
        format!("{}_", self.text)
    }

    /// Type a character, whatever it is, for a caller with rules of its own.
    pub fn push(&mut self, c: char) {
        self.text.push(c);
    }

    /// Take the last character back.
    pub fn pop(&mut self) -> Option<char> {
        self.text.pop()
    }

    pub fn clear(&mut self) {
        self.text.clear();
    }

    /// The text, leaving it empty.
    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.text)
    }
}

/// A character typed into a list of values. Two hex digits make a value, and the space after it
/// comes by itself, so `704055` types `70 40 55`; a space ends a value of one digit sooner.
fn type_list(text: &mut String, c: char) {
    let count = text.split_whitespace().count();
    let last = text.rsplit(' ').next().unwrap_or("");
    if c.is_ascii_hexdigit() {
        if last.len() == 2 {
            if count == MAX_VALUES {
                return;
            }
            text.push(' ');
        }
        text.push(c.to_ascii_uppercase());
    } else if c == ' ' && !last.is_empty() && count < MAX_VALUES {
        text.push(' ');
    }
}

impl From<String> for TypedValue {
    fn from(text: String) -> Self {
        Self { text }
    }
}

impl From<&str> for TypedValue {
    fn from(text: &str) -> Self {
        Self {
            text: text.to_owned(),
        }
    }
}

impl Deref for TypedValue {
    type Target = str;

    fn deref(&self) -> &str {
        &self.text
    }
}

impl fmt::Display for TypedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl PartialEq<str> for TypedValue {
    fn eq(&self, other: &str) -> bool {
        self.text == other
    }
}

impl PartialEq<&str> for TypedValue {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

impl PartialEq<String> for TypedValue {
    fn eq(&self, other: &String) -> bool {
        &self.text == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Type each character of `keys` under `rule`, and what the last one left.
    fn typed(value: &mut TypedValue, keys: &str, rule: Rule) -> Typed {
        keys.chars()
            .fold(Typed::Editing, |_, c| value.key(KeyCode::Char(c), rule))
    }

    #[test]
    fn two_hex_digits_commit_at_the_second() {
        let mut value = TypedValue::default();
        assert_eq!(value.key(KeyCode::Char('a'), HEX), Typed::Editing);
        assert_eq!(value.shown(), "A_");
        assert_eq!(
            value.key(KeyCode::Char('f'), HEX),
            Typed::Commit("AF".to_owned())
        );
        assert!(value.is_empty(), "a commit leaves it empty");
    }

    #[test]
    fn hex_ignores_what_isnt_hex_and_enter() {
        let mut value = TypedValue::default();
        assert_eq!(typed(&mut value, "g-7", HEX), Typed::Editing);
        assert_eq!(value, "7");
        assert_eq!(value.key(KeyCode::Enter, HEX), Typed::Editing);
        assert_eq!(value, "7", "one digit isn't a value yet");
    }

    #[test]
    fn backspace_takes_one_back_and_esc_cancels() {
        let mut value = TypedValue::default();
        value.key(KeyCode::Char('4'), HEX);
        assert_eq!(value.key(KeyCode::Backspace, HEX), Typed::Editing);
        assert!(value.is_empty());
        assert_eq!(
            value.key(KeyCode::Backspace, HEX),
            Typed::Editing,
            "empty stays empty"
        );
        value.key(KeyCode::Char('4'), HEX);
        assert_eq!(value.key(KeyCode::Esc, HEX), Typed::Cancel);
        assert!(value.is_empty());
    }

    #[test]
    fn others_commit_on_enter() {
        let mut value = TypedValue::default();
        let rule = Rule::from(Typing::Whole { signed: true });
        assert_eq!(typed(&mut value, "-12", rule), Typed::Editing);
        assert_eq!(
            value.key(KeyCode::Enter, rule),
            Typed::Commit("-12".to_owned())
        );
        assert!(value.is_empty());
        assert_eq!(
            value.key(KeyCode::Enter, rule),
            Typed::Commit(String::new()),
            "even empty"
        );
    }

    #[test]
    fn a_whole_number_takes_four_and_a_sign_first() {
        let mut value = TypedValue::default();
        typed(&mut value, "1-2345", Typing::Whole { signed: true }.into());
        assert_eq!(value, "1234");
        let mut value = TypedValue::default();
        typed(&mut value, "+5", Typing::Whole { signed: false }.into());
        assert_eq!(value, "5", "unsigned takes no sign");
    }

    #[test]
    fn text_takes_anything_up_to_its_longest() {
        let mut value = TypedValue::default();
        typed(&mut value, "C-3 E-3", Rule::Text { longest: 5 });
        assert_eq!(value, "C-3 E");
    }

    #[test]
    fn a_hex_list_spaces_its_values() {
        let mut value = TypedValue::default();
        typed(&mut value, "7040 5", Typing::HexList.into());
        assert_eq!(value, "70 40 5");
        let mut value = TypedValue::default();
        typed(
            &mut value,
            &"00".repeat(MAX_VALUES + 1),
            Typing::HexList.into(),
        );
        assert_eq!(
            value.split_whitespace().count(),
            MAX_VALUES,
            "no more than it holds"
        );
    }

    #[test]
    fn named_moves_weights_and_rates_keep_their_characters() {
        let mut value = TypedValue::default();
        typed(&mut value, "C#3!x", Typing::Named { list: false }.into());
        assert_eq!(value, "C#3x");
        let mut value = TypedValue::default();
        typed(&mut value, "C-3 D-3", Typing::Named { list: true }.into());
        assert_eq!(value, "C-3 D-3");
        let mut value = TypedValue::default();
        typed(&mut value, "pl rx", Typing::Moves.into());
        assert_eq!(value, "PL R");
        let mut value = TypedValue::default();
        typed(&mut value, &"f".repeat(WEIGHTS + 1), Typing::Weights.into());
        assert_eq!(value.len(), WEIGHTS);
        let mut value = TypedValue::default();
        typed(&mut value, "1/4 bar!", Typing::Rate.into());
        assert_eq!(value, "1/4 bar");
    }

    #[test]
    fn listed_and_toggled_type_nothing() {
        for typing in [Typing::Listed, Typing::Toggled] {
            let mut value = TypedValue::default();
            typed(&mut value, "a1", typing.into());
            assert!(value.is_empty(), "{typing:?}");
        }
    }
}
