//! The mixer: a strip for each track, with its level meter, its fader, its sends, mute and solo, laid
//! out as Polyend Tracker's mixer is.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Row};
use vt_model::song::level_gain;

use crate::app::App;
use crate::glyphs;
use crate::meter::{height_of, to_db};
use crate::theme;
use crate::track_page::{Control, MIXER};
use crate::views::track::value;
use crate::views::widgets::meter::{EIGHTHS, level_cell};
use crate::views::widgets::setting::{self, jump_label, name_line, value_spans};
use crate::views::widgets::table::{Columns, take_top};

/// Room for the names of the settings, at the left. Beside the meters it holds the scale's mark,
/// with a gap after it.
const LABEL_WIDTH: u16 = 7;
const MARK_WIDTH: u16 = 5;
/// How wide a track's strip is: its name fits, and eight strips fit beside the main meter at 80.
const STRIP_WIDTH: u16 = 7;
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
fn meter_row(
    app: &App,
    columns: &Columns,
    shown: std::ops::Range<usize>,
    row: usize,
    rows: usize,
) -> Row<'static> {
    let zero = ((height_of(0.0) * rows as f32) as usize).min(rows - 1);
    let mark = if row == zero { "0" } else { "" };
    let mut cells = columns.row();
    cells
        .cell(Line::styled(mark, theme::dim()).right_aligned())
        .skip(1);
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
        cells.cell(vec![
            channel(0),
            channel(1),
            Span::raw(" "),
            fader_cell(row, rows, app.song().tracks[index].level),
        ]);
    }
    cells.done()
}

/// Draw the mixer.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let song = app.song();
    let room = area.width.saturating_sub(LABEL_WIDTH);
    let strips = usize::from(room / STRIP_WIDTH).max(1);
    let first = first_shown(app.cursor.track, song.tracks.len(), strips);
    let shown = first..(first + strips).min(song.tracks.len());
    let mut widths = vec![MARK_WIDTH, LABEL_WIDTH - MARK_WIDTH];
    widths.extend(std::iter::repeat_n(STRIP_WIDTH, shown.len()));
    let columns = Columns::new(widths);

    let mut rest = area;
    frame.render_widget(
        Paragraph::new(Line::styled("Mixer", bold)),
        take_top(&mut rest, 2),
    );
    let mut names = columns.row();
    names.skip(2);
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
        names.cell(Line::styled(name, style));
    }
    let mut rows = vec![names.done()];

    let settings = MIXER.len() as u16;
    let meter_rows = usize::from(area.height.saturating_sub(ROWS_ABOVE_METERS + settings));
    for row in (0..meter_rows).rev() {
        rows.push(meter_row(app, &columns, shown.clone(), row, meter_rows));
    }

    let current = app.track_control();
    for control in MIXER {
        let mut cells = columns.row();
        let labels = jump_label(app, control);
        cells.span(name_line(app, control.label(), &labels, LABEL_WIDTH), 2);
        for index in shown.clone() {
            let track = &song.tracks[index];
            let live = app.levels.get(index).copied().unwrap_or_default();
            let here = index == app.cursor.track;
            let selected = here && control == current;
            // A mute or solo that's on is its glyph, in the active colour.
            let switched = match control {
                Control::Mute => Some((live.mute, glyphs::MUTE)),
                Control::Solo => Some((live.solo, glyphs::SOLO)),
                _ => None,
            };
            let text = match (control, switched) {
                _ if !control.exists_on(track) => String::new(),
                (_, Some((true, glyph))) => glyph.to_string(),
                (_, Some((false, _))) => "-".to_owned(),
                _ => setting::text(app, selected, || value(control, track, song)),
            };
            let style = if selected {
                theme::cursor(app.mode)
            } else if switched.is_some_and(|(on, _)| on) {
                theme::active()
            } else if live.mute {
                theme::dim()
            } else {
                Style::new()
            };
            // A jump's labels go on the cursor's track, which it stays on.
            if here {
                cells.cell(value_spans(app, control, text, style));
            } else {
                cells.cell(Line::styled(text, style));
            }
        }
        rows.push(cells.done());
    }
    columns.stack(frame, &mut rest, rows);
}
