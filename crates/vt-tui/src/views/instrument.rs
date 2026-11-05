//! The instrument view: the current instrument's settings, and a list of the others.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::song::Instrument;

use crate::app::App;
use crate::input::mode::Mode;
use crate::instruments::Row;
use crate::theme;

/// A row's value as text.
fn value(row: Row, instrument: &Instrument) -> String {
    match row {
        Row::Sample => instrument
            .sample
            .clone()
            .unwrap_or_else(|| "(no sample: press i)".to_owned()),
        Row::Volume => format!("{:02X}", instrument.volume),
        Row::Attack => format!("{} ms", instrument.attack_ms),
        Row::Release => format!("{} ms", instrument.release_ms),
    }
}

/// The file name alone, since a kit's sounds have long paths that would hide it.
fn file_name(sample: &str) -> &str {
    sample.rsplit('/').next().unwrap_or(sample)
}

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let song = app.song();
    let fallback = Instrument::default();
    let current = song
        .instruments
        .get(&app.instrument)
        .map_or(&fallback, |i| i.as_ref());
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut lines = vec![
        Line::styled(format!("Instrument {:02X}", app.instrument), bold),
        Line::raw(""),
    ];
    for row in Row::ALL {
        let text = if row == app.instrument_row && app.mode == Mode::Insert {
            format!("{}_", app.entry.text)
        } else {
            value(row, current)
        };
        let style = if row == app.instrument_row {
            theme::cursor(app.mode)
        } else {
            Style::new()
        };
        lines.push(Line::from(vec![
            Span::raw(format!("  {:<9}", row.label())),
            Span::styled(text, style),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("All instruments", bold));
    for (id, instrument) in &song.instruments {
        let style = if *id == app.instrument {
            theme::active()
        } else {
            theme::dim()
        };
        lines.push(Line::styled(
            format!(
                "  {id:02X}  {}",
                instrument
                    .sample
                    .as_deref()
                    .map_or("(no sample)", file_name)
            ),
            style,
        ));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
