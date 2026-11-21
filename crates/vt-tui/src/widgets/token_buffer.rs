//! A text-like setting's buffer (decision 463): a dialog opened from its cell or row, its whole
//! text drawn as typed, brackets, commas and all, wrapped over as many lines as it takes, so a
//! pattern longer than a cell is seen whole. The cursor's token is lit, what's typed has a `_`
//! where the next character goes, the token where the text stops reading is in the warning colour
//! with `✗` under it (decision 456), and while the phrase plays the token that wrote the row
//! playing is in the playhead's colour (decision 460), with the rows line under the text. A field
//! with something under each token, `Weights`' notes or the chords a `Tonnetz`'s moves reach,
//! keeps a cell a token so what's under it fits. Each token is drawn in its kind's colour
//! (decision 471): what writes a row plain, as a note is in the grid, a rest and an off bit faint,
//! as an empty cell is, a group's brackets and the commas between dim, and what changes a token,
//! `*2` `!` `@3` `?30` and a Euclid's `(3,8)`, in an event FX's colour, as `RTG` and `CHA`, which do
//! what `*` and `?` do, are in the grid.
//!
//! The cell a buffer opens from shows the same text on one line, cut to its width.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::notation::{Bracket, Field, Kind, Mark, Table};

use crate::input::mode::Mode;
use crate::overlays::command::completion::CommandMenu;
use crate::widgets::popup;
use crate::widgets::theme;
use crate::widgets::tokens::{BAD, CUT, Layout};

/// The most a buffer's text takes across before it wraps, so a rhythm of 128 characters is two
/// lines of it.
pub(crate) const ROOM: usize = 64;

/// The rows line's label in a buffer.
const ROWS: &str = "Rows  ";

/// The style a token of a kind is drawn in, by its text (decision 471): what writes a row plain,
/// as a note is in the grid; a rest, an off bit and a weight of `0`, which write nothing, faint, as
/// an empty cell is; a group's brackets dim, as they hold rather than write; what changes the
/// token before it in an event FX's colour, as `RTG` and `CHA` are; and what doesn't read in the
/// warning colour.
pub(crate) fn kind_style(kind: Kind, text: &str) -> Style {
    match kind {
        Kind::Rest => theme::faint(),
        Kind::Bit if !matches!(text, "x" | "X" | "1") => theme::faint(),
        Kind::Weight if text == "0" => theme::faint(),
        Kind::Modifier => change(),
        Kind::Open(_) | Kind::Close(_) => theme::dim(),
        Kind::Unread => theme::warning(),
        Kind::Hit | Kind::Value | Kind::Weight | Kind::Move | Kind::Bit => Style::new(),
    }
}

/// What changes a token: an event FX's colour, as `RTG` and `CHA`, which do what `*` and `?` do.
fn change() -> Style {
    Style::new().fg(theme::PEACH)
}

/// Whether a token is a Euclid's, `(3,8)`, its brackets or a number inside them, which is drawn
/// as what changes a token, as it changes the one before it.
fn euclid(field: &Field, nth: usize) -> bool {
    let tokens = field.tokens();
    let Some(token) = tokens.get(nth) else {
        return false;
    };
    if token.kind.bracket().map(|(bracket, _)| bracket) == Some(Bracket::Round) {
        return true;
    }
    let Some(depth) = token.depth.checked_sub(1) else {
        return false;
    };
    tokens[..nth]
        .iter()
        .rev()
        .find(|before| before.depth == depth && matches!(before.kind, Kind::Open(_)))
        .is_some_and(|open| open.kind == Kind::Open(Bracket::Round))
}

/// The style of a token where nothing's lit on it: its kind's, or a Euclid's.
fn plain_style(field: &Field, nth: usize) -> Style {
    let Some(token) = field.tokens().get(nth) else {
        return Style::new();
    };
    if token.kind != Kind::Unread && euclid(field, nth) {
        return change();
    }
    kind_style(token.kind, field.token(nth).unwrap_or_default())
}

/// The style of what's between two tokens as typed: a comma dim, as a bracket is, or a Euclid's
/// as its numbers are, and a space plain.
fn gap_style(field: &Field, before: Option<usize>, text: &str) -> Style {
    if text.trim().is_empty() {
        return Style::new();
    }
    let inside = before.is_some_and(|nth| {
        euclid(field, nth)
            && field.tokens()[nth].kind != Kind::Close(Bracket::Round)
            && euclid(field, nth + 1)
    });
    if inside { change() } else { theme::dim() }
}

