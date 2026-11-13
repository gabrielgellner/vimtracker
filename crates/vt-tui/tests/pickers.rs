//! The view and track pickers.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_tui::input::keys::parse_sequence;
use vt_tui::input::mode::{Mode, View};
use vt_tui::input::picker::Pick;

mod common;
use common::*;

#[test]
fn the_track_picker_goes_to_a_track_by_number_or_name_like_a_buffer() {
    let (mut app, _realtime) = app();
    command(&mut app, "track add 2");
    press(&mut app, "W");
    command(&mut app, "name BD");
    press(&mut app, "W");
    press(&mut app, " fb");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(
        picker.current().map(|item| item.pick.clone()),
        Some(Pick::Track(2))
    );
    assert_eq!(picker.shown().count(), app.song().tracks.len());
    press(&mut app, "bd");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 1);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.cursor.track, 1);
    assert_eq!(app.view, View::Phrase);
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!(app.cursor.track, 2, "C-o comes back");

    press(&mut app, " fb04");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.cursor.track, 3);
    press(&mut app, " 1 fb");
    press(&mut app, "02");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::Song);
    assert_eq!(app.cursor.track, 1);
    assert_eq!(vt_tui::views::status::column_name(&app), "Track 02 chain");
}

#[test]
fn the_view_picker_flips_back_and_escape_changes_nothing() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    assert_eq!(app.view, View::Track);
    press(&mut app, "  ");
    assert_eq!(app.mode, Mode::Picker);
    // Keys that would edit in other modes only search here.
    press(&mut app, "x");
    assert_eq!(app.view, View::Track);
    // Esc clears the search, then closes.
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Picker);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.view, View::Track);
    assert!(app.picker.is_none());
    // The list starts on the view before this one.
    press(&mut app, " ff");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::Phrase);
    press(&mut app, "  ");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::Track);
}

#[test]
fn the_view_picker_searches_and_moves_with_ctrl_keys() {
    let (mut app, _realtime) = app();
    press(&mut app, "  tra");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::Track);
    press(&mut app, "  zz");
    special(&mut app, KeyCode::Backspace);
    special(&mut app, KeyCode::Backspace);
    assert_eq!(
        app.picker.as_ref().unwrap().shown().count(),
        8,
        "every view"
    );
    app.key(parse_sequence("C-p").unwrap()[0], Instant::now());
    assert_eq!(app.picker.as_ref().unwrap().selected, 0);
    app.key(parse_sequence("C-n").unwrap()[0], Instant::now());
    assert_eq!(app.picker.as_ref().unwrap().selected, 1);
    special(&mut app, KeyCode::Esc);
}

#[test]
fn the_track_picker_names_a_synths_voice_and_can_find_it() {
    let (mut app, _realtime) = app();
    choose_machine(&mut app, "VA");
    let voice = app.song().tracks[0].machine.name();
    press(&mut app, " fb");
    let labels: Vec<String> = app
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
        app.picker
            .as_ref()
            .unwrap()
            .current()
            .map(|item| item.pick.clone()),
        Some(Pick::Track(0))
    );
}
