//! A macro's mapping view, after Live's mapping browser (decision 285): a window over the track
//! page listing each setting the macro holds, with the ends of its range, `Min` at the macro's
//! `00` and `Max` at its `FF`, and its response's `Curve` and `S`, the cursor on one of them, and
//! beside them the response of the mapping under the cursor, drawn (decision 314).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use crate::app::App;
use crate::macroing::{MapField, MappingView, bend_text, end_text, macro_name};
use crate::theme;
use crate::views::widgets;
use crate::views::widgets::image;

/// How wide the setting's column is, each end's, and each bend's.
const SETTING_WIDTH: usize = 26;
const END_WIDTH: usize = 7;
const BEND_WIDTH: usize = 6;
/// Room after the bends for `by hand`.
const BY_HAND_WIDTH: usize = 8;
/// The response's drawing, in characters, inside its box: square at the author's 15 by 39
/// pixels, as the compressor's curve is.
pub(crate) const RESPONSE_WIDTH: usize = 13;
pub(crate) const RESPONSE_HEIGHT: usize = 5;
/// The room between the table and the drawing's box.
const RESPONSE_GAP: usize = 2;
/// The keys it takes, along its bottom.
const KEYS: &str = " = - move  i types  x straight  ~ swaps  dd unmaps  Esc ";

/// How wide each column of a mapping's row is, by what's in it.
fn width_of(field: MapField) -> usize {
    if field.bends() { BEND_WIDTH } else { END_WIDTH }
}

/// Draw the mapping view in the middle of an area.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &MappingView) {
    let Some(track) = app.song().tracks.get(app.cursor.track) else {
        return;
    };
    let held = &track.macros[view.at];
    let table = 2 + SETTING_WIDTH + 2 * END_WIDTH + 2 * BEND_WIDTH + BY_HAND_WIDTH;
    let boxed = RESPONSE_WIDTH + 2;
    let width = (2 + table + RESPONSE_GAP + boxed + 1).max(KEYS.len() + 4) as u16;
    // A line above the drawing's box and one below it.
    let height = (held.maps.len() + 5).max(RESPONSE_HEIGHT + 2 + 4) as u16;
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
    let mut heading = format!("  {:<SETTING_WIDTH$}", "Setting");
    for field in MapField::ALL {
        heading.push_str(&format!(
            "{:<width$}",
            field.name(),
            width = width_of(field)
        ));
    }
    let mut lines = vec![
        Line::raw(""),
        Line::styled(heading, theme::dim()),
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
        // A generator's `Wave` or `Rate` is reached by hand alone, which its row says, dim.
        let by_hand = target.by_hand();
        let cell = |field: MapField| {
            let on = here && view.field == field;
            let text = match &view.typing {
                Some(typed) if on => format!("{typed}_"),
                _ => match field {
                    MapField::Min => end_text(target, map.min),
                    MapField::Max => end_text(target, map.max),
                    MapField::Curve => bend_text(map.curve),
                    MapField::S => bend_text(map.s),
                },
            };
            let style = if on {
                theme::cursor(app.mode)
            } else if by_hand || (field.bends() && map.is_straight()) {
                // A straight response is dim, so a bent one stands out.
                theme::dim()
            } else {
                Style::new()
            };
            let padded = format!("{text:<width$}", width = width_of(field) - 1);
            [Span::styled(padded, style), Span::raw(" ")]
        };
        let name_style = if by_hand { theme::dim() } else { Style::new() };
        let mut spans = vec![
            Span::styled(format!(" {marker}"), theme::dim()),
            Span::styled(format!("{name:<SETTING_WIDTH$}"), name_style),
        ];
        for field in MapField::ALL {
            spans.extend(cell(field));
        }
        if by_hand {
            spans.push(Span::styled("by hand", theme::dim()));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);
    // The response of the mapping under the cursor, its box drawn empty until the terminal has
    // its picture.
    let response = Rect {
        x: inner.x + (table + RESPONSE_GAP) as u16,
        y: inner.y + 1,
        width: boxed as u16,
        height: (RESPONSE_HEIGHT + 2) as u16,
    }
    .intersection(inner);
    let drawing = match app.mapping_image {
        Some(id) => image::boxed(id, RESPONSE_WIDTH, RESPONSE_HEIGHT),
        None => empty_box(RESPONSE_WIDTH, RESPONSE_HEIGHT),
    };
    frame.render_widget(Paragraph::new(drawing), response);
}

/// A faint box with nothing in it, as [`image::boxed`] draws one round a picture.
fn empty_box(width: usize, height: usize) -> Vec<Line<'static>> {
    let rule = "─".repeat(width);
    let edge = theme::faint();
    let mut lines = vec![Line::styled(format!("┌{rule}┐"), edge)];
    for _ in 0..height {
        lines.push(Line::styled(format!("│{}│", " ".repeat(width)), edge));
    }
    lines.push(Line::styled(format!("└{rule}┘"), edge));
    lines
}
