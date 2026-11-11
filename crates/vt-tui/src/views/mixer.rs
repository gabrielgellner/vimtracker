//! The mixer: a strip for each track, with its level meter, its fader, its sends, mute and solo, laid
//! out as Polyend Tracker's mixer is. The sound tracks, the groups, and the returns and master each
//! stand apart, and a group's strip and the tracks in it share its colour (decision 126). A folded
//! group's tracks are left out, with its name marked, and an empty group is folded away unless it's
//! opened (decision 128). Over each part, on the row under the title, is its name on a rule, as a
//! device's title is on its frame, the cursor's brighter (decision 144).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Row};
use vt_model::constants::MAX_GROUPS;
use vt_model::song::{Song, level_gain};

use crate::app::App;
use crate::glyphs;
use crate::meter::{height_of, to_db};
use crate::theme;
use crate::track_page::{Control, MIXER};
use crate::views::track::value;
use crate::views::widgets::group::{self, Switched};
use crate::views::widgets::meter::{EIGHTHS, level_cell};
use crate::views::widgets::setting::{self, jump_label, name_line, value_spans};
use crate::views::widgets::table::{Cells, Columns, fitted, take_top};

/// Room for the names of the settings, at the left. Beside the meters it holds the scale's mark,
/// with a gap after it.
const LABEL_WIDTH: u16 = 7;
const MARK_WIDTH: u16 = 5;
/// How wide a track's strip is: its name fits, and eight strips fit beside the main meter at 80.
const STRIP_WIDTH: u16 = 7;
/// The room before the first group's strip and before the first return's, so the sound tracks,
/// the groups and the returns with the master read as three parts.
const SECTION_GAP: u16 = 2;
/// The rule's line, as a device frame's top border is drawn.
const RULE: char = '─';
/// The names of the parts, over their strips: the sound tracks, the groups, and the returns with
/// the master.
const PART_NAMES: [&str; 3] = ["Tracks", "Groups", "Returns"];
/// The rows that aren't the meters: the title, the parts' names, the track names, and under them the group a
/// track is in, or the tracks a group sums.
const ROWS_ABOVE_METERS: u16 = 4;

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

/// The strips shown, and where the gaps between the parts go.
struct Strips {
    /// The tracks drawn, in order, with those folded away left out (decision 128).
    shown: Vec<usize>,
    /// Where the groups start, and the returns.
    groups: usize,
    returns: usize,
}

impl Strips {
    fn new(song: &Song, shown: Vec<usize>) -> Self {
        let groups = song.sound_tracks();
        Self {
            shown,
            groups,
            returns: groups + MAX_GROUPS,
        }
    }

    /// Which part a track is in: the sound tracks, the groups, or the returns and the master.
    fn part(&self, index: usize) -> usize {
        usize::from(index >= self.groups) + usize::from(index >= self.returns)
    }

    /// Whether the strip at this place starts a part, after another shown before it.
    fn gap_before(&self, place: usize) -> bool {
        place > 0 && self.part(self.shown[place]) != self.part(self.shown[place - 1])
    }

    /// How many strips are shown from this place on that are in the same part as it.
    fn run_from(&self, place: usize) -> usize {
        let part = self.part(self.shown[place]);
        self.shown[place..]
            .iter()
            .take_while(|&&index| self.part(index) == part)
            .count()
    }

    /// The columns: the labels' two, then each strip's, with a gap's before each part.
    fn columns(&self) -> Columns {
        let mut widths = vec![MARK_WIDTH, LABEL_WIDTH - MARK_WIDTH];
        for place in 0..self.shown.len() {
            if self.gap_before(place) {
                widths.push(SECTION_GAP);
            }
            widths.push(STRIP_WIDTH);
        }
        Columns::new(widths)
    }

    /// Put a cell in each strip's column, from the left, leaving the gaps empty.
    fn fill<L: Into<Line<'static>>>(
        &self,
        cells: &mut Cells<'_>,
        mut cell: impl FnMut(usize) -> L,
    ) {
        for (place, &index) in self.shown.iter().enumerate() {
            if self.gap_before(place) {
                cells.skip(1);
            }
            cells.cell(cell(index));
        }
    }
}

/// A strip's name: a sound track's number and name, or a fixed track's icon and name, a group's in
/// its colour, faint while nothing's in it, and marked while its tracks are folded into it.
fn name_cell(app: &App, index: usize) -> Line<'static> {
    let song = app.song();
    let track = &song.tracks[index];
    let mut style = if track.is_sound() {
        Style::new()
    } else if group::is_empty(song, index) {
        theme::faint()
    } else {
        group::style(song, index).unwrap_or_default()
    };
    if index == app.cursor.track {
        style = style.add_modifier(Modifier::BOLD);
    }
    let name = if track.is_sound() {
        format!("{:02} {}", index + 1, track.name)
    } else {
        format!(
            "{}{}{}",
            glyphs::track(track),
            glyphs::ICON_SPACE,
            track.name
        )
    };
    if app.folds.holds(song, index) {
        let mark = [glyphs::FOLDED, glyphs::ICON_SPACE];
        return Line::styled(name.chars().chain(mark).collect::<String>(), style);
    }
    Line::styled(name, style)
}

/// Under a strip's name: the group a sound track plays through, in that group's colour, or the
/// tracks a group sums.
fn routing_cell(song: &Song, index: usize) -> Line<'static> {
    let track = &song.tracks[index];
    if track.is_group() {
        let members: Vec<usize> = song.group_members(index).collect();
        return Line::styled(group::runs(&members), theme::dim());
    }
    group::mark(song, index).map_or_else(Line::default, Line::from)
}

