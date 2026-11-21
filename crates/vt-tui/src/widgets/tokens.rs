//! A field of tokens (decision 456): a text-like setting drawn a token at a time, the cursor's lit,
//! what's typed with a `_` where the next character goes, and the token where the text stops
//! reading in the warning colour with `✗` under it. Each token takes a cell, over what it's for
//! in a second line, as `Weights`' digits are over their notes. While the phrase plays, the token
//! that wrote the row playing is in the playhead's colour (decision 460).

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use vt_model::notation::Field;

use crate::input::mode::Mode;
use crate::widgets::theme;

/// The mark under a token the text stops reading at.
pub(crate) const BAD: &str = "✗";

/// A field's two lines, its tokens and what's under them.
pub(crate) struct TokenCells<'a> {
    pub field: &'a Field,
    /// The token the cursor is on, while it's in the field.
    pub cursor: Option<usize>,
    /// Where what's typed goes, by byte, while it's typed.
    pub typing_at: Option<usize>,
    /// Whether the cursor is on the field's row, which lights the tokens as one value while it
    /// isn't in them.
    pub current: bool,
    /// The room each token takes, so what's under it fits.
    pub cell: usize,
    /// What's under each token: what it's for.
    pub under: &'a [&'static str],
    /// The token that wrote the row playing, while the phrase plays.
    pub playing: Option<usize>,
}

/// A piece of the field's line: a token by its place, or where what's typed goes.
enum Item {
    Token(usize),
    Typing,
}

impl TokenCells<'_> {
    /// The tokens' line and the line under it.
    pub(crate) fn lines(&self) -> [Vec<Span<'static>>; 2] {
        let field = self.field;
        let mut items: Vec<(usize, Item)> = field
            .tokens()
            .iter()
            .enumerate()
            .map(|(nth, token)| (token.at.start, Item::Token(nth)))
            .collect();
        if let Some(at) = self.typing_at {
            let before = items.iter().filter(|(start, _)| *start < at).count();
            items.insert(before, (at, Item::Typing));
        }
        // Where the text stops reading: a token, or past the last, which isn't marked while a
        // token's still to be typed.
        let bad = field.bad().and_then(|_| match field.bad_token() {
            Some(nth) => Some(nth),
            None if self.typing_at.is_some() => None,
            None => Some(field.tokens().len()),
        });
        let lit = theme::cursor(Mode::Normal);
        let shown: Vec<Span<'static>> = items
            .iter()
            .map(|(_, item)| match item {
                Item::Typing => Span::styled("_", theme::cursor(Mode::Insert)),
                Item::Token(nth) => {
                    let text = field.token(*nth).unwrap_or_default().to_owned();
                    let style = match bad {
                        Some(bad) if bad == *nth => theme::warning(),
                        _ if self.typing_at.is_none() && self.cursor == Some(*nth) => lit,
                        _ if self.playing == Some(*nth) => theme::active(),
                        _ => Style::new(),
                    };
                    Span::styled(text, style)
                }
            })
            .collect();
        let gap = " ".repeat(self.cell - 1);
        let top = if self.current && self.cursor.is_none() && self.typing_at.is_none() {
            // Lit as one value, the gaps and all, as a value on its row is.
            let all: Vec<&str> = shown.iter().map(|span| span.content.as_ref()).collect();
            vec![Span::styled(all.join(&gap), lit)]
        } else {
            let mut top = Vec::with_capacity(shown.len() * 2);
            for (nth, span) in shown.into_iter().enumerate() {
                if nth > 0 {
                    top.push(Span::raw(gap.clone()));
                }
                top.push(span);
            }
            top
        };
        let cell = self.cell;
        let under = self
            .under
            .iter()
            .enumerate()
            .map(|(nth, what)| match bad {
                Some(bad) if bad == nth => Span::styled(format!("{BAD:<cell$}"), theme::warning()),
                _ if self.current && self.cursor == Some(nth) => Span::styled(
                    format!("{what:<cell$}"),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                _ => Span::styled(format!("{what:<cell$}"), theme::dim()),
            })
            .collect();
        [top, under]
    }
}
