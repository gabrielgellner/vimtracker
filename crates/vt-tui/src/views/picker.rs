//! The picker: a centred window with a search line above a list, and groups beside it to browse.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::input::picker::{Focus, Picker};
use crate::theme;

const WIDTH: u16 = 60;
const HEIGHT: u16 = 16;
/// Room for the groups' column.
const GROUPS_WIDTH: usize = 12;

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
    let browsing = picker.focus != Focus::Search;
    let mut lines = vec![if browsing {
        Line::styled("h l  groups and list   / search", theme::dim())
    } else {
        Line::raw(format!("> {}_", picker.query))
    }];
    let shown = picker.shown().count();
    // Scrolled on their own, so the group being browsed stays in view.
    let names = if browsing {
        picker.group_names()
    } else {
        Vec::new()
    };
    let first_group = (picker.group + 1).saturating_sub(room);
    for row in 0..room.min(shown.max(names.len())) {
        let mut spans = Vec::new();
        if browsing {
            let at = first_group + row;
            let name = names.get(at).map_or("", String::as_str);
            let marker = if at == picker.group { ">" } else { " " };
            let style = if at == picker.group && picker.focus == Focus::Groups {
                theme::selected()
            } else if at == picker.group {
                Style::new()
            } else {
                theme::dim()
            };
            spans.push(Span::styled(
                format!("{marker} {name:<width$}", width = GROUPS_WIDTH - 2),
                style,
            ));
        }
        let index = first + row;
        if index < shown {
            let style = if index == picker.selected && picker.focus != Focus::Groups {
                theme::selected()
            } else {
                Style::new()
            };
            spans.push(Span::styled(picker.text_of(index), style));
        }
        lines.push(Line::from(spans));
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
