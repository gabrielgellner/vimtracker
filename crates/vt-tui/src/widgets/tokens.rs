//! A field of tokens (decision 456): a text-like setting drawn a token at a time, the cursor's lit,
//! what's typed with a `_` where the next character goes, and the token where the text stops
//! reading in the warning colour with `✗` under it. Each token takes a cell, over what it's for
//! in a second line, as `Weights`' digits are over their notes and a `Tonnetz`'s moves over their
//! chords. A field longer than its room shows the tokens round the cursor, `…` where it's cut,
//! as the `:` line scrolls (decision 459). While the phrase plays, the token that wrote the row
//! playing is in the playhead's colour (decision 460).

use std::ops::Range;

use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use vt_model::notation::Field;
use vt_model::scale::Scale;
use vt_model::shape::{
    Context, GenParam, MAX_LIST, MoveList, Shape, ShapeColumn, TonnetzMove, degree_count,
    degree_name,
};

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
    /// The room each token takes, so what's under it fits: a character and the gap after it,
    /// which tokens longer than a character keep between them too.
    pub cell: usize,
    /// What's under each token: what it's for.
    pub under: &'a [&'a str],
    /// The tokens drawn, by place: all of them, or those round the cursor where they don't fit.
    pub shown: Range<usize>,
    /// The token that wrote the row playing, while the phrase plays.
    pub playing: Option<usize>,
}

/// What stands for the tokens a field cut off, at either end.
pub(crate) const CUT: &str = "…";

/// The room each of `Weights`' digits takes, so the note under it fits, `C#` and a space.
pub(crate) const WEIGHT_CELL: usize = 3;
/// The room each of a `Tonnetz`'s moves takes, so the chord it reaches fits under it, `C#m` and a
/// space.
const MOVE_CELL: usize = 4;
/// The room each of a mask's bits takes.
const BIT_CELL: usize = 2;
/// The spaces between a `List`'s values in its field, so each stands apart.
const LIST_GAP: usize = 2;
/// The room each of an `Own` rhythm's hits and rests takes, and a space.
const RHYTHM_CELL: usize = 2;

/// How a generator setting's field is laid out, in the shape panel and on the Gen page alike.
pub(crate) struct Layout {
    /// The room each token takes, [`TokenCells::cell`].
    pub cell: usize,
    /// The room each takes with the gap after it, for what's in sight.
    pub step: usize,
    /// What's under each token: `Weights`' notes, the chord each move reaches, or nothing.
    pub under: Vec<String>,
    /// What's beside the tokens: a mask's byte.
    pub beside: Option<String>,
}

impl Layout {
    /// The layout of a setting's field, from the shape it's in and what its settings act on.
    pub(crate) fn of(param: GenParam, field: &Field, shape: &Shape, context: &Context) -> Self {
        let scale = match context.column {
            ShapeColumn::Note { scale, .. } => Some(scale),
            _ => None,
        };
        let (cell, under) = match param {
            GenParam::Weights => {
                let notes = scale.map_or_else(Vec::new, |scale| {
                    (0..degree_count(scale))
                        .map(|degree| degree_name(scale, degree).to_owned())
                        .collect()
                });
                (WEIGHT_CELL, notes)
            }
            GenParam::MoveList => (MOVE_CELL, chords(shape, context).split_off(1)),
            GenParam::Flip | GenParam::Keep => (BIT_CELL, Vec::new()),
            // An `Own` rhythm's tokens a space apart, as its text is typed.
            GenParam::OwnText | GenParam::MiniText => (RHYTHM_CELL, Vec::new()),
            _ => (LIST_GAP + 1, Vec::new()),
        };
        let step = match param {
            // A list's longest value and its gap.
            GenParam::Values => {
                let longest = field.tokens().iter().map(|token| token.at.len()).max();
                longest.unwrap_or(2) + LIST_GAP
            }
            GenParam::OwnText | GenParam::MiniText => {
                let longest = field.tokens().iter().map(|token| token.at.len()).max();
                longest.unwrap_or(1) + RHYTHM_CELL - 1
            }
            _ => cell,
        };
        let beside = matches!(param, GenParam::Flip | GenParam::Keep)
            .then(|| format!("   {}", param.shown(shape, context)));
        Self {
            cell,
            step,
            under,
            beside,
        }
    }

