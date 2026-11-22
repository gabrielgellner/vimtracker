//! The command line: commands, completion with `Ctrl-n`, `Ctrl-p`, `Ctrl-y` and `Ctrl-e`, the list
//! of matches, and its history.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_tui::app::App;
use vt_tui::input::keys::Key;
use vt_tui::input::mode::Mode;
use vt_tui::services::config::Settings;

use crate::common::*;

#[test]
fn commands_change_tempo_and_length() {
    let (mut app, _rt) = app();
    command(&mut app, "bpm 137.63");
    assert_eq!(app.song().millibpm, 137_630);
    command(&mut app, "bpm 900");
    assert_eq!(app.song().millibpm, 137_630);
    assert!(!app.services.status.is_empty());
    for _ in 0..10 {
        press(&mut app, "j");
    }
    command(&mut app, "len 4");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 4);
    assert_eq!(notes(&app).len(), 16, "the rest is kept");
    assert_eq!(
        app.services.cursor.step, 10,
        "and the cursor can stay on it"
    );
}

/// Press and let go of a key that isn't a character.
fn tap_special(app: &mut App, code: KeyCode) {
    app.key(Key::new(code), Instant::now());
    app.key_released(Key::new(code), Instant::now());
}

/// The names the command line's list of matches shows, and the highlighted one.
fn menu(app: &App) -> (Vec<String>, String) {
    let menu = app
        .overlays
        .command
        .menu
        .as_ref()
        .expect("the list is open");
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
fn ctrl_n_completes_a_command_name_in_place() {
    let (mut app, _rt) = app();
    press(&mut app, ":sa");
    keys(&mut app, "^n");
    assert_eq!(app.overlays.command.line, "sample");
    assert!(
        app.overlays.command.menu.is_none(),
        "one match needs no list"
    );
    special(&mut app, KeyCode::Esc);
    press(&mut app, ":set sc");
    keys(&mut app, "^n");
    assert_eq!(app.overlays.command.line, "set scale=");
    press(&mut app, "dor");
    keys(&mut app, "^n");
    assert_eq!(app.overlays.command.line, "set scale=dorian");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.services.status, "Scale C Dorian", "it parses and runs");
    assert!(app.services.live.scale.on);
}

#[test]
fn several_matches_list_above_the_command_line_and_ctrl_n_moves_through_them() {
    let (mut app, _rt) = app();
    press(&mut app, ":s");
    keys(&mut app, "^n");
    assert_eq!(app.overlays.command.line, "s", "nothing more in common");
    assert_eq!(
        menu(&app),
        (
            ["sample", "slice", "set", "scale", "swing", "solo", "seed"]
                .map(String::from)
                .to_vec(),
            "sample".to_owned()
        )
    );
    keys(&mut app, "^n");
    keys(&mut app, "^n");
    keys(&mut app, "^n");
    assert_eq!(menu(&app).1, "scale");
    keys(&mut app, "^p");
    assert_eq!(menu(&app).1, "set");
    assert_eq!(app.overlays.command.line, "s", "moving puts nothing in");
    // Typing narrows it, keeping the highlight.
    tap(&mut app, "e");
    assert_eq!(
        menu(&app),
        (vec!["set".into(), "seed".into()], "set".into())
    );
    keys(&mut app, "^n");
    keys(&mut app, "^n");
    assert_eq!(menu(&app).1, "set", "it goes round");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.overlays.command.line, "set", "Enter takes it");
    assert!(app.overlays.command.menu.is_none());
    assert_eq!(app.modes.mode, Mode::Command, "and the line stays open");
    // Esc closes the list and keeps what's typed.
    press(&mut app, " ");
    keys(&mut app, "^n");
    assert_eq!(
        menu(&app).0.len(),
        6,
        "the settings, and readonly and readwrite"
    );
    tap(&mut app, "r");
    assert_eq!(menu(&app).0, ["root=", "readonly", "readwrite"]);
    special(&mut app, KeyCode::Esc);
    assert!(app.overlays.command.menu.is_none());
    assert_eq!(
        (app.modes.mode, app.overlays.command.line.as_str()),
        (Mode::Command, "set r")
    );
    // `Ctrl-p` opens on the last, and a new word closes the list.
    special(&mut app, KeyCode::Esc);
    press(&mut app, ":track ");
    keys(&mut app, "^p");
    assert_eq!(menu(&app).1, "del");
    press(&mut app, "a");
    assert_eq!(menu(&app).0, ["add"]);
    tap(&mut app, "dd");
    press(&mut app, " ");
    assert!(app.overlays.command.menu.is_none());
    // Backspace widens it again, and a word nothing matches closes it.
    special(&mut app, KeyCode::Esc);
    press(&mut app, ":se");
    keys(&mut app, "^n");
    tap(&mut app, "x");
    assert!(app.overlays.command.menu.is_none());
}

