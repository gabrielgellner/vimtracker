//! The macro panel at the bottom of any page (decision 385, #964): `Space t` shows or hides the
//! cursor's track's eight macros above the status line, `Space T` puts the keys in it, where `h`
//! and `l` go between them and `=` `-` `+` `_`, `i` and the wheel turn them as the Gen page's band
//! does, one undo step a press or a turn, and `Esc` gives the keys back to the page as it was.

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseEvent, MouseEventKind};
use vt_tui::app::App;
use vt_tui::controls::{Control, Page};
use vt_tui::input::mode::{Mode, View};

use crate::common::*;

/// The whole screen at the author's 163 by 42, as the program draws it.
fn whole_screen(app: &mut App) -> Buffer {
    let backend = ratatui::backend::TestBackend::new(163, 42);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, app, None, Instant::now()))
        .unwrap();
    terminal.backend().buffer().clone()
}

/// A row of the screen as text.
fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

/// The screen's rows as text.
fn rows(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height).map(|y| row(buffer, y)).collect()
}

fn macro_value(app: &App, track: usize, at: usize) -> u16 {
    app.song().tracks[track].macros[at].value
}

#[test]
fn space_t_shows_the_panel_above_the_status_line_and_hides_it_again() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    let before = whole_screen(&mut app);
    press(&mut app, " t");
    assert!(app.overlays.macro_panel.open);
    assert_eq!(app.modes.mode, Mode::Normal, "the keys stay on the page");
    let shown = rows(&whole_screen(&mut app));
    // Three rows above the two of the status line: its rule naming the track, the names, and
    // the values with their bars.
    assert!(shown[37].contains("Macros"), "{}", shown[37]);
    assert!(shown[37].contains("Track 01"), "{}", shown[37]);
    assert!(shown[38].contains("Macro 1"), "{}", shown[38]);
    assert!(shown[38].contains("Macro 8"), "{}", shown[38]);
    assert!(shown[39].contains("0000"), "{}", shown[39]);
    assert!(
        !rows(&before)[37].contains("Macros"),
        "the page had those rows"
    );
    press(&mut app, " t");
    assert!(!app.overlays.macro_panel.open);
    assert_eq!(
        whole_screen(&mut app),
        before,
        "hidden, the screen is as it was"
    );
}

#[test]
fn the_panel_fits_beside_every_page_without_breaking_it() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, " t");
    for keys in [" 1", " 2", " 3", " 4", " 5", " 6", " 7", " 8", " 9", " 0"] {
        press(&mut app, keys);
        let shown = rows(&whole_screen(&mut app));
        assert!(shown[37].contains("Macros"), "{keys}: {}", shown[37]);
        assert!(shown[38].contains("Macro 8"), "{keys}: {}", shown[38]);
        assert!(
            shown[40].contains(app.key_scope().label()),
            "{keys}: {}",
            shown[40]
        );
    }
}

#[test]
fn space_cap_t_turns_a_macro_and_esc_gives_the_page_back_as_it_was() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    // Somewhere in the phrase view, off its first step and column.
    press(&mut app, "jjl");
    let (cursor, view, top) = (app.services.cursor, app.modes.view, app.phrase_view.top);
    press(&mut app, " T");
    assert_eq!(app.modes.mode, Mode::Macros);
    assert!(app.overlays.macro_panel.open, "it shows as the keys go in");
    press(&mut app, "ll=");
    assert_eq!(macro_value(&app, 0, 2), 0x0100);
    press(&mut app, "+");
    assert_eq!(macro_value(&app, 0, 2), 0x1100);
    press(&mut app, "h-");
    assert_eq!(macro_value(&app, 0, 1), 0, "it stops at the bottom");
    press(&mut app, "l_");
    assert_eq!(macro_value(&app, 0, 2), 0x0100);
    // Two hex digits, kept as the second is typed, as on the Gen page.
    press(&mut app, "i8");
    assert!(app.overlays.macro_panel.typing.is_some());
    press(&mut app, "0");
    assert_eq!(macro_value(&app, 0, 2), 0x8000);
    press(&mut app, "x");
    assert_eq!(macro_value(&app, 0, 2), 0, "back to where it starts");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.modes.mode, Mode::Normal);
    assert_eq!(app.modes.view, view);
    assert_eq!(app.services.cursor, cursor, "the cursor where it was");
    assert_eq!(app.phrase_view.top, top);
    assert!(app.overlays.macro_panel.open, "Esc leaves it showing");
    // It comes back on the macro it was left on.
    press(&mut app, " T=");
    assert_eq!(macro_value(&app, 0, 2), 0x0100);
    // `Space t` in it hides it and gives the keys back.
    press(&mut app, " t");
    assert_eq!(app.modes.mode, Mode::Normal);
    assert!(!app.overlays.macro_panel.open);
}

#[test]
fn esc_gives_the_track_page_back_on_the_setting_it_was_on() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, " 5}jl");
    let (control, page) = (app.track_control(), app.track_page.page);
    press(&mut app, " T==");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.modes.view, View::Track);
    assert_eq!(app.track_page.page, page);
    assert_eq!(app.track_control(), control);
    assert_eq!(macro_value(&app, 0, 0), 0x0200);
}

