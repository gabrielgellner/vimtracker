//! An open guide, in a window over the screen.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::guides::Guide;
use crate::theme;

/// The widest the window gets, since long lines are hard to read.
const WIDTH: u16 = 100;
/// The border, and a margin on the right.
const AROUND_TEXT: u16 = 4;

/// How wide a guide's text is in an area of this width.
pub fn text_width(width: u16) -> usize {
    usize::from(WIDTH.min(width).saturating_sub(AROUND_TEXT)).max(1)
}

/// How many rows of a guide fit in an area of this height, inside the border.
pub fn page(height: u16) -> usize {
    usize::from(height.saturating_sub(2)).max(1)
}

/// Draw the guide from a scroll position.
pub fn draw(frame: &mut Frame, area: Rect, guide: Guide, scroll: usize) {
    let width = WIDTH.min(area.width);
    let window = Rect {
        x: area.x + (area.width - width) / 2,
        width,
        ..area
    };
    let lines: Vec<_> = guide
        .lines(text_width(area.width))
        .into_iter()
        .skip(scroll)
        .collect();
    frame.render_widget(Clear, window);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::new()
                .style(theme::window())
                .borders(Borders::ALL)
                .title(format!(" {} ", guide.title()))
                .title_bottom(" j k scroll  C-d C-u page  g G ends  ? guides  q close ")
                .title_style(Style::new().add_modifier(Modifier::BOLD)),
        ),
        window,
    );
}