/// `Ctrl-y` takes the highlighted match and closes the list, or with none open completes as
/// `Ctrl-n` does; `Ctrl-e` closes the list and puts back what was typed (decision 477).
#[test]
fn ctrl_y_accepts_a_match_and_ctrl_e_puts_back_what_was_typed() {
    let (mut app, _rt) = app();
    press(&mut app, ":sa");
    keys(&mut app, "^y");
    assert_eq!(
        app.overlays.command.line, "sample",
        "one match goes in at once"
    );
    special(&mut app, KeyCode::Esc);
    press(&mut app, ":ne");
    keys(&mut app, "^y");
    assert_eq!(app.overlays.command.line, "new", "what the matches share");
    assert_eq!(
        menu(&app),
        (vec!["new".into(), "new!".into()], "new".into())
    );
    keys(&mut app, "^e");
    assert!(app.overlays.command.menu.is_none());
    assert_eq!(app.overlays.command.line, "ne", "what was typed is back");
    assert_eq!(app.modes.mode, Mode::Command);
    keys(&mut app, "^n^n");
    assert_eq!(menu(&app).1, "new!");
    keys(&mut app, "^y");
    assert!(app.overlays.command.menu.is_none());
    assert_eq!(
        (app.modes.mode, app.overlays.command.line.as_str()),
        (Mode::Command, "new!")
    );
    keys(&mut app, "^e");
    assert_eq!(
        app.overlays.command.line, "new!",
        "with no list, Ctrl-e does nothing"
    );
}

