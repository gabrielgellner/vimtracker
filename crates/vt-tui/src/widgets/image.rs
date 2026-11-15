//! Where a terminal's image goes: its placeholders, a row of text for each row of cells, in the
//! colour that says which image they're part of, and the faint box the track page's curves sit in.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::graphics;
use crate::widgets::theme;

/// An image's placeholders, a span for each row, in its colour.
pub(crate) fn rows(id: u32, width: usize, height: usize) -> Vec<Span<'static>> {
    let style = Style::new().fg(graphics::id_color(id));
    graphics::placeholder_lines(width, height)
        .into_iter()
        .map(|row| Span::styled(row, style))
        .collect()
}

/// An image in a box, a line for each row. The box is square and faint, as it's part of the page's
/// grid rather than a window over it.
pub(crate) fn boxed(id: u32, width: usize, height: usize) -> Vec<Line<'static>> {
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

#[cfg(test)]
mod tests {
    use ratatui::text::Text;

    use super::*;
    use crate::widgets::pin::{assert_drawn, drawn};

    #[test]
    fn an_image_in_a_box_is_its_placeholders_in_its_colour_inside_a_faint_square() {
        assert_drawn(
            &drawn(Text::from(boxed(5, 4, 2)), 6, 4),
            &[
                "┌────┐",
                "aaaaaa",
                "│▒▒▒▒│",
                "abbbba",
                "│▒▒▒▒│",
                "abbbba",
                "└────┘",
                "aaaaaa",
                "a fg OVERLAY0",
                "b fg #000005",
            ],
        );
    }
}
