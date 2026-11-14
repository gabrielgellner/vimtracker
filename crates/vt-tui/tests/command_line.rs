//! The command line: commands, completion with `Tab`, the list of matches, and its history.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_tui::app::App;
use vt_tui::config::Settings;
use vt_tui::input::keys::Key;
use vt_tui::input::mode::Mode;

mod common;
use common::*;

#[test]
fn commands_change_tempo_and_length() {
    let (mut app, _rt) = app();
    command(&mut app, "bpm 137.63");
    assert_eq!(app.song().millibpm, 137_630);
    command(&mut app, "bpm 900");
    assert_eq!(app.song().millibpm, 137_630);
    assert!(!app.status.is_empty());
    for _ in 0..10 {
        press(&mut app, "j");
    }
    command(&mut app, "len 4");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 4);
    assert_eq!(notes(&app).len(), 16, "the rest is kept");
    assert_eq!(app.cursor.step, 10, "and the cursor can stay on it");
}

/// Press and let go of a key that isn't a character.
fn tap_special(app: &mut App, code: KeyCode) {
    app.key(Key::new(code), Instant::now());
    app.key_released(Key::new(code), Instant::now());
}

/// The names the command line's list of matches shows, and the highlighted one.
fn menu(app: &App) -> (Vec<String>, String) {
    let menu = app.command_menu.as_ref().expect("the list is open");
    let labels = menu
        .picker
        .items()
        .iter()
        .map(|item| item.label.clone())
        .collect();
    let highlighted = menu.picker.current().map(|item| item.label.clone());
    (labels, highlighted.unwrap_or_default())
}

#[test]
fn tab_completes_a_command_name_in_place() {
    let (mut app, _rt) = app();
    press(&mut app, ":sa");
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(app.command_line, "sample");
    assert!(app.command_menu.is_none(), "one match needs no list");
    special(&mut app, KeyCode::Esc);
    press(&mut app, ":set sc");
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(app.command_line, "set scale=");
    press(&mut app, "dor");
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(app.command_line, "set scale=dorian");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.status, "Scale C Dorian", "it parses and runs");
    assert!(app.scale.on);
}

#[test]
fn several_matches_list_above_the_command_line_and_tab_moves_through_them() {
    let (mut app, _rt) = app();
    press(&mut app, ":s");
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(app.command_line, "s", "nothing more in common");
    assert_eq!(
        menu(&app),
        (
            ["sample", "slice", "set", "scale", "swing", "solo", "seed"]
                .map(String::from)
                .to_vec(),
            "sample".to_owned()
        )
    );
    tap_special(&mut app, KeyCode::Tab);
    tap_special(&mut app, KeyCode::Tab);
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).1, "scale");
    tap_special(&mut app, KeyCode::BackTab);
    assert_eq!(menu(&app).1, "set");
    assert_eq!(app.command_line, "s", "moving puts nothing in");
    // Typing narrows it, keeping the highlight.
    tap(&mut app, "e");
    assert_eq!(
        menu(&app),
        (vec!["set".into(), "seed".into()], "set".into())
    );
    tap_special(&mut app, KeyCode::Tab);
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).1, "set", "it goes round");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.command_line, "set", "Enter takes it");
    assert!(app.command_menu.is_none());
    assert_eq!(app.mode, Mode::Command, "and the line stays open");
    // Esc closes the list and keeps what's typed.
    press(&mut app, " ");
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).0.len(), 4);
    tap(&mut app, "r");
    assert_eq!(menu(&app).0, ["root="]);
    special(&mut app, KeyCode::Esc);
    assert!(app.command_menu.is_none());
    assert_eq!(
        (app.mode, app.command_line.as_str()),
        (Mode::Command, "set r")
    );
    // Shift-Tab opens on the last, and a new word closes the list.
    special(&mut app, KeyCode::Esc);
    press(&mut app, ":track ");
    app.key(Key::new(KeyCode::BackTab), Instant::now());
    assert_eq!(menu(&app).1, "del");
    press(&mut app, "a");
    assert_eq!(menu(&app).0, ["add"]);
    tap(&mut app, "dd");
    press(&mut app, " ");
    assert!(app.command_menu.is_none());
    // Backspace widens it again, and a word nothing matches closes it.
    special(&mut app, KeyCode::Esc);
    press(&mut app, ":se");
    tap_special(&mut app, KeyCode::Tab);
    tap(&mut app, "x");
    assert!(app.command_menu.is_none());
}

#[test]
fn paths_and_kits_complete_from_the_disk() {
    let (mut app, _rt) = app();
    let root = folder();
    std::fs::create_dir_all(root.join("songs/verse")).unwrap();
    std::fs::create_dir_all(root.join("kits/Ableton")).unwrap();
    std::fs::write(root.join("kits/909.ron"), "").unwrap();
    std::fs::write(root.join("kits/808.ron"), "").unwrap();
    std::fs::write(root.join("kick.wav"), "").unwrap();
    press(&mut app, &format!(":e {}/so", root.display()));
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(app.command_line, format!("e {}/songs/", root.display()));
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(
        app.command_line,
        format!("e {}/songs/verse/", root.display())
    );
    special(&mut app, KeyCode::Esc);
    press(&mut app, &format!(":sample {}/k", root.display()));
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).0, ["kick.wav", "kits/"], "the list shows names");
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);
    app.settings = Settings {
        kits: vec![root.join("kits").display().to_string()],
        samples: Vec::new(),
        presets: String::new(),
    };
    press(&mut app, ":kit ");
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(menu(&app).0, ["808", "909", "Ableton/"]);
    press(&mut app, "9");
    special(&mut app, KeyCode::Esc);
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(app.command_line, "kit 909");
    press(&mut app, " a");
    tap_special(&mut app, KeyCode::Tab);
    assert_eq!(app.command_line, "kit 909 all");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn up_and_down_go_through_earlier_commands_and_search_with_what_is_typed() {
    let (mut app, _rt) = app();
    for line in ["bpm 130", "set skip=4", "bpm 140", "set skip=4"] {
        command(&mut app, line);
    }
    assert_eq!(
        app.command_history.lines(),
        ["bpm 130", "bpm 140", "set skip=4"],
        "a line run again moves to the end"
    );
    press(&mut app, ":");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(app.command_line, "set skip=4");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(app.command_line, "bpm 140");
    tap_special(&mut app, KeyCode::Down);
    tap_special(&mut app, KeyCode::Down);
    assert_eq!(app.command_line, "", "back to what was typed");
    press(&mut app, "b");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(app.command_line, "bpm 140");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(app.command_line, "bpm 130");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(app.command_line, "bpm 130", "nothing older starts with b");
    tap_special(&mut app, KeyCode::Down);
    assert_eq!(app.command_line, "bpm 140");
    // Editing a line found starts a new search from it.
    for _ in 0..3 {
        tap_special(&mut app, KeyCode::Backspace);
    }
    assert_eq!(app.command_line, "bpm ");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(app.command_line, "bpm 140");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().millibpm, 140_000);
    // A line that doesn't parse is kept too, to be put right, but Esc keeps nothing.
    command(&mut app, "bmp 120");
    press(&mut app, ":quit soon");
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.command_history.lines().last().map(String::as_str),
        Some("bmp 120")
    );
}
