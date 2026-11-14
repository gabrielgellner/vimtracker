//! Settings laid out as tables: columns of fixed widths, so values line up whatever their length,
//! with a value that needs more room spanning the columns beside it, and one too long even for
//! those cut with `…` rather than pushing on what follows.

use std::ops::Range;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Row, Table, Widget};

/// What stands in for the end of a value that doesn't fit.
const CUT: &str = "…";

/// A table's columns: how wide each is, and the room between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Columns {
    widths: Vec<u16>,
    spacing: u16,
    /// Whether the last column takes whatever room is left, rather than its width.
    open_ended: bool,
}

impl Columns {
    /// Columns of these widths, side by side.
    pub fn new(widths: impl Into<Vec<u16>>) -> Self {
        Self {
            widths: widths.into(),
            spacing: 0,
            open_ended: false,
        }
    }

    /// The same columns with this much room between each and the next.
    #[must_use]
    pub fn spaced(self, spacing: u16) -> Self {
        Self { spacing, ..self }
    }

    /// The same columns, the last taking whatever room is left, with its text never cut.
    #[must_use]
    pub fn open_ended(self) -> Self {
        Self {
            open_ended: true,
            ..self
        }
    }

    /// How wide a cell is that starts in a column and spans some, with the room between them.
    pub fn room(&self, first: usize, span: usize) -> usize {
        let last = first + span;
        if self.open_ended && last >= self.widths.len() {
            return usize::MAX;
        }
        let widths: usize = self
            .widths
            .iter()
            .skip(first)
            .take(span)
            .map(|width| usize::from(*width))
            .sum();
        widths + usize::from(self.spacing) * span.saturating_sub(1)
    }

    /// How wide the columns are together, leaving out an open end.
    pub fn width(&self) -> u16 {
        let count = self.widths.len() - usize::from(self.open_ended);
        let widths: u16 = self.widths.iter().take(count).sum();
        widths + self.spacing * self.widths.len().saturating_sub(1) as u16
    }

    /// A row of cells to fill from the left.
    pub fn row(&self) -> Cells<'_> {
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

    /// Where a cell that starts in a column and spans some is, over all of an area's rows, laid out
    /// as the table's columns are. It's for a cell holding a widget or a terminal's image, rather
    /// than text. Past the area's right edge, it's cut, as the table is.
    pub fn area_of(&self, area: Rect, first: usize, span: usize) -> Rect {
        let whole = Rect {
            width: self.width().max(area.width),
            ..area
        };
        let columns = Layout::horizontal(self.constraints())
            .flex(Flex::Start)
            .spacing(self.spacing)
            .split(whole);
        let last = (first + span.max(1) - 1).min(columns.len() - 1);
        let (start, end) = (columns[first.min(last)], columns[last]);
        Rect {
            x: start.x,
            width: end.right() - start.x,
            ..area
        }
        .intersection(area)
    }

    /// Draw rows at the top of an area and take them off it, so what's drawn next goes under them.
    pub fn stack(&self, frame: &mut Frame, area: &mut Rect, rows: Vec<Row<'static>>) {
        let top = take_top(area, rows.len() as u16);
        self.draw(frame, top, rows);
    }

    /// Draw rows in an area. With too little room for the columns, the table is cut at the area's
    /// right edge, as a paragraph is, rather than its columns squeezed to fit.
    pub fn draw(&self, frame: &mut Frame, area: Rect, rows: Vec<Row<'static>>) {
        let table = Table::new(rows, self.constraints()).column_spacing(self.spacing);
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
pub struct Cells<'a> {
    columns: &'a Columns,
    /// The column the next cell starts in.
    at: usize,
    cells: Vec<(Line<'static>, u16)>,
}

impl Cells<'_> {
    /// A cell in the next column.
    pub fn cell(&mut self, line: impl Into<Line<'static>>) -> &mut Self {
        self.span(line, 1)
    }

    /// A cell across the next few columns, cut to fit them.
    pub fn span(&mut self, line: impl Into<Line<'static>>, columns: usize) -> &mut Self {
        let room = self.columns.room(self.at, columns);
        self.cells
            .push((fitted(line.into(), room), columns.max(1) as u16));
        self.at += columns.max(1);
        self
    }

    /// Leave the next few columns empty.
    pub fn skip(&mut self, columns: usize) -> &mut Self {
        if columns > 0 {
            self.cells.push((Line::default(), columns as u16));
            self.at += columns;
        }
        self
    }

    /// Leave the columns empty up to this one.
    pub fn to(&mut self, column: usize) -> &mut Self {
        self.skip(column.saturating_sub(self.at))
    }

    /// Change each cell's text, such as to fade it.
    pub fn map(&mut self, change: impl Fn(Line<'static>) -> Line<'static>) -> &mut Self {
        for (line, _) in &mut self.cells {
            *line = change(std::mem::take(line));
        }
        self
    }

    /// The row, to put in a table.
    pub fn done(&mut self) -> Row<'static> {
        Row::new(
            std::mem::take(&mut self.cells)
                .into_iter()
                .map(|(line, span)| Cell::new(line).column_span(span)),
        )
    }
}

/// An empty row, a line high, to part groups of rows. A `Row`'s own default has no height.
pub fn blank_row() -> Row<'static> {
    Row::new(Vec::<Cell>::new())
}

/// A line cut to fit a width, ending in `…` where something's left out.
pub fn fitted(line: Line<'static>, width: usize) -> Line<'static> {
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
pub fn centred_run(widths: &[usize], at: usize, room: usize) -> Range<usize> {
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
pub fn take_top(area: &mut Rect, rows: u16) -> Rect {
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
    fn a_cell_spanning_columns_has_their_room_and_the_gaps_between() {
        let columns = Columns::new([10, 2, 3, 11]).spaced(1);
        assert_eq!(columns.room(1, 3), 18);
        assert_eq!(columns.width(), 29);
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
