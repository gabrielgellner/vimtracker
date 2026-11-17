//! The line that describes the setting under the cursor (decision 387, #969): `Space d` shows it in
//! two rows above the macro panel, on the Track page's pages, in the shape panel and in the macro
//! panel, saying what the setting does and what its value means now, and follows the cursor.
//! Hidden, or on a page with nothing to describe, the screen is as it was.

use std::time::Instant;

use ratatui::buffer::Buffer;
use vt_model::macros::{MacroMap, MacroTarget, Zone};
use vt_model::settings::SettingId;
use vt_model::song::Song;
use vt_tui::app::App;
use vt_tui::controls::Control;
use vt_tui::input::mode::Mode;
use vt_tui::screen::describe;

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

/// The track page's cursor onto a setting of the page on show.
fn go_to(app: &mut App, control: Control) {
    let track = app.song().tracks[app.services.cursor.track].clone();
    let rows = app.track_page.rows(Some(&track));
    let (row, field) = rows
        .iter()
        .enumerate()
        .find_map(|(row, each)| {
            each.iter()
                .position(|found| *found == Some(control))
                .map(|field| (row, field))
        })
        .unwrap_or_else(|| panic!("{control:?} isn't on the page"));
    app.track_page.move_to(Some(&track), row, field);
    assert_eq!(app.track_control(), control);
}

fn line(app: &App) -> String {
    describe::text(app).expect("something to describe")
}

#[test]
fn space_d_shows_the_line_above_the_status_line_and_hides_it_again() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, " 5");
    let before = whole_screen(&mut app);
    press(&mut app, " d");
    assert!(app.overlays.describing);
    let shown = whole_screen(&mut app);
    // The status line keeps its two rows at the bottom, and the line takes the two above them.
    let said = row(&shown, 38) + &row(&shown, 39);
    assert!(said.contains("Machine:"), "{said}");
    assert!(said.contains("what makes the track's sound"), "{said}");
    press(&mut app, " d");
    assert!(!app.overlays.describing);
    assert_eq!(whole_screen(&mut app), before, "hidden, it's as it was");
}

#[test]
fn it_sits_above_the_macro_panel_when_both_show() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, " 5 t d");
    let shown = whole_screen(&mut app);
    assert!(row(&shown, 35).contains("Machine:"), "{}", row(&shown, 35));
    assert!(row(&shown, 37).contains("Macros"), "{}", row(&shown, 37));
}

#[test]
fn off_or_with_nothing_to_describe_nothing_changes() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    for keys in [" 1", " 2", " 3", " 4", " 5", " 6", " 7", " 8", " 9", " 0"] {
        press(&mut app, keys);
        // What the page said as it opened goes with the next key.
        app.services.status.clear();
        let off = whole_screen(&mut app);
        press(&mut app, " d");
        let on = whole_screen(&mut app);
        press(&mut app, " d");
        assert_eq!(whole_screen(&mut app), off, "{keys}: off again");
        // Only the Track page's pages have a setting under the cursor.
        if !matches!(keys, " 5" | " 6" | " 7" | " 8") {
            assert_eq!(on, off, "{keys}: nothing to describe");
        }
    }
}

#[test]
fn it_follows_the_cursor_on_the_pages() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, " 5 d");
    // A list: what the setting does, then what its choice does.
    go_to(&mut app, Control::FilterType);
    assert_eq!(
        line(&app),
        "Filter: which filter the sound goes through. LP: Low-pass, clean, 12 or 24 dB."
    );
    // A number, in its units.
    go_to(
        &mut app,
        Control::Base(vt_model::action::TrackControl::Cutoff),
    );
    assert_eq!(
        line(&app),
        "Cut 20.0 kHz: where the filter starts to take away the sound."
    );
}

#[test]
fn it_says_a_generator_setting_in_the_shape_panel() {
    let (mut app, _realtime) = app_with(&["--- -- --- --- ---"; 8]);
    keys(&mut app, " dvGS");
    assert_eq!(app.modes.mode, Mode::Shape);
    assert_eq!(
        line(&app),
        "Fill: what goes in the rows, which decides the other settings. Ramp: From one value to the other."
    );
    keys(&mut app, "j");
    assert_eq!(
        line(&app),
        "From C-3: one end of what the fill writes, where a ramp starts."
    );
    keys(&mut app, "jjjjj");
    assert_eq!(line(&app), "Where All rows: which rows the fill changes.");
    keys(&mut app, "=");
    assert_eq!(line(&app), "Where Every: which rows the fill changes.");
    // `Chance`, whose amount reads inside the words.
    keys(&mut app, "=j");
    assert_eq!(line(&app), "Amount: each row has a 50% chance of changing.");
    let shown = whole_screen(&mut app);
    let said = row(&shown, 38) + &row(&shown, 39);
    assert!(said.contains(&line(&app)[..20]), "{said}");
}

#[test]
fn it_says_what_a_macro_moves_in_the_macro_panel() {
    let mut song = Song::new(2);
    let held = &mut song.tracks[0].macros[1];
    held.value = 0x4080;
    held.name = Some("Bright".to_owned());
    held.maps.push(MacroMap {
        target: MacroTarget::Setting(SettingId::CUT),
        min: 0x1000,
        max: 0xF000,
        curve: 0,
        s: 0,
        zone: Default::default(),
    });
    held.maps.push(MacroMap {
        target: MacroTarget::Setting(SettingId::RES),
        min: 0x2000,
        max: 0x6000,
        curve: 0,
        s: 0,
        // Over the right half alone, which the line says (decision 392).
        zone: Zone::new(0x80, 0xFF).unwrap(),
    });
    let (mut app, _realtime) = app_for(song);
    press(&mut app, " T d");
    assert_eq!(app.modes.mode, Mode::Macros);
    assert_eq!(line(&app), "Macro 1: holds nothing yet, now at 00.");
    press(&mut app, "l");
    assert_eq!(
        line(&app),
        "Macro 2 Bright: moves Cut 31 Hz to 13.3 kHz and Res 13 % to 38 % over 80–FF, now at 40."
    );
}
