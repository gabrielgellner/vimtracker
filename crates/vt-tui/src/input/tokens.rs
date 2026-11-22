//! The token editor's keys (decision 456), in the buffer a text-like setting opens from its cell
//! or row (decision 463): `Enter` (or `l` on the shape panel) opens it on the first token, `i` and
//! `a` typing at the end, keeping the text, and `cc` and `S` typing it all again. Its cursor is on
//! a whole token, moving by the grammar, each token turned in its own units, taken away, rested,
//! replaced and typed over, the text checked on every key. One key table, in a mode of its own;
//! the buffer is a dialog, so a key it doesn't bind does nothing but play, until `Esc` or `Enter`
//! closes it.
//!
//! Whoever shows the field keeps its cursor, by its token's place, and is told what each key did,
//! so it can write the text that reads into its setting and say what the cursor is on.
//!
//! `K` lists what can go at the cursor, and `C-n` while typing completes the word from the same
//! list, which is the `:` line's list of matches (decision 458), with Neovim's keys: `C-n` and
//! `C-p` move through it, `C-y` takes one and `C-e` closes it (decision 477).

use ratatui::crossterm::event::KeyCode;
use vt_model::notation::{Around, Bracket, Field, GrammarId, Motion, Object, Place, Spot};
use vt_model::shape::GenParam;
use vt_model::shape::params::Typing;

use crate::input::bindings::Cmd;
use crate::input::keys::Key;
use crate::overlays::command::completion::{CommandMenu, Completions, offered};

/// What a key does in a field of tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKey {
    /// `Esc`: the buffer closed, the last text that read put back.
    Back,
    /// `Enter`: the buffer closed, while the text reads.
    Done,
    Move(Motion),
    /// `0`: the first token, or a count's digit after another, as in Vim.
    Zero,
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
    /// `K`: what can go in the token's place, listed with what each means.
    Offer,
}