/// A piece of a buffer's line: a token, the text between two as typed, or where what's typed
/// goes, with what's under it.
struct Piece {
    text: String,
    style: Style,
    under: Option<Span<'static>>,
    /// The token it is, by its place.
    token: Option<usize>,
    /// Where what's typed goes.
    typing: bool,
    /// Where the list `K` or `Tab` opens goes under: the cursor's token, or what's typed.
    here: bool,
}

impl Piece {
    fn width(&self) -> usize {
        self.text.chars().count()
    }

    fn gap(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            style: Style::new(),
            under: None,
            token: None,
            typing: false,
            here: false,
        }
    }

    fn typing() -> Self {
        Self {
            text: "_".to_owned(),
            style: theme::cursor(Mode::Insert),
            under: None,
            token: None,
            typing: true,
            here: true,
        }
    }
}

/// What a buffer shows of a field: where the cursor is, where what's typed goes, and the token
/// playing.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Marks {
    pub cursor: Option<usize>,
    pub typing_at: Option<usize>,
    pub playing: Option<usize>,
}

/// Where the text stops reading: a token, or past the last, which isn't marked while a token's
/// still to be typed.
fn bad_at(field: &Field, typing: bool) -> Option<usize> {
    field.bad()?;
    match field.bad_token() {
        Some(nth) => Some(nth),
        None if typing => None,
        None => Some(field.tokens().len()),
    }
}

/// How a token is drawn: in the warning colour where the text stops reading, lit under the
/// cursor, in the playhead's colour while it plays, and otherwise in its kind's style.
fn token_style(field: &Field, nth: usize, marks: Marks, bad: Option<usize>) -> Style {
    match bad {
        Some(bad) if bad == nth => theme::warning(),
        _ if marks.typing_at.is_none() && marks.cursor == Some(nth) => theme::cursor(Mode::Normal),
        _ if marks.playing == Some(nth) => theme::active(),
        _ => plain_style(field, nth),
    }
}

/// A field's pieces as typed: each token, and the text between, as it is.
fn as_typed(field: &Field, marks: Marks) -> Vec<Piece> {
    let text = field.text();
    let tokens = field.tokens();
    let bad = bad_at(field, marks.typing_at.is_some());
    let mut cuts = vec![0, text.len()];
    for token in tokens {
        cuts.extend([token.at.start, token.at.end]);
    }
    cuts.extend(marks.typing_at);
    cuts.sort_unstable();
    cuts.dedup();
    let mut pieces = Vec::new();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if marks.typing_at == Some(from) {
            pieces.push(Piece::typing());
        }
        let owner = tokens
            .iter()
            .position(|token| token.at.start <= from && from < token.at.end);
        let piece = &text[from..to];
        pieces.push(match owner {
            Some(nth) => Piece {
                text: piece.to_owned(),
                style: token_style(field, nth, marks, bad),
                under: (bad == Some(nth) && token_starts(field, nth, from))
                    .then(|| Span::styled(BAD, theme::warning())),
                token: Some(nth),
                typing: false,
                here: marks.typing_at.is_none()
                    && marks.cursor == Some(nth)
                    && token_starts(field, nth, from),
            },
            None => Piece {
                style: gap_style(
                    field,
                    tokens.iter().rposition(|token| token.at.end <= from),
                    piece,
                ),
                ..Piece::gap(piece)
            },
        });
    }
    if marks.typing_at.is_some_and(|at| at >= text.len()) && !pieces.iter().any(|p| p.typing) {
        pieces.push(Piece::typing());
    }
    if bad == Some(tokens.len()) {
        pieces.push(Piece {
            under: Some(Span::styled(BAD, theme::warning())),
            ..Piece::gap(" ")
        });
    }
    pieces
}

/// Whether a piece starting at a byte is the start of its token, where its `✗` goes.
fn token_starts(field: &Field, nth: usize, from: usize) -> bool {
    field
        .tokens()
        .get(nth)
        .is_some_and(|token| token.at.start == from)
}

