//! How keys arrive: the transport keys, keys held and repeated, the leader, Cmd with the movement
//! keys, auditions and holds with `;`, and the keyboard played as notes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Transport;
use vt_engine::link::Realtime;
use vt_model::song::Song;
use vt_model::step::Note;
use vt_tui::app::App;
use vt_tui::input::keys::{Key, parse_sequence};
use vt_tui::input::mode::{Mode, View};

mod common;
use common::*;

#[test]
fn the_transport_keys_play_pause_and_restart() {
    let (mut app, mut rt) = app();
    press(&mut app, "jjj");
    let mut now = Instant::now();
    let mut hit = |app: &mut App, rt: &mut Realtime, key: Key| {
        now += Duration::from_millis(1000);
        app.key(key, now);
        app.key_released(key, now);
        block(rt, app);
    };
    let space_c = |app: &mut App| press(app, " c");
    let (tab, back_tab) = (Key::new(KeyCode::Tab), Key::new(KeyCode::BackTab));

    // Space c plays from the cursor, and restarts there.
    space_c(&mut app);
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Playing);
    assert_eq!(app.playhead, 3);

    // Tab pauses, and carries on from there even if the cursor moves.
    hit(&mut app, &mut rt, tab);
    assert_eq!(app.transport, Transport::Paused);
    press(&mut app, "jj");
    hit(&mut app, &mut rt, tab);
    assert_eq!(app.transport, Transport::Playing);
    assert_eq!(app.playhead, 3);

    // Shift-Tab plays from the start, playing or paused.
    hit(&mut app, &mut rt, back_tab);
    assert_eq!(app.transport, Transport::Playing);
    assert_eq!(app.playhead, 0);
    hit(&mut app, &mut rt, tab);
    assert_eq!(app.transport, Transport::Paused);
    hit(&mut app, &mut rt, back_tab);
    assert_eq!(app.transport, Transport::Playing);
    assert_eq!(app.playhead, 0);

    // Enter is only for confirming.
    hit(&mut app, &mut rt, Key::new(KeyCode::Enter));
    assert_eq!(app.transport, Transport::Playing);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn ctrl_i_jumps_forward_and_tab_still_plays() {
    let (mut app, mut rt) = app();
    let now = Instant::now();
    // The kitty protocol tells `Ctrl-I` from `Tab`, so jumping forward with nowhere to go plays
    // nothing.
    app.key(parse_sequence("C-i").unwrap()[0], now);
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Stopped);
    app.key(Key::new(KeyCode::Tab), now + Duration::from_millis(1000));
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Playing);
}

#[test]
fn enter_does_not_play_from_any_mode() {
    let (mut app, mut rt) = app();
    let enter = Key::new(KeyCode::Enter);
    app.key(enter, Instant::now());
    press(&mut app, "i");
    app.key(enter, Instant::now() + Duration::from_millis(1000));
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Stopped);
    assert_eq!(app.mode, Mode::Insert);
}

#[test]
fn a_held_transport_key_is_ignored_while_it_repeats() {
    let (mut app, mut rt) = app();
    let (tab, now) = (Key::new(KeyCode::Tab), Instant::now());
    app.key(tab, now);
    for _ in 0..20 {
        app.key_repeated(tab, now);
    }
    // Repeats sent as presses too, as kitty on macOS has sent some keys'.
    app.key(tab, now);
    block(&mut rt, &mut app);
    assert_eq!(
        app.transport,
        Transport::Playing,
        "only the first press counts"
    );
    // In Insert too, where Tab plays without leaving it.
    app.key_released(tab, Instant::now());
    press(&mut app, "i");
    app.key(tab, now);
    app.key_repeated(tab, now);
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Paused);
}

#[test]
fn quick_taps_of_tab_all_count() {
    let (mut app, mut rt) = app();
    let (tab, now) = (Key::new(KeyCode::Tab), Instant::now());
    let mut expected = Transport::Playing;
    for _ in 0..6 {
        app.key(tab, now);
        app.key_released(tab, Instant::now());
        block(&mut rt, &mut app);
        assert_eq!(app.transport, expected);
        expected = if expected == Transport::Playing {
            Transport::Paused
        } else {
            Transport::Playing
        };
    }
}

#[test]
fn shift_tab_let_go_after_shift_still_counts_as_let_go() {
    let (mut app, mut rt) = app();
    let now = Instant::now();
    // Shift-Tab, with Shift let go first, so the release is plain Tab's.
    app.key(Key::new(KeyCode::BackTab), now);
    app.key_released(Key::new(KeyCode::Tab), Instant::now());
    for _ in 0..100 {
        block(&mut rt, &mut app);
    }
    assert!(app.playhead > 0);
    app.key(Key::new(KeyCode::BackTab), now);
    block(&mut rt, &mut app);
    assert_eq!(app.playhead, 0, "the next Shift-Tab plays from the start");
}

