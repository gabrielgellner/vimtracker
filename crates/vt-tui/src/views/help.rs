//! The which-key popup and `Space k`, both generated from the binding table.

use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::App;
use crate::theme;

/// How long a prefix waits before showing what can come next.
const POPUP_DELAY: Duration = Duration::from_millis(300);

/// Draw the list of next keys, if a prefix has waited long enough.
pub fn draw_popup(frame: &mut Frame, area: Rect, app: &App, now: Instant) {
    let rows = app.popup_rows(now, POPUP_DELAY);
    if rows.is_empty() {
        return;
    }
    let lines: Vec<Line> = rows
        .iter()
        .map(|(keys, description)| Line::raw(format!("{keys:<6} {description}")))
        .collect();
    let height = (lines.len() as u16 + 2).min(area.height);
    let width = 44.min(area.width);
    let popup = Rect {
        x: area.right() - width,
        y: area.bottom() - height,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(Block::new().style(theme::window()).borders(Borders::ALL)),
        popup,
    );
}

/// Draw every key. `j` and `k` scroll, and any other key closes it.
pub fn draw_help(frame: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = app
        .help_rows()
        .into_iter()
        .skip(app.help_scroll)
        .map(Line::raw)
        .collect();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::new()
                .style(theme::window())
                .borders(Borders::ALL)
                .title("Keys: j and k scroll, any other key closes")
                .title_style(Style::new().add_modifier(Modifier::BOLD)),
        ),
        area,
    );
}
