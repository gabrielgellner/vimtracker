//! The grammars of a generator's text-like settings, and a [`Field`] that edits one a token at a
//! time (#1086, decision 456).
//!
//! A setting whose value is a short string with a grammar of its own, `Weights`' digits now and a
//! `List`'s values, `Tonnetz`'s moves and Tidal's mini-notation later, is edited as a row of
//! tokens: a cursor on a whole token, moving by the grammar, turned by `=` and `-` in its own
//! units. A grammar is declared once here, as a generator's settings are in `shape::params`, so
//! the panel, the Gen page, the status line and `Space d` read the one declaration of how its text
//! splits, whether it reads, what each token means, how it turns, what can go in its place and
//! which rows it writes.

mod field;
mod weights;

use std::ops::Range;

use crate::shape::ShapeColumn;

pub use field::{Field, Insert, Motion, Object, Place};

/// Which grammar a setting's text is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrammarId {
    /// A random fill's `Weights`: a hex digit for each of the scale's degrees, from the root, as
    /// decision 388 has them.
    Weights,
}

impl GrammarId {
    /// Every grammar.
    pub const ALL: [Self; 1] = [Self::Weights];

    /// Its declaration.
    pub fn grammar(self) -> &'static Grammar {
        match self {
            Self::Weights => &weights::WEIGHTS,
        }
    }
}

/// A kind of bracket, round a group of tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bracket {
    /// `[ ]`, a group that splits its place.
    Square,
    /// `< >`, one of them each lap.
    Angle,
    /// `( )`, a Euclid's numbers.
    Round,
}

impl Bracket {
    /// The bracket a character opens or closes, and whether it opens.
    pub fn of(c: char) -> Option<(Self, bool)> {
        Some(match c {
            '[' => (Self::Square, true),
            ']' => (Self::Square, false),
            '<' => (Self::Angle, true),
            '>' => (Self::Angle, false),
            '(' => (Self::Round, true),
            ')' => (Self::Round, false),
            _ => return None,
        })
    }
}

/// What a token is, for what it means and how it turns. A grammar says which it splits into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A weight: a hex digit, how often its degree comes.
    Weight,
    /// A value in the column's own units.
    Value,
    /// A rest: `.`, or `~` as Tidal writes one.
    Rest,
    /// What follows a token and changes it, `*2` or `?30`.
    Modifier,
    /// A bracket opening a group.
    Open(Bracket),
    /// A bracket closing one.
    Close(Bracket),
    /// Text the grammar can't read as a token.
    Unread,
}

impl Kind {
    /// Whether it opens or closes a group.
    pub fn bracket(self) -> Option<(Bracket, bool)> {
        match self {
            Self::Open(bracket) => Some((bracket, true)),
            Self::Close(bracket) => Some((bracket, false)),
            _ => None,
        }
    }
}

/// One token of a text: what it is, where it is and how many groups it's inside. A group's
/// brackets are at the depth of the group itself, and what it holds one deeper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    /// Its place in the text, by byte.
    pub at: Range<usize>,
    pub depth: usize,
}

impl Token {
    /// A token at depth 0, which [`split`] works out again.
    pub fn new(kind: Kind, at: Range<usize>) -> Self {
        Self { kind, at, depth: 0 }
    }
}

/// Why a text doesn't read: where, by byte, and why, as the status line says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bad {
    pub at: usize,
    pub why: String,
}

/// What the token at a place is for: its name on the status line, `Weight of G`, and what its
/// meaning's `{}` is about, `G`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct About {
    pub name: String,
    pub subject: String,
}

/// One of what `K` and `Tab` offer in a token's place, with what it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub text: String,
    pub meaning: String,
}

