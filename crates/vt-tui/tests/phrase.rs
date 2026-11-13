//! The phrase view's note entry: insert mode, the octave, the grid's moves, undo, and the
//! accent and slide keys held while notes go in.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Transport;
use vt_engine::link::Realtime;
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::input::keys::{Key, parse_sequence};
use vt_tui::input::mode::Mode;

mod common;
use common::*;

#[test]
fn insert_mode_writes_notes_where_the_cursor_is() {
    let (mut app, _rt) = app();
    tap(&mut app, "i");
    assert_eq!(app.mode, Mode::Insert);
    tap(&mut app, "a");
    assert_eq!(notes(&app)[0], Note::On(60));
    assert_eq!(app.cursor.step, 0, "it stays, to play another in");
    tap(&mut app, "k");
    assert_eq!(notes(&app)[0], Note::On(72));
    special(&mut app, KeyCode::Backspace);
    assert_eq!(notes(&app)[0], Note::Empty, "Backspace clears it there");
    assert_eq!(app.cursor.step, 0);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn semicolon_turns_auto_jump_on_and_entering_moves_on_by_the_grid() {
    let (mut app, _rt) = app();
    let depth = app.history.undo_depth();
    tap(&mut app, "i;");
    assert!(app.auto_jump);
    assert_eq!(app.history.undo_depth(), depth, "it isn't an edit to undo");
    assert!(app.dirty, "but it is saved");
    tap(&mut app, "ak1");
    let written = notes(&app);
    assert_eq!(written[0], Note::On(60));
    assert_eq!(written[4], Note::On(72));
    assert_eq!(written[8], Note::Off);
    assert_eq!(app.cursor.step, 12);
    // Back to the last entry, and clear it.
    special(&mut app, KeyCode::Backspace);
    assert_eq!(app.cursor.step, 8);
    assert_eq!(notes(&app)[8], Note::Empty);
    tap(&mut app, ";");
    assert!(!app.auto_jump);
    tap(&mut app, "a");
    assert_eq!(app.cursor.step, 8);

    // It's kept with the song.
    tap(&mut app, ";");
    special(&mut app, KeyCode::Esc);
    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    let (mut other, _realtime) = self::app();
    command(&mut other, &format!("e {}", path.display()));
    assert!(other.auto_jump, "{}", other.status);
    std::fs::remove_dir_all(&path).unwrap();
}

#[test]
fn octave_keys_change_entered_notes() {
    let (mut app, _rt) = app();
    entering_steps_down(&mut app);
    tap(&mut app, "izaxxa");
    let written = notes(&app);
    assert_eq!(written[0], Note::On(48));
    assert_eq!(written[1], Note::On(72));
}

#[test]
fn the_octave_is_numbered_as_live_numbers_it_from_c3_at_middle_c() {
    let (mut app, _rt) = app();
    entering_steps_down(&mut app);
    assert!(
        status_line(&app).contains("Oct 3 "),
        "{}",
        status_line(&app)
    );
    tap(&mut app, "ia");
    assert_eq!(notes(&app)[0], Note::On(60), "`a` is middle C");
    assert_eq!(Note::On(60).to_string(), "C-3");
    // Down to the lowest octave, which is -1, and no further.
    tap(&mut app, "zzzzzzzzza");
    assert_eq!(app.octave, -1);
    assert!(
        status_line(&app).contains("Oct -1 "),
        "{}",
        status_line(&app)
    );
    assert_eq!(notes(&app)[1], Note::On(12));
    assert_eq!(Note::On(12).to_string(), "c_1");
    // And up to the highest, 8, where the keys stop at `G-8`.
    tap(&mut app, "xxxxxxxxxxxxa");
    assert_eq!(app.octave, 8);
    assert_eq!(notes(&app)[2], Note::On(120));
    assert_eq!(Note::On(120).to_string(), "C-8");
}

#[test]
fn undo_and_redo_keys_restore_notes() {
    let (mut app, _rt) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    assert_eq!(notes(&app)[0], Note::On(60));
    press(&mut app, "u");
    assert_eq!(notes(&app)[0], Note::Empty);
    app.key(
        Key {
            ctrl: true,
            ..Key::char('r')
        },
        Instant::now(),
    );
    assert_eq!(notes(&app)[0], Note::On(60));
}

#[test]
fn the_cursor_stops_at_phrase_edges() {
    let (mut app, _rt) = app();
    press(&mut app, "k");
    assert_eq!(app.cursor.step, 0);
    for _ in 0..40 {
        press(&mut app, "j");
    }
    assert_eq!(app.cursor.step, 15);
}

#[test]
fn arrows_move_in_insert_without_writing() {
    let (mut app, _realtime) = app();
    press(&mut app, "i");
    special(&mut app, KeyCode::Down);
    assert_eq!(app.cursor.step, 1);
    special(&mut app, KeyCode::Right);
    assert_eq!(app.cursor.step, 5);
    special(&mut app, KeyCode::Right);
    assert_eq!(app.cursor.step, 9);
    special(&mut app, KeyCode::Up);
    assert_eq!(app.cursor.step, 8);
    special(&mut app, KeyCode::Left);
    assert_eq!(app.cursor.step, 4);
    special(&mut app, KeyCode::Left);
    assert_eq!(app.cursor.step, 0);
    special(&mut app, KeyCode::Left);
    assert_eq!(app.cursor.step, 0);
    assert_eq!(app.mode, Mode::Insert);
    assert!(notes(&app).iter().all(|note| *note == Note::Empty));
    assert!(!app.dirty);

    special(&mut app, KeyCode::Esc);
    command(&mut app, "set skip=3");
    press(&mut app, "i");
    special(&mut app, KeyCode::Right);
    assert_eq!(app.cursor.step, 3);
}

#[test]
fn an_arrow_drops_a_half_typed_value() {
    let (mut app, _realtime) = app();
    press(&mut app, "li8");
    special(&mut app, KeyCode::Down);
    assert_eq!(app.mode, Mode::Insert);
    assert_eq!(app.cursor.step, 1);
    assert!(app.entry.text.is_empty());
    press(&mut app, "40");
    let velocities: Vec<_> = app.song().tracks[0].phrases[&0]
        .steps()
        .iter()
        .map(|s| s.velocity)
        .collect();
    assert_eq!(velocities[0], None);
    assert_eq!(velocities[1], Some(0x40));
}

#[test]
fn the_ableton_layout_plays_and_c_and_v_set_the_velocity_written() {
    let (mut app, _realtime) = app();
    tap(&mut app, "ia");
    assert_eq!(notes(&app)[0], Note::On(60));
    tap(&mut app, "w");
    assert_eq!(notes(&app)[0], Note::On(61));
    tap(&mut app, "k");
    assert_eq!(notes(&app)[0], Note::On(72));
    tap(&mut app, "xl");
    assert_eq!(notes(&app)[0], Note::On(86), "up to `l`, as in Ableton");
    tap(&mut app, "zz");
    assert_eq!(app.octave, 2);

    let velocity = |app: &App| app.song().tracks[0].phrases[&0].steps()[0].velocity;
    tap(&mut app, "a");
    assert_eq!(velocity(&app), Some(0x64), "filled in from the default");
    tap(&mut app, "c");
    assert_eq!(app.note_velocity, 0x54, "only the top digit moves");
    tap(&mut app, "a");
    assert_eq!(velocity(&app), Some(0x54));
    assert_eq!(notes(&app)[0], Note::On(48));

    // The whole note is one undo step.
    special(&mut app, KeyCode::Esc);
    tap(&mut app, "u");
    assert_eq!(velocity(&app), Some(0x64));

    tap(&mut app, "i");
    tap(&mut app, "v");
    assert_eq!(app.note_velocity, 0x64, "back on the default");
    tap(&mut app, "v");
    assert_eq!(app.note_velocity, 0x74);
    tap(&mut app, "v");
    assert_eq!(app.note_velocity, 0x7F);
    tap(&mut app, "v");
    assert_eq!(app.note_velocity, 0x7F);
    tap(&mut app, "c");
    assert_eq!(app.note_velocity, 0x74, "steps back on from the top");
    tap(&mut app, "c");
    assert_eq!(app.note_velocity, 0x64);
    for _ in 0..12 {
        tap(&mut app, "c");
    }
    assert_eq!(app.note_velocity, 0x04);
}

#[test]
fn a_note_then_right_lands_on_the_next_beat() {
    let (mut app, _realtime) = app();
    tap(&mut app, "i");
    for _ in 0..4 {
        tap(&mut app, "a");
        special(&mut app, KeyCode::Right);
    }
    assert_eq!(app.cursor.step, 15);
    let written: Vec<usize> = notes(&app)
        .iter()
        .enumerate()
        .filter(|(_, note)| matches!(note, Note::On(_)))
        .map(|(step, _)| step)
        .collect();
    assert_eq!(written, [0, 4, 8, 12]);
}

fn written_steps(app: &App) -> Vec<usize> {
    notes(app)
        .iter()
        .enumerate()
        .filter(|(_, note)| matches!(note, Note::On(_)))
        .map(|(step, _)| step)
        .collect()
}

fn shifted(app: &mut App, code: KeyCode) {
    let key = Key {
        shift: true,
        ..Key::new(code)
    };
    app.key(key, Instant::now());
}

#[test]
fn right_keeps_a_notes_offset_within_the_beat() {
    let (mut app, _realtime) = app();
    tap(&mut app, "jji");
    for _ in 0..4 {
        tap(&mut app, "a");
        special(&mut app, KeyCode::Right);
    }
    assert_eq!(written_steps(&app), [2, 6, 10, 14]);
}

#[test]
fn left_and_right_move_by_the_grid_from_the_cursor() {
    let (mut app, _realtime) = app();
    command(&mut app, "set skip=8");
    tap(&mut app, "ia");
    special(&mut app, KeyCode::Right);
    tap(&mut app, "a");
    assert_eq!(written_steps(&app), [0, 8]);
    special(&mut app, KeyCode::Left);
    assert_eq!(app.cursor.step, 0);

    // With auto-jump the cursor is already on the next cell, and an arrow goes on from there.
    let (mut app, _realtime) = self::app();
    app.auto_jump = true;
    tap(&mut app, "8Gi");
    tap(&mut app, "a");
    assert_eq!(app.cursor.step, 11);
    special(&mut app, KeyCode::Right);
    assert_eq!(app.cursor.step, 15);
    special(&mut app, KeyCode::Left);
    special(&mut app, KeyCode::Left);
    assert_eq!(app.cursor.step, 7);
}

#[test]
fn shift_arrows_snap_to_the_grid() {
    let (mut app, _realtime) = app();
    press(&mut app, "jji");
    press(&mut app, "a");
    assert_eq!(app.cursor.step, 2);
    shifted(&mut app, KeyCode::Right);
    assert_eq!(app.cursor.step, 4);
    shifted(&mut app, KeyCode::Right);
    assert_eq!(app.cursor.step, 8);
    special(&mut app, KeyCode::Up);
    shifted(&mut app, KeyCode::Left);
    assert_eq!(app.cursor.step, 4);
    shifted(&mut app, KeyCode::Left);
    assert_eq!(app.cursor.step, 0);
}

#[test]
fn backspace_clears_a_note_with_what_it_was_written_with_but_keeps_fx() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Backspace);
    let first = &app.song().tracks[0].phrases[&0].steps()[0];
    assert_eq!(first.note, Note::Empty);
    assert_eq!(first.velocity, None);

    // With auto-jump, it goes back to the note first.
    let (mut app, _realtime) = self::app();
    entering_steps_down(&mut app);
    press(&mut app, "ia");
    assert_eq!(app.cursor.step, 1);
    special(&mut app, KeyCode::Backspace);
    assert_eq!(app.cursor.step, 0);
    let first = &app.song().tracks[0].phrases[&0].steps()[0];
    assert_eq!(first.note, Note::Empty);
    assert_eq!(first.velocity, None);
}