#[test]
fn repeats_after_a_leader_transport_key_dont_reach_normal_keys() {
    let (mut app, mut rt) = app();
    let now = Instant::now();
    app.key(Key::char(' '), now);
    app.key(Key::char('c'), now);
    for _ in 0..10 {
        app.key_repeated(Key::char('c'), now);
    }
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Playing);
    assert!(app.pending().is_empty());
}

#[test]
fn a_held_motion_repeats() {
    let (mut app, _rt) = app();
    let (j, now) = (Key::char('j'), Instant::now());
    app.key(j, now);
    app.key_repeated(j, now);
    app.key_repeated(j, now);
    // A press of the key that's down is a repeat too, and still moves.
    app.key(j, now);
    assert_eq!(app.cursor.step, 4);
    app.key_released(j, Instant::now());
    app.key(j, now);
    assert_eq!(app.cursor.step, 5);
}

#[test]
fn keys_from_the_terminal_reach_the_app_as_their_characters() {
    use ratatui::crossterm::event::{KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode};
    let (mut app, _rt) = app();
    let now = Instant::now();
    let send = |app: &mut App, code: KeyCode, modifiers: KeyModifiers| {
        let key = Key::from_event(KeyEvent::new(code, modifiers));
        app.key_event(key, KeyEventKind::Press, now);
        app.key_event(key, KeyEventKind::Release, now);
    };
    // Shift alone comes as a key, and does nothing.
    send(
        &mut app,
        KeyCode::Modifier(ModifierKeyCode::LeftShift),
        KeyModifiers::SHIFT,
    );
    send(&mut app, KeyCode::CapsLock, KeyModifiers::NONE);
    assert!(app.status.is_empty(), "{}", app.status);
    // Shift-; as kitty's remote control sends it, with no shifted key.
    send(&mut app, KeyCode::Char(';'), KeyModifiers::SHIFT);
    assert_eq!(app.mode, Mode::Command);
    send(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.mode, Mode::Normal);
    // Shift-g goes to the last step, and Shift-4 to the last field.
    send(&mut app, KeyCode::Char('g'), KeyModifiers::SHIFT);
    assert_eq!(app.cursor.step, 15);
    send(&mut app, KeyCode::Char('4'), KeyModifiers::SHIFT);
    let last = app.cursor.field;
    send(&mut app, KeyCode::Char('0'), KeyModifiers::NONE);
    assert_ne!(app.cursor.field, last);
}

#[test]
fn the_leader_waits_and_an_unbound_key_cancels_it() {
    let (mut app, _rt) = app();
    press(&mut app, " ");
    assert_eq!(app.pending().len(), 1);
    press(&mut app, "z");
    assert!(app.pending().is_empty());
    assert!(app.status.contains("isn't bound"));
    press(&mut app, " ");
    special(&mut app, KeyCode::Esc);
    assert!(app.pending().is_empty());
}

#[test]
fn tab_plays_and_pauses_from_insert_and_keeps_writing() {
    let (mut app, mut rt) = app();
    press(&mut app, "i");
    let start = Instant::now();
    app.key(Key::new(KeyCode::Tab), start);
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Playing);
    assert_eq!(app.mode, Mode::Insert);
    press(&mut app, "a");
    assert_eq!(notes(&app)[0], Note::On(60));
    app.key(Key::new(KeyCode::Tab), start + Duration::from_millis(1000));
    app.key_released(Key::new(KeyCode::Tab), Instant::now());
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Paused);
    assert_eq!(app.mode, Mode::Insert);
    app.key(
        Key::new(KeyCode::BackTab),
        start + Duration::from_millis(2000),
    );
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Playing);
    assert_eq!(app.mode, Mode::Insert);
}

#[test]
fn semicolon_auditions_in_every_view_and_changes_nothing() {
    let (mut app, mut rt) = app();
    with_a_sound(&mut app);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let version = Arc::clone(app.song());
    // Hand over the edit, and let the note heard while typing finish.
    block(&mut rt, &mut app);
    let _ = loudest(&mut rt, 96_000);
    assert_eq!(loudest(&mut rt, 2000), 0.0);

    press(&mut app, "k");
    tap(&mut app, ";");
    assert!(
        loudest(&mut rt, 2000) > 0.01,
        "the phrase view auditions a row"
    );
    assert!(Arc::ptr_eq(&version, app.song()));
    assert_eq!(app.transport, Transport::Stopped);

    press(&mut app, "j");
    tap(&mut app, ";");
    let _ = loudest(&mut rt, 4800);
    assert_eq!(loudest(&mut rt, 2000), 0.0, "an empty row is silent");

    press(&mut app, " 2");
    tap(&mut app, ";");
    assert!(
        loudest(&mut rt, 2000) > 0.01,
        "the chain view auditions its row"
    );
}