/// A field's pieces a cell a token, with what each is for under it, as `Weights`' notes.
fn in_cells(field: &Field, layout: &Layout, marks: Marks) -> Vec<Piece> {
    let cell = layout.cell;
    let bad = bad_at(field, marks.typing_at.is_some());
    let mut pieces = Vec::new();
    for (nth, token) in field.tokens().iter().enumerate() {
        if marks
            .typing_at
            .is_some_and(|at| at <= token.at.start && !pieces.iter().any(|p: &Piece| p.typing))
        {
            pieces.push(Piece {
                text: format!("{:<cell$}", "_"),
                ..Piece::typing()
            });
        }
        let what = layout.under.get(nth).map_or("", String::as_str);
        let (under, style) = match bad {
            Some(bad) if bad == nth => (format!("{BAD:<cell$}"), theme::warning()),
            _ if marks.cursor == Some(nth) => (
                format!("{what:<cell$}"),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            _ => (format!("{what:<cell$}"), theme::dim()),
        };
        let text = field.token(nth).unwrap_or_default().to_owned();
        let width = text.chars().count().min(under.chars().count());
        // What's under the token, then under the room after it.
        let (first, rest): (String, String) = (
            under.chars().take(width).collect(),
            under.chars().skip(width).collect(),
        );
        pieces.push(Piece {
            text,
            style: token_style(field, nth, marks, bad),
            under: Some(Span::styled(first, style)),
            token: Some(nth),
            typing: false,
            here: marks.typing_at.is_none() && marks.cursor == Some(nth),
        });
        pieces.push(Piece {
            under: Some(Span::styled(rest.clone(), style)),
            ..Piece::gap(&" ".repeat(rest.chars().count()))
        });
    }
    if marks.typing_at.is_some() && !pieces.iter().any(|p| p.typing) {
        pieces.push(Piece::typing());
    }
    if let Some(beside) = &layout.beside {
        pieces.push(Piece {
            style: theme::dim(),
            ..Piece::gap(beside)
        });
    }
    pieces
}

/// A buffer's lines in `room` characters, and where on them the cursor's token, or what's typed,
/// starts: by line, from the first, and by character.
pub(crate) struct Wrapped {
    pub lines: Vec<Line<'static>>,
    pub cursor: Option<(usize, usize)>,
}

/// A field's pieces wrapped into lines `room` wide, breaking between pieces, and a line under
/// each where something's under its tokens.
fn wrap(pieces: Vec<Piece>, room: usize, under: bool) -> Wrapped {
    let room = room.max(1);
    // Words: the runs of pieces between spaces, so `x(3,8)` stays whole.
    let mut words: Vec<(bool, Vec<Piece>)> = Vec::new();
    for piece in pieces {
        let space = piece.token.is_none() && !piece.typing && piece.text.trim().is_empty();
        match words.last_mut() {
            Some((was, word)) if *was == space && !space => word.push(piece),
            _ => words.push((space, vec![piece])),
        }
    }
    let mut rows: Vec<Vec<Piece>> = vec![Vec::new()];
    let mut x = 0;
    for (space, word) in words {
        let width: usize = word.iter().map(Piece::width).sum();
        // A line breaks before a word, and the spaces after one stay at the end of its line.
        if !space && x > 0 && x + width > room {
            rows.push(Vec::new());
            x = 0;
        }
        x += width;
        rows.last_mut().unwrap().extend(word);
    }
    let mut lines = Vec::new();
    let mut cursor = None;
    for row in rows {
        let has_under = under || row.iter().any(|piece| piece.under.is_some());
        let mut top = Vec::new();
        let mut below = Vec::new();
        let mut x = 0;
        for piece in row {
            if cursor.is_none() && piece.here {
                cursor = Some((lines.len(), x));
            }
            let width = piece.width();
            below.push(piece.under.unwrap_or_else(|| Span::raw(" ".repeat(width))));
            top.push(Span::styled(piece.text, piece.style));
            x += width;
        }
        lines.push(Line::from(top));
        if has_under {
            lines.push(Line::from(below));
        }
    }
    Wrapped { lines, cursor }
}

/// A field's lines in its buffer, `room` wide: as typed, or a cell a token where something's
/// under each.
pub(crate) fn wrapped(field: &Field, layout: &Layout, marks: Marks, room: usize) -> Wrapped {
    if layout.under.is_empty() {
        wrap(as_typed(field, marks), room, false)
    } else {
        wrap(in_cells(field, layout, marks), room, true)
    }
}

/// A field's text on its cell, as typed, `room` characters at most, its end cut to a `…`: lit
/// as one value where the cursor's on the cell, `lit`, and otherwise with the token playing in
/// the playhead's colour.
pub(crate) fn cell_line(
    field: &Field,
    playing: Option<usize>,
    room: usize,
    lit: Option<Style>,
) -> Vec<Span<'static>> {
    let marks = Marks {
        playing,
        ..Marks::default()
    };
    let pieces = as_typed(field, marks);
    let total: usize = pieces.iter().map(Piece::width).sum();
    let mut spans = Vec::new();
    let mut left = if total > room {
        room.saturating_sub(1)
    } else {
        room
    };
    for piece in pieces {
        if left == 0 {
            break;
        }
        let text: String = piece.text.chars().take(left).collect();
        left -= text.chars().count();
        spans.push(Span::styled(text, lit.unwrap_or(piece.style)));
    }
    if total > room && room > 0 {
        spans.push(Span::styled(CUT, lit.unwrap_or_else(theme::dim)));
    }
    spans
}

