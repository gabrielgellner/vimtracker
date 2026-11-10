//! Where a terminal's image goes: its placeholders, a row of text for each row of cells, in the
//! colour that says which image they're part of, and the faint box the track page's curves sit in.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::kitty;
use crate::theme;

/// An image's placeholders, a span for each row, in its colour.
pub fn rows(id: u32, width: usize, height: usize) -> Vec<Span<'static>> {
    let style = Style::new().fg(kitty::id_color(id));
    kitty::placeholder_lines(width, height)
        .into_iter()
        .map(|row| Span::styled(row, style))
        .collect()
}

/// An image in a box, a line for each row. The box is square and faint, as it's part of the page's
/// grid rather than a window over it.
pub fn boxed(id: u32, width: usize, height: usize) -> Vec<Line<'static>> {
    let rule = "─".repeat(width);
    let edge = theme::faint();
    let mut lines = vec![Line::styled(format!("┌{rule}┐"), edge)];
    for row in rows(id, width, height) {
        lines.push(Line::from(vec![
            Span::styled("│", edge),
            row,
            Span::styled("│", edge),
        ]));
    }
    lines.push(Line::styled(format!("└{rule}┘"), edge));
    lines
}