#[test]
fn a_held_audition_stops_when_let_go_and_a_tapped_one_plays_out() {
    let (mut app, mut rt) = app();
    with_a_sound(&mut app);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    block(&mut rt, &mut app);
    let _ = loudest(&mut rt, 96_000);

    // The sample is a second long. Held, it stops when `;` is let go.
    let now = Instant::now();
    app.key(Key::char(';'), now);
    assert!(loudest(&mut rt, 4800) > 0.1);
    app.key_repeated(Key::char(';'), now + Duration::from_millis(200));
    app.key_released(Key::char(';'), now + Duration::from_millis(400));
    let _ = loudest(&mut rt, 4800);
    assert_eq!(loudest(&mut rt, 2000), 0.0, "held, it stops");

    // Tapped, it plays on.
    hold_for(&mut app, ';', 100);
    let _ = loudest(&mut rt, 4800);
    assert!(loudest(&mut rt, 2000) > 0.1, "tapped, it plays out");
    let _ = loudest(&mut rt, 96_000);

    // The chain view's `;` is held the same way.
    press(&mut app, " 2");
    let now = Instant::now();
    app.key(Key::char(';'), now);
    assert!(loudest(&mut rt, 4800) > 0.1);
    app.key_released(Key::char(';'), now + Duration::from_millis(400));
    let _ = loudest(&mut rt, 4800);
    assert_eq!(loudest(&mut rt, 2000), 0.0, "the chain view's stops too");
}

#[test]
fn a_tapped_audition_of_a_synth_that_holds_ends_when_let_go() {
    let (mut app, mut rt) = app();
    choose_machine(&mut app, "VA");
    press(&mut app, " 3ia");
    special(&mut app, KeyCode::Esc);
    block(&mut rt, &mut app);
    let _ = loudest(&mut rt, 96_000);
    let now = Instant::now();
    app.key(Key::char(';'), now);
    let _ = loudest(&mut rt, 48_000);
    assert!(
        loudest(&mut rt, 2400) > 0.01,
        "it holds while the key is down"
    );
    app.key_released(Key::char(';'), now + Duration::from_millis(100));
    let _ = loudest(&mut rt, 9600);
    assert!(loudest(&mut rt, 2400) < 1e-3, "and no longer drones on");

    // A tap shorter than a preview still sounds as long as one.
    let now = Instant::now();
    app.key(Key::char(';'), now);
    let _ = loudest(&mut rt, 480);
    app.key_released(Key::char(';'), now + Duration::from_millis(10));
    let _ = loudest(&mut rt, 4800);
    assert!(loudest(&mut rt, 2400) > 0.01, "a quick tap is still heard");
    let _ = loudest(&mut rt, 9600);
    assert!(loudest(&mut rt, 2400) < 1e-3);
}

#[test]
fn a_held_mute_or_solo_goes_back_when_let_go_and_a_tap_stays() {
    let (mut app, _rt) = app();
    press(&mut app, " ");
    let now = Instant::now();
    app.key(Key::char('m'), now);
    assert!(app.levels[0].mute, "it mutes at once");
    app.key_repeated(Key::char('m'), now + Duration::from_millis(500));
    assert!(app.levels[0].mute, "a repeat doesn't switch it back");
    app.key_released(Key::char('m'), now + Duration::from_millis(600));
    assert!(!app.levels[0].mute, "held, it goes back");

    press(&mut app, " ");
    hold_for(&mut app, 'm', 100);
    assert!(app.levels[0].mute, "tapped, it stays");
    // Held from muted, it unmutes until it's let go.
    press(&mut app, " ");
    hold_for(&mut app, 'm', 400);
    assert!(app.levels[0].mute);

    press(&mut app, " ");
    let now = Instant::now();
    app.key(Key::char('s'), now);
    assert!(app.levels[0].solo);
    app.key_released(Key::char('s'), now + Duration::from_millis(400));
    assert!(!app.levels[0].solo, "a held solo goes back too");
    press(&mut app, " ");
    hold_for(&mut app, 's', 100);
    assert!(app.levels[0].solo);
}

