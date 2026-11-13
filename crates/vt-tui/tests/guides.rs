//! The guides, `Space ?`, and the list of keys, `Space k`.

use ratatui::crossterm::event::KeyCode;
use vt_model::step::Note;
use vt_tui::input::mode::Mode;

mod common;
use common::*;

#[test]
fn space_question_opens_guides_and_space_k_lists_keys() {
    let (mut app, _realtime) = app();
    press(&mut app, " ?");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "note");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Guide);
    assert_eq!(app.guide.map(|g| g.title()), Some("Note entry"));
    press(&mut app, "q");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.guide, None);
    press(&mut app, " k");
    assert_eq!(app.mode, Mode::Help);
}

#[test]
fn a_guide_stops_scrolling_at_its_end() {
    let (mut app, _realtime) = app();
    press(&mut app, " ?");
    special(&mut app, KeyCode::Enter);
    app.guide_page = 10;
    let last = app.guide.unwrap().lines(app.guide_width).len() - 10;
    press(&mut app, "G");
    assert_eq!(app.guide_scroll, last);
    press(&mut app, "j");
    assert_eq!(app.guide_scroll, last);
    press(&mut app, "g");
    assert_eq!(app.guide_scroll, 0);
    press(&mut app, "?");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().map(|p| p.query.as_str()), Some(""));
}

/// What the note entry and navigation guides promise.
#[test]
fn the_guides_moves_do_what_they_say() {
    let (mut app, _realtime) = app();
    press(&mut app, "4j");
    assert_eq!(app.cursor.step, 4);
    press(&mut app, "}");
    assert_eq!(app.cursor.step, 8);
    press(&mut app, "gg");
    press(&mut app, "2}");
    assert_eq!(app.cursor.step, 8);
    press(&mut app, "gg");
    press(&mut app, "8G");
    assert_eq!(app.cursor.step, 7);
    press(&mut app, "gg");

    press(&mut app, "i");
    press(&mut app, "a");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "ji");
    assert_eq!(app.mode, Mode::Insert);
    assert_eq!(app.cursor.step, 1, "the note stayed put, and j went on one");
    special(&mut app, KeyCode::Esc);

    press(&mut app, "gg");
    press(&mut app, "i;");
    tap(&mut app, "aaaa");
    special(&mut app, KeyCode::Esc);
    let written: Vec<usize> = notes(&app)
        .iter()
        .enumerate()
        .filter(|(_, note)| matches!(note, Note::On(_)))
        .map(|(step, _)| step)
        .collect();
    assert_eq!(written, [0, 4, 8, 12]);

    press(&mut app, "i;");
    assert!(!app.auto_jump);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "gg");
    let before = app.cursor.step;
    press(&mut app, "ia");
    assert_eq!(app.cursor.step, before);
}