/// A grammar, declared once: how its text splits into tokens, whether it reads, what each kind of
/// token means and how it turns, what can go in its place and which rows it writes, and what the
/// editor's keys do with it.
#[derive(Debug)]
pub struct Grammar {
    /// Every kind of token it splits into. Each has a meaning, which a test holds it to.
    pub kinds: &'static [Kind],
    /// The tokens of a text, in order, at depth 0, which [`split`] works out.
    pub split: fn(&str) -> Vec<Token>,
    /// Whether a text reads, on this column, or where and why it doesn't.
    pub read: fn(&str, ShapeColumn) -> Result<(), Bad>,
    /// What a kind of token does, for `Space d`, with `{}` where what it's for goes.
    pub meaning: fn(Kind) -> &'static str,
    /// What the token at a place, counted from 0, is for.
    pub about: fn(Kind, usize, ShapeColumn) -> About,
    /// A token turned by one of `=` and `-`, by its sign, or of `+` and `_` where `large`, as its
    /// new text, or none where it doesn't turn.
    pub turn: fn(Kind, &str, i16, bool) -> Option<String>,
    /// What can go in place of a token of a kind, for `K` and `Tab` (#1088).
    pub offer: fn(Kind, ShapeColumn) -> Vec<Offer>,
    /// The rows of the column the token at a place wrote, from each row's value there, for the
    /// grid to light (#1090).
    pub rows: fn(usize, ShapeColumn, &[Option<u8>]) -> Vec<usize>,
    /// How many tokens it always has on a column, where none can be added or taken away.
    pub fixed: fn(ShapeColumn) -> Option<usize>,
    /// What `x` puts back on a field of fixed length.
    pub blank: &'static str,
    /// What `~` puts in a token's place.
    pub rest: &'static str,
    /// How long each token is, where every one is as long, so typing over one ends with it.
    pub width: Option<usize>,
    /// What goes between two tokens.
    pub between: &'static str,
    /// Whether a character typed among the tokens is a token typed in place, as decision 388's
    /// digits are, rather than a key.
    pub types: fn(char) -> bool,
}

/// A rest, `.`, or `~` as Tidal writes one, so its strings paste.
pub fn is_rest(text: &str) -> bool {
    matches!(text, "." | "~")
}

