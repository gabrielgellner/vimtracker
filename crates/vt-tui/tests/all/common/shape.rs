//! What the shape panel's and the Gen page's files share: the panel as drawn, the rhythm a
//! generator wrote, and the panel's and the page's rows.

use vt_model::shape::GenParam;
use vt_model::step::Note;
use vt_tui::app::App;
use vt_tui::controls::Control;

use super::keys;

/// The open shape panel drawn this wide and high, a line a row.
pub fn shape_panel(app: &App, width: u16, height: u16) -> String {
    let panel = app.overlays.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::overlays::shape::draw::draw(frame, frame.area(), &app.ctx(), &panel))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            let line: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A track's rows with a note, as `x`.
pub fn rhythm(app: &App, track: usize) -> String {
    app.song().tracks[track].phrases[&0]
        .steps()
        .iter()
        .map(|step| match step.note {
            Note::On(_) => 'x',
            _ => '.',
        })
        .collect()
}

/// Down the open panel to a setting.
pub fn down_to(app: &mut App, row: GenParam) {
    for _ in 0..GenParam::ALL.len() {
        if app.overlays.shape.as_ref().unwrap().row == row {
            return;
        }
        keys(app, "j");
    }
    panic!("no {row:?} row");
}

/// The setting the cursor is on, on the Gen page.
pub fn here(app: &App) -> Option<GenParam> {
    match app.track_control() {
        Control::Gen { param, .. } => Some(param),
        _ => None,
    }
}
