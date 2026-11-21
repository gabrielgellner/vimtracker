//! What's typed into a value after `i`, and the rule it's typed by: one buffer for Insert's text,
//! the shape panel's rows and the mapping view's ends, so two hex digits commit in one place.

use std::fmt;
use std::ops::Deref;

use ratatui::crossterm::event::KeyCode;
use vt_model::notation::{MAX_MINI, MAX_RHYTHM};
use vt_model::shape::params::Typing;
use vt_model::shape::{MAX_LIST, WEIGHTS};

/// The longest a note or a nudge is typed, as `C#3` or `+B`, with room to spare.
const NAMED_TEXT: usize = 4;
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
pub(crate) const HEX: Rule = Rule::Typing(Typing::Hex);

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
            Typing::Named { list } => type_named(text, c, list),
            Typing::HexList => type_list(text, c),
            // A `Tonnetz`' moves, `P`, `L` and `R`, with spaces between as wanted.
            Typing::Moves
                if text.len() < MOVES_TEXT
                    && (matches!(c.to_ascii_uppercase(), 'P' | 'L' | 'R') || c == ' ') =>
            {
                text.push(c.to_ascii_uppercase());
            }
            // An `Own` rhythm, Tidal's mini-notation, read as a whole once it's kept.
            Typing::Rhythm if text.len() < MAX_RHYTHM && (c.is_ascii_graphic() || c == ' ') => {
                text.push(c);
            }
            // A `Mini`'s line likewise, its column's values among its hits.
            Typing::Mini if text.len() < MAX_MINI && (c.is_ascii_graphic() || c == ' ') => {
                text.push(c);
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

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Type a character, whatever it is, for a caller with rules of its own.
    pub fn push(&mut self, c: char) {
        self.text.push(c);
    }

    /// Take the last character back.
    pub(crate) fn pop(&mut self) -> Option<char> {
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
/// comes by itself, so `704055` types `70 40 55`; a space ends a value of one digit sooner. It
/// takes up to [`MAX_LIST`] values.
fn type_list(text: &mut String, c: char) {
    let count = text.split_whitespace().count();
    let last = text.rsplit(' ').next().unwrap_or("");
    if c.is_ascii_hexdigit() {
        if last.len() == 2 {
            if count == MAX_LIST {
                return;
            }
            text.push(' ');
        }
        text.push(c.to_ascii_uppercase());
    } else if c == ' ' && !last.is_empty() && count < MAX_LIST {
        text.push(' ');
    }
}

/// A character typed into a note's name or a nudge, or in a `list` of them, spaces between, up
/// to [`MAX_LIST`] of them.
fn type_named(text: &mut String, c: char, list: bool) {
    let last = text.rsplit(' ').next().unwrap_or("");
    if list && c == ' ' {
        if !last.is_empty() && text.split_whitespace().count() < MAX_LIST {
            text.push(' ');
        }
    } else if last.len() < NAMED_TEXT
        && (c.is_ascii_alphanumeric() || matches!(c, '#' | '-' | '+' | '_'))
    {
        text.push(c);
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
            &"00".repeat(MAX_LIST + 1),
            Typing::HexList.into(),
        );
        assert_eq!(
            value.split_whitespace().count(),
            MAX_LIST,
            "no more than it holds"
        );
    }

    #[test]
    fn a_list_of_notes_takes_up_to_256_of_them() {
        let named = Typing::Named { list: true }.into();
        let mut value = TypedValue::default();
        typed(&mut value, &"C#3 ".repeat(MAX_LIST + 1), named);
        assert_eq!(value.split_whitespace().count(), MAX_LIST);
        assert!(
            value
                .split_whitespace()
                .take(MAX_LIST - 1)
                .all(|note| note == "C#3")
        );
        let mut value = TypedValue::default();
        typed(&mut value, "C-3  E-3x5", named);
        assert_eq!(value, "C-3 E-3x", "a note is four characters at most");
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