    /// The tokens in sight in `room` characters, round the cursor or where what's typed goes.
    fn shown(
        &self,
        field: &Field,
        cursor: Option<usize>,
        typing_at: Option<usize>,
        room: usize,
    ) -> Range<usize> {
        let focus = match typing_at {
            Some(at) => field
                .tokens()
                .iter()
                .position(|token| token.at.end >= at)
                .unwrap_or(field.tokens().len()),
            None => cursor.unwrap_or(0),
        };
        let beside = self.beside.as_deref().map_or(0, str::len);
        in_sight(
            field.tokens().len(),
            focus,
            self.step,
            room.saturating_sub(beside),
        )
    }

    /// Whether a setting is drawn as a field of tokens when the cursor isn't in it: `Weights`, a
    /// `Tonnetz`'s moves and a mask always show as what they are, and a `List`'s values only
    /// while the cursor is in them, as a long list reads better as text.
    pub(crate) fn always(param: GenParam) -> bool {
        matches!(
            param,
            GenParam::Weights | GenParam::MoveList | GenParam::Flip | GenParam::Keep
        )
    }

    /// A field's two lines in `room` characters, as many tokens as fit round the cursor, or where
    /// what's typed goes, with what's beside them after the first and the token `playing` lit.
    pub(crate) fn lines(
        &self,
        field: &Field,
        cursor: Option<usize>,
        typing_at: Option<usize>,
        current: bool,
        playing: Option<usize>,
        room: usize,
    ) -> [Vec<Span<'static>>; 2] {
        let beside = self.beside.as_deref().unwrap_or_default();
        let shown = self.shown(field, cursor, typing_at, room);
        let under: Vec<&str> = self.under.iter().map(String::as_str).collect();
        let [mut top, below] = TokenCells {
            field,
            cursor,
            typing_at,
            current,
            cell: self.cell,
            under: &under,
            shown,
            playing,
        }
        .lines();
        if !beside.is_empty() {
            top.push(Span::styled(beside.to_owned(), theme::dim()));
        }
        [top, below]
    }
}

/// Whether a setting's buffer has the rows line under its text: those whose tokens a render
/// records, `Weights`, a `List`'s values and an `Own` rhythm's hits (decision 460).
pub(crate) fn has_rows_line(param: GenParam) -> bool {
    matches!(
        param,
        GenParam::Weights | GenParam::Values | GenParam::OwnText | GenParam::MiniText
    )
}

/// The keys a setting's buffer says along its foot (decision 463).
pub(crate) fn buffer_hint(param: GenParam) -> &'static str {
    match param {
        GenParam::Values => "= - turn  x out  a after  K what goes  Enter close",
        GenParam::OwnText | GenParam::MiniText => {
            "= - turn  x out  ~ rest  a after  K what goes  Enter close"
        }
        GenParam::MoveList => "= - turn P L R  x out  K what goes  Enter close",
        GenParam::Flip | GenParam::Keep => "= - switch  0 1 types  Enter close",
        GenParam::Weights => "= - change  0-F types  K what goes  Enter close",
        _ => "h l move  K what goes  Enter close",
    }
}

/// A `Tonnetz`'s first chord and the one each of its moves reaches, named as `C`, `Cm`.
fn chords(shape: &Shape, context: &Context) -> Vec<String> {
    let scale = match context.column {
        ShapeColumn::Note { scale, .. } => scale,
        _ => Scale::default(),
    };
    let moves = shape.tonnetz.list.as_slice().len();
    let chords = shape
        .tonnetz
        .chords(shape.from, shape.seed, scale, moves + 1);
    chords.into_iter().map(|chord| chord.name()).collect()
}