/// A part's name on a rule this wide, such as `─ Tracks ────`, in the frame's colour and the name
/// dim, as a device's title is, or both brighter with the cursor in it. Too narrow for the rule as
/// well, as over a lone strip, it's the name alone, cut to fit if it must be.
fn part_rule(name: &str, width: usize, focused: bool) -> Line<'static> {
    let (rule, name_style) = if focused {
        (theme::OVERLAY2, Style::new().fg(theme::TEXT))
    } else {
        (theme::SURFACE1, theme::dim())
    };
    let rule = Style::new().fg(rule);
    let label = format!(" {name} ");
    let named = 2 + label.chars().count();
    if width < named {
        return fitted(Line::styled(name.to_owned(), name_style), width);
    }
    Line::from(vec![
        Span::styled(RULE.to_string(), rule),
        Span::styled(label, name_style),
        Span::styled(RULE.to_string().repeat(width - named + 1), rule),
    ])
}

/// The row of the parts' names, each on a rule over its strips that stops a column short of the
/// next part's gap.
fn parts_row(app: &App, columns: &Columns, strips: &Strips) -> Row<'static> {
    let mut cells = columns.row();
    cells.skip(2);
    let cursor = strips.part(app.cursor.track);
    let mut place = 0;
    while place < strips.shown.len() {
        if strips.gap_before(place) {
            cells.skip(1);
        }
        let count = strips.run_from(place);
        let part = strips.part(strips.shown[place]);
        let width = usize::from(STRIP_WIDTH) * count - 1;
        cells.span(part_rule(PART_NAMES[part], width, part == cursor), count);
        place += count;
    }
    cells.done()
}

/// One row of the meters, counting rows up from the bottom.
fn meter_row(
    app: &App,
    columns: &Columns,
    strips: &Strips,
    row: usize,
    rows: usize,
) -> Row<'static> {
    let zero = ((height_of(0.0) * rows as f32) as usize).min(rows - 1);
    let mark = if row == zero { "0" } else { "" };
    let mut cells = columns.row();
    cells
        .cell(Line::styled(mark, theme::dim()).right_aligned())
        .skip(1);
    strips.fill(&mut cells, |index| {
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
        vec![
            channel(0),
            channel(1),
            Span::raw(" "),
            fader_cell(row, rows, app.song().tracks[index].level),
        ]
    });
    cells.done()
}

/// Draw the mixer.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let song = app.song();
    // Room is kept for both gaps, whether or not they're in view.
    let room = area.width.saturating_sub(LABEL_WIDTH + 2 * SECTION_GAP);
    let count = usize::from(room / STRIP_WIDTH).max(1);
    let tracks: Vec<usize> = app.folds.shown(song).collect();
    let at = tracks.partition_point(|&track| track < app.cursor.track);
    let first = first_shown(at, tracks.len(), count);
    let strips = Strips::new(
        song,
        tracks[first..(first + count).min(tracks.len())].to_vec(),
    );
    let columns = strips.columns();

    let mut rest = area;
    frame.render_widget(
        Paragraph::new(Line::styled("Mixer", bold)),
        take_top(&mut rest, 1),
    );
    let mut names = columns.row();
    names.skip(2);
    strips.fill(&mut names, |index| name_cell(app, index));
    let mut routing = columns.row();
    routing.skip(2);
    strips.fill(&mut routing, |index| routing_cell(song, index));
    let mut rows = vec![
        parts_row(app, &columns, &strips),
        names.done(),
        routing.done(),
    ];

    let settings = MIXER.len() as u16;
    let meter_rows = usize::from(area.height.saturating_sub(ROWS_ABOVE_METERS + settings));
    for row in (0..meter_rows).rev() {
        rows.push(meter_row(app, &columns, &strips, row, meter_rows));
    }

    let current = app.track_control();
    for control in MIXER {
        let mut cells = columns.row();
        let labels = jump_label(app, control);
        cells.span(name_line(app, control.label(), &labels, LABEL_WIDTH), 2);
        strips.fill(&mut cells, |index| {
            let track = &song.tracks[index];
            let muted = group::muted(app, index);
            let here = index == app.cursor.track;
            let selected = here && control == current;
            // A mute or solo that's on is its glyph: in the active colour for the track's own, and
            // in its group's colour for its group's.
            let switched = match control {
                Control::Mute => Some((muted, glyphs::MUTE)),
                Control::Solo => Some((group::soloed(app, index), glyphs::SOLO)),
                _ => None,
            };
            let text = match (control, switched) {
                _ if !control.exists_on(track) => String::new(),
                (_, Some((Switched::Off, _))) => "-".to_owned(),
                (_, Some((_, glyph))) => glyph.to_string(),
                _ => setting::text(app, selected, || value(control, track, song)),
            };
            let style = match switched {
                _ if selected => theme::cursor(app.mode),
                Some((why, _)) if why.is_on() => why.style(),
                _ if muted.is_on() => theme::dim(),
                _ if group::is_empty(song, index) => theme::faint(),
                _ => Style::new(),
            };
            // A jump's labels go on the cursor's track, which it stays on.
            if here {
                Line::from(value_spans(app, control, text, style))
            } else {
                Line::styled(text, style)
            }
        });
        rows.push(cells.done());
    }
    columns.stack(frame, &mut rest, rows);
}