/// The token editor's rows of the binding table, but its text objects.
pub const KEYS: &[(&str, Cmd, &str)] = &[
    (
        "Esc",
        Cmd::InTokens(TokenKey::Back),
        "Close, putting back the last text that read",
    ),
    (
        "Enter",
        Cmd::InTokens(TokenKey::Done),
        "Close, where the text reads",
    ),
    (
        "h",
        Cmd::InTokens(TokenKey::Move(Motion::Left)),
        "The token before, into and out of groups",
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
        "0",
        Cmd::InTokens(TokenKey::Zero),
        "The first token, or after a count's digits, another",
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
        "D",
        Cmd::InTokens(TokenKey::Delete(Object::ToEnd)),
        "Take away from the token to the end of its group, or of the line",
    ),
    (
        "C",
        Cmd::InTokens(TokenKey::Change(Object::ToEnd)),
        "Type over from the token to the end of its group, or of the line",
    ),
    (
        "i",
        Cmd::InTokens(TokenKey::Type(Place::Before)),
        "Type before the token, or on a field of fixed length over it",
    ),
    (
        "a",
        Cmd::InTokens(TokenKey::Type(Place::After)),
        "Type after the token, or on a field of fixed length over it",
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
    (
        "K",
        Cmd::InTokens(TokenKey::Offer),
        "List what can go in the token's place, each with what it means",
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
    /// The buffer closed.
    Out,
    /// What's typed kept, and the buffer closed: `Enter`, typing or not.
    Kept,
    /// Nothing done, and why.
    Refused(String),
}

/// A field of tokens with the cursor in it: the text, a count typed before a key, and the list
/// `K` or `C-n` opened.
#[derive(Debug, Clone)]
pub struct Tokens {
    pub field: Field,
    count: Option<usize>,
    pub menu: Option<CommandMenu>,
}

/// The title of the list `K` opens on a token, and of the one it and `C-n` open while typing.
const IN_PLACE: &str = "In its place";
const HERE: &str = "What can go here";

/// Whether a field is a `List`'s values on a column of hex values.
fn hex_list(field: &Field) -> bool {
    std::ptr::eq(field.grammar(), GrammarId::List.grammar())
        && GenParam::Values.typing(field.column()) == Typing::HexList
}

/// The largest count.
const MOST: usize = 999;

/// What the status line says as `i`, `a`, `cc` or `S` opens a field of fixed length.
const FIXED: &str = "A field of fixed length: type over its tokens in turn, or s over one";

/// How the buffer opens from a setting's cell or row (decision 463).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opening {
    /// `Enter` on a cell, `l` on the shape panel's row: on the first token.
    Tokens,
    /// `i` and `a`: typing at the end, keeping the text.
    Append,
    /// `cc` and `S`: typing it all again.
    Replace,
}

impl Tokens {
    pub fn new(field: Field) -> Self {
        Self {
            field,
            count: None,
            menu: None,
        }
    }

    /// The buffer opened on a field, and the token the cursor starts on, or why it's opened on
    /// the first token rather than typing: on a field of fixed length, there's no room after
    /// its tokens, so `i` and `a` open on the first, its tokens typed over in turn, and `cc` and
    /// `S` put each back to what it starts as first; and a field with all the tokens it takes
    /// has no room for another.
    pub fn open(mut field: Field, how: Opening) -> (Self, usize, Option<String>) {
        let fixed = field.fixed().is_some();
        let last = field.tokens().len().saturating_sub(1);
        let (at, why) = match how {
            Opening::Tokens => (0, None),
            Opening::Append if fixed => (0, Some(FIXED.to_owned())),
            Opening::Append => match field.append() {
                Ok(()) => (last, None),
                Err(why) => (last, Some(why.to_owned())),
            },
            Opening::Replace => {
                field.replace_all();
                (0, fixed.then(|| FIXED.to_owned()))
            }
        };
        (Self::new(field), at, why)
    }

    /// Whether every key goes to the text or the list: while typing, or with a list open.
    pub fn completes(&self) -> bool {
        self.field.inserting().is_some() || self.menu.is_some()
    }

    /// What can go at the cursor, starting with what's typed of the word, as a list of matches.
    fn found(&self, at: usize, around: &Around, title: &'static str) -> Completions {
        let (_, bytes) = self.field.spot(at);
        let typed = self.field.typed_word().unwrap_or_default();
        offered(bytes.start, title, typed, &self.field.offers(at, around))
    }

    /// `K`: what can go at the cursor, each with what it means, opened on the token there.
    pub fn offer(&mut self, at: usize, around: &Around) -> Did {
        let typing = self.field.inserting().is_some();
        let found = self.found(at, around, if typing { HERE } else { IN_PLACE });
        if found.candidates.is_empty() {
            return Did::Refused("Nothing is listed here".to_owned());
        }
        let mut picker = found.picker();
        if let (Spot::On { text, .. }, _) = self.field.spot(at)
            && let Some(own) = found
                .candidates
                .iter()
                .position(|candidate| candidate.text == text)
        {
            picker.selected = own;
        }
        let typed = self.field.typed_word().unwrap_or_default().to_owned();
        self.menu = Some(CommandMenu {
            picker,
            start: found.start,
            typed,
        });
        Did::Moved
    }

    /// `C-n`, `C-p` or `C-y` while typing with no list open: the only match put in place of the
    /// word, or what every match starts with and the list of them, the first highlighted, or the
    /// last for `C-p`, as on the `:` line.
    fn complete(&mut self, at: &mut usize, around: &Around, backwards: bool) -> Did {
        if self.field.inserting().is_none() {
            return Did::Moved;
        }
        let found = self.found(*at, around, HERE);
        match found.candidates.as_slice() {
            [] => Did::Refused(match self.field.typed_word() {
                Some("") | None => "Nothing is listed here".to_owned(),
                Some(word) => format!("Nothing here starts with {word}"),
            }),
            [only] => {
                let only = only.text.clone();
                self.chosen(at, &only)
            }
            _ => {
                let typed = self.field.typed_word().unwrap_or_default().to_owned();
                let common = found.common().to_owned();
                let did = self.chosen(at, &common);
                let mut menu = CommandMenu {
                    picker: found.picker(),
                    start: found.start,
                    typed,
                };
                if backwards {
                    menu.step(-1);
                }
                self.menu = self.field.inserting().is_some().then_some(menu);
                did
            }
        }
    }

    /// What was offered, put in place of the token or the word.
    fn chosen(&mut self, at: &mut usize, text: &str) -> Did {
        match self.field.choose(*at, text) {
            Ok(to) => {
                *at = to.unwrap_or(*at);
                Did::Changed
            }
            Err(why) => Did::Refused(why.to_owned()),
        }
    }

    /// A key while the list is open, or none where it closes the list and is read as it would be
    /// without it. `C-n` and `C-p`, the arrows and, among the tokens, `j` and `k` move through it,
    /// `C-y` and `Enter` put what's highlighted in place, `Esc` closes it, keeping what's typed,
    /// and `C-e` closes it, putting back what was typed before it opened; while typing, the rest
    /// type, and the list narrows to what still matches.
    fn menu_key(&mut self, key: Key, at: &mut usize) -> Option<Did> {
        let menu = self.menu.as_mut()?;
        let typing = self.field.inserting().is_some();
        match (key.code, key.ctrl) {
            (KeyCode::Down, _) | (KeyCode::Char('n'), true) => menu.step(1),
            (KeyCode::Up, _) | (KeyCode::Char('p'), true) => menu.step(-1),
            (KeyCode::Char('j'), false) if !typing => menu.step(1),
            (KeyCode::Char('k'), false) if !typing => menu.step(-1),
            (KeyCode::Enter, _) | (KeyCode::Char('y'), true) => {
                let chosen = menu.highlighted().map(str::to_owned);
                self.menu = None;
                return Some(match chosen {
                    Some(text) => self.chosen(at, &text),
                    None => Did::Moved,
                });
            }
            (KeyCode::Char('e'), true) => {
                let typed = std::mem::take(&mut menu.typed);
                self.menu = None;
                return Some(if typing {
                    self.chosen(at, &typed)
                } else {
                    Did::Moved
                });
            }
            (KeyCode::Esc, _) => self.menu = None,
            _ if typing => return None,
            _ => {
                self.menu = None;
                return None;
            }
        }
        Some(Did::Moved)
    }

    /// After a key typed with the list open: what still matches, the highlight kept on the same
    /// one where it's there, or the list closed once nothing does or a new word starts.
    fn narrow(&mut self, at: usize, around: &Around) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        if self.field.inserting().is_none() {
            return;
        }
        let found = self.found(at, around, menu.picker.title);
        if found.start != menu.start || found.candidates.is_empty() {
            return;
        }
        let kept = menu.highlighted().map(str::to_owned);
        let mut picker = found.picker();
        if let Some(own) = found
            .candidates
            .iter()
            .position(|candidate| Some(&candidate.text) == kept.as_ref())
        {
            picker.selected = own;
        }
        self.menu = Some(CommandMenu {
            picker,
            start: found.start,
            typed: self.field.typed_word().unwrap_or_default().to_owned(),
        });
    }

    /// Whether a character typed among the tokens is a token typed in place.
    pub fn types(&self, c: char) -> bool {
        (self.field.grammar().types)(c)
    }

    /// A key of the table, from the token at `at`, which it moves.
    pub fn key(&mut self, key: TokenKey, at: &mut usize) -> Did {
        if let (TokenKey::Zero, Some(count)) = (key, self.count) {
            self.count = Some((count * 10).min(MOST));
            return Did::Moved;
        }
        let key = match key {
            TokenKey::Zero => TokenKey::Move(Motion::First),
            key => key,
        };
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
            // Kept, as `Enter` while typing is, so a pattern's `Enter` writes it either way.
            TokenKey::Done => {
                return match field.bad() {
                    Some(bad) => Did::Refused(format!(
                        "{}: Enter keeps text that reads, and Esc puts back the last that did",
                        bad.why
                    )),
                    None => Did::Kept,
                };
            }
            // `h` from the first stays there, as the buffer is a dialog.
            TokenKey::Move(motion) => {
                if let Some(to) = field.moved(*at, motion, count) {
                    *at = to;
                }
                return Did::Moved;
            }
            // `0` is the first token by now; and what's offered depends on what's around the
            // field, which [`Tokens::offer`] is given by whoever shows it.
            TokenKey::Zero | TokenKey::Offer => return Did::Moved,
            // A field of fixed length has no room between its tokens, so it's typed over.
            TokenKey::Type(_) if field.fixed().is_some() => {
                field.insert(*at, Place::Over, false).map(|()| None)
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

    /// A key that isn't the table's: with a list open, the list's; while typing, into the text,
    /// `K` at the start of a word listing what can go there, `C-n`, `C-p` and `C-y` completing,
    /// `Esc` back to the tokens and `Enter` closing the buffer where the text reads; and among
    /// them, a token the grammar types in place, which moves on to the next, or a count's digit.
    /// Any other key held with `Ctrl` does nothing.
    pub fn text(&mut self, key: Key, at: &mut usize, around: &Around) -> Did {
        if let Some(did) = self.menu_key(key, at) {
            return did;
        }
        let code = key.code;
        let field = &mut self.field;
        if field.inserting().is_some() {
            if key.ctrl {
                return match code {
                    KeyCode::Char(c @ ('n' | 'p' | 'y')) => self.complete(at, around, c == 'p'),
                    _ => Did::Moved,
                };
            }
            match code {
                KeyCode::Char('K') if field.typed_word() == Some("") => {
                    return self.offer(*at, around);
                }
                KeyCode::Esc => *at = field.end_insert(),
                // `Enter` keeps what's typed and closes the buffer, as a dialog's does, where the
                // text reads.
                KeyCode::Enter => {
                    *at = field.end_insert();
                    return match field.bad() {
                        Some(_) => Did::Changed,
                        None => Did::Kept,
                    };
                }
                KeyCode::Backspace => field.backspace(),
                // A list of hex values takes a space after each two digits, as it did typed whole,
                // so `704055` is three values.
                KeyCode::Char(c) if hex_list(field) && c.is_ascii_hexdigit() => {
                    if field.typed_word().is_some_and(|word| word.len() >= 2) {
                        field.typed(' ');
                    }
                    field.typed(c.to_ascii_uppercase());
                }
                KeyCode::Char(c) if !field.typed(c) => *at = field.end_insert(),
                _ => {}
            }
            self.narrow(*at, around);
            return Did::Changed;
        }
        let KeyCode::Char(c) = code else {
            return Did::Moved;
        };
        if key.ctrl {
            return Did::Moved;
        }
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