#[test]
fn a_turn_in_the_panel_is_one_undo_step_as_on_the_macros_page() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, " T===");
    assert_eq!(macro_value(&app, 0, 0), 0x0300);
    // `u` undoes in the panel, a press at a time, and the keys stay in it.
    press(&mut app, "u");
    assert_eq!(macro_value(&app, 0, 0), 0x0200);
    assert_eq!(app.modes.mode, Mode::Macros);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "u");
    assert_eq!(macro_value(&app, 0, 0), 0x0100, "and on the page");

    // The same keys on the Gen page's band leave the same song and the same undo.
    let (mut band, _realtime) = tracks_of_different_lengths();
    press(&mut band, " 7Lgg0===u");
    assert_eq!(band.track_control(), Control::Macro(0));
    assert_eq!(macro_value(&band, 0, 0), 0x0200);
    let (mut panel, _realtime) = tracks_of_different_lengths();
    press(&mut panel, " T===u");
    assert_eq!(panel.song().tracks, band.song().tracks);
    assert_eq!(
        panel.services.history.undo_depth(),
        band.services.history.undo_depth()
    );
}

#[test]
fn a_turn_of_the_wheel_in_the_panel_is_one_undo_step() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    app.wheel.setting.on = true;
    press(&mut app, " T");
    let depth = app.services.history.undo_depth();
    let start = Instant::now();
    let report = MouseEvent {
        kind: MouseEventKind::ScrollRight,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    for tick in 0..5 {
        let at = start + Duration::from_millis(150 * tick);
        app.mouse_event(report, at);
        app.tick_at(at);
    }
    assert_eq!(macro_value(&app, 0, 0), 0x0500, "a step a slow tick");
    app.tick_at(start + Duration::from_secs(5));
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(macro_value(&app, 0, 0), 0);
}

#[test]
fn the_panel_follows_the_cursors_track() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, " t");
    press(&mut app, "W");
    assert_eq!(app.services.cursor.track, 1);
    let shown = rows(&whole_screen(&mut app));
    assert!(shown[37].contains("Track 02"), "{}", shown[37]);
    // In it, `W` and `B` go between tracks too, and a turn is the track's it's on.
    press(&mut app, " T=W=+");
    assert_eq!(macro_value(&app, 1, 0), 0x0100);
    assert_eq!(macro_value(&app, 2, 0), 0x1100);
    press(&mut app, "B");
    assert_eq!(app.services.cursor.track, 1);
    // The project page has no track of its own, so it shows the track last on.
    special(&mut app, KeyCode::Esc);
    press(&mut app, " 9");
    let shown = rows(&whole_screen(&mut app));
    assert!(shown[37].contains("Track 02"), "{}", shown[37]);
}

#[test]
fn the_status_line_names_the_panels_macro_and_what_it_holds() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    // Cut on macro 2.
    press(&mut app, " 5}jM2");
    press(&mut app, " Tl_");
    let status = status_line(&app);
    assert!(status.contains("MACROS"), "{status}");
    let said = "Macro 2, 94 %: holds Cut";
    assert!(status.contains(said), "{status}");
    // As the band says it on the Gen page.
    special(&mut app, KeyCode::Esc);
    press(&mut app, " 7Lgg0l");
    assert_eq!(app.track_control(), Control::Macro(1));
    let status = status_line(&app);
    assert!(status.contains(said), "{status}");
}

#[test]
fn a_macro_holding_nothing_is_dim_as_on_the_macros_page() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    // Cut on macro 2, and macro 1 holding nothing.
    press(&mut app, " 5}jM2 t");
    let screen = whole_screen(&mut app);
    let line = row(&screen, 39);
    let at = |text: &str, from: usize| line[from..].find(text).unwrap() + from;
    let first = at("0000", 0);
    let second = at("0000", first + 4);
    let colour = |x: usize| screen[(x as u16, 39)].fg;
    assert_ne!(colour(first), colour(second));
    assert_eq!(colour(first), vt_tui::widgets::theme::dim().fg.unwrap());
}

#[test]
fn gd_on_a_setting_a_macro_holds_still_goes_to_the_gen_page() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, " 5}jM1 t");
    press(&mut app, "gd");
    assert_eq!(app.modes.view, View::Track);
    assert_eq!(app.track_page.page, Page::Macros);
    assert_eq!(app.track_control(), Control::Macro(0));
    assert_eq!(app.modes.mode, Mode::Normal);
}

#[test]
fn the_panel_isnt_saved_in_the_song() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    let song = std::sync::Arc::clone(app.song());
    press(&mut app, " t");
    assert!(
        std::sync::Arc::ptr_eq(app.song(), &song),
        "the song isn't changed"
    );
    assert!(!app.services.dirty);
    press(&mut app, " T");
    special(&mut app, KeyCode::Esc);
    assert!(std::sync::Arc::ptr_eq(app.song(), &song));
    assert_eq!(app.services.history.undo_depth(), 0, "nor anything to undo");
}