#[test]
fn a_save_while_a_mute_key_is_down_keeps_it_as_it_was() {
    let (mut app, _rt) = app();
    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    press(&mut app, " ");
    let now = Instant::now();
    app.key(Key::char('m'), now);
    command(&mut app, "w");
    let saved = vt_model::project::load_song(&path).unwrap();
    assert!(!saved.tracks[0].mute, "it may yet go back");
    assert!(!app.dirty);
    // Let go as a tap, it stays muted, and the song has changed since the save.
    app.key_released(Key::char('m'), now + Duration::from_millis(100));
    assert!(app.levels[0].mute);
    assert!(app.dirty);
    command(&mut app, "w");
    assert!(vt_model::project::load_song(&path).unwrap().tracks[0].mute);
    std::fs::remove_dir_all(&path).unwrap();
}

#[test]
fn losing_focus_lets_go_of_a_held_mute() {
    let (mut app, _rt) = app();
    press(&mut app, " ");
    let now = Instant::now();
    app.key(Key::char('m'), now);
    app.focus_lost(now + Duration::from_millis(400));
    assert!(!app.levels[0].mute, "held past the hold time, it goes back");
    // Its release, if it comes after all, does nothing more.
    app.key_released(Key::char('m'), now + Duration::from_millis(500));
    assert!(!app.levels[0].mute);
}

#[test]
fn semicolon_while_playing_says_why_it_did_nothing() {
    let (mut app, mut rt) = app();
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Playing);
    press(&mut app, ";");
    assert!(app.status.contains("stopped"), "{}", app.status);
}

#[test]
fn cmd_with_the_movement_keys_changes_notes_and_selections() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let Note::On(start) = written_notes(&app)[0] else {
        panic!("no note");
    };
    press(&mut app, "gg");
    with_cmd(&mut app, "D-l");
    with_cmd(&mut app, "D-k");
    assert_eq!(written_notes(&app), [Note::On(start + 13)]);
    press(&mut app, "V");
    with_cmd(&mut app, "D-h");
    with_cmd(&mut app, "D-j");
    assert_eq!(written_notes(&app), [Note::On(start)], "on a selection too");
}

#[test]
fn cmd_with_the_movement_keys_changes_the_mixer() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    with_cmd(&mut app, "D-l");
    assert_eq!(app.song().tracks[0].level, 0xE1);
    with_cmd(&mut app, "D-j");
    assert_eq!(app.song().tracks[0].level, 0xD1);
}

/// Each Cmd key does here what the key it stands for does (decision 148): the same change to the
/// song, the cursor left where that leaves it, and the mode the same. Each is undone after, so
/// the next starts from the same place, and at least one of them changes something.
#[track_caller]
fn cmd_keys_do_as_their_adjust_keys(app: &mut App, place: &str) {
    use vt_tui::input::bindings::CMD_ADJUSTS;
    let start = Song::clone(app.song());
    let mut changed = false;
    for (plain, cmd) in CMD_ADJUSTS {
        let where_it_was = (app.cursor, app.track_page.clone(), app.mode);
        press(app, plain);
        let by_plain = Song::clone(app.song());
        let left_at = (app.cursor, app.track_page.clone(), app.mode);
        changed |= by_plain != start;
        if by_plain != start {
            press(app, "u");
        }
        assert_eq!(**app.song(), start, "{plain} undone {place}");
        (app.cursor, app.track_page) = (where_it_was.0, where_it_was.1.clone());
        with_cmd(app, cmd);
        assert_eq!(**app.song(), by_plain, "{cmd} as {plain} {place}");
        assert_eq!(
            (app.cursor, app.track_page.clone(), app.mode),
            left_at,
            "{cmd} leaves the cursor as {plain} does {place}"
        );
        if by_plain != start {
            press(app, "u");
        }
        (app.cursor, app.track_page) = (where_it_was.0, where_it_was.1);
    }
    assert!(changed, "the keys change something {place}");
}

