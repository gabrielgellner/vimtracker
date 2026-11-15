//! Settings laid out as tables: columns of fixed widths, so values line up whatever their length,
//! with a value that needs more room spanning the columns beside it, and one too long even for
//! those cut with `…` rather than pushing on what follows.

use std::ops::Range;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Row, Table, Widget};

/// What stands in for the end of a value that doesn't fit.
const CUT: &str = "…";

/// A table's columns: how wide each is, side by side with no room between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Columns {
    widths: Vec<u16>,
    /// Whether the last column takes whatever room is left, rather than its width.
    open_ended: bool,
}

impl Columns {
    /// Columns of these widths, side by side.
    pub(crate) fn new(widths: impl Into<Vec<u16>>) -> Self {
        Self {
            widths: widths.into(),
            open_ended: false,
        }
    }

    /// The same columns, the last taking whatever room is left, with its text never cut.
    #[must_use]
    pub(crate) fn open_ended(self) -> Self {
        Self {
            open_ended: true,
            ..self
        }
    }

    /// How wide a cell is that starts in a column and spans some.
    pub(crate) fn room(&self, first: usize, span: usize) -> usize {
        let last = first + span;
        if self.open_ended && last >= self.widths.len() {
            return usize::MAX;
        }
        self.widths
            .iter()
            .skip(first)
            .take(span)
            .map(|width| usize::from(*width))
            .sum()
    }

    /// How wide the columns are together, leaving out an open end.
    pub(crate) fn width(&self) -> u16 {
        let count = self.widths.len() - usize::from(self.open_ended);
        self.widths.iter().take(count).sum()
    }

    /// A row of cells to fill from the left.
    pub(crate) fn row(&self) -> Cells<'_> {
        Cells {
            columns: self,
            at: 0,
            cells: Vec::new(),
        }
    }

    /// Each column's width, as a table or a layout takes it.
    fn constraints(&self) -> Vec<Constraint> {
        self.widths
            .iter()
            .enumerate()
            .map(|(index, width)| {
                if self.open_ended && index == self.widths.len() - 1 {
                    Constraint::Fill(1)
                } else {
                    Constraint::Length(*width)
                }
            })
            .collect()
    }

    /// Draw rows at the top of an area and take them off it, so what's drawn next goes under them.
    pub(crate) fn stack(&self, frame: &mut Frame, area: &mut Rect, rows: Vec<Row<'static>>) {
        let top = take_top(area, rows.len() as u16);
        self.draw(frame, top, rows);
    }

    /// Draw rows in an area. With too little room for the columns, the table is cut at the area's
    /// right edge, as a paragraph is, rather than its columns squeezed to fit.
    pub(crate) fn draw(&self, frame: &mut Frame, area: Rect, rows: Vec<Row<'static>>) {
        let table = Table::new(rows, self.constraints()).column_spacing(0);
        let width = self.width();
        if width <= area.width {
            frame.render_widget(table, area);
            return;
        }
        // Drawn whole over a copy of what's there, then copied back as far as the area goes.
        let whole = Rect { width, ..area };
        let shown = area.intersection(frame.area());
        let buffer = frame.buffer_mut();
        let mut copy = Buffer::empty(whole);
        for position in shown.positions() {
            copy[position] = buffer[position].clone();
        }
        table.render(whole, &mut copy);
        for position in shown.positions() {
            buffer[position] = copy[position].clone();
        }
    }
}

/// A row's cells, put in from the left, each spanning one column or more.
pub(crate) struct Cells<'a> {
    columns: &'a Columns,
    /// The column the next cell starts in.
    at: usize,
    cells: Vec<(Line<'static>, u16)>,
}

impl Cells<'_> {
    /// A cell in the next column.
    pub(crate) fn cell(&mut self, line: impl Into<Line<'static>>) -> &mut Self {
        self.span(line, 1)
    }

    /// A cell across the next few columns, cut to fit them.
    pub(crate) fn span(&mut self, line: impl Into<Line<'static>>, columns: usize) -> &mut Self {
        let room = self.columns.room(self.at, columns);
        self.cells
            .push((fitted(line.into(), room), columns.max(1) as u16));
        self.at += columns.max(1);
        self
    }

    /// Leave the next few columns empty.
    pub(crate) fn skip(&mut self, columns: usize) -> &mut Self {
        if columns > 0 {
            self.cells.push((Line::default(), columns as u16));
            self.at += columns;
        }
        self
    }

    /// Leave the columns empty up to this one.
    pub(crate) fn to(&mut self, column: usize) -> &mut Self {
        self.skip(column.saturating_sub(self.at))
    }

    /// The row, to put in a table.
    pub(crate) fn done(&mut self) -> Row<'static> {
        Row::new(
            std::mem::take(&mut self.cells)
                .into_iter()
                .map(|(line, span)| Cell::new(line).column_span(span)),
        )
    }
}

