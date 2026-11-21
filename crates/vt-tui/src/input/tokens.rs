//! The token editor's keys (decision 456): a text-like setting gone into with `l`, its cursor on a
//! whole token, moving by the grammar, each token turned in its own units, taken away, rested,
//! replaced and typed over, the text checked on every key. One key table, in a mode of its own
//! whose keys come before the panel's, so a key it doesn't bind leaves the tokens and does what
//! it does on the rows.
//!
//! Whoever shows the field keeps its cursor, by its token's place, and is told what each key did,
//! so it can write the text that reads into its setting and say what the cursor is on.

use ratatui::crossterm::event::KeyCode;
use vt_model::notation::{Bracket, Field, Motion, Object, Place};

use crate::input::bindings::Cmd;

/// What a key does in a field of tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKey {
    /// `Esc`: back to the row, the last text that read put back.
    Back,
    /// `Enter`: back to the row, while the text reads.
    Done,
    Move(Motion),
    Increase,
    Decrease,
    IncreaseLarge,
    DecreaseLarge,
    /// `x`: the token taken away, or on a field of fixed length put back as it starts.
    Remove,
    /// `~`: the token rested, or given back.
    Rest,
    /// `r`: one token typed in the token's place.
    Replace,
    /// `i` `a` `s`: typing before, after or over the token.
    Type(Place),
    Delete(Object),
    Change(Object),
    Yank(Object),
    Paste,
}

/// The token editor's rows of the binding table, but its text objects.
pub const KEYS: &[(&str, Cmd, &str)] = &[
    (
        "Esc",
        Cmd::InTokens(TokenKey::Back),
        "Back to the row, putting back the last text that read",
    ),
    (
        "Enter",
        Cmd::InTokens(TokenKey::Done),
        "Back to the row, where the text reads",
    ),
    (
        "h",
        Cmd::InTokens(TokenKey::Move(Motion::Left)),
        "The token before, into and out of groups; from the first, back to the row",
    ),
    (
        "l",
        Cmd::InTokens(TokenKey::Move(Motion::Right)),
        "The next token, into and out of groups",
    ),
    (
        "w",
        Cmd::InTokens(TokenKey::Move(Motion::Next)),
        "The next token at the same depth, a group as one",
    ),
    (
        "b",
        Cmd::InTokens(TokenKey::Move(Motion::Previous)),
        "The token before at the same depth, a group as one",
    ),
    (
        "^",
        Cmd::InTokens(TokenKey::Move(Motion::First)),
        "The first token",
    ),
    (
        "$",
        Cmd::InTokens(TokenKey::Move(Motion::Last)),
        "The last token",
    ),
    (
        "%",
        Cmd::InTokens(TokenKey::Move(Motion::Match)),
        "The bracket that matches this one",
    ),
    (
        "(",
        Cmd::InTokens(TokenKey::Move(Motion::GroupStart)),
        "Out to the start of the group",
    ),
    (
        ")",
        Cmd::InTokens(TokenKey::Move(Motion::GroupEnd)),
        "Out to the end of the group",
    ),
    (
        "=",
        Cmd::InTokens(TokenKey::Increase),
        "Turn the token up, in its own units",
    ),
    (
        "-",
        Cmd::InTokens(TokenKey::Decrease),
        "Turn the token down, in its own units",
    ),
    (
        "+",
        Cmd::InTokens(TokenKey::IncreaseLarge),
        "Turn the token up a large step",
    ),
    (
        "_",
        Cmd::InTokens(TokenKey::DecreaseLarge),
        "Turn the token down a large step",
    ),
    (
        "x",
        Cmd::InTokens(TokenKey::Remove),
        "Take the token away, or on a field of fixed length put back what it starts as",
    ),
    (
        "~",
        Cmd::InTokens(TokenKey::Rest),
        "Rest the token, or give back what it was",
    ),
    ("r", Cmd::InTokens(TokenKey::Replace), "Replace the token"),
    (
        "i",
        Cmd::InTokens(TokenKey::Type(Place::Before)),
        "Type before the token, or on a field of fixed length type it all",
    ),
    (
        "a",
        Cmd::InTokens(TokenKey::Type(Place::After)),
        "Type after the token, or on a field of fixed length type it all",
    ),
    (
        "s",
        Cmd::InTokens(TokenKey::Type(Place::Over)),
        "Type over the token",
    ),
    (
        "p",
        Cmd::InTokens(TokenKey::Paste),
        "Paste after the token, or over it on a field of fixed length",
    ),
];

/// The rows of an operator's text objects: `it` `at`, `i[` `a[`, `i<` `a<`, `i(` `a(`.
macro_rules! objects {
    ($op:literal, $key:ident, $does:literal) => {
        &[
            (
                concat!($op, " i t"),
                Cmd::InTokens(TokenKey::$key(Object::Token)),
                concat!($does, " the token"),
            ),
            (
                concat!($op, " a t"),
                Cmd::InTokens(TokenKey::$key(Object::TokenAround)),
                concat!($does, " the token and its modifiers"),
            ),
            (
                concat!($op, " i ["),
                Cmd::InTokens(TokenKey::$key(Object::Inside(Bracket::Square))),
                concat!($does, " what the [ ] group holds"),
            ),
            (
                concat!($op, " a ["),
                Cmd::InTokens(TokenKey::$key(Object::Around(Bracket::Square))),
                concat!($does, " the [ ] group"),
            ),
            (
                concat!($op, " i <"),
                Cmd::InTokens(TokenKey::$key(Object::Inside(Bracket::Angle))),
                concat!($does, " what the < > group holds"),
            ),
            (
                concat!($op, " a <"),
                Cmd::InTokens(TokenKey::$key(Object::Around(Bracket::Angle))),
                concat!($does, " the < > group"),
            ),
            (
                concat!($op, " i ("),
                Cmd::InTokens(TokenKey::$key(Object::Inside(Bracket::Round))),
                concat!($does, " a Euclid's numbers"),
            ),
            (
                concat!($op, " a ("),
                Cmd::InTokens(TokenKey::$key(Object::Around(Bracket::Round))),
                concat!($does, " a Euclid, with its brackets"),
            ),
        ]
    };
}

