//! Gen beside the grid follows the grid's cursor (decision 468): the panel's cursor goes to the
//! device of the cursor's column, `Empty · Note` where it has none, and the status line names
//! it; while the keys are in the panel nothing follows, its device lights its rows in the grid
//! and its column's values show as `S`'s bars over the grid's other columns, what they were when
//! the keys came into the device fainter behind.

use std::sync::Arc;

use ratatui::buffer::Buffer;
use vt_engine::link::Realtime;
use vt_model::shape::ShapeColumn;
use vt_model::song::{Phrase, Song};
use vt_tui::app::App;
use vt_tui::controls::Control;
use vt_tui::input::mode::View;
use vt_tui::widgets::theme;

use crate::common::*;

/// One track of sixteen rows, a note on each, its velocities made live as a ramp, then Gen beside
/// the grid with the keys back in the grid on the note column.
fn followed() -> (App, Realtime) {
    followed_on(&["C-4 40 --- --- ---"; 16], "lvGSL")
}

/// One track of rows, the keys typed on it, then Gen beside the grid with the keys back in the
/// grid on the note column of row 00.
fn followed_on(rows: &[&str], typed: &str) -> (App, Realtime) {
    let mut song = Song::new(1);
    let steps = rows.iter().map(|row| row.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let (mut app, realtime) = app_for(song);
    keys(&mut app, typed);
    keys(&mut app, " 8^wh0gg");
    assert_eq!(app.modes.view, View::Phrase);
    assert!(app.gen_beside_shown());
    (app, realtime)
}

/// The setting the panel's cursor is on.
fn panel_on(app: &App) -> Control {
    app.gen_page.control(app.services.track_here())
}

/// The live generator the panel's cursor is on, by its place in the phrase's list.
fn panel_generator(app: &App) -> Option<usize> {
    match panel_on(app) {
        Control::Gen { generator, .. } => Some(generator),
        _ => None,
    }
}

/// The screen as drawn at 163x42, every cell with its style.
fn drawn(app: &mut App) -> Buffer {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(163, 42)).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, app, None, std::time::Instant::now()))
        .unwrap();
    terminal.backend().buffer().clone()
}