/// The chord each of `P`, `L` and `R` reaches in the place of a `Tonnetz`'s move `at`, from the
/// chord before it, for `K` to say; none on any other setting.
pub(crate) fn reached(param: GenParam, shape: &Shape, context: &Context, at: usize) -> Vec<String> {
    if param != GenParam::MoveList {
        return Vec::new();
    }
    TonnetzMove::ALL
        .into_iter()
        .map(|each| {
            let mut moves = shape.tonnetz.list.as_slice().to_vec();
            match moves.get_mut(at) {
                Some(place) => *place = each,
                None => moves.push(each),
            }
            let mut tried = *shape;
            tried.tonnetz.list = MoveList::new(&moves);
            let at = at.min(moves.len() - 1);
            chords(&tried, context)
                .get(at + 1)
                .cloned()
                .unwrap_or_default()
        })
        .collect()
}

/// What the status line says after the name of the token at `at` in a setting's field: how much
/// room a list has, `, 3 of 256 values`; the chords a move goes between, `, Cm to G#`; or a
/// mask's byte, `, 8D in hex`.
pub(crate) fn said_after(
    param: GenParam,
    field: &Field,
    at: usize,
    shape: &Shape,
    context: &Context,
) -> String {
    match param {
        GenParam::Values => {
            let most = field.most().unwrap_or(MAX_LIST);
            format!(", {} of {most} values", field.tokens().len())
        }
        GenParam::MoveList => {
            let chords = chords(shape, context);
            match (chords.get(at), chords.get(at + 1)) {
                (Some(from), Some(to)) => format!(", {from} to {to}"),
                _ => String::new(),
            }
        }
        GenParam::Flip | GenParam::Keep => format!(", {} in hex", param.shown(shape, context)),
        _ => String::new(),
    }
}

/// The tokens a field of `count` shows in `room` characters, a `cell` each, round the one in
/// `focus`: from the first while it's in sight, then with it last, and the last ones at the end,
/// a `…` taking a token's cell at each end that's cut.
pub(crate) fn in_sight(count: usize, focus: usize, cell: usize, room: usize) -> Range<usize> {
    // The last token needs only its own room, not the gap after it.
    let cells = ((room + 1) / cell.max(1)).max(3);
    if count <= cells {
        return 0..count;
    }
    // Cut at one end, a cell is the `…`; cut at both, two are.
    let one_cut = cells - 1;
    if focus < one_cut {
        return 0..one_cut;
    }
    let tail = count - one_cut;
    if focus >= tail {
        return tail..count;
    }
    focus + 1 - (cells - 2)..focus + 1
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
            .filter(|(nth, _)| self.shown.contains(nth))
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
        let mut shown: Vec<Span<'static>> = items
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
        let count = field.tokens().len();
        let (before, after) = (self.shown.start > 0, self.shown.end < count);
        if before {
            shown.insert(0, Span::styled(CUT, theme::dim()));
        }
        if after {
            shown.push(Span::styled(CUT, theme::dim()));
        }
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
        let mut under: Vec<Span<'static>> = Vec::new();
        if before && !self.under.is_empty() {
            under.push(Span::raw(" ".repeat(cell)));
        }
        under.extend(
            self.under
                .iter()
                .enumerate()
                .filter(|(nth, _)| self.shown.contains(nth))
                .map(|(nth, what)| match bad {
                    Some(bad) if bad == nth => {
                        Span::styled(format!("{BAD:<cell$}"), theme::warning())
                    }
                    _ if self.current && self.cursor == Some(nth) => Span::styled(
                        format!("{what:<cell$}"),
                        Style::new().add_modifier(Modifier::BOLD),
                    ),
                    _ => Span::styled(format!("{what:<cell$}"), theme::dim()),
                }),
        );
        [top, under]
    }
}