/// `d`, `c` and `y` with each text object.
pub const OBJECTS: [&[(&str, Cmd, &str)]; 3] = [
    objects!("d", Delete, "Take away"),
    objects!("c", Change, "Type over"),
    objects!("y", Yank, "Keep, for p,"),
];

/// What a key did, for whoever shows the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Did {
    /// The cursor moved, or nothing changed.
    Moved,
    /// The text changed, or what's typed did.
    Changed,
    /// Back to the row.
    Out,
    /// `i` or `a` on a field of fixed length, which has no room between its tokens: type it all,
    /// as on the row.
    TypeAll,
    /// Nothing done, and why.
    Refused(String),
}

/// A field of tokens with the cursor in it: the text, and a count typed before a key.
#[derive(Debug, Clone)]
pub struct Tokens {
    pub field: Field,
    count: Option<usize>,
}

/// The largest count.
const MOST: usize = 999;

impl Tokens {
    pub fn new(field: Field) -> Self {
        Self { field, count: None }
    }

    /// Whether a character typed among the tokens is a token typed in place.
    pub fn types(&self, c: char) -> bool {
        (self.field.grammar().types)(c)
    }

    /// A key of the table, from the token at `at`, which it moves.
    pub fn key(&mut self, key: TokenKey, at: &mut usize) -> Did {
        let count = self.count.take().unwrap_or(1);
        let field = &mut self.field;
        let turn = |field: &mut Field, sign, large| {
            (0..count)
                .try_for_each(|_| field.turn(*at, sign, large))
                .map(|()| None)
        };
        // The cursor's new place, where a change moves it.
        let changed: Result<Option<usize>, &'static str> = match key {
            TokenKey::Back => {
                if field.bad().is_some() {
                    field.put_back();
                }
                return Did::Out;
            }
            TokenKey::Done => {
                return match field.bad() {
                    Some(bad) => Did::Refused(format!(
                        "{}: Enter keeps text that reads, and Esc puts back the last that did",
                        bad.why
                    )),
                    None => Did::Out,
                };
            }
            TokenKey::Move(motion) => {
                return match field.moved(*at, motion, count) {
                    Some(to) => {
                        *at = to;
                        Did::Moved
                    }
                    None => Did::Out,
                };
            }
            TokenKey::Type(place) if place != Place::Over && field.fixed().is_some() => {
                return Did::TypeAll;
            }
            TokenKey::Yank(object) => {
                return match field.yank(*at, object) {
                    Ok(()) => Did::Moved,
                    Err(why) => Did::Refused(why.to_owned()),
                };
            }
            TokenKey::Increase => turn(field, 1, false),
            TokenKey::Decrease => turn(field, -1, false),
            TokenKey::IncreaseLarge => turn(field, 1, true),
            TokenKey::DecreaseLarge => turn(field, -1, true),
            TokenKey::Remove => (0..count).try_fold(*at, |at, _| field.remove(at)).map(Some),
            TokenKey::Rest => field.rest(*at).map(|()| None),
            TokenKey::Replace => field.insert(*at, Place::Over, true).map(|()| None),
            TokenKey::Type(place) => field.insert(*at, place, false).map(|()| None),
            TokenKey::Delete(object) => field.delete(*at, object).map(Some),
            TokenKey::Change(object) => field.change(*at, object).map(|()| None),
            TokenKey::Paste => field.paste(*at).map(Some),
        };
        match changed {
            Ok(to) => {
                *at = to.unwrap_or(*at);
                Did::Changed
            }
            Err(why) => Did::Refused(why.to_owned()),
        }
    }

    /// A key that isn't the table's: while typing, into the text, `Esc` and `Enter` back to the
    /// tokens; and among them, a token the grammar types in place, which moves on to the next,
    /// or a count's digit.
    pub fn text(&mut self, code: KeyCode, at: &mut usize) -> Did {
        let field = &mut self.field;
        if field.inserting().is_some() {
            match code {
                KeyCode::Esc | KeyCode::Enter => *at = field.end_insert(),
                KeyCode::Backspace => field.backspace(),
                KeyCode::Char(c) if !field.typed(c) => *at = field.end_insert(),
                _ => {}
            }
            return Did::Changed;
        }
        let KeyCode::Char(c) = code else {
            return Did::Moved;
        };
        if self.types(c) {
            let field = &mut self.field;
            field.put(*at, &c.to_string());
            *at = (*at + 1).min(field.tokens().len().saturating_sub(1));
            return Did::Changed;
        }
        // A count, but a 0 alone, as in Vim.
        if let Some(digit) = c.to_digit(10).filter(|d| *d > 0 || self.count.is_some()) {
            let count = self.count.unwrap_or(0) * 10 + digit as usize;
            self.count = Some(count.min(MOST));
        }
        Did::Moved
    }
}
