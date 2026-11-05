//! The picker: a centred window with a search line above a list.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::input::picker::Picker;
use crate::theme;

const WIDTH: u16 = 60;
const HEIGHT: u16 = 16;

/// Draw the window over whatever is underneath.
pub fn draw(frame: &mut Frame, area: Rect, picker: &Picker) {
    let width = WIDTH.min(area.width);
    let height = HEIGHT.min(area.height);
    let window = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    let room = usize::from(height.saturating_sub(3));
    // Scroll just enough to keep the highlight in the list.
    let first = (picker.selected + 1).saturating_sub(room);
    let mut lines = vec![Line::raw(format!("> {}_", picker.query))];
    for (index, item) in picker.shown().enumerate().skip(first).take(room) {
        let style = if index == picker.selected {
            theme::selected()
        } else {
            Style::new()
        };
        lines.push(Line::styled(item.label.clone(), style));
    }
    frame.render_widget(Clear, window);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::new()
                .style(theme::window())
                .borders(Borders::ALL)
                .title(picker.title)
                .title_style(Style::new().add_modifier(Modifier::BOLD)),
        ),
        window,
    );
}
