//! A macro's mapping view, after Live's mapping browser (decision 285): a window over the track
//! page listing each setting the macro holds, with the ends of its range, `Min` at the macro's
//! `00` and `Max` at its `FF`, the cursor on one of them.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use crate::app::App;
use crate::macroing::{MappingView, end_text, macro_name};
use crate::theme;
use crate::views::widgets;

/// How wide the setting's column is, and each end's.
const SETTING_WIDTH: usize = 26;
const END_WIDTH: usize = 7;
/// Room after the ends for `by hand`.
const BY_HAND_WIDTH: usize = 8;
/// The keys it takes, along its bottom.
const KEYS: &str = " = - move  i types  ~ swaps  dd unmaps  Esc ";

/// Draw the mapping view in the middle of an area.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &MappingView) {
    let Some(track) = app.song().tracks.get(app.cursor.track) else {
        return;
    };
    let held = &track.macros[view.at];
    let width =
        (2 + 2 + SETTING_WIDTH + 2 * END_WIDTH + BY_HAND_WIDTH + 2).max(KEYS.len() + 4) as u16;
    let height = (held.maps.len() + 5) as u16;
    let window = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    };
    let count = match held.maps.len() {
        1 => "1 mapping".to_owned(),
        count => format!("{count} mappings"),
    };
    let title = Line::from(vec![
        Span::styled(
            format!(" Macro {} {} ", view.at + 1, macro_name(held, view.at)),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {count} "), theme::dim()),
    ]);
    let block = widgets::window_border()
        .style(theme::window())
        .title(title)
        .title_bottom(Line::styled(KEYS, theme::dim()));
    let inner = block.inner(window);
    frame.render_widget(Clear, window);
    frame.render_widget(block, window);
    let mut lines = vec![
        Line::raw(""),
        Line::styled(
            format!(
                "  {:<SETTING_WIDTH$}{:<END_WIDTH$}{:<END_WIDTH$}",
                "Setting", "Min", "Max"
            ),
            theme::dim(),
        ),
        Line::raw(""),
    ];
    for (row, map) in held.maps.iter().enumerate() {
        let target = map.target;
        let here = row == view.row;
        let marker = if here { "›" } else { " " };
        let name: String = target
            .name_on(track)
            .chars()
            .take(SETTING_WIDTH - 1)
            .collect();
        // A generator's setting is reached by hand for now, which its row says, dim.
        let by_hand = target.setting().is_none();
        let end = |max: bool, value: u16| {
            let text = match &view.typing {
                Some(typed) if here && view.max == max => format!("{typed}_"),
                _ => end_text(target, value),
            };
            let style = if here && view.max == max {
                theme::cursor(app.mode)
            } else if by_hand {
                theme::dim()
            } else {
                Style::new()
            };
            let padded = format!("{text:<width$}", width = END_WIDTH - 1);
            [Span::styled(padded, style), Span::raw(" ")]
        };
        let name_style = if by_hand { theme::dim() } else { Style::new() };
        let mut spans = vec![
            Span::styled(format!(" {marker}"), theme::dim()),
            Span::styled(format!("{name:<SETTING_WIDTH$}"), name_style),
        ];
        spans.extend(end(false, map.min));
        spans.extend(end(true, map.max));
        if by_hand {
            spans.push(Span::styled("by hand", theme::dim()));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}