#[test]
fn cmd_with_the_movement_keys_does_as_the_adjust_keys_on_every_page() {
    // The phrase's grid, on a note, and on a selection, which stays.
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "gg");
    cmd_keys_do_as_their_adjust_keys(&mut app, "on a note");
    press(&mut app, "l");
    cmd_keys_do_as_their_adjust_keys(&mut app, "on a velocity");
    press(&mut app, "hV");
    cmd_keys_do_as_their_adjust_keys(&mut app, "on a selection");
    assert_eq!(app.mode, Mode::VisualStep);
    special(&mut app, KeyCode::Esc);

    // The chain's phrase and transpose columns, on a second row with a phrase of its own, so
    // there's another to go to.
    press(&mut app, " 2Ggd 2");
    assert_eq!(app.view, View::Chain);
    assert_eq!(app.cursor.slot, 1);
    cmd_keys_do_as_their_adjust_keys(&mut app, "on a chain's phrase");
    press(&mut app, "l");
    cmd_keys_do_as_their_adjust_keys(&mut app, "on a chain's transpose");

    // The song's length and a track's chain.
    press(&mut app, " 1");
    assert_eq!(app.view, View::Song);
    cmd_keys_do_as_their_adjust_keys(&mut app, "on a section's length");

    // The project page's tempo.
    press(&mut app, " 9");
    assert_eq!(app.view, View::Project);
    project_cursor_on(&mut app, vt_tui::project_page::ProjectControl::Bpm);
    cmd_keys_do_as_their_adjust_keys(&mut app, "on the tempo");
    project_cursor_on(&mut app, vt_tui::project_page::ProjectControl::Groove);
    cmd_keys_do_as_their_adjust_keys(&mut app, "on the groove");

    // The mixer's Trim, Level and Pan.
    press(&mut app, " 4");
    assert_eq!(app.view, View::Mixer);
    cmd_keys_do_as_their_adjust_keys(&mut app, "on Level");
    press(&mut app, "k");
    cmd_keys_do_as_their_adjust_keys(&mut app, "on Trim");
    press(&mut app, "jj");
    cmd_keys_do_as_their_adjust_keys(&mut app, "on Pan");

    // The track page's Sound and Mod pages.
    press(&mut app, " 5jjjjjjjj");
    assert_eq!(app.view, View::Track);
    cmd_keys_do_as_their_adjust_keys(&mut app, "on the Sound page");
    press(&mut app, "L0l");
    cmd_keys_do_as_their_adjust_keys(&mut app, "on the Mod page");
}

#[test]
fn cmd_with_the_movement_keys_does_as_the_adjust_keys_in_the_fx_rack() {
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "K");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    cmd_keys_do_as_their_adjust_keys(&mut app, "on a slot's effect");
    press(&mut app, "l");
    assert_eq!(vt_tui::views::status::column_name(&app), "Type");
    cmd_keys_do_as_their_adjust_keys(&mut app, "on the type in a slot's title");
    press(&mut app, "j");
    assert_eq!(vt_tui::views::status::column_name(&app), "Attack");
    cmd_keys_do_as_their_adjust_keys(&mut app, "in a device's grid");
}

#[test]
fn cmd_with_the_movement_keys_steps_through_the_audio_pages_choices() {
    let (mut app, _realtime) = on_the_audio_page();
    // On the buffer, playing at 256, so there's a choice either way.
    press(&mut app, "jj");
    app.audio.buffer = Some(256);
    // With no large step through a list, Cmd-k and Cmd-j step as Cmd-l and Cmd-h, as before.
    for (plain, cmds) in [("=", ["D-l", "D-k"]), ("-", ["D-h", "D-j"])] {
        press(&mut app, plain);
        let by_plain = app.audio_request.take();
        assert!(by_plain.is_some(), "{plain} chooses");
        for cmd in cmds {
            with_cmd(&mut app, cmd);
            assert_eq!(app.audio_request.take(), by_plain, "{cmd} as {plain}");
        }
    }
}

#[test]
fn every_mode_that_binds_the_adjust_keys_binds_their_cmd_keys() {
    use vt_tui::input::bindings::{CMD_ADJUSTS, Resolved, Table, bindings};
    let table = Table::default();
    for row in bindings() {
        if let Some((_, cmd)) = CMD_ADJUSTS.iter().find(|(plain, _)| *plain == row.keys) {
            let key = parse_sequence(cmd).unwrap();
            let means = table.resolve(row.mode, &key);
            // The sample page's Cmd-l and Cmd-h nudge a frame, finer than `=` and `-`'s pixel.
            if matches!(row.mode, Mode::Sample | Mode::SampleVisual)
                && matches!(*cmd, "D-l" | "D-h")
            {
                assert!(matches!(means, Resolved::Command(_)));
                continue;
            }
            assert_eq!(
                means,
                Resolved::Command(row.cmd),
                "{cmd} as {} in {:?}",
                row.keys,
                row.mode
            );
        }
    }
}

