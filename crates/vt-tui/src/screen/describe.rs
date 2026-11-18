//! The line that describes the setting under the cursor (decision 387): its name, what it does
//! and what its value means now, in a sentence or two, in two rows above the macro panel. `Space d`
//! shows it or hides it, for the session only, not in the song. It follows the cursor of the
//! macro panel while the keys are in it, of the shape panel while that's open, and of the Track
//! page's pages; anywhere else it takes no rows, so with it off, or on another page, the screen is
//! as it was.
//!
//! What each setting does is the model's, written once beside its descriptor
//! ([`vt_model::param::Descriptor::meaning`]), and [`vt_model::param::description`] puts the line
//! together, so the three places say a setting the same way.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use vt_model::param::description;
use vt_model::song::Song;

use crate::app::App;
use crate::controls::{Control, ParamView, view};
use crate::input::mode::{Mode, View};
use crate::overlays::mapping::{MappingView, mapping_line, moves};
use crate::overlays::shape::ShapePanel;

/// The rows the line takes while it shows: one for most settings, and room for a second where
/// the words run on, kept the same so the page above doesn't jump as the cursor moves.
pub const ROWS: u16 = 2;

/// The rows the line takes now: none while it's off, or while nothing on screen has a setting
/// under the cursor.
pub fn rows(app: &App) -> u16 {
    match app.overlays.describing && subject(app).is_some() {
        true => ROWS,
        false => 0,
    }
}

/// What the line describes now, if anything: the macro panel's macro while the keys are in it,
/// the mapping view's mapping, the shape panel's setting while it's open, or the Track page's.
enum Subject<'a> {
    Macro(usize),
    Mapping(&'a MappingView),
    Shape,
    Page(Control),
}

fn subject(app: &App) -> Option<Subject<'_>> {
    if app.modes.mode == Mode::Macros {
        return Some(Subject::Macro(app.overlays.macro_panel.at));
    }
    if app.modes.mode == Mode::Mapping
        && let Some(view) = &app.overlays.mapping
    {
        return Some(Subject::Mapping(view));
    }
    if app.overlays.shape.is_some() {
        return Some(Subject::Shape);
    }
    (app.modes.view == View::Track).then(|| Subject::Page(app.track_control()))
}

/// The line as it reads now, or nothing where there's nothing to describe.
pub fn text(app: &App) -> Option<String> {
    let subject = subject(app)?;
    if let Subject::Shape = subject {
        return app.overlays.shape.as_ref().map(shape_line);
    }
    let track = app.services.track_here()?;
    Some(match subject {
        Subject::Macro(at) | Subject::Page(Control::Macro(at)) => {
            macro_line(app.song(), app.services.cursor.track, at)
        }
        Subject::Mapping(view) => mapping_line(app.song(), app.services.cursor.track, view)?,
        // As it's heard, as the status line says it: where the steps' faders and the macros
        // have it.
        Subject::Page(control) => {
            let heard = app.services.track_as_heard(app.services.cursor.track)?;
            let heard = heard.with_macros();
            let found = view(control, &heard, app.song(), app.services.scale_here().on)?;
            control_line(control.name_on(track), &found)
        }
        Subject::Shape => return None,
    })
}

/// A setting of a page, by its view: its value in its units where it has them, as `632 Hz`, and
/// otherwise as its cell shows it.
pub(crate) fn control_line(name: &str, found: &ParamView) -> String {
    let value = match found.units.is_some() && !found.text.is_empty() {
        true => &found.text,
        false => &found.cell,
    };
    let chosen = chosen(found);
    description(name, value, &found.meaning, chosen)
}

/// What the choice a list is on does, where it says.
fn chosen(found: &ParamView) -> &str {
    usize::try_from(found.value)
        .ok()
        .and_then(|at| found.choices.get(at))
        .map_or("", |choice| &choice.meaning)
}

/// The shape panel's setting under its cursor, as the panel shows its value.
pub(crate) fn shape_line(panel: &ShapePanel) -> String {
    // In `Weights`' digits, the one under the cursor.
    if let Some((_, note, weight)) = panel.weight_here() {
        return format!(
            "Weight of {note}: {weight:X}, how often {note} is chosen, from 0, never, to F, the most."
        );
    }
    let context = panel.context();
    let row = panel.row;
    let described = row.described(&panel.shape, &context);
    let chosen = row
        .chosen(&panel.shape, &context)
        .and_then(|at| described.choices.get(at))
        .map_or("", |choice| &choice.meaning);
    description(
        row.name(),
        &row.shown(&panel.shape, &context),
        &described.meaning,
        chosen,
    )
}

/// A macro: what it moves, from where to where, and where it is now.
pub(crate) fn macro_line(song: &Song, index: usize, at: usize) -> String {
    let track = &song.tracks[index];
    let name = match track.macros[at].shown_name() {
        Some(named) => format!("Macro {} {named}", at + 1),
        None => format!("Macro {}", at + 1),
    };
    let now = track.macros[at].value >> 8;
    match moves(song, index, at) {
        Some(moves) => format!("{name}: moves {moves}, now at {now:02X}."),
        None => format!("{name}: holds nothing yet, now at {now:02X}."),
    }
}

/// Draw the line in its rows, the setting's name in bold before what it says.
pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }
    let Some(text) = text(app) else {
        return;
    };
    let line = match text.split_once(':') {
        Some((name, rest)) => Line::from(vec![
            Span::styled(
                format!("{name}:"),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::raw(rest.to_owned()),
        ]),
        None => Line::raw(text),
    };
    let area = Rect {
        x: area.x + 1,
        width: area.width.saturating_sub(2),
        ..area
    };
    frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: true }), area);
}
