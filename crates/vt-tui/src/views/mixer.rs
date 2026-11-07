//! The mixer: every track's level, pan, sends, mute and solo on one page.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::input::mode::Mode;
use crate::theme;
use crate::track_page::{Control, MIXER};
use crate::views::track::{name_spans, value, value_spans};

/// Room for a track's number and name.
const NAME_WIDTH: usize = 8;
/// How wide a setting's column is.
const COLUMN_WIDTH: usize = 8;
/// The rows that aren't tracks: the title, a gap and the column names.
const ROWS_AROUND_TRACKS: u16 = 3;

/// The first track shown, keeping the cursor's in view.
fn first_shown(cursor: usize, tracks: usize, rows: usize) -> usize {
    let rows = rows.max(1);
    cursor
        .saturating_sub(rows / 2)
        .min(tracks.saturating_sub(rows))
}

/// Draw the mixer.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut heading = vec![Span::raw(" ".repeat(NAME_WIDTH))];
    for control in MIXER {
        let label = app.field_jump.as_ref().and_then(|jump| jump.label(control));
        heading.extend(name_spans(
            app,
            control.label(),
            &label.map_or_else(String::new, String::from),
            COLUMN_WIDTH,
        ));
    }
    let mut lines = vec![
        Line::styled("Mixer", bold),
        Line::raw(""),
        Line::from(heading),
    ];
    let song = app.song();
    let rows = usize::from(area.height.saturating_sub(ROWS_AROUND_TRACKS));
    let first = first_shown(app.cursor.track, song.tracks.len(), rows);
    let current = app.track_control();
    for (index, track) in song.tracks.iter().enumerate().skip(first).take(rows.max(1)) {
        let live = app.levels.get(index).copied().unwrap_or_default();
        let name = format!("{:02} {}", index + 1, track.name);
        let here = index == app.cursor.track;
        let name_style = if here { bold } else { Style::new() };
        let mut spans = vec![Span::styled(format!("{name:<NAME_WIDTH$}"), name_style)];
        for control in MIXER {
            let selected = here && control == current;
            let text = match control {
                Control::Mute => if live.mute { "M" } else { "-" }.to_owned(),
                Control::Solo => if live.solo { "S" } else { "-" }.to_owned(),
                _ if selected && app.mode == Mode::Insert => format!("{}_", app.entry.text),
                _ => value(control, track, song.swing),
            };
            let style = if selected {
                theme::cursor(app.mode)
            } else if live.mute {
                theme::dim()
            } else {
                Style::new()
            };
            let gap = COLUMN_WIDTH.saturating_sub(text.chars().count());
            // A jump's labels go on the cursor's track, which it stays on.
            if here {
                spans.extend(value_spans(app, control, text, style));
            } else {
                spans.push(Span::styled(text, style));
            }
            spans.push(Span::raw(" ".repeat(gap)));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
