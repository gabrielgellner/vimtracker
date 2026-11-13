//! The project page's Song sub-page, and the title both its sub-pages share. The Song sub-page
//! has the project's name and folder, how it plays and how notes go in, a groove drawn as its
//! pushes, the audio output it plays through, and the actions that save, render and open projects.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Row};

use vt_model::groove::{FULL_VELOCITY, MAX_PUSH};

use crate::app::App;
use crate::config::home_folder;
use crate::explorer::place_name;
use crate::input::mode::{Mode, View};
use crate::project_page::{PROJECT_PAGES, PROJECT_ROWS, ProjectControl};
use crate::theme;
use crate::views::status::grid_name;
use crate::views::widgets::meter::{EIGHTHS, hanging_cell};
use crate::views::widgets::setting;
use crate::views::widgets::table::{Columns, blank_row, take_top};

/// Room for a setting's name, and where the right column starts.
const LABEL: u16 = 10;
const RIGHT_COLUMN: u16 = 40;
/// The most of a folder's path shown, so whether it's saved still fits beside it.
const FOLDER_WIDTH: usize = 64;
/// How wide each of a groove's steps is drawn, room for a push of `-120` and a gap, and the gap
/// before its units.
const STEP_WIDTH: u16 = 5;
const UNITS_GAP: u16 = 2;

/// A path that fits, keeping its end, which says most about where it is.
fn kept_end(path: String) -> String {
    let length = path.chars().count();
    if length <= FOLDER_WIDTH {
        return path;
    }
    let kept: String = path.chars().skip(length + 1 - FOLDER_WIDTH).collect();
    format!("…{kept}")
}

/// A setting's value as text, apart from the tempo's fine part.
fn value(app: &App, control: ProjectControl) -> String {
    let song = app.song();
    let on_off = |on: bool| if on { "on" } else { "off" }.to_owned();
    match control {
        ProjectControl::Name => app.project_name(),
        ProjectControl::Folder => app.folder.as_deref().map_or_else(
            || "Not saved yet".to_owned(),
            |folder| kept_end(place_name(folder, home_folder().as_deref())),
        ),
        ProjectControl::Bpm => (song.millibpm / 1000).to_string(),
        ProjectControl::Groove => song.groove.name().to_owned(),
        ProjectControl::GrooveShown => app.shown_groove().0.name().to_owned(),
        ProjectControl::GrooveSteps => app.shown_groove().1.steps().to_string(),
        ProjectControl::Humanise => format!("{} ticks", app.shown_groove().1.humanise()),
        ProjectControl::Push(step) => push_text(app.shown_groove().1.push(usize::from(step))),
        ProjectControl::Velocity(step) => {
            format!("{}", app.shown_groove().1.velocity(usize::from(step)))
        }
        ProjectControl::Launch => song.launch.to_string(),
        ProjectControl::Reset => match app.section_here().and_then(|section| section.reset) {
            Some(1) => "1 step".to_owned(),
            Some(steps) => format!("{steps} steps"),
            None => "off".to_owned(),
        },
        ProjectControl::Seed => song.seed.to_string(),
        ProjectControl::Scale if app.scale.on => app.scale.kind.name().to_owned(),
        ProjectControl::Scale => "off".to_owned(),
        ProjectControl::Root => app.scale.root_name().to_owned(),
        ProjectControl::Grid => grid_name(song.skip),
        ProjectControl::AutoJump => on_off(app.auto_jump),
        ProjectControl::Audio if app.audio.device.is_empty() => "None".to_owned(),
        ProjectControl::Audio => format!("{}, {} Hz", app.audio.device, app.rate),
        ProjectControl::Save
        | ProjectControl::SaveAs
        | ProjectControl::Render
        | ProjectControl::Open
        | ProjectControl::New => control.label().to_owned(),
    }
}

/// The tempo's fine part, as the digits after the point: two, or three where the last isn't 0.
fn fine(millibpm: u32) -> String {
    let thousandths = millibpm % 1000;
    if thousandths.is_multiple_of(10) {
        format!(".{:02}", thousandths / 10)
    } else {
        format!(".{thousandths:03}")
    }
}