/// An empty row, a line high, to part groups of rows. A `Row`'s own default has no height.
pub(crate) fn blank_row() -> Row<'static> {
    Row::new(Vec::<Cell>::new())
}

/// A line cut to fit a width, ending in `…` where something's left out.
pub(crate) fn fitted(line: Line<'static>, width: usize) -> Line<'static> {
    if line.width() <= width {
        return line;
    }
    let room = width.saturating_sub(1);
    let mut used = 0;
    let mut spans = Vec::new();
    let mut cut_style = line.style;
    for span in &line.spans {
        let mut kept = String::new();
        for grapheme in span.styled_graphemes(Style::new()) {
            let wide = Span::raw(grapheme.symbol).width();
            if used + wide > room {
                break;
            }
            used += wide;
            kept.push_str(grapheme.symbol);
        }
        cut_style = span.style;
        let whole = kept.len() == span.content.len();
        if !kept.is_empty() {
            spans.push(Span::styled(kept, span.style));
        }
        if !whole {
            break;
        }
    }
    if width > 0 {
        spans.push(Span::styled(CUT, cut_style));
    }
    Line { spans, ..line }
}

/// Which of a row of columns of these widths fit in the room, as many as can, around the one at
/// `at`: half of them before it where there are, as many as are left after. Empty if not even one
/// fits.
pub(crate) fn centred_run(widths: &[usize], at: usize, room: usize) -> Range<usize> {
    let count = widths.len();
    (1..=count)
        .rev()
        .map(|shown| {
            let first = at.saturating_sub(shown / 2).min(count - shown);
            first..first + shown
        })
        .find(|run| widths[run.clone()].iter().sum::<usize>() <= room)
        .unwrap_or(0..0)
}

/// The top rows of an area, taken off it, as far as it goes.
pub(crate) fn take_top(area: &mut Rect, rows: u16) -> Rect {
    let rows = rows.min(area.height);
    let top = Rect {
        height: rows,
        ..*area
    };
    area.y += rows;
    area.height -= rows;
    top
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_centred_run_fits_as_many_columns_as_it_can_around_one() {
        assert_eq!(
            centred_run(&[4; 10], 5, 12),
            4..7,
            "three, the cursor's in the middle"
        );
        assert_eq!(centred_run(&[4; 10], 0, 12), 0..3);
        assert_eq!(centred_run(&[4; 10], 9, 12), 7..10);
        assert_eq!(centred_run(&[4; 10], 5, 100), 0..10);
        assert_eq!(
            centred_run(&[4, 4, 7, 4], 3, 11),
            2..4,
            "a wide one takes more room"
        );
        assert_eq!(centred_run(&[4; 3], 1, 3), 0..0, "not even one fits");
    }

    fn text(line: &Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn a_line_too_long_is_cut_with_an_ellipsis() {
        let line = Line::from(vec![Span::raw("Up-"), Span::raw("down-inc")]);
        assert_eq!(text(&fitted(line.clone(), 11)), "Up-down-inc");
        assert_eq!(text(&fitted(line.clone(), 8)), "Up-down…");
        assert_eq!(text(&fitted(line.clone(), 3)), "Up…");
        assert_eq!(text(&fitted(line, 0)), "");
    }

    #[test]
    fn a_cell_spanning_columns_has_their_room() {
        let columns = Columns::new([10, 2, 3, 11]);
        assert_eq!(columns.room(1, 3), 16);
        assert_eq!(columns.width(), 26);
        let open = Columns::new([10, 2]).open_ended();
        assert_eq!(open.room(1, 1), usize::MAX);
        assert_eq!(open.width(), 10);
    }

    #[test]
    fn taking_rows_off_the_top_stops_at_the_bottom() {
        let mut area = Rect::new(0, 2, 10, 3);
        assert_eq!(take_top(&mut area, 2), Rect::new(0, 2, 10, 2));
        assert_eq!(take_top(&mut area, 2), Rect::new(0, 4, 10, 1));
        assert_eq!(area.height, 0);
    }
}