/// The rows line under a field of tokens (decision 460, the study's Mockup 1): a mark for each
/// row of the span, `x` where a token writes it, `?` where one with a chance does and `.` where
/// nothing does. The rows the token under the cursor writes, `lit`, are lit as it is, and the row
/// `playing` is in the playhead's colour. Spaced while they fit, then close, then a page of them
/// at a time, the one with the cursor's first row or the row playing. While the text doesn't
/// read it's of the last text that did, and says so.
pub(crate) fn rows_line(
    table: &Table,
    field: &Field,
    lit: &[usize],
    playing: Option<usize>,
    room: usize,
) -> Vec<Span<'static>> {
    let range = table.range();
    let marks = table.marks(field);
    let playing = playing.filter(|row| range.first <= *row && *row <= range.last);
    let bad = field.bad().is_some();
    let note = "  still the last text that read";
    let room = if bad {
        room.saturating_sub(note.len())
    } else {
        room
    }
    .max(1);
    let spaced = marks.len() * 2 <= room + 1;
    let fits = if spaced { marks.len() } else { room };
    let focus = lit.first().copied().or(playing).unwrap_or(range.first) - range.first;
    let start = focus / fits * fits;
    let mut spans = Vec::new();
    for (offset, mark) in marks.iter().enumerate().skip(start).take(fits) {
        let row = range.first + offset;
        let style = if lit.contains(&row) {
            theme::cursor(Mode::Normal)
        } else if playing == Some(row) {
            theme::active()
        } else if *mark == Mark::Nothing {
            theme::faint()
        } else {
            Style::new()
        };
        if spaced && offset > start {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(mark.glyph().to_string(), style));
    }
    if bad {
        spans.push(Span::styled(note, theme::dim()));
    }
    spans
}

/// A buffer to draw: its setting's name, the keys it says along its foot, its field and what's
/// lit in it, the rows line under the text where the field has one, and the list `K` or `Tab`
/// opened.
pub(crate) struct Buffer<'a> {
    pub title: &'a str,
    pub hint: &'a str,
    pub field: &'a Field,
    pub layout: &'a Layout,
    pub marks: Marks,
    pub rows: Option<Vec<Span<'static>>>,
    pub menu: Option<&'a CommandMenu>,
}