/// What a setting says dim after its value, if anything.
fn after(app: &App, control: ProjectControl) -> Option<String> {
    match control {
        ProjectControl::Folder if app.folder.is_none() => None,
        ProjectControl::Folder if app.dirty => Some("unsaved changes".to_owned()),
        ProjectControl::Folder => Some("saved".to_owned()),
        // A reset belongs to a section, so with more than one it says which.
        ProjectControl::Reset if app.song().sections.len() > 1 => {
            Some(format!("in section {}", app.cursor.section + 1))
        }
        ProjectControl::Audio if !app.audio.running => Some("stopped".to_owned()),
        // The groove drawn says whether the project plays it, and one that comes with the program
        // that changing it copies.
        ProjectControl::GrooveShown => {
            let (id, _) = app.shown_groove();
            let plays = id == app.song().groove;
            Some(match (id.custom_number(), plays) {
                (Some(_), true) => "the project's own, which it plays".to_owned(),
                (Some(_), false) => "the project's own".to_owned(),
                (None, true) => "which the project plays; a change makes a copy".to_owned(),
                (None, false) => "a change makes a copy".to_owned(),
            })
        }
        _ => control.meaning().map(str::to_owned),
    }
}

/// The settings' columns: a name and its value on the left, and the same on the right, whose value
/// has the rest of the room.
fn columns() -> Columns {
    Columns::new([LABEL, RIGHT_COLUMN - LABEL, LABEL, 0]).open_ended()
}

/// One row: each setting's name dim, then its value, the cursor's under it. A value with nothing to
/// its right has the rest of the row.
fn row(app: &App, controls: &[ProjectControl], columns: &Columns) -> Row<'static> {
    let here = app.project_control();
    let typing = app.mode == Mode::Insert;
    let mut cells = columns.row();
    for (index, control) in controls.iter().enumerate() {
        cells.to(control.column() * 2);
        let is_here = *control == here;
        let on_fine = is_here && app.project_fine;
        let on_value = is_here && !on_fine;
        let text = setting::text(app, on_value, || value(app, *control));
        let default = *control == ProjectControl::Root && !app.scale.on
            || *control == ProjectControl::Folder && app.folder.is_none();
        let style = setting::style(app, on_value, default);
        // An action is its own name, so the cursor is on that rather than a value beside it, and
        // what it does is in the value's place.
        let mut spans = Vec::new();
        if control.is_action() {
            cells.cell(Line::styled(text, style));
        } else {
            cells.cell(Line::styled(control.label(), theme::dim()));
            spans.push(Span::styled(text, style));
        }
        if *control == ProjectControl::Bpm {
            let digits = if on_fine && typing {
                format!(".{}_", app.entry.text)
            } else {
                fine(app.song().millibpm)
            };
            spans.push(Span::styled(digits, setting::style(app, on_fine, true)));
        }
        if let Some(note) = after(app, *control) {
            if !spans.is_empty() {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(note, theme::dim()));
        }
        let alone = index + 1 == controls.len();
        let span = if alone { 3 - control.column() * 2 } else { 1 };
        cells.span(spans, span);
    }
    cells.done()
}

/// A push as it's shown, with its sign when it isn't 0.
fn push_text(push: i16) -> String {
    if push > 0 {
        format!("+{push}")
    } else {
        push.to_string()
    }
}

/// A groove's columns: the row's name, a column for each step, and its units after a gap.
fn groove_columns(steps: usize) -> Columns {
    let mut widths = vec![LABEL];
    widths.extend(std::iter::repeat_n(STEP_WIDTH, steps));
    widths.extend([UNITS_GAP, 0]);
    Columns::new(widths).open_ended()
}

