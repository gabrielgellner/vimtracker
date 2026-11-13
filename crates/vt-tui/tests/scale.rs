//! The scale: entering and moving notes through it, turning it off and on, and fitting a phrase
//! to it.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_model::step::Note;
use vt_tui::app::App;
use vt_tui::input::keys::Key;
use vt_tui::input::mode::Mode;

mod common;
use common::*;

#[test]
fn with_a_scale_the_degrees_go_up_from_the_root_and_on_along_the_row_above() {
    let (mut app, _realtime) = app();
    command(&mut app, "set root=d");
    command(&mut app, "set scale=dorian");
    assert!(app.scale.on, "setting one turns it on");
    entering_steps_down(&mut app);
    tap(&mut app, "iasdfghjqwertyu");
    assert_eq!(
        written_notes(&app),
        [62, 64, 65, 67, 69, 71, 72, 74, 76, 77, 79, 81, 83, 84].map(Note::On)
    );
}

#[test]
fn a_scale_of_other_than_seven_notes_keeps_counting_along_the_rows() {
    let (mut app, _realtime) = app();
    command(&mut app, "set scale=minor-pentatonic");
    entering_steps_down(&mut app);
    tap(&mut app, "iasdfghjqwertyu");
    assert_eq!(
        written_notes(&app),
        [60, 63, 65, 67, 70, 72, 75, 77, 79, 82, 84, 87, 89, 91].map(Note::On),
        "the row above isn't the home row an octave up, but the degrees after it"
    );
}

#[test]
fn with_a_scale_the_other_keys_are_silent_and_note_off_and_octave_still_work() {
    let (mut app, _realtime) = app();
    command(&mut app, "set scale=major");
    entering_steps_down(&mut app);
    let start = app.cursor.step;
    press(&mut app, "iip;");
    press(&mut app, ";");
    assert!(written_notes(&app).is_empty());
    assert_eq!(app.cursor.step, start, "and it doesn't move");
    press(&mut app, "1");
    assert_eq!(written_notes(&app), [Note::Off]);
    press(&mut app, "xa");
    assert_eq!(written_notes(&app)[1], Note::On(72));
}

#[test]
fn replacing_a_note_folds_too() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "set root=d");
    press(&mut app, "ggrd");
    assert_eq!(
        written_notes(&app)[0],
        Note::On(66),
        "the third of D major is F#"
    );
}

#[test]
fn option_s_turns_the_scale_off_and_on_and_remembers_it() {
    let (mut app, _realtime) = app();
    entering_steps_down(&mut app);
    command(&mut app, "set root=d");
    command(&mut app, "set scale=dorian");
    press(&mut app, "i");
    alt(&mut app, "M-s");
    assert!(!app.scale.on);
    assert_eq!(app.mode, Mode::Insert, "in Insert, it stays");
    press(&mut app, "s");
    assert_eq!(written_notes(&app), [Note::On(62)], "the piano again");
    app.key_released(Key::char('s'), Instant::now());
    alt(&mut app, "M-s");
    assert_eq!(app.scale.root_name(), "D");
    assert_eq!(app.scale.kind.name(), "Dorian");
    press(&mut app, "s");
    assert_eq!(written_notes(&app)[1], Note::On(64));
    special(&mut app, KeyCode::Esc);
    alt(&mut app, "M-s");
    assert!(!app.scale.on);
    command(&mut app, "set scale=on");
    assert!(app.scale.on);
    command(&mut app, "set scale=off");
    assert!(!app.scale.on);
}

#[test]
fn the_scale_is_not_undone_but_is_saved() {
    let (mut app, _realtime) = app();
    let depth = app.history.undo_depth();
    command(&mut app, "set root=d");
    command(&mut app, "set scale=dorian");
    assert_eq!(app.history.undo_depth(), depth);
    assert!(app.dirty);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "u");
    assert!(app.scale.on, "undoing a note leaves it");
    assert_eq!(app.scale.root_name(), "D");

    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    let (mut other, _realtime) = self::app();
    command(&mut other, &format!("e {}", path.display()));
    assert_eq!(other.scale, app.scale, "{}", other.status);
}

