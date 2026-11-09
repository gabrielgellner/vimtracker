//! The mixer: a strip for each track, with its level meter, its fader, its sends, mute and solo, laid
//! out as Polyend Tracker's mixer is.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_model::song::level_gain;

use crate::app::App;
use crate::input::mode::Mode;
use crate::meter::{height_of, to_db};
use crate::theme;
use crate::track_page::{Control, MIXER};
use crate::views::meter::{EIGHTHS, level_cell};
use crate::views::track::{name_spans, value, value_spans};

/// Room for the names of the settings, at the left.
const LABEL_WIDTH: usize = 7;
/// How wide a track's strip is: its name fits, and eight strips fit beside the main meter at 80.
const STRIP_WIDTH: usize = 7;
/// The rows that aren't the meters: the title, a gap and the track names.
const ROWS_ABOVE_METERS: u16 = 3;

/// The first track shown, keeping the cursor's in view.
fn first_shown(cursor: usize, tracks: usize, strips: usize) -> usize {
    let strips = strips.max(1);
    cursor
        .saturating_sub(strips / 2)
        .min(tracks.saturating_sub(strips))
}

/// A fader for one row, counting rows up from the bottom, showing where a track's level is set.
fn fader_cell(row: usize, rows: usize, level: u8) -> Span<'static> {
    let db = to_db(level_gain(f32::from(level)));
    let eighths = (height_of(db) * (rows * 8) as f32).round() as usize;
    let filled = eighths.saturating_sub(row * 8).min(8);
    Span::styled(
        EIGHTHS[filled].to_string(),
        Style::new().fg(theme::OVERLAY0).bg(theme::MANTLE),
    )
}

/// One row of the meters, counting rows up from the bottom.
fn meter_line(app: &App, shown: std::ops::Range<usize>, row: usize, rows: usize) -> Line<'static> {
    let zero = ((height_of(0.0) * rows as f32) as usize).min(rows - 1);
    let mark = if row == zero { "0" } else { "" };
    let mut spans = vec![Span::styled(format!("{mark:>5}  "), theme::dim())];
    for index in shown {
        let meter = app.track_meters.get(index).copied().unwrap_or_default();
        let channel = |side: usize| {
            let channel = meter.channels[side];
            level_cell(
                row,
                rows,
                channel.average(),
                channel.peak.now,
                channel.peak.top,
                1,
            )
        };
        spans.push(channel(0));
        spans.push(channel(1));
        spans.push(Span::raw(" "));
        spans.push(fader_cell(row, rows, app.song().tracks[index].level));
        spans.push(Span::raw(" ".repeat(STRIP_WIDTH - 4)));
    }
    Line::from(spans)
}

/// Draw the mixer.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let song = app.song();
    let room = usize::from(area.width).saturating_sub(LABEL_WIDTH);
    let strips = (room / STRIP_WIDTH).max(1);
    let first = first_shown(app.cursor.track, song.tracks.len(), strips);
    let shown = first..(first + strips).min(song.tracks.len());

    let mut names = vec![Span::raw(" ".repeat(LABEL_WIDTH))];
    for index in shown.clone() {
        let track = &song.tracks[index];
        let name = if track.is_sound() {
            format!("{:02} {}", index + 1, track.name)
        } else {
            track.name.clone()
        };
        let style = if index == app.cursor.track {
            bold
        } else {
            Style::new()
        };
        names.push(Span::styled(format!("{name:<STRIP_WIDTH$}"), style));
    }
    let mut lines = vec![
        Line::styled("Mixer", bold),
        Line::raw(""),
        Line::from(names),
    ];

    let settings = MIXER.len() as u16;
    let rows = usize::from(area.height.saturating_sub(ROWS_ABOVE_METERS + settings));
    for row in (0..rows).rev() {
        lines.push(meter_line(app, shown.clone(), row, rows));
    }

    let current = app.track_control();
    for control in MIXER {
        let label = app.field_jump.as_ref().and_then(|jump| jump.label(control));
        let mut spans = name_spans(
            app,
            control.label(),
            &label.map_or_else(String::new, String::from),
            LABEL_WIDTH,
        );
        for index in shown.clone() {
            let track = &song.tracks[index];
            let live = app.levels.get(index).copied().unwrap_or_default();
            let here = index == app.cursor.track;
            let selected = here && control == current;
            let text = match control {
                _ if !control.exists_on(track) => String::new(),
                Control::Mute => if live.mute { "M" } else { "-" }.to_owned(),
                Control::Solo => if live.solo { "S" } else { "-" }.to_owned(),
                _ if selected && app.mode == Mode::Insert => format!("{}_", app.entry.text),
                _ => value(control, track, song.groove),
            };
            let style = if selected {
                theme::cursor(app.mode)
            } else if live.mute {
                theme::dim()
            } else {
                Style::new()
            };
            let gap = STRIP_WIDTH.saturating_sub(text.chars().count());
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