#[test]
fn the_note_heard_while_typing_uses_the_velocity() {
    let (mut app, mut rt) = app();
    with_a_sound(&mut app);
    block(&mut rt, &mut app);
    let heard = |app: &mut App, rt: &mut Realtime, keys: &str| {
        press(app, keys);
        let level = loudest(rt, 2000);
        for c in keys.chars() {
            app.key_released(Key::char(c), Instant::now());
        }
        let _ = loudest(rt, 96_000);
        level
    };
    let quiet = heard(&mut app, &mut rt, "icccccc");
    assert_eq!(quiet, 0.0, "changing velocity plays nothing");
    let soft = heard(&mut app, &mut rt, "a");
    tap(&mut app, "vvvvvvvv");
    let full = heard(&mut app, &mut rt, "a");
    assert!(soft * 2.0 < full, "{soft} vs {full}");
}

#[test]
fn length_changes_work_while_playing() {
    let (mut app, _realtime) = app();
    app.transport = Transport::Playing;
    command(&mut app, "len 4");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 4);
    command(&mut app, "double");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 8);
    press(&mut app, " pd");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 16);
    press(&mut app, "ggjjge");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 3);
    assert!(!app.status.contains("paused"), "{}", app.status);
    press(&mut app, "]L");
    assert_eq!(app.song().tracks[0].phrases[&0].start(), 4);
    assert_eq!(
        app.song().tracks[0].phrases[&0].len(),
        3,
        "slides keep the length"
    );
}