#[test]
fn moving_a_note_goes_through_the_scale_except_chromatically_or_by_an_octave() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "gg");
    command(&mut app, "set scale=major");
    let note = |app: &App| written_notes(app)[0];
    press(&mut app, "=");
    assert_eq!(note(&app), Note::On(62));
    press(&mut app, "2=");
    assert_eq!(note(&app), Note::On(65), "D, E, F");
    press(&mut app, ".");
    assert_eq!(note(&app), Note::On(69), "the count again");
    press(&mut app, "-");
    assert_eq!(note(&app), Note::On(67));
    alt(&mut app, "C-a");
    assert_eq!(note(&app), Note::On(68), "G#, out of key");
    press(&mut app, "=");
    assert_eq!(note(&app), Note::On(69), "and on to the next in");
    alt(&mut app, "C-x");
    press(&mut app, "+");
    assert_eq!(note(&app), Note::On(80), "an octave up, still out of key");
    press(&mut app, "v>");
    assert_eq!(note(&app), Note::On(81), "the operator as well");
}

#[test]
fn with_the_scale_off_notes_move_chromatically() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "gg=");
    assert_eq!(written_notes(&app)[0], Note::On(61));
    alt(&mut app, "C-a");
    assert_eq!(written_notes(&app)[0], Note::On(62));
}

#[test]
fn scale_fit_moves_a_phrases_notes_and_can_be_undone() {
    let (mut app, _realtime) = app();
    entering_steps_down(&mut app);
    tap(&mut app, "iawe");
    special(&mut app, KeyCode::Esc);
    let before = written_notes(&app);
    assert_eq!(before, [60, 61, 63].map(Note::On));
    command(&mut app, "scale fit");
    assert_eq!(written_notes(&app), before, "the scale is off");
    assert!(app.status.contains("off"), "{}", app.status);
    command(&mut app, "set scale=major");
    assert_eq!(written_notes(&app), before, "setting a scale moves nothing");
    command(&mut app, "scale fit");
    assert_eq!(written_notes(&app), [60, 60, 62].map(Note::On));
    tap(&mut app, "u");
    assert_eq!(written_notes(&app), before);
}

#[test]
fn the_scale_list_sets_the_kind_and_turns_it_on() {
    let (mut app, _realtime) = app();
    command(&mut app, "scale");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "dorian");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.scale.kind.name(), "Dorian");
    assert!(app.scale.on);
}

#[test]
fn the_screen_shows_the_scale_and_notes_outside_it() {
    let draw = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(100, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                vt_tui::views::phrase::draw(frame, frame.area(), app);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let status = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(100, 2);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::status::draw(frame, frame.area(), app))
            .unwrap();
        (0..100)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_owned())
            .collect::<String>()
    };
    let (mut app, _realtime) = app();
    entering_steps_down(&mut app);
    tap(&mut app, "iaw");
    special(&mut app, KeyCode::Esc);
    // The cursor isn't on these.
    tap(&mut app, "G");
    let peach = |app: &App| {
        let buffer = draw(app);
        (0..24)
            .flat_map(|y| (0..100).map(move |x| (x, y)))
            .filter(|at| buffer[*at].fg == vt_tui::theme::PEACH)
            .count()
    };
    assert_eq!(peach(&app), 0, "off, nothing is marked");
    assert!(!status(&app).contains("Scale"));
    command(&mut app, "set root=c");
    assert!(status(&app).contains("Scale C Maj"), "{}", status(&app));
    assert_eq!(peach(&app), 3, "C# on its three letters, and C isn't");
    alt(&mut app, "M-s");
    assert_eq!(peach(&app), 0);
}
