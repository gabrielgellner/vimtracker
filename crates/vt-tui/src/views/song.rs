//! The song view: a row per section, with its length, how it starts and each track's chain, and a
//! bar down the sections `:loop` keeps playing (decision 173). A group with no tracks in it is
//! faint, as it plays nothing (decision 126). A group and its tracks are named in its colour, and
//! a folded group's tracks are left out, its name marked, while an empty group is folded away
//! unless it's opened (decision 128).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use vt_engine::engine::Transport;
use vt_model::label::GROUP_NAME_CHARS;
use vt_model::song::{SectionStart, Song};

use crate::app::App;
use crate::glyphs;
use crate::input::mode::Mode;
use crate::song_view::{COLUMNS_BEFORE_TRACKS, LENGTH_COLUMN, REPEAT_COLUMN, START_COLUMN};
use crate::theme;
use crate::views::widgets::group;
use crate::views::widgets::table::centred_run;

/// Room for a section's number, its playing marker and its loop's bar.
const NUMBER_WIDTH: usize = 5;
/// Down the side of the sections `:loop` keeps playing.
const LOOP_BAR: &str = "\u{2502}";
const LENGTH_WIDTH: usize = 8;
const REPEAT_WIDTH: usize = 5;
const START_WIDTH: usize = 7;
/// A track's column: two hex digits, or a name over them, and a gap.
const TRACK_WIDTH: usize = 4;
/// A group's column: its whole name, the mark beside it while it's folded, and a gap
/// (decision 308).
const GROUP_WIDTH: usize = GROUP_NAME_CHARS + 2;
/// The rows that aren't sections: the title, a gap and the column names.
const ROWS_AROUND_SECTIONS: u16 = 3;

/// The first of a run that fits, keeping the cursor's in view.
fn first_shown(cursor: usize, count: usize, room: usize) -> usize {
    let room = room.max(1);
    cursor
        .saturating_sub(room / 2)
        .min(count.saturating_sub(room))
}

/// How wide a track's column is: a group's holds its whole name.
fn column_width(song: &Song, track: usize) -> usize {
    if song.tracks[track].is_group() {
        GROUP_WIDTH
    } else {
        TRACK_WIDTH
    }
}

/// Draw the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let song = app.song();
    let before = NUMBER_WIDTH + LENGTH_WIDTH + REPEAT_WIDTH + START_WIDTH;
    let room = usize::from(area.width).saturating_sub(before);
    // The tracks not folded away, of which as many as fit around the cursor's are drawn, or the
    // cursor's alone if none do.
    let cursor_track = app.song_column.saturating_sub(COLUMNS_BEFORE_TRACKS);
    let tracks: Vec<usize> = app.folds.shown(song).collect();
    let at = tracks.partition_point(|&track| track < cursor_track);
    let widths: Vec<usize> = tracks
        .iter()
        .map(|&track| column_width(song, track))
        .collect();
    let mut run = centred_run(&widths, at, room);
    if run.is_empty() && !tracks.is_empty() {
        let only = at.min(tracks.len() - 1);
        run = only..only + 1;
    }
    let shown_tracks = tracks[run].to_vec();

    let mut heading = vec![Span::raw(format!(
        "{:<NUMBER_WIDTH$}{:<LENGTH_WIDTH$}{:<REPEAT_WIDTH$}{:<START_WIDTH$}",
        "", "Length", "Rpt", "Start"
    ))];
    for &track in &shown_tracks {
        let name = &song.tracks[track].name;
        let mut label = if name.is_empty() {
            format!("{:02}", track + 1)
        } else {
            name.clone()
        };
        // A group with its tracks folded into it says so, as the mixer does, beside its whole
        // name, as its column has room for both.
        let width = column_width(song, track);
        if app.folds.holds(song, track) {
            label = label.chars().take(width - 2).collect();
            label.extend([glyphs::FOLDED, glyphs::ICON_SPACE]);
        } else {
            label = label.chars().take(width - 1).collect();
        }
        // A group and the tracks in it are in its colour, and an empty group is faint.
        let style = if group::is_empty(song, track) {
            theme::faint()
        } else {
            group::style(song, track).unwrap_or_default()
        };
        heading.push(Span::styled(format!("{label:<width$}"), style));
    }
    let mut lines = vec![
        Line::styled("Song", bold),
        Line::raw(""),
        Line::from(heading).style(theme::dim()),
    ];

    let rows = usize::from(area.height.saturating_sub(ROWS_AROUND_SECTIONS));
    let first = first_shown(app.cursor.section, song.sections.len(), rows);
    let moving = matches!(app.transport, Transport::Playing | Transport::Paused);
    let selection = app.selected_sections();
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
        let chosen = selection.is_some_and(|sections| sections.holds(index));
        let number = format!("{marker}{:02}", index + 1);
        let number_style = if marker == ">" {
            theme::active()
        } else if chosen {
            theme::selected()
        } else if here {
            bold
        } else {
            Style::new()
        };
        // A bar down the sections being looped, dim while the loop waits for the launch grid.
        let looped = app.looping.is_some_and(|sections| sections.holds(index));
        let bar = if looped { LOOP_BAR } else { " " };
        let bar_style = if app.loop_waiting {
            theme::dim()
        } else {
            theme::active()
        };
        let mut spans = vec![
            Span::styled(number, number_style),
            Span::styled(bar, bar_style),
            Span::raw(" ".repeat(NUMBER_WIDTH - 4)),
        ];
        let playing_here = moving && index == app.playing_section;
        let mut cell = |text: String, column: usize, width: usize, faint: Option<Style>| {
            let selected = here && app.song_column == column;
            let typing = selected && app.mode == Mode::Insert;
            let text = if typing {
                format!("{}_", app.entry.text)
            } else {
                text
            };
            let style = if selected {
                theme::cursor(app.mode)
            } else if chosen {
                theme::selected()
            } else if let Some(faint) = faint {
                faint
            } else {
                Style::new()
            };
            let gap = width.saturating_sub(text.chars().count());
            spans.push(Span::styled(text, style));
            spans.push(Span::raw(" ".repeat(gap)));
        };
        cell(
            song.section_length(section).to_string(),
            LENGTH_COLUMN,
            LENGTH_WIDTH,
            section.length.is_none().then(theme::dim),
        );
        cell(
            section.repeat.get().to_string(),
            REPEAT_COLUMN,
            REPEAT_WIDTH,
            (section.repeat.get() == 1).then(theme::dim),
        );
        let start = match section.start {
            SectionStart::New => "new",
            SectionStart::Reset => "reset",
        };
        cell(start.to_owned(), START_COLUMN, START_WIDTH, None);
        for &track in &shown_tracks {
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
            let faint = if group::is_empty(song, track) {
                Some(theme::faint())
            } else {
                (chain.is_none() || replaced).then(theme::dim)
            };
            cell(
                format!("{text}{mark}"),
                COLUMNS_BEFORE_TRACKS + track,
                column_width(song, track),
                faint,
            );
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
