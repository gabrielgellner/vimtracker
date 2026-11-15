//! The guides, `Space ?`, and the list of keys, `Space k`.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_model::step::Note;
use vt_tui::input::mode::Mode;
use vt_tui::overlays::help::{POPUP_DELAY, draw_popup};

use crate::common::*;

#[test]
fn space_question_opens_guides_and_space_k_lists_keys() {
    let (mut app, _realtime) = app();
    press(&mut app, " ?");
    assert_eq!(app.modes.mode, Mode::Picker);
    press(&mut app, "note");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.modes.mode, Mode::Guide);
    assert_eq!(app.overlays.guide.map(|g| g.title()), Some("Note entry"));
    press(&mut app, "q");
    assert_eq!(app.modes.mode, Mode::Normal);
    assert_eq!(app.overlays.guide, None);
    press(&mut app, " k");
    assert_eq!(app.modes.mode, Mode::Help);
}

/// The guide under the list's cursor as `Space ?` opens it, from where the keys before it went.
fn guide_opened_from(keys: &str) -> String {
    let (mut app, _realtime) = app();
    press(&mut app, keys);
    press(&mut app, " ?");
    assert_eq!(app.modes.mode, Mode::Picker, "from {keys:?}");
    let picker = app.overlays.picker.as_ref().unwrap();
    picker.current().unwrap().label.clone()
}

#[test]
fn space_question_opens_on_the_guide_for_the_page_showing() {
    assert_eq!(guide_opened_from(""), "Navigation");
    assert_eq!(guide_opened_from(" 2"), "Phrases and chains");
    assert_eq!(guide_opened_from(" 1"), "Phrases and chains");
    assert_eq!(
        guide_opened_from(" 4"),
        "The Sound page, output and groups",
        "the mixer is in the Sound page's guide"
    );
    assert_eq!(guide_opened_from(" 5"), "The Sound page, output and groups");
    assert_eq!(guide_opened_from(" 6"), "The Mod page, envelopes and LFOs");
    assert_eq!(guide_opened_from(" 7"), "The FX page and its effects");
    assert!(guide_opened_from(" 8").starts_with("The Gen page"));
    assert_eq!(guide_opened_from(" 9"), "Saving and rendering");
    assert_eq!(
        guide_opened_from(" 9L"),
        "The meter",
        "the audio page is in its guide"
    );
    assert_eq!(guide_opened_from(" 0"), "The sample editor");
}

#[test]
fn question_in_a_guide_goes_back_to_the_list_on_that_guide() {
    let (mut app, _realtime) = app();
    press(&mut app, " ?gg");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.overlays.guide.map(|g| g.title()),
        Some("Your first loop")
    );
    press(&mut app, "?");
    let picker = app.overlays.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "Your first loop");
}

#[test]
fn a_guide_stops_scrolling_at_its_end() {
    let (mut app, _realtime) = app();
    press(&mut app, " ?");
    special(&mut app, KeyCode::Enter);
    app.overlays.guide_page = 10;
    let last = app
        .overlays
        .guide
        .unwrap()
        .lines(app.overlays.guide_width)
        .len()
        - 10;
    press(&mut app, "G");
    assert_eq!(app.overlays.guide_scroll, last);
    press(&mut app, "j");
    assert_eq!(app.overlays.guide_scroll, last);
    press(&mut app, "g");
    assert_eq!(app.overlays.guide_scroll, 0);
    press(&mut app, "?");
    assert_eq!(app.modes.mode, Mode::Picker);
    assert_eq!(
        app.overlays.picker.as_ref().map(|p| p.query.as_str()),
        Some("")
    );
}

/// What the note entry and navigation guides promise.
#[test]
fn the_guides_moves_do_what_they_say() {
    let (mut app, _realtime) = app();
    press(&mut app, "4j");
    assert_eq!(app.services.cursor.step, 4);
    press(&mut app, "}");
    assert_eq!(app.services.cursor.step, 8);
    press(&mut app, "gg");
    press(&mut app, "2}");
    assert_eq!(app.services.cursor.step, 8);
    press(&mut app, "gg");
    press(&mut app, "8G");
    assert_eq!(app.services.cursor.step, 7);
    press(&mut app, "gg");

    press(&mut app, "i");
    press(&mut app, "a");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "ji");
    assert_eq!(app.modes.mode, Mode::Insert);
    assert_eq!(
        app.services.cursor.step, 1,
        "the note stayed put, and j went on one"
    );
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
    assert!(!app.services.live.auto_jump);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "gg");
    let before = app.services.cursor.step;
    press(&mut app, "ia");
    assert_eq!(app.services.cursor.step, before);
}

#[test]
fn the_space_popup_is_wide_enough_for_every_description() {
    let (mut app, _realtime) = app();
    press(&mut app, " ");
    let later = Instant::now() + Duration::from_secs(1);
    let rows = app.popup_rows(later, POPUP_DELAY);
    assert!(rows.iter().any(|(keys, _)| keys == "i"), "{rows:?}");
    let (width, height) = (163, 42);
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| draw_popup(frame, frame.area(), &app, later))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let screen: Vec<String> = (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    for (keys, description) in rows {
        assert!(
            screen.iter().any(|line| line.contains(description)),
            "{keys} {description} is cut off:\n{}",
            screen.join("\n")
        );
    }
}