#[test]
fn a_loop_survives_saving_with_what_is_outside_it() {
    let folder = folder();
    let (mut app, _realtime) = app();
    command(&mut app, &format!("w {}", folder.display()));
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "len 4");
    press(&mut app, "]L");
    command(&mut app, "w");
    let saved = vt_model::project::load_song(&folder).unwrap();
    let phrase = &saved.tracks[0].phrases[&0];
    assert_eq!((phrase.start(), phrase.len()), (4, 4));
    assert_eq!(phrase.steps().len(), 16);
    assert_eq!(phrase.steps()[0].note, Note::On(60), "above the loop");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn reset_shows_on_the_status_line_and_undoes() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    let time = Instant::now();
    let status = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(100, 2);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::status::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..100)
            .map(|x| buffer[(x, 0)].symbol())
            .collect::<String>()
            .contains("Reset 64")
    };
    assert!(!status(&app));
    command(&mut app, "reset 64");
    assert_eq!(app.song().sections[0].reset, Some(64));
    assert!(status(&app));
    app.key(Key::char('u'), time);
    assert_eq!(app.song().sections[0].reset, None);
    command(&mut app, "reset 0");
    assert_eq!(app.song().sections[0].reset, None);
    assert!(app.status.contains(":reset"));
}

#[test]
fn option_keys_narrow_and_widen_the_grid_in_normal_and_insert() {
    let key = |name: &str| parse_sequence(name).unwrap()[0];
    let (mut app, _realtime) = app();
    assert_eq!(app.song().skip, 4);
    app.key(key("M-1"), Instant::now());
    assert_eq!(app.song().skip, 2);
    app.key(key("M-1"), Instant::now());
    app.key(key("M-1"), Instant::now());
    assert_eq!(app.song().skip, 1, "it stops at 1/16");
    press(&mut app, "j");
    assert_eq!(app.cursor.step, 1, "the 1 isn't a count");
    press(&mut app, "k");
    for _ in 0..6 {
        app.key(key("M-2"), Instant::now());
    }
    assert_eq!(app.song().skip, 16, "it stops at a bar");

    // A size between them goes to the next one along, either way.
    command(&mut app, "set skip=6");
    app.key(key("M-1"), Instant::now());
    assert_eq!(app.song().skip, 4);
    command(&mut app, "set skip=6");
    app.key(key("M-2"), Instant::now());
    assert_eq!(app.song().skip, 8);
    command(&mut app, "set skip=32");
    app.key(key("M-2"), Instant::now());
    assert_eq!(app.song().skip, 32, "past the sizes, it stays");
    app.key(key("M-1"), Instant::now());
    assert_eq!(app.song().skip, 16);

    // In Insert it writes nothing, and the arrows follow it from the note just written.
    command(&mut app, "set skip=4");
    press(&mut app, "i");
    app.key(key("M-1"), Instant::now());
    assert_eq!(app.song().skip, 2);
    assert_eq!(app.mode, Mode::Insert);
    assert_eq!(app.cursor.step, 0);
    let notes = |app: &App| {
        app.phrase_here()
            .unwrap()
            .steps()
            .iter()
            .filter(|step| step.note != Note::Empty)
            .count()
    };
    assert_eq!(notes(&app), 0, "M-1 isn't a note off");
    press(&mut app, "a");
    assert_eq!(notes(&app), 1);
    app.key(key("M-2"), Instant::now());
    assert_eq!(app.song().skip, 4);
    special(&mut app, KeyCode::Right);
    assert_eq!(app.cursor.step, 4);
}