impl Buffer<'_> {
    /// The room its text has across in an area.
    pub(crate) fn room(area: Rect) -> usize {
        ROOM.min(usize::from(area.width).saturating_sub(4))
    }

    /// Draw it in an area, under a row of the screen from a column, moved left or cut to stay
    /// in the area, its list under the cursor's token.
    pub(crate) fn draw(self, frame: &mut Frame, area: Rect, row: u16, column: u16) {
        let room = Self::room(area);
        let Wrapped { mut lines, cursor } = wrapped(self.field, self.layout, self.marks, room);
        let text_lines = lines.len();
        if let Some(rows) = self.rows {
            lines.push(Line::raw(""));
            let mut line = vec![Span::styled(ROWS, theme::dim())];
            line.extend(rows);
            lines.push(Line::from(line));
        }
        let lines: Vec<Line> = lines
            .into_iter()
            .map(|line| {
                let mut spans = vec![Span::raw(" ")];
                spans.extend(line.spans);
                Line::from(spans)
            })
            .collect();
        let widest = lines.iter().map(Line::width).max().unwrap_or(0);
        let width = (widest.max(room.min(32)) + 3).max(self.hint.chars().count() + 4) as u16;
        let height = lines.len() as u16 + 2;
        let window = popup::below(area, row, column, width, height);
        let border = popup::border()
            .title(format!(" {} ", self.title))
            .title_bottom(format!(" {} ", self.hint))
            .title_style(Style::new().add_modifier(Modifier::BOLD));
        let inside = popup::draw(frame, window, border);
        frame.render_widget(Paragraph::new(lines), inside);
        if let (Some(menu), Some((line, x))) = (self.menu, cursor)
            && line < text_lines
        {
            let mut picker = menu.picker.clone();
            crate::overlays::picker::draw::draw_menu_under(
                frame,
                inside.y + line as u16,
                inside.x + 1 + x as u16,
                &mut picker,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::notation::GrammarId;
    use vt_model::shape::ShapeColumn;

    /// Each piece of a field's cell as typed, with its style.
    fn styles(grammar: GrammarId, text: &str, playing: Option<usize>) -> Vec<(String, Style)> {
        let field = Field::new(grammar.grammar(), text, ShapeColumn::Velocity);
        cell_line(&field, playing, 128, None)
            .into_iter()
            .map(|span| (span.content.into_owned(), span.style))
            .collect()
    }

    fn style_of(pieces: &[(String, Style)], text: &str) -> Style {
        pieces
            .iter()
            .find(|(piece, _)| piece == text)
            .unwrap_or_else(|| panic!("no {text} in {pieces:?}"))
            .1
    }

    #[test]
    fn a_rhythm_is_coloured_by_what_each_token_is() {
        let pieces = styles(GrammarId::Rhythm, "x(3,8) [. x]*2 <x ~>?30", None);
        let change = Some(theme::PEACH);
        assert_eq!(style_of(&pieces, "x"), Style::new());
        assert_eq!(style_of(&pieces, "."), theme::faint());
        assert_eq!(style_of(&pieces, "~"), theme::faint());
        assert_eq!(style_of(&pieces, "["), theme::dim());
        assert_eq!(style_of(&pieces, ">"), theme::dim());
        assert_eq!(style_of(&pieces, "*2").fg, change);
        assert_eq!(style_of(&pieces, "?30").fg, change);
        // A Euclid is a change: its brackets, its numbers and its comma.
        for euclid in ["(", "3", ",", "8", ")"] {
            assert_eq!(style_of(&pieces, euclid).fg, change, "{euclid}");
        }
    }

    #[test]
    fn what_doesnt_read_is_a_warning_and_playing_goes_over_a_kind() {
        let pieces = styles(GrammarId::Rhythm, "x q", None);
        assert_eq!(style_of(&pieces, "q"), theme::warning());
        // The rest playing is green, not faint.
        let pieces = styles(GrammarId::Rhythm, "x .", Some(1));
        assert_eq!(style_of(&pieces, "."), theme::active());
    }

    #[test]
    fn what_writes_nothing_is_faint() {
        let pieces = styles(GrammarId::Bits, "x.x.....", None);
        assert_eq!(style_of(&pieces, "x"), Style::new());
        assert_eq!(style_of(&pieces, "."), theme::faint());
        assert_eq!(kind_style(Kind::Weight, "F"), Style::new());
        assert_eq!(kind_style(Kind::Weight, "0"), theme::faint());
    }

    #[test]
    fn the_cursor_lights_a_token_over_its_kind() {
        let field = Field::new(GrammarId::Rhythm.grammar(), "x*2", ShapeColumn::Velocity);
        let marks = Marks {
            cursor: Some(1),
            ..Marks::default()
        };
        let layout = Layout {
            cell: 1,
            step: 2,
            under: Vec::new(),
            beside: None,
        };
        let lines = wrapped(&field, &layout, marks, ROOM).lines;
        let lit = lines[0].spans.iter().find(|span| span.content == "*2");
        assert_eq!(
            lit.map(|span| span.style),
            Some(theme::cursor(Mode::Normal))
        );
    }
}