#[test]
fn a_held_key_on_the_keyboard_holds_its_note() {
    let (mut app, mut realtime) = app();
    choose_machine(&mut app, "VA");
    press(&mut app, ";");
    press(&mut app, "a");
    let _ = loudest(&mut realtime, 48_000);
    assert!(
        loudest(&mut realtime, 4800) > 0.01,
        "still sounding after a second"
    );
    // The terminal's repeats don't start it again, even sent as presses, as kitty sends `a`'s,
    // and another key's release doesn't end it.
    app.key_repeated(Key::char('a'), Instant::now());
    app.key_released(Key::char('s'), Instant::now());
    let _ = loudest(&mut realtime, 4800);
    assert!(loudest(&mut realtime, 4800) > 0.01);
    app.key_released(Key::char('a'), Instant::now());
    let _ = loudest(&mut realtime, 48_000);
    assert!(loudest(&mut realtime, 4800) < 1e-3, "letting go ends it");

    // Leaving the keyboard lets go too.
    press(&mut app, "a");
    let _ = loudest(&mut realtime, 4800);
    press(&mut app, ";");
    assert_eq!(app.mode, Mode::Normal);
    let _ = loudest(&mut realtime, 48_000);
    assert!(loudest(&mut realtime, 4800) < 1e-3);
}

#[test]
fn an_unbound_key_with_cmd_does_nothing_but_its_release_lets_go() {
    use ratatui::crossterm::event::KeyEventKind;
    let (mut app, mut realtime) = app();
    choose_machine(&mut app, "VA");
    let command_a = parse_sequence("D-a").unwrap()[0];
    let now = Instant::now();
    // `Cmd-a` isn't bound, so it isn't read as `a` and writes no note.
    press(&mut app, " 3i");
    app.key_event(command_a, KeyEventKind::Press, now);
    app.key_event(command_a, KeyEventKind::Release, now);
    assert_eq!(written_notes(&app), []);
    special(&mut app, KeyCode::Esc);

    // A note held on the keyboard ends when its key is let go after Cmd goes down.
    press(&mut app, " 5;a");
    let _ = loudest(&mut realtime, 4800);
    assert!(loudest(&mut realtime, 4800) > 0.01);
    app.key_event(command_a, KeyEventKind::Release, now);
    let _ = loudest(&mut realtime, 48_000);
    assert!(loudest(&mut realtime, 4800) < 1e-3, "letting go ends it");
}

