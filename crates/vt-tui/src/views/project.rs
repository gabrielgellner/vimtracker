//! The project page: its name and folder, how it plays and how notes go in, the audio output it
//! plays through, and the actions that save, render and open projects.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::config::home_folder;
use crate::explorer::place_name;
use crate::input::mode::Mode;
use crate::project_page::{PROJECT_ROWS, ProjectControl};
use crate::theme;
use crate::views::status::{format_swing, grid_name};

/// Room for a setting's name, and where the right column starts.
const LABEL: usize = 10;
const RIGHT_COLUMN: usize = 40;
/// The most of a folder's path shown, so whether it's saved still fits beside it.
const FOLDER_WIDTH: usize = 64;

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
        ProjectControl::Swing => format_swing(song.swing),
        ProjectControl::Launch => song.launch.to_string(),
        ProjectControl::Reset => match app.section_here().and_then(|section| section.reset) {
            Some(1) => "1 step".to_owned(),
            Some(steps) => format!("{steps} steps"),
            None => "off".to_owned(),
        },
        ProjectControl::Seed => song.seed.to_string(),
        ProjectControl::Limiter => on_off(song.limiter),
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
        _ => control.meaning().map(str::to_owned),
    }
}

/// One row: each setting's name dim, then its value, the cursor's under it.
fn row(app: &App, controls: &[ProjectControl]) -> Line<'static> {
    let here = app.project_control();
    let typing = app.mode == Mode::Insert;
    let mut spans = Vec::new();
    let mut used = 0;
    for control in controls {
        let at = control.column() * RIGHT_COLUMN;
        spans.push(Span::raw(" ".repeat(at.saturating_sub(used))));
        used = used.max(at);
        let is_here = *control == here;
        let on_fine = is_here && app.project_fine;
        // An action is its own name, so the cursor is on that rather than a value beside it.
        if !control.is_action() {
            spans.push(Span::styled(
                format!("{:<LABEL$}", control.label()),
                theme::dim(),
            ));
            used += LABEL;
        }
        let text = if is_here && typing && !on_fine {
            format!("{}_", app.entry.text)
        } else {
            value(app, *control)
        };
        let style = if is_here && !on_fine {
            theme::cursor(app.mode)
        } else if *control == ProjectControl::Root && !app.scale.on
            || *control == ProjectControl::Folder && app.folder.is_none()
        {
            theme::dim()
        } else {
            Style::new()
        };
        used += text.chars().count();
        spans.push(Span::styled(text, style));
        if *control == ProjectControl::Bpm {
            let digits = if on_fine && typing {
                format!(".{}_", app.entry.text)
            } else {
                fine(app.song().millibpm)
            };
            let style = if on_fine {
                theme::cursor(app.mode)
            } else {
                theme::dim()
            };
            used += digits.chars().count();
            spans.push(Span::styled(digits, style));
        }
        if let Some(note) = after(app, *control) {
            let gap = if control.is_action() {
                LABEL.saturating_sub(control.label().len())
            } else {
                2
            };
            used += gap + note.chars().count();
            spans.push(Span::raw(" ".repeat(gap)));
            spans.push(Span::styled(note, theme::dim()));
        }
    }
    Line::from(spans)
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let mut lines = vec![Line::styled("Project", bold), Line::raw("")];
    for controls in PROJECT_ROWS {
        // The settings, the audio and the actions each stand apart.
        if matches!(
            controls.first(),
            Some(ProjectControl::Bpm | ProjectControl::Audio | ProjectControl::Save)
        ) {
            lines.push(Line::raw(""));
        }
        lines.push(row(app, controls));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