/// `Tab` and `Shift-Tab` play on the command line, as on every page, and complete nothing.
#[test]
fn tab_plays_on_the_command_line() {
    use vt_engine::engine::Transport;
    let (mut app, mut rt) = app();
    press(&mut app, ":s");
    tap_special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    assert_eq!(app.services.reported.transport, Transport::Playing);
    assert_eq!(
        (app.modes.mode, app.overlays.command.line.as_str()),
        (Mode::Command, "s")
    );
    assert!(app.overlays.command.menu.is_none(), "nothing is listed");
    tap_special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    assert_eq!(app.services.reported.transport, Transport::Paused);
    tap_special(&mut app, KeyCode::BackTab);
    block(&mut rt, &mut app);
    assert_eq!(app.services.reported.transport, Transport::Playing);
    assert_eq!(app.modes.mode, Mode::Command);
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
    keys(&mut app, "^n");
    assert_eq!(
        app.overlays.command.line,
        format!("e {}/songs/", root.display())
    );
    keys(&mut app, "^n");
    assert_eq!(
        app.overlays.command.line,
        format!("e {}/songs/verse/", root.display())
    );
    special(&mut app, KeyCode::Esc);
    press(&mut app, &format!(":sample {}/k", root.display()));
    keys(&mut app, "^n");
    assert_eq!(menu(&app).0, ["kick.wav", "kits/"], "the list shows names");
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);
    app.services.settings = Settings {
        kits: vec![root.join("kits").display().to_string()],
        samples: Vec::new(),
        presets: String::new(),
        describe: false,
    };
    press(&mut app, ":kit ");
    keys(&mut app, "^n");
    assert_eq!(menu(&app).0, ["808", "909", "Ableton/"]);
    press(&mut app, "9");
    special(&mut app, KeyCode::Esc);
    keys(&mut app, "^n");
    assert_eq!(app.overlays.command.line, "kit 909");
    press(&mut app, " a");
    keys(&mut app, "^n");
    assert_eq!(app.overlays.command.line, "kit 909 all");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn up_and_down_go_through_earlier_commands_and_search_with_what_is_typed() {
    let (mut app, _rt) = app();
    for line in ["bpm 130", "set skip=4", "bpm 140", "set skip=4"] {
        command(&mut app, line);
    }
    assert_eq!(
        app.overlays.command.history.lines(),
        ["bpm 130", "bpm 140", "set skip=4"],
        "a line run again moves to the end"
    );
    press(&mut app, ":");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(app.overlays.command.line, "set skip=4");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(app.overlays.command.line, "bpm 140");
    tap_special(&mut app, KeyCode::Down);
    tap_special(&mut app, KeyCode::Down);
    assert_eq!(app.overlays.command.line, "", "back to what was typed");
    press(&mut app, "b");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(app.overlays.command.line, "bpm 140");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(app.overlays.command.line, "bpm 130");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(
        app.overlays.command.line, "bpm 130",
        "nothing older starts with b"
    );
    tap_special(&mut app, KeyCode::Down);
    assert_eq!(app.overlays.command.line, "bpm 140");
    // Editing a line found starts a new search from it.
    for _ in 0..3 {
        tap_special(&mut app, KeyCode::Backspace);
    }
    assert_eq!(app.overlays.command.line, "bpm ");
    tap_special(&mut app, KeyCode::Up);
    assert_eq!(app.overlays.command.line, "bpm 140");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().millibpm, 140_000);
    // A line that doesn't parse is kept too, to be put right, but Esc keeps nothing.
    command(&mut app, "bmp 120");
    press(&mut app, ":quit soon");
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.overlays
            .command
            .history
            .lines()
            .last()
            .map(String::as_str),
        Some("bmp 120")
    );
}

#[test]
fn the_keys_of_the_command_line_are_in_the_binding_table() {
    let (app, _rt) = app();
    let help = app.help_rows();
    let at = help.iter().position(|line| line == "Command").unwrap();
    let rows: Vec<&String> = help[at + 1..]
        .iter()
        .take_while(|line| !line.is_empty())
        .collect();
    for row in ["Run the line", "another random name", "the next match"] {
        assert!(rows.iter().any(|line| line.contains(row)), "{row}");
    }
}

/// `Tab` plays in every mode, as decision 477 has it: in the Visual modes, after an operator,
/// and over a list, a guide or the keys, which keep their place, as well as on the pages.
#[test]
fn tab_plays_in_every_mode() {
    use vt_engine::engine::Transport;
    for (keys, mode) in [
        ("", Mode::Normal),
        ("v", Mode::VisualField),
        ("V", Mode::VisualStep),
        ("d", Mode::OperatorPending),
        (" ?", Mode::Picker),
        (" ?\r", Mode::Guide),
        (" k", Mode::Help),
        (" /", Mode::Picker),
        ("i", Mode::Insert),
    ] {
        let (mut app, mut rt) = app();
        for c in keys.chars() {
            if c == '\r' {
                tap_special(&mut app, KeyCode::Enter);
            } else {
                press(&mut app, &c.to_string());
            }
        }
        assert_eq!(app.modes.mode, mode, "after {keys:?}");
        tap_special(&mut app, KeyCode::Tab);
        block(&mut rt, &mut app);
        assert_eq!(
            app.services.reported.transport,
            Transport::Playing,
            "{mode:?}"
        );
        assert_eq!(app.modes.mode, mode, "{mode:?} stays");
        tap_special(&mut app, KeyCode::Tab);
        block(&mut rt, &mut app);
        assert_eq!(
            app.services.reported.transport,
            Transport::Paused,
            "{mode:?}"
        );
    }
}