/// A line of a buffer as text.
fn line(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

/// The screen's line showing a row of the grid, by the row's number in the gutter.
fn grid_line(buffer: &Buffer, row: usize) -> u16 {
    let label = format!("  {row:02X}  │");
    (0..buffer.area.height)
        .find(|y| line(buffer, *y).contains(&label))
        .unwrap()
}

/// The panel follows the grid's cursor: on the note column, which has nothing live, `Empty`
/// naming it; on the velocity column, its ramp, the status line saying so.
#[test]
fn the_panel_follows_the_grids_cursor_column_by_column() {
    let (mut app, _rt) = followed();
    assert!(matches!(panel_on(&app), Control::NewGenerator(_)));
    let screen = screen_cells(&mut app).join("\n");
    assert!(screen.contains("Empty · Note"), "{screen}");
    assert!(
        screen.contains("K  make Note live over the phrase"),
        "{screen}"
    );
    assert!(
        screen.contains("Gen   Phrase 00 · on Note, row 00"),
        "{screen}"
    );
    keys(&mut app, "l");
    assert_eq!(panel_generator(&app), Some(0));
    assert_eq!(app.services.cursor.step, 0, "following isn't moving");
    assert_eq!(
        app.services.status,
        "Vel is live, rows 00–0F: Ctrl-w l turns it"
    );
    let screen = screen_cells(&mut app).join("\n");
    assert!(!screen.contains("Empty · "), "{screen}");
    assert!(screen.contains("· on Vel, row 00"), "{screen}");
    // Along the row to the nudge, which has nothing live, and back.
    keys(&mut app, "l");
    assert!(matches!(panel_on(&app), Control::NewGenerator(_)));
    assert!(
        app.services.status.contains("isn't live"),
        "{:?} {}",
        app.services.cursor.field,
        app.services.status
    );
    keys(&mut app, "h");
    assert_eq!(panel_generator(&app), Some(0));
}

/// Of two generators on a column, the one whose rows hold the cursor's row; the gate is Length's.
#[test]
fn the_generator_holding_the_row_and_the_gate_follows_length() {
    let (mut app, _rt) = followed_on(&["C-4 40 SGL4"; 16], "lvjjjjjjjSL8jvGSLlllvGSL");
    let generators = |app: &App| app.services.phrase_here().unwrap().generators().to_vec();
    let at = |column: fn(&ShapeColumn) -> bool, first: usize| {
        generators(&app)
            .iter()
            .position(|found| column(&found.column) && found.rows().unwrap().first == first)
    };
    let (top, bottom) = (
        at(|column| *column == ShapeColumn::Velocity, 0),
        at(|column| *column == ShapeColumn::Velocity, 8),
    );
    let length = at(|column| *column == ShapeColumn::Length, 8);
    assert!(
        top.is_some() && bottom.is_some() && length.is_some(),
        "{:?}",
        generators(&app)
            .iter()
            .map(|found| (found.column, found.rows()))
            .collect::<Vec<_>>()
    );
    keys(&mut app, "l");
    assert_eq!(panel_generator(&app), top);
    keys(&mut app, "10j");
    assert_eq!(panel_generator(&app), bottom, "the one holding row 0A");
    keys(&mut app, "ll");
    assert_eq!(app.services.cursor.field, vt_model::selection::Field::Gate);
    assert_eq!(panel_generator(&app), length, "the gate is Length's");
}

/// `K` on `Empty · Note` makes the note column live, and the panel is on its device.
#[test]
fn k_on_the_empty_naming_a_column_makes_it_live() {
    let (mut app, _rt) = followed();
    keys(&mut app, "^wlK");
    assert_eq!(app.modes.view, View::Gen);
    let generators = app.services.phrase_here().unwrap().generators().to_vec();
    let note = generators
        .iter()
        .position(|found| matches!(found.column, ShapeColumn::Note { .. }));
    assert!(note.is_some(), "Note is live");
    assert_eq!(panel_generator(&app), note);
}

/// While the keys are in the panel nothing follows, and the panel's device lights its rows in
/// the grid; back in the grid, it follows again and the rows aren't lit.
#[test]
fn in_the_panel_its_device_leads_and_lights_its_rows() {
    let (mut app, _rt) = followed();
    keys(&mut app, "l^wl");
    assert_eq!(panel_generator(&app), Some(0));
    let screen = drawn(&mut app);
    let y = grid_line(&screen, 3);
    // The velocity's cells, past the note's, are on the lit band.
    let row = line(&screen, y);
    let x = row[..row.find("C-4").unwrap()].chars().count() as u16 + 4;
    assert_eq!(screen[(x, y)].bg, theme::SURFACE0, "{row}");
    // To `Phrase`, which writes no column: nothing lit, no bars.
    keys(&mut app, "}");
    assert!(panel_generator(&app).is_none());
    keys(&mut app, "j");
    assert!(
        panel_generator(&app).is_none(),
        "nothing follows in the panel"
    );
    let screen = drawn(&mut app);
    assert_ne!(screen[(x, y)].bg, theme::SURFACE0);
    assert!(!line(&screen, 0).contains("VEL"), "{}", line(&screen, 0));
    // Back in the grid, the panel follows the cursor's column again.
    keys(&mut app, "<Esc>");
    assert_eq!(app.modes.view, View::Phrase);
    assert_eq!(panel_generator(&app), Some(0));
    keys(&mut app, "^wl");
    keys(&mut app, "<Esc>");
    let screen = drawn(&mut app);
    assert_ne!(screen[(x, y)].bg, theme::SURFACE0, "not lit from the grid");
}

/// While the keys are in a device, its column's values show as `S`'s bars over the grid's other
/// columns, what they were when the keys came in fainter behind, with the column's name and
/// range and how many rows changed; back in the grid they're gone.
#[test]
fn in_a_device_its_columns_values_are_bars_over_the_other_columns() {
    let (mut app, _rt) = followed();
    keys(&mut app, "l");
    let alone = screen_cells(&mut app);
    assert!(!alone[0].contains("VEL 00 ─ 7F"), "{}", alone[0]);
    keys(&mut app, "^wl");
    let lines = screen_cells(&mut app);
    assert!(lines[0].contains("VEL 00 ─ 7F"), "{}", lines[0]);
    assert!(lines[1].contains("0 changed"), "{}", lines[1]);
    // The last row's ramp is full, drawn over the FX columns beside the velocity.
    let screen = drawn(&mut app);
    let y = grid_line(&screen, 0x0F);
    assert!(
        line(&screen, y).contains("C-4 7F ██████████"),
        "{}",
        line(&screen, y)
    );
    // With `To` turned down, every row is lower than it was, what it was faint behind it.
    keys(&mut app, "jj____");
    let lines = screen_cells(&mut app);
    assert!(lines[1].contains("15 changed"), "{}", lines[1]);
    let screen = drawn(&mut app);
    let y = grid_line(&screen, 0x0F);
    let faint = (0..163).any(|x| screen[(x, y)].fg == theme::dimmed(theme::MAUVE));
    assert!(faint, "what row 0F was, faint: {}", line(&screen, y));
    // Back in the grid they're gone.
    keys(&mut app, "<Esc>");
    let lines = screen_cells(&mut app);
    assert!(!lines[0].contains("VEL 00 ─ 7F"), "{}", lines[0]);
}

/// A note column's bars are its contour, over the scale's roots.
#[test]
fn a_note_columns_bars_are_its_contour() {
    let (mut app, _rt) = followed_on(&["--- -- --- --- ---"; 8], "vGSjjjjjj===L");
    keys(&mut app, "^wl");
    let lines = screen_cells(&mut app);
    assert!(lines[0].contains("NOTE"), "{}", lines[0]);
    assert!(
        lines.iter().any(|line| line.contains('●')),
        "{}",
        lines.join("\n")
    );
}

/// A column whose values aren't amounts has no bars.
#[test]
fn a_column_whose_values_arent_amounts_has_none() {
    let (mut app, _rt) = followed_on(&["C-3 40 arp:37 --- ---"; 8], "llllllvjjjSKramp<Enter>L");
    keys(&mut app, "llllll");
    assert!(panel_generator(&app).is_some());
    keys(&mut app, "^wl");
    let lines = screen_cells(&mut app);
    assert!(
        lines[0].contains("Chain 00"),
        "the title as it was: {}",
        lines[0]
    );
}