#[test]
fn the_status_line_shows_the_grid_in_the_phrase_view() {
    let draw = |app: &App| {
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
    assert!(draw(&app).contains("Grid 1/4"), "{}", draw(&app));
    app.key(parse_sequence("M-1").unwrap()[0], Instant::now());
    assert!(draw(&app).contains("Grid 1/8"), "{}", draw(&app));
    press(&mut app, " 5");
    assert!(!draw(&app).contains("Grid"), "{}", draw(&app));
}

fn hold_at(app: &mut App, at: f64, c: char) {
    app.key(Key::char(c), time_at(at));
}

fn let_go_of_at(app: &mut App, at: f64, c: char) {
    app.key_released(Key::char(c), time_at(at));
}

/// The rows of the cursor's phrase.
fn rows_here(app: &App) -> Vec<Step> {
    app.phrase_here().unwrap().steps().to_vec()
}

/// Press a key and hold it, as a test can't from a script.
fn hold(app: &mut App, c: char) {
    app.key(Key::char(c), Instant::now());
}

fn let_go_of(app: &mut App, c: char) {
    app.key_released(Key::char(c), Instant::now());
}

#[test]
fn a_tapped_accent_goes_on_the_note_just_written_and_off_again() {
    let (mut app, _rt) = app();
    command(&mut app, "set skip=4");
    app.auto_jump = true;
    tap(&mut app, "ia");
    assert_eq!(app.cursor.step, 4, "auto-jump moved on");
    let depth = app.history.undo_depth();
    tap(&mut app, "'");
    let rows = rows_here(&app);
    assert_eq!(rows[0].velocity, Some(0x7F), "the row just written");
    assert_eq!(rows[4].velocity, None, "not the cursor's");
    assert_eq!(app.status, "Accent on");
    assert_eq!(app.history.undo_depth(), depth + 1, "one change to undo");
    assert_eq!(app.cursor.step, 4, "the cursor stays");

    // Again takes it off, back to the velocity notes are written with.
    tap(&mut app, "'");
    assert_eq!(rows_here(&app)[0].velocity, Some(0x64));
    assert_eq!(app.status, "Accent off");
    tap(&mut app, "v");
    assert_eq!(app.note_velocity, 0x74);
    tap(&mut app, "'");
    tap(&mut app, "'");
    assert_eq!(
        rows_here(&app)[0].velocity,
        Some(0x64),
        "a written velocity that's an accent itself leaves a new song's"
    );
    tap(&mut app, "c");

    // An empty velocity plays at full, which is an accent, so a tap takes it off.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "ggl");
    tap(&mut app, "x");
    press(&mut app, "h");
    assert_eq!(rows_here(&app)[0].velocity, None);
    tap(&mut app, "i'");
    assert_eq!(rows_here(&app)[0].velocity, Some(0x64));
    assert_eq!(app.status, "Accent off");
    // A row with no note has nothing to accent.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "j");
    tap(&mut app, "i'");
    assert_eq!(app.status, "No note here to accent");
    assert_eq!(rows_here(&app)[1], Step::default());

    // Undo takes each back as a note is.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "u");
    assert_eq!(rows_here(&app)[0].velocity, None);
}

#[test]
fn with_the_cursor_moved_off_a_tapped_flag_goes_on_the_cursors_row() {
    let (mut app, _rt) = app();
    entering_steps_down(&mut app);
    tap(&mut app, "iasd");
    assert_eq!(app.cursor.step, 3);
    special(&mut app, KeyCode::Up);
    tap(&mut app, "'");
    let rows = rows_here(&app);
    assert_eq!(rows[2].velocity, Some(0x7F), "the arrow moved onto it");
    assert_eq!(rows[1].velocity, Some(0x64));
    // Leaving Insert and coming back forgets the note just written.
    tap(&mut app, "f");
    assert_eq!(app.cursor.step, 3);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    tap(&mut app, "i");
    tap(&mut app, "/");
    assert_eq!(
        rows_here(&app)[2].gate_kind(),
        vt_model::step::GateKind::Hold
    );
    assert_eq!(rows_here(&app)[3].gate, None);
}

#[test]
fn a_tapped_slide_puts_hld_on_the_gate_and_keeps_its_pulses_and_length() {
    use vt_model::step::{Gate, GateKind};
    let (mut app, _rt) = app();
    command(&mut app, "set skip=4");
    app.auto_jump = true;
    tap(&mut app, "ia");
    tap(&mut app, "/");
    let hold = Gate {
        kind: GateKind::Hold,
        pulses: 1,
        length: None,
    };
    assert_eq!(rows_here(&app)[0].gate, Some(hold));
    assert_eq!(app.status, "Slide on");
    tap(&mut app, "/");
    assert_eq!(rows_here(&app)[0].gate, None, "back to the empty cell");
    assert_eq!(app.status, "Slide off");

    // A cell with pulses and a length of its own keeps them, so the row lasts as long, and is a
    // `SGL` with them again after.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "gg");
    let three = Step {
        note: Note::On(60),
        velocity: Some(0x64),
        gate: Some(Gate {
            kind: GateKind::Single,
            pulses: 3,
            length: Some(0x80),
        }),
        ..Step::default()
    };
    let phrase = app.phrase_id().unwrap();
    app.apply(&vt_model::action::Action::SetStep {
        track: 0,
        phrase,
        step: 0,
        value: three.clone(),
    });
    tap(&mut app, "i/");
    let slid = rows_here(&app)[0].gate.unwrap();
    assert_eq!(
        (slid.kind, slid.pulses, slid.length),
        (GateKind::Hold, 3, Some(0x80))
    );
    tap(&mut app, "/");
    assert_eq!(rows_here(&app)[0], three);
}

#[test]
fn notes_played_with_a_flag_held_are_written_with_it_and_letting_go_switches_nothing() {
    use vt_model::step::GateKind;
    let (mut app, _rt) = app();
    entering_steps_down(&mut app);
    tap(&mut app, "ia");
    // Hold the accent, and roll three notes under it.
    hold(&mut app, '\'');
    tap(&mut app, "sd");
    // kitty's repeats of the held key, as presses or repeats, do nothing.
    hold(&mut app, '\'');
    app.key_repeated(Key::char('\''), Instant::now());
    tap(&mut app, "f");
    let_go_of(&mut app, '\'');
    let rows = rows_here(&app);
    let velocities: Vec<_> = rows[..4].iter().map(|step| step.velocity).collect();
    assert_eq!(
        velocities,
        [Some(0x64), Some(0x7F), Some(0x7F), Some(0x7F)],
        "the note before is left alone"
    );
    assert_eq!(app.status, "Accent on");

    // Both held, a note gets both, on any track: here an `Acid`.
    special(&mut app, KeyCode::Esc);
    choose_machine(&mut app, "Acid");
    press(&mut app, " 3");
    tap(&mut app, "i");
    hold(&mut app, '/');
    hold(&mut app, '\'');
    hold(&mut app, 'g');
    let row = rows_here(&app)[4].clone();
    assert_eq!(row.note, Note::On(67));
    assert_eq!(row.velocity, Some(0x7F));
    assert_eq!(row.gate_kind(), GateKind::Hold);
    assert_eq!(app.status, "Accent and slide on");
    let_go_of(&mut app, 'g');
    let_go_of(&mut app, '\'');
    let_go_of(&mut app, '/');
    assert_eq!(rows_here(&app)[4], row, "letting go switches nothing");

    // Leaving Insert with one held lets go of it too, and its release does nothing.
    hold(&mut app, '/');
    special(&mut app, KeyCode::Esc);
    let before = rows_here(&app);
    let_go_of(&mut app, '/');
    assert_eq!(rows_here(&app), before);
    // So does losing focus.
    tap(&mut app, "i");
    hold(&mut app, '/');
    app.focus_lost(Instant::now());
    let_go_of(&mut app, '/');
    assert_eq!(rows_here(&app), before);
}

#[test]
fn a_note_recorded_with_a_flag_held_is_written_with_it() {
    use vt_model::step::GateKind;
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    play_to(&mut app, &mut rt, &mut at, 9.0);
    hold_at(&mut app, at, '\'');
    hold_at(&mut app, at, '/');
    hold_at(&mut app, at, 'a');
    play_to(&mut app, &mut rt, &mut at, 9.5);
    let_go_of_at(&mut app, at, 'a');
    let_go_of_at(&mut app, at, '/');
    let_go_of_at(&mut app, at, '\'');
    let rows = rows_of(&app, 1);
    assert_eq!(rows[1].note, Note::On(60), "{rows:?}");
    assert_eq!(rows[1].velocity, Some(0x7F));
    assert_eq!(rows[1].gate_kind(), GateKind::Hold);
    // A tap has no row to go on yet.
    tap_at(&mut app, at, "'");
    assert_eq!(app.status, "While recording, hold ' as you play a note");
    assert_eq!(rows_of(&app, 1), rows);
}

#[test]
fn space_k_lists_the_accent_and_slide_keys() {
    let (app, _rt) = app();
    let rows = app.help_rows();
    let insert = rows.iter().position(|row| row == "Insert").unwrap();
    assert!(rows[insert + 1].trim_start().starts_with("'"), "{rows:?}");
    assert!(rows[insert + 2].trim_start().starts_with("/"), "{rows:?}");
}
