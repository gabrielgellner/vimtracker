//! The song view: a row per section, with its length, how it starts and each track's chain.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_engine::engine::Transport;
use vt_model::song::SectionStart;

use crate::app::App;
use crate::input::mode::Mode;
use crate::song_view::COLUMNS_BEFORE_TRACKS;
use crate::theme;

/// Room for a section's number and playing marker.
const NUMBER_WIDTH: usize = 5;
const LENGTH_WIDTH: usize = 8;
const START_WIDTH: usize = 7;
/// A track's column: two hex digits, or a name over them, and a gap.
const TRACK_WIDTH: usize = 4;
/// The rows that aren't sections: the title, a gap and the column names.
const ROWS_AROUND_SECTIONS: u16 = 3;

/// The first of a run that fits, keeping the cursor's in view.
fn first_shown(cursor: usize, count: usize, room: usize) -> usize {
    let room = room.max(1);
    cursor
        .saturating_sub(room / 2)
        .min(count.saturating_sub(room))
}

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let song = app.song();
    let before = NUMBER_WIDTH + LENGTH_WIDTH + START_WIDTH;
    let room = usize::from(area.width).saturating_sub(before) / TRACK_WIDTH;
    let cursor_track = app.song_column.saturating_sub(COLUMNS_BEFORE_TRACKS);
    let first_track = first_shown(cursor_track, song.tracks.len(), room);
    let shown_tracks = first_track..(first_track + room.max(1)).min(song.tracks.len());

    let mut heading = format!(
        "{:<NUMBER_WIDTH$}{:<LENGTH_WIDTH$}{:<START_WIDTH$}",
        "", "Length", "Start"
    );
    for track in shown_tracks.clone() {
        let name = &song.tracks[track].name;
        let label = if name.is_empty() {
            format!("{:02}", track + 1)
        } else {
            name.clone()
        };
        heading.push_str(&format!("{label:<TRACK_WIDTH$}"));
    }
    let mut lines = vec![
        Line::styled("Song", bold),
        Line::raw(""),
        Line::styled(heading, theme::dim()),
    ];

    let rows = usize::from(area.height.saturating_sub(ROWS_AROUND_SECTIONS));
    let first = first_shown(app.cursor.section, song.sections.len(), rows);
    let moving = matches!(app.transport, Transport::Playing | Transport::Paused);
    for (index, section) in song
        .sections
        .iter()
        .enumerate()
        .skip(first)
        .take(rows.max(1))
    {
        let here = index == app.cursor.section;
        let marker = if moving && index == app.playing_section {
            ">"
        } else {
            " "
        };
        let number = format!("{marker}{:02}", index + 1);
        let number_style = if marker == ">" {
            theme::active()
        } else if here {
            bold
        } else {
            Style::new()
        };
        let mut spans = vec![Span::styled(
            format!("{number:<NUMBER_WIDTH$}"),
            number_style,
        )];
        let playing_here = moving && index == app.playing_section;
        let mut cell = |text: String, column: usize, width: usize, faint: bool| {
            let selected = here && app.song_column == column;
            let typing = selected && app.mode == Mode::Insert;
            let text = if typing {
                format!("{}_", app.entry.text)
            } else {
                text
            };
            let style = if selected {
                theme::cursor(app.mode)
            } else if faint {
                theme::dim()
            } else {
                Style::new()
            };
            let gap = width.saturating_sub(text.chars().count());
            spans.push(Span::styled(text, style));
            spans.push(Span::raw(" ".repeat(gap)));
        };
        cell(
            song.section_length(section).to_string(),
            0,
            LENGTH_WIDTH,
            section.length.is_none(),
        );
        let start = match section.start {
            SectionStart::New => "new",
            SectionStart::Reset => "reset",
        };
        cell(start.to_owned(), 1, START_WIDTH, false);
        for track in shown_tracks.clone() {
            let chain = section.chains.get(track).copied().flatten();
            let jam = app.jam.get(track).copied().unwrap_or_default();
            let text = chain.map_or_else(|| "--".to_owned(), |id| format!("{id:02X}"));
            // `~` is waiting to start, `>` is launched, and a row's chain that's been replaced dims.
            let mark = if jam.cued == Some(chain) {
                "~"
            } else if chain.is_some() && jam.playing == Some(chain) {
                ">"
            } else {
                ""
            };
            let replaced = playing_here && jam.playing.is_some_and(|playing| playing != chain);
            cell(
                format!("{text}{mark}"),
                COLUMNS_BEFORE_TRACKS + track,
                TRACK_WIDTH,
                chain.is_none() || replaced,
            );
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