/// A text's tokens by its grammar, each at its depth among the groups.
pub fn split(grammar: &Grammar, text: &str) -> Vec<Token> {
    let mut tokens = (grammar.split)(text);
    let mut depth = 0usize;
    for token in &mut tokens {
        match token.kind {
            Kind::Open(_) => {
                token.depth = depth;
                depth += 1;
            }
            Kind::Close(_) => {
                depth = depth.saturating_sub(1);
                token.depth = depth;
            }
            _ => token.depth = depth,
        }
    }
    tokens
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::scale::Scale;

    /// A grammar for the editor's tests, of words between spaces and brackets: rests, hex values
    /// and modifiers stuck to what they change.
    pub(crate) static WORDS: Grammar = Grammar {
        kinds: &[
            Kind::Value,
            Kind::Rest,
            Kind::Modifier,
            Kind::Open(Bracket::Square),
            Kind::Close(Bracket::Square),
            Kind::Open(Bracket::Angle),
            Kind::Close(Bracket::Angle),
            Kind::Open(Bracket::Round),
            Kind::Close(Bracket::Round),
            Kind::Unread,
        ],
        split: words,
        read: |text, _| {
            let mut open = Vec::new();
            for token in split(&WORDS, text) {
                match token.kind {
                    Kind::Open(bracket) => open.push((bracket, token.at.start)),
                    Kind::Close(bracket) => match open.pop() {
                        Some((opened, _)) if opened == bracket => {}
                        _ => return Err(bad(token.at.start, "a close with no open")),
                    },
                    Kind::Unread => return Err(bad(token.at.start, "not a value")),
                    _ => {}
                }
            }
            match open.pop() {
                Some((_, at)) => Err(bad(at, "an open with no close")),
                None => Ok(()),
            }
        },
        meaning: |kind| match kind {
            Kind::Value => "a value, {}",
            Kind::Rest => "nothing, {}",
            Kind::Modifier => "a change to what's before it",
            Kind::Open(_) | Kind::Close(_) => "a group",
            Kind::Unread => "what doesn't read",
            Kind::Weight => "",
        },
        about: |_, at, _| About {
            name: format!("Token {at}"),
            subject: at.to_string(),
        },
        turn: |kind, text, sign, large| match kind {
            Kind::Value => {
                let value = i16::from_str_radix(text, 16).ok()?;
                let by = if large { 0x10 } else { 1 };
                Some(format!("{:02X}", (value + sign * by).clamp(0, 0xFF)))
            }
            _ => None,
        },
        offer: |_, _| Vec::new(),
        rows: |_, _, _| Vec::new(),
        fixed: |_| None,
        blank: "00",
        rest: ".",
        width: None,
        between: " ",
        types: |_| false,
    };

    fn bad(at: usize, why: &str) -> Bad {
        Bad {
            at,
            why: why.to_owned(),
        }
    }

    /// Words between spaces, each bracket a token, `*` and `?` starting a modifier.
    fn words(text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut start = None;
        let word = |tokens: &mut Vec<Token>, from: usize, to: usize| {
            let word = &text[from..to];
            let kind = if is_rest(word) {
                Kind::Rest
            } else if word.starts_with(['*', '?']) {
                Kind::Modifier
            } else if u8::from_str_radix(word, 16).is_ok() {
                Kind::Value
            } else {
                Kind::Unread
            };
            tokens.push(Token::new(kind, from..to));
        };
        for (at, c) in text.char_indices() {
            let ends = c.is_whitespace() || c == ',' || Bracket::of(c).is_some();
            let modifier = matches!(c, '*' | '?');
            if (ends || modifier)
                && let Some(from) = start.take()
            {
                word(&mut tokens, from, at);
            }
            if let Some((bracket, opens)) = Bracket::of(c) {
                let kind = if opens {
                    Kind::Open(bracket)
                } else {
                    Kind::Close(bracket)
                };
                tokens.push(Token::new(kind, at..at + 1));
            } else if !ends && start.is_none() {
                start = Some(at);
            }
        }
        if let Some(from) = start {
            word(&mut tokens, from, text.len());
        }
        tokens
    }

    fn note() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale::default(),
            writes: None,
        }
    }

    /// Every kind of token a grammar has says what it means, for `Space d` (decision 456, as
    /// decision 387's tests hold each setting to its words): a grammar with a kind and no meaning
    /// fails here. And what it splits into is among the kinds it says it has.
    #[test]
    fn every_kind_of_token_says_what_it_means() {
        let said = |meaning: &str| !meaning.is_empty() && !meaning.ends_with('.');
        for id in GrammarId::ALL {
            let grammar = id.grammar();
            for kind in grammar.kinds {
                assert!(said((grammar.meaning)(*kind)), "{id:?}'s {kind:?}");
            }
            for text in ["F08F", "FFFFFFFFFFFF", "F0 8", "G1.", ""] {
                for token in split(grammar, text) {
                    assert!(grammar.kinds.contains(&token.kind), "{id:?}: {text}");
                }
            }
        }
        for kind in WORDS.kinds {
            assert!(said((WORDS.meaning)(*kind)), "{kind:?}");
        }
    }

    #[test]
    fn a_text_splits_into_tokens_at_their_depths() {
        let text = "10 [. 20*2] <30 40>";
        let tokens = split(&WORDS, text);
        let shown: Vec<(&str, usize)> = tokens
            .iter()
            .map(|token| (&text[token.at.clone()], token.depth))
            .collect();
        assert_eq!(
            shown,
            [
                ("10", 0),
                ("[", 0),
                (".", 1),
                ("20", 1),
                ("*2", 1),
                ("]", 0),
                ("<", 0),
                ("30", 1),
                ("40", 1),
                (">", 0),
            ]
        );
        assert_eq!(tokens[2].kind, Kind::Rest);
        assert_eq!(tokens[4].kind, Kind::Modifier);
        assert!(is_rest("~"));
        assert!((WORDS.read)(text, note()).is_ok());
        assert_eq!(
            (WORDS.read)("10 [20", note()),
            Err(bad(3, "an open with no close"))
        );
    }
}