/// A groove's pushes drawn as ticks above a line for the steps that land late and below it for
/// those that land early, each as tall as its push, and its values under it, then its velocities,
/// the cursor's under it. One colour, as the curves are.
fn groove_rows(app: &App, columns: &Columns) -> Vec<Row<'static>> {
    let (_, groove) = app.shown_groove();
    let here = app.project_control();
    let steps = groove.steps();
    // Each tick sits over the last digit of its value.
    let tick = |filled: usize, late: bool| {
        let cell = if late {
            Span::raw(EIGHTHS[filled].to_string())
        } else {
            hanging_cell(filled, theme::TEXT, theme::BASE, 1)
        };
        Line::from(cell).right_aligned()
    };
    let eighths = |push: i16| {
        let share = f32::from(push.unsigned_abs()) / f32::from(MAX_PUSH as u16);
        ((share * 8.0).ceil() as usize).min(8)
    };
    let mut above = columns.row();
    let mut below = columns.row();
    above.skip(1);
    below.skip(1);
    for push in groove.pushes() {
        let filled = eighths(*push);
        above.cell(tick(if *push > 0 { filled } else { 0 }, true));
        below.cell(tick(if *push < 0 { filled } else { 0 }, false));
    }
    let mut line = columns.row();
    line.skip(1).span(
        Line::styled("─".repeat(columns.room(1, steps)), theme::faint()),
        steps,
    );
    let values = |control: fn(u8) -> ProjectControl, units: &'static str| {
        let mut cells = columns.row();
        cells.cell(Line::styled(control(0).label(), theme::dim()));
        for step in 0..steps {
            let field = control(step as u8);
            let is_here = field == here;
            let text = setting::text(app, is_here, || value(app, field));
            // What's left on the grid or at full velocity matters less.
            let default = field == ProjectControl::Push(0)
                || matches!(field, ProjectControl::Velocity(_))
                    && text == FULL_VELOCITY.to_string();
            let style = setting::style(app, is_here, default);
            cells.cell(Line::styled(text, style).right_aligned());
        }
        cells.skip(1).cell(Line::styled(units, theme::dim()));
        cells.done()
    };
    vec![
        above.done(),
        line.done(),
        below.done(),
        values(ProjectControl::Push, "ticks"),
        values(ProjectControl::Velocity, "%"),
    ]
}

/// A sub-page's name in the project page's title. The Audio sub-page's says it's this machine's,
/// as its settings are kept in the config folder (`audio.ron`, `wheel.ron` and `roto.ron`)
/// rather than the song.
fn page_name(view: View) -> &'static str {
    match view {
        View::Audio => "Audio · this machine",
        _ => "Song",
    }
}

/// The project page's title, on either sub-page: `Project`, then each sub-page's name, the one
/// shown bold and the other dim, as the track page's pages are.
pub fn title(app: &App) -> Line<'static> {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut title = vec![Span::styled("Project ", bold)];
    title.extend(PROJECT_PAGES.map(|view| {
        let text = format!(" {} ", page_name(view));
        if view == app.view {
            Span::styled(text, bold)
        } else {
            Span::styled(text, theme::dim())
        }
    }));
    Line::from(title)
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let mut rest = area;
    frame.render_widget(Paragraph::new(title(app)), take_top(&mut rest, 2));
    let columns = columns();
    let mut rows = Vec::new();
    for controls in PROJECT_ROWS {
        // The settings, the groove, the audio and the actions each stand apart.
        match controls.first() {
            Some(
                ProjectControl::Bpm
                | ProjectControl::GrooveShown
                | ProjectControl::Audio
                | ProjectControl::Save,
            ) => rows.push(blank_row()),
            // The pushes and velocities are drawn together, in columns of their own.
            Some(ProjectControl::Push(_)) => {
                columns.stack(frame, &mut rest, std::mem::take(&mut rows));
                let groove = groove_columns(app.shown_groove().1.steps());
                let drawn = groove_rows(app, &groove);
                groove.stack(frame, &mut rest, drawn);
                continue;
            }
            Some(ProjectControl::Velocity(_)) => continue,
            _ => {}
        }
        rows.push(row(app, controls, &columns));
    }
    columns.stack(frame, &mut rest, rows);
}
