//! The view and track pickers.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_tui::controls::Page;
use vt_tui::input::keys::parse_sequence;
use vt_tui::input::mode::{Mode, View};
use vt_tui::overlays::picker::Pick;

use crate::common::*;

#[test]
fn the_track_picker_goes_to_a_track_by_number_or_name_like_a_buffer() {
    let (mut app, _realtime) = app();
    command(&mut app, "track add 2");
    press(&mut app, "W");
    command(&mut app, "name BD");
    press(&mut app, "W");
    press(&mut app, " fb");
    let picker = app.overlays.picker.as_ref().unwrap();
    assert_eq!(
        picker.current().map(|item| item.pick.clone()),
        Some(Pick::Track(2))
    );
    assert_eq!(picker.shown().count(), app.song().tracks.len());
    press(&mut app, "bd");
    assert_eq!(app.overlays.picker.as_ref().unwrap().shown().count(), 1);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.services.cursor.track, 1);
    assert_eq!(app.modes.view, View::Phrase);
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!(app.services.cursor.track, 2, "C-o comes back");

    press(&mut app, " fb04");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.services.cursor.track, 3);
    press(&mut app, " 1 fb");
    press(&mut app, "02");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.modes.view, View::Song);
    assert_eq!(app.services.cursor.track, 1);
    assert_eq!(vt_tui::screen::status::column_name(&app), "Track 02 chain");
}

#[test]
fn the_view_picker_flips_back_and_escape_changes_nothing() {
    let (mut app, _realtime) = app();
    press(&mut app, " 5");
    assert_eq!(app.modes.view, View::Track);
    press(&mut app, "  ");
    assert_eq!(app.modes.mode, Mode::Picker);
    // Keys that would edit in other modes only search here.
    press(&mut app, "x");
    assert_eq!(app.modes.view, View::Track);
    // Esc clears the search, then closes.
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.modes.mode, Mode::Picker);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.modes.mode, Mode::Normal);
    assert_eq!(app.modes.view, View::Track);
    assert!(app.overlays.picker.is_none());
    // The list starts on the view before this one.
    press(&mut app, " ff");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.modes.view, View::Phrase);
    press(&mut app, "  ");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.modes.view, View::Track);
}

#[test]
fn the_view_picker_searches_and_moves_with_ctrl_keys() {
    let (mut app, _realtime) = app();
    press(&mut app, "  fx");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.modes.view, View::Track);
    assert_eq!(app.track_page.page, Page::Fx);
    press(&mut app, "  zz");
    special(&mut app, KeyCode::Backspace);
    special(&mut app, KeyCode::Backspace);
    assert_eq!(
        app.overlays.picker.as_ref().unwrap().shown().count(),
        11,
        "every view, with each of the track page's pages"
    );
    app.key(parse_sequence("C-p").unwrap()[0], Instant::now());
    assert_eq!(app.overlays.picker.as_ref().unwrap().selected, 0);
    app.key(parse_sequence("C-n").unwrap()[0], Instant::now());
    assert_eq!(app.overlays.picker.as_ref().unwrap().selected, 1);
    special(&mut app, KeyCode::Esc);
}

#[test]
fn the_track_picker_names_a_synths_voice_and_can_find_it() {
    let (mut app, _realtime) = app();
    choose_machine(&mut app, "VA");
    let voice = app.song().tracks[0].machine.name();
    press(&mut app, " fb");
    let labels: Vec<String> = app
        .overlays
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(listed)
        .collect();
    assert!(labels[0].ends_with(voice), "{labels:?}");
    assert!(!labels[1].ends_with(voice), "{labels:?}");

    press(&mut app, &voice.to_lowercase());
    assert_eq!(
        app.overlays
            .picker
            .as_ref()
            .unwrap()
            .current()
            .map(|item| item.pick.clone()),
        Some(Pick::Track(0))
    );
}

#[test]
fn the_view_picker_lists_each_view_with_the_key_that_shows_it() {
    let (mut app, _realtime) = app();
    press(&mut app, " 6");
    press(&mut app, "  ");
    let listed: Vec<String> = app
        .overlays
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(listed)
        .collect();
    assert_eq!(
        listed,
        [
            "mod  Space 6",
            "phrase  Space 3",
            "chain  Space 2",
            "song  Space 1",
            "mixer  Space 4",
            "project  Space 9, H",
            "audio  Space 9, L",
            "sample  Space 0",
            "sound  Space 5",
            "fx  Space 7",
            "gen  Space 8",
        ]
    );
    // The view before this one is highlighted, so Enter flips back.
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.modes.view, View::Phrase);
    press(&mut app, "  ");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        (app.modes.view, app.track_page.page),
        (View::Track, Page::Mod)
    );
}

#[test]
fn the_pickers_keys_are_in_the_binding_table_so_which_key_and_help_list_them() {
    use std::time::Duration;
    use vt_tui::overlays::help::POPUP_DELAY;
    let (mut app, _realtime) = app();
    press(&mut app, "  ");
    assert_eq!(app.modes.mode, Mode::Picker);
    // The start of a sequence waits, and which-key says what can follow it.
    press(&mut app, "z");
    let later = Instant::now() + Duration::from_secs(1);
    let rows = app.popup_rows(later, POPUP_DELAY);
    let keys: Vec<&str> = rows.iter().map(|(keys, _)| keys.as_str()).collect();
    assert_eq!(keys, ["z", "t", "b"]);
    // `Esc` lets it go and leaves the picker open.
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.modes.mode, Mode::Picker);
    assert!(app.popup_rows(later, POPUP_DELAY).is_empty());
    // A `g` not followed by `g` is dropped, and the key after it means what it does alone.
    let before = app.overlays.picker.as_ref().unwrap().selected;
    press(&mut app, "gj");
    assert_eq!(app.overlays.picker.as_ref().unwrap().selected, before + 1);
    press(&mut app, "gg");
    assert_eq!(app.overlays.picker.as_ref().unwrap().selected, 0);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.modes.mode, Mode::Normal);
    assert!(app.pending().is_empty());
    // `Space k` lists them under the picker's mode.
    let help = app.help_rows();
    let at = help.iter().position(|row| row == "Picker").unwrap();
    assert!(help[at + 1..].iter().any(|row| row.contains("z z")));
}

#[test]
fn question_mark_in_a_picker_opens_the_keys_at_the_pickers_and_goes_back_to_it() {
    let (mut app, _realtime) = app();
    press(&mut app, "  ");
    press(&mut app, "3j");
    let selected = app.overlays.picker.as_ref().unwrap().selected;
    press(&mut app, "?");
    assert_eq!(app.modes.mode, Mode::Help);
    let at = app.help_rows().iter().position(|row| row == "Picker");
    assert_eq!(Some(app.overlays.help.scroll), at, "at the picker's keys");
    // Any key but `j` and `k` closes it, back to the picker as it was.
    press(&mut app, "q");
    assert_eq!(app.modes.mode, Mode::Picker);
    let picker = app.overlays.picker.as_ref().unwrap();
    assert_eq!((picker.selected, picker.query.as_str()), (selected, ""));
    // In the search, it's typed.
    press(&mut app, "/?");
    assert_eq!(app.modes.mode, Mode::Picker);
    assert_eq!(app.overlays.picker.as_ref().unwrap().query, "?");
    // `Space k` still closes to Normal.
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.modes.mode, Mode::Normal);
    press(&mut app, " k");
    assert_eq!(app.modes.mode, Mode::Help);
    press(&mut app, "q");
    assert_eq!(app.modes.mode, Mode::Normal);
}