#[test]
fn a_press_of_the_held_key_is_the_terminal_repeating_it() {
    // A sample that fades out over a second, so starting it again is loud.
    let (mut app, mut realtime) = app();
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fade.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for frame in 0..48_000 {
        writer.write_sample((16_000 - frame / 3) as i16).unwrap();
    }
    writer.finalize().unwrap();
    command(&mut app, &format!("sample {}", path.display()));
    press(&mut app, " 5;a");
    let _ = loudest(&mut realtime, 24_000);
    let held = loudest(&mut realtime, 2400);
    // kitty on macOS has sent a held `a`'s repeats as presses, with no release between.
    press(&mut app, "a");
    let after = loudest(&mut realtime, 2400);
    assert!(
        after < held,
        "{after} after the press, {held} before: it started again"
    );
    // After a release, a press starts it again.
    app.key_released(Key::char('a'), Instant::now());
    press(&mut app, "a");
    assert!(loudest(&mut realtime, 2400) > held * 1.5);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_held_note_key_in_insert_writes_once_and_holds_its_note() {
    let (mut app, mut realtime) = app();
    choose_machine(&mut app, "VA");
    press(&mut app, " 3");
    entering_steps_down(&mut app);
    press(&mut app, "ia");
    assert_eq!(written_notes(&app), [Note::On(60)]);
    assert_eq!(app.cursor.step, 1);
    let _ = loudest(&mut realtime, 48_000);
    assert!(
        loudest(&mut realtime, 4800) > 0.01,
        "still sounding after a second"
    );
    // Its repeats, and presses of it while it's down, write nothing more, and auto-jump moved on
    // once.
    app.key_repeated(Key::char('a'), Instant::now());
    press(&mut app, "a");
    app.key_released(Key::char('s'), Instant::now());
    assert_eq!(written_notes(&app), [Note::On(60)]);
    assert_eq!(app.cursor.step, 1);
    let _ = loudest(&mut realtime, 4800);
    assert!(loudest(&mut realtime, 4800) > 0.01);
    app.key_released(Key::char('a'), Instant::now());
    let _ = loudest(&mut realtime, 48_000);
    assert!(loudest(&mut realtime, 4800) < 1e-3, "letting go ends it");

    // After a release it writes again, and leaving Insert lets go too.
    press(&mut app, "a");
    assert_eq!(written_notes(&app), [Note::On(60), Note::On(60)]);
    let _ = loudest(&mut realtime, 4800);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    let _ = loudest(&mut realtime, 48_000);
    assert!(loudest(&mut realtime, 4800) < 1e-3);
}

#[test]
fn in_insert_off_and_scale_degrees_are_written_once_and_other_keys_repeat() {
    let (mut app, _realtime) = app();
    entering_steps_down(&mut app);
    press(&mut app, "i1");
    app.key_repeated(Key::char('1'), Instant::now());
    press(&mut app, "1");
    assert_eq!(written_notes(&app), [Note::Off], "OFF is written once");
    assert_eq!(app.cursor.step, 1);
    app.key_released(Key::char('1'), Instant::now());
    special(&mut app, KeyCode::Esc);

    // With the scale on, `j` is a degree, and is held as a note is.
    command(&mut app, "set scale=on");
    press(&mut app, "i");
    press(&mut app, "j");
    app.key_repeated(Key::char('j'), Instant::now());
    assert_eq!(written_notes(&app).len(), 2);
    assert_eq!(app.cursor.step, 2);
    app.key_released(Key::char('j'), Instant::now());

    // The arrows and Backspace repeat.
    let down = Key::new(KeyCode::Down);
    special(&mut app, KeyCode::Down);
    app.key_repeated(down, Instant::now());
    assert_eq!(app.cursor.step, 4);
    app.key_released(down, Instant::now());
    let backspace = Key::new(KeyCode::Backspace);
    special(&mut app, KeyCode::Backspace);
    app.key_repeated(backspace, Instant::now());
    app.key_repeated(backspace, Instant::now());
    assert_eq!(app.cursor.step, 1);
    assert_eq!(
        written_notes(&app),
        [Note::Off],
        "Backspace cleared on the way"
    );

    // So do hex digits, typed into the velocity.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "li");
    press(&mut app, "4");
    app.key_repeated(Key::char('4'), Instant::now());
    let id = app.phrase_id().unwrap();
    let velocity = app.song().tracks[0].phrases[&id].steps()[app.cursor.step - 1].velocity;
    assert_eq!(velocity, Some(0x44), "the repeat typed the second digit");
}

#[test]
fn losing_focus_lets_go_of_a_held_note() {
    let (mut app, mut realtime) = app();
    choose_machine(&mut app, "VA");
    // On the keyboard.
    press(&mut app, ";a");
    let _ = loudest(&mut realtime, 4800);
    app.focus_lost(Instant::now());
    let _ = loudest(&mut realtime, 48_000);
    assert!(
        loudest(&mut realtime, 4800) < 1e-3,
        "the keyboard's note ends"
    );
    // The release never came, but the next press of the key is a press.
    press(&mut app, "a");
    assert!(loudest(&mut realtime, 4800) > 0.01);
    app.key_released(Key::char('a'), Instant::now());
    press(&mut app, ";");

    // In Insert.
    press(&mut app, " 3ia");
    let _ = loudest(&mut realtime, 4800);
    app.focus_lost(Instant::now());
    assert_eq!(app.mode, Mode::Insert, "it stays in Insert");
    let _ = loudest(&mut realtime, 48_000);
    assert!(loudest(&mut realtime, 4800) < 1e-3, "Insert's note ends");
    press(&mut app, "s");
    assert_eq!(
        written_notes(&app),
        [Note::On(62)],
        "the next note is written"
    );
}

/// How many preview voices are sounding after a short block.
fn voices(realtime: &mut Realtime) -> usize {
    let _ = loudest(realtime, 480);
    realtime.engine().previews_sounding()
}

/// Let go of keys, and wait for what they held to finish its release.
fn let_go(app: &mut App, realtime: &mut Realtime, keys: &str) {
    for c in keys.chars() {
        app.key_released(Key::char(c), Instant::now());
    }
    let _ = loudest(realtime, 48_000);
}

#[test]
fn the_keyboard_plays_a_chord_and_each_key_lets_go_of_its_own_note() {
    let (mut app, mut realtime) = app();
    choose_machine(&mut app, "VA");
    press(&mut app, ";ad");
    assert_eq!(voices(&mut realtime), 2);
    press(&mut app, "f");
    assert_eq!(voices(&mut realtime), 3);
    // The terminal repeats the newest key, and kitty may send an older one's as presses, and
    // neither starts a note again.
    press(&mut app, "a");
    app.key_repeated(Key::char('f'), Instant::now());
    assert_eq!(voices(&mut realtime), 3);
    let_go(&mut app, &mut realtime, "d");
    assert_eq!(voices(&mut realtime), 2, "the other two still sound");
    assert!(loudest(&mut realtime, 4800) > 0.01);
    let_go(&mut app, &mut realtime, "af");
    assert_eq!(voices(&mut realtime), 0);

    // Leaving the keyboard lets go of every note.
    press(&mut app, "as");
    assert_eq!(voices(&mut realtime), 2);
    press(&mut app, ";");
    assert_eq!(app.mode, Mode::Normal);
    let _ = loudest(&mut realtime, 48_000);
    assert_eq!(voices(&mut realtime), 0);
}

#[test]
fn in_insert_every_note_key_writes_and_held_keys_sound_together() {
    let (mut app, mut realtime) = app();
    choose_machine(&mut app, "VA");
    press(&mut app, " 3");
    entering_steps_down(&mut app);
    // With auto-jump, keys rolled one into the next each write and move on, and sound together
    // while they're down.
    press(&mut app, "ia");
    press(&mut app, "dg");
    assert_eq!(
        written_notes(&app),
        [Note::On(60), Note::On(64), Note::On(67)]
    );
    assert_eq!(app.cursor.step, 3);
    assert_eq!(voices(&mut realtime), 3);
    // Their repeats, and presses of a key that's down, write nothing more.
    app.key_repeated(Key::char('g'), Instant::now());
    press(&mut app, "a");
    assert_eq!(written_notes(&app).len(), 3);
    assert_eq!(app.cursor.step, 3);
    assert_eq!(voices(&mut realtime), 3);
    let_go(&mut app, &mut realtime, "a");
    assert_eq!(voices(&mut realtime), 2, "the other two still sound");
    let_go(&mut app, &mut realtime, "dg");
    assert_eq!(voices(&mut realtime), 0);

    // Without it, the last key down wins the cell, and the chord is still heard.
    app.auto_jump = false;
    press(&mut app, "sf");
    assert_eq!(written_notes(&app)[3], Note::On(65));
    assert_eq!(app.cursor.step, 3);
    assert_eq!(voices(&mut realtime), 2);
    let_go(&mut app, &mut realtime, "sf");

    // Two keys holding the same note, an octave apart, hold it until both are let go.
    press(&mut app, "kxa");
    assert_eq!(written_notes(&app)[3], Note::On(72));
    assert_eq!(voices(&mut realtime), 1, "C-4 twice is one note");
    let_go(&mut app, &mut realtime, "k");
    assert_eq!(voices(&mut realtime), 1);
    let_go(&mut app, &mut realtime, "a");
    assert_eq!(voices(&mut realtime), 0);
    press(&mut app, "z");

    // Losing focus lets go of the whole chord, and leaving Insert does too.
    press(&mut app, "fd");
    assert_eq!(voices(&mut realtime), 2);
    app.focus_lost(Instant::now());
    let _ = loudest(&mut realtime, 48_000);
    assert_eq!(voices(&mut realtime), 0);
    press(&mut app, "dg");
    assert_eq!(written_notes(&app)[3], Note::On(67), "the keys write after");
    assert_eq!(voices(&mut realtime), 2);
    special(&mut app, KeyCode::Esc);
    let _ = loudest(&mut realtime, 48_000);
    assert_eq!(voices(&mut realtime), 0);
}

#[test]
fn the_transport_is_glyphs_with_the_waits_named() {
    use vt_tui::glyphs::{PLAY, STOP, WAITING};
    let (mut app, mut rt) = app();
    assert!(status_line(&app).contains(&format!(" {STOP}  120 BPM")));
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    assert!(status_line(&app).contains(&format!(" {PLAY}  120 BPM")));
    // A launch cued on any track waits for the grid too.
    app.jam[2].cued = Some(Some(1));
    assert!(status_line(&app).contains(&format!(" {PLAY} {WAITING} LAUNCH  ")));
}

#[test]
fn the_space_digits_show_the_mixer_each_track_page_and_the_project() {
    use vt_tui::track_page::Page;
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    assert_eq!(app.view, View::Mixer);
    for (digit, page) in [
        ('5', Page::Sound),
        ('6', Page::Mod),
        ('7', Page::Fx),
        ('8', Page::Gen),
    ] {
        press(&mut app, &format!(" {digit}"));
        assert_eq!(app.mode, Mode::Normal, "no list opens");
        assert_eq!((app.view, app.track_page.page), (View::Track, page));
    }
    // Each page keeps its cursor, and H and L still go between them.
    press(&mut app, " 5j");
    let sound = app.track_control();
    press(&mut app, " 6 5");
    assert_eq!(app.track_control(), sound);
    // C-o comes back from one page to another.
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!(app.track_page.page, Page::Mod);
    press(&mut app, "H");
    assert_eq!(app.track_page.page, Page::Sound);
    assert_eq!(app.track_control(), sound);
    press(&mut app, " 9");
    assert_eq!(app.view, View::Project);
    press(&mut app, " 0");
    assert_eq!(app.view, View::Sample);
}

#[test]
fn the_kit_and_sample_lists_open_from_their_commands() {
    let (mut app, _realtime) = app();
    command(&mut app, "sample");
    assert_eq!(app.mode, Mode::Picker);
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);
    command(&mut app, "kit");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().title, "Kits");
}
