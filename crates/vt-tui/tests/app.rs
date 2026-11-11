use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;
use vt_engine::engine::{Engine, Transport};
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::constants::{FIXED_TRACKS, MAX_GROUPS};
use vt_model::groove::GrooveId;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::samples::SamplePool;
use vt_model::settings::{SettingId, gate_value};
use vt_model::song::{
    Chain, ChainEntry, FilterType, Launch, Machine, Phrase, SectionStart, Song, SynthVoice,
    TrackKind,
};
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::config::Settings;
use vt_tui::input::keys::{Key, parse_sequence};
use vt_tui::input::mode::{Mode, View};
use vt_tui::input::picker::Pick;

fn app() -> (App, Realtime) {
    let song = Song::new(2);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

fn press(app: &mut App, keys: &str) {
    let now = Instant::now();
    for c in keys.chars() {
        app.key(Key::char(c), now);
    }
}

/// Go to a track that's shown with `W` or `B`, as many times as there are shown tracks between,
/// as they step over the folded ones.
fn to_track(app: &mut App, track: usize) {
    let shown: Vec<usize> = app.folds.shown(app.song()).collect();
    let place = |track: usize| shown.iter().position(|&shown| shown == track).unwrap();
    let (from, to) = (place(app.cursor.track), place(track));
    let key = if to > from { "W" } else { "B" };
    press(app, &key.repeat(from.abs_diff(to)));
    assert_eq!(app.cursor.track, track);
}

/// Press and let go of each key in turn, as typing does, so a key typed twice isn't taken as held.
fn tap(app: &mut App, keys: &str) {
    let now = Instant::now();
    for c in keys.chars() {
        app.key(Key::char(c), now);
        app.key_released(Key::char(c), Instant::now());
    }
}

/// An item as a list shows it: its name, and its meaning beside it.
fn listed(item: &vt_tui::input::picker::Item) -> String {
    if item.meaning.is_empty() {
        item.label.clone()
    } else {
        format!("{}  {}", item.label, item.meaning)
    }
}

/// The explorer's rows as they're shown: each indented by its depth, a folder with its arrow, and
/// a folder at the top by why it's there.
fn explorer_rows(app: &App) -> Vec<String> {
    use vt_tui::input::picker::Row;
    let picker = app.picker.as_ref().expect("the explorer is open");
    picker
        .rows()
        .iter()
        .map(|row| {
            let Row::Item(item) = row else {
                return String::new();
            };
            let arrow = match picker.folder_open(*item) {
                Some(true) => "v ",
                Some(false) => "> ",
                None => "  ",
            };
            let depth = picker.depth(*item);
            let shown = picker.item(*item);
            let label = if depth == 0 && !shown.meaning.is_empty() {
                &shown.meaning
            } else {
                &shown.label
            };
            format!("{}{arrow}{label}", "  ".repeat(depth))
        })
        .collect()
}

/// The folders at the top of the explorer: each one's name and why it's there.
fn explorer_roots(app: &App) -> Vec<(String, String)> {
    let picker = app.picker.as_ref().expect("the explorer is open");
    picker
        .items()
        .iter()
        .enumerate()
        .filter(|(at, _)| picker.depth(*at) == 0)
        .map(|(_, item)| (item.label.clone(), item.meaning.clone()))
        .collect()
}

fn special(app: &mut App, code: KeyCode) {
    app.key(Key::new(code), Instant::now());
}

fn command(app: &mut App, line: &str) {
    press(app, ":");
    press(app, line);
    special(app, KeyCode::Enter);
}

fn folder() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "vt-tui-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn notes(app: &App) -> Vec<Note> {
    let id = app.phrase_id().unwrap();
    app.song().tracks[app.cursor.track].phrases[&id]
        .steps()
        .iter()
        .map(|s| s.note)
        .collect()
}

/// Run the audio side for a block.
fn block(realtime: &mut Realtime, app: &mut App) {
    let mut out = vec![0.0; 512];
    realtime.process(&mut out);
    app.tick();
}

/// Turn auto-jump on with a grid of one row, so entries go down the rows one by one.
fn entering_steps_down(app: &mut App) {
    command(app, "set skip=1");
    app.auto_jump = true;
}

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
fn quit_refuses_unsaved_changes() {
    let (mut app, _rt) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, " qq");
    assert!(!app.should_quit);
    assert!(app.status.contains("Unsaved"));
    command(&mut app, "q!");
    assert!(app.should_quit);
}

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

#[test]
fn saving_needs_a_folder_then_round_trips() {
    let (mut app, _rt) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "w");
    assert!(app.dirty);
    assert_eq!(app.mode, Mode::Picker, "the explorer chooses where");
    assert_eq!(app.picker.as_ref().unwrap().title, "Save in");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.naming, None, "nothing waits for a name");
    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    assert!(!app.dirty, "{}", app.status);
    assert_eq!(app.folder.as_deref(), Some(path.as_path()));

    let (mut other, _rt2) = self::app();
    command(&mut other, &format!("e {}", path.display()));
    assert_eq!(notes(&other)[0], Note::On(60));
    assert!(!other.dirty);
}

#[test]
fn the_window_title_names_the_project() {
    let (mut app, _rt) = app();
    assert_eq!(
        app.window_title(),
        format!("vimtracker: {}", app.project_name()),
        "its random name before it's saved"
    );
    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        app.window_title(),
        format!("vimtracker: {name}"),
        "after save as"
    );

    let (mut other, _rt2) = self::app();
    command(&mut other, &format!("e {}", path.display()));
    assert_eq!(
        other.window_title(),
        format!("vimtracker: {name}"),
        "after open"
    );
    let fresh = folder();
    command(&mut other, &format!("new {}", fresh.display()));
    let name = fresh.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        other.window_title(),
        format!("vimtracker: {name}"),
        "after new"
    );
}

#[test]
fn a_sample_can_be_loaded_and_played() {
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("tone.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav, spec).unwrap();
    for _ in 0..48_000 {
        writer.write_sample(16_000_i16).unwrap();
    }
    writer.finalize().unwrap();

    let (mut app, mut rt) = app();
    command(&mut app, &format!("sample {}", wav.display()));
    assert!(app.status.contains("loaded"), "{}", app.status);
    // Writing a note plays it while its key is held.
    press(&mut app, "ia");
    let mut out = vec![0.0; 512];
    rt.process(&mut out);
    rt.process(&mut out);
    assert!(out.iter().any(|s| *s != 0.0), "the note should sound");
    assert_eq!(notes(&app)[0], Note::On(60));
}

#[test]
fn opening_refuses_unsaved_changes() {
    let (mut app, _rt) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let path = folder();
    command(&mut app, &format!("new {}", path.display()));
    assert!(app.status.contains("Unsaved"));
    assert!(!path.exists());
}

#[test]
fn track_keys_move_and_keep_the_section_position() {
    let (mut app, _rt) = app();
    press(&mut app, "jjj");
    press(&mut app, "W");
    assert_eq!(app.cursor.track, 1);
    assert_eq!(app.cursor.step, 3);
    assert_eq!(app.phrase_id(), Some(0), "its own first phrase");
    press(&mut app, &"W".repeat(3 + MAX_GROUPS));
    assert_eq!(
        app.cursor.track,
        4 + MAX_GROUPS,
        "the master stops the cursor"
    );
    press(&mut app, &"B".repeat(5 + MAX_GROUPS));
    assert_eq!(app.cursor.track, 0);
}

#[test]
fn added_tracks_save_load_and_play() {
    let (mut app, mut rt) = app();
    command(&mut app, "track add 3");
    assert_eq!(app.song().sound_tracks(), 5);
    assert_eq!(app.levels.len(), 5 + FIXED_TRACKS);
    press(&mut app, "WWWW");
    assert_eq!(app.cursor.track, 4);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    let (mut other, _rt2) = self::app();
    command(&mut other, &format!("e {}", path.display()));
    assert_eq!(other.song().sound_tracks(), 5);
    app.tick();
    let mut out = vec![0.0; 512];
    rt.process(&mut out);
    assert_eq!(rt.engine().current().song.sound_tracks(), 5);
}

#[test]
fn only_the_last_track_can_be_removed_and_one_stays() {
    let (mut app, _rt) = app();
    press(&mut app, "W");
    command(&mut app, "track del");
    assert_eq!(app.song().sound_tracks(), 1);
    assert_eq!(app.cursor.track, 0);
    assert_eq!(app.phrase_id(), Some(0));
    command(&mut app, "track del");
    assert_eq!(app.song().sound_tracks(), 1);
    assert!(app.status.contains("at least one"));
    press(&mut app, "uj");
    assert_eq!(app.song().sound_tracks(), 2);
    assert_eq!(app.levels.len(), 2 + FIXED_TRACKS);
}

#[test]
fn the_fixed_tracks_stay_after_new_ones_and_cant_be_removed() {
    let (mut app, _rt) = app();
    let master = 4 + MAX_GROUPS;
    press(&mut app, &"W".repeat(master));
    assert_eq!(app.cursor.track, master);
    command(&mut app, "track add 2");
    assert_eq!(app.cursor.track, master + 2, "still on the master");
    assert_eq!(app.song().tracks[master + 2].name, "M");
    assert_eq!(app.song().sound_tracks(), 4);
    command(&mut app, "track del");
    assert_eq!(app.cursor.track, master + 1, "still on the master");
    assert_eq!(app.song().tracks[master + 1].name, "M");
}

#[test]
fn a_fixed_tracks_mute_stays_with_it_as_tracks_come_and_go() {
    let (mut app, _rt) = app();
    let return_a = 2 + MAX_GROUPS;
    // Past the groups, which are empty and so folded away.
    press(&mut app, "WW");
    assert_eq!(app.song().tracks[app.cursor.track].name, "A");
    press(&mut app, " m");
    assert!(app.levels[return_a].mute);
    command(&mut app, "track add");
    assert!(app.levels[return_a + 1].mute, "return A is here now");
    assert!(!app.levels[2].mute, "and the new track isn't muted");
    command(&mut app, "track del");
    assert!(app.levels[return_a].mute);
}

#[test]
fn the_master_cant_be_muted_or_soloed() {
    let (mut app, _rt) = app();
    let master = 4 + MAX_GROUPS;
    press(&mut app, &"W".repeat(master));
    press(&mut app, " m");
    assert!(!app.levels[master].mute);
    assert!(app.status.contains("master"), "{}", app.status);
    press(&mut app, " s");
    assert!(!app.levels[master].solo);
}

#[test]
fn a_fixed_tracks_pages_only_show_what_it_has() {
    let (mut app, _rt) = app();
    to_track(&mut app, 2 + MAX_GROUPS);
    press(&mut app, " 4");
    let lines = track_page_text(&app);
    let fixed = format!("{} Return A", vt_tui::glyphs::RETURN);
    assert!(lines[0].starts_with(&fixed), "{}", lines[0]);
    assert!(lines[2].starts_with("Trim"), "{}", lines[2]);
    assert!(lines[3].starts_with("Level"), "{}", lines[3]);
    assert!(lines[4].starts_with("Pan"), "{}", lines[4]);
    assert!(!lines.iter().any(|line| line.contains("Machine")));
    press(&mut app, "WW");
    let lines = track_page_text(&app);
    let master = format!("{} Master", vt_tui::glyphs::MASTER);
    assert!(lines[0].starts_with(&master), "{}", lines[0]);
    assert!(lines[2].starts_with("Trim"), "{}", lines[2]);
    assert!(lines[3].starts_with("Level"), "{}", lines[3]);
    assert!(!lines.iter().any(|line| line.contains("Pan")));
}

#[test]
fn the_mixer_leaves_out_what_a_fixed_track_doesnt_have() {
    let (mut app, _rt) = app();
    let return_a = 2 + MAX_GROUPS;
    press(&mut app, " 6");
    // Past the groups, which are empty and so folded away.
    press(&mut app, "ll");
    press(&mut app, "jj");
    assert_eq!(app.cursor.track, return_a);
    press(&mut app, "i40");
    assert_eq!(
        app.song().tracks[return_a].send_a,
        0,
        "a return sends nowhere"
    );
    press(&mut app, "ll");
    assert_eq!(app.cursor.track, return_a + 2);
    press(&mut app, "ki40");
    assert_eq!(
        app.song().tracks[return_a + 2].pan,
        0x80,
        "the master has no pan"
    );
}

#[test]
fn the_mixers_cursor_stays_on_rows_a_track_has_and_comes_back() {
    use vt_tui::views::status::column_name;
    let (mut app, _rt) = app();
    // With the empty groups shown.
    press(&mut app, " 6zRjj");
    assert_eq!(column_name(&app), "Send A");
    press(&mut app, "ll");
    assert_eq!(app.cursor.track, 2);
    assert_eq!(column_name(&app), "Send A", "a group sends");
    press(&mut app, &"l".repeat(MAX_GROUPS));
    assert_eq!(app.cursor.track, 2 + MAX_GROUPS);
    assert_eq!(
        column_name(&app),
        "Pan",
        "a return has no sends, so it's on the row above"
    );
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Mute", "and going down skips them");
    press(&mut app, "k");
    assert_eq!(column_name(&app), "Pan");
    press(&mut app, "ll");
    assert_eq!(column_name(&app), "Level", "the master only has its level");
    press(&mut app, "jG");
    assert_eq!(column_name(&app), "Level");
    press(&mut app, &"h".repeat(4 + MAX_GROUPS));
    assert_eq!(app.cursor.track, 0);
    assert_eq!(column_name(&app), "Level", "where it went is where it is");
    press(&mut app, "jjl");
    assert_eq!(column_name(&app), "Send A");
    press(&mut app, "lh");
    assert_eq!(column_name(&app), "Send A", "back on a track that has it");

    // `$` and `0` go to the last and first tracks that have the row.
    press(&mut app, "$");
    assert_eq!(
        (app.cursor.track, column_name(&app)),
        (1 + MAX_GROUPS, "Send A".to_owned()),
        "the last group"
    );
    press(&mut app, "0");
    assert_eq!(
        (app.cursor.track, column_name(&app)),
        (0, "Send A".to_owned())
    );
    press(&mut app, "k$");
    assert_eq!(
        (app.cursor.track, column_name(&app)),
        (3 + MAX_GROUPS, "Pan".to_owned()),
        "B, as M has none"
    );
    press(&mut app, "k$");
    assert_eq!(
        (app.cursor.track, column_name(&app)),
        (4 + MAX_GROUPS, "Level".to_owned())
    );
}

#[test]
fn track_count_is_limited_to_the_constant() {
    let (mut app, _rt) = app();
    command(&mut app, "track add 62");
    assert_eq!(app.song().sound_tracks(), 64);
    command(&mut app, "track add");
    assert_eq!(app.song().sound_tracks(), 64);
    assert!(app.status.contains("at most"));
}

#[test]
fn mute_isnt_in_undo_history_but_is_saved() {
    let (mut app, _rt) = app();
    tap(&mut app, " m");
    assert!(app.levels[0].mute);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "u");
    assert_eq!(notes(&app)[0], Note::Empty);
    assert!(app.levels[0].mute, "undo must not change a mute");
    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    let (mut other, _rt2) = self::app();
    command(&mut other, &format!("e {}", path.display()));
    assert!(other.levels[0].mute);
    press(&mut other, " M");
    assert!(!other.levels[0].mute);
}

#[test]
fn muted_and_soloed_tracks_change_what_sounds() {
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("tone.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav, spec).unwrap();
    for _ in 0..48_000 {
        writer.write_sample(16_000_i16).unwrap();
    }
    writer.finalize().unwrap();
    let (mut app, mut rt) = app();
    command(&mut app, &format!("sample {}", wav.display()));
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    press(&mut app, " m");
    app.tick();
    // Let the entered note's preview finish, since previews sound on muted tracks.
    let mut out = vec![0.0; 2 * 24_000];
    rt.process(&mut out);
    let mut out = vec![0.0; 512];
    app.key(Key::new(KeyCode::Enter), Instant::now());
    rt.process(&mut out);
    assert!(out.iter().all(|s| *s == 0.0), "a muted track is silent");
    press(&mut app, " m");
    press(&mut app, "W");
    press(&mut app, " s");
    rt.process(&mut out);
    assert!(out.iter().all(|s| *s == 0.0), "another track is soloed");
}

#[test]
fn groove_commands_set_the_project_and_the_cursors_track_and_undo() {
    let (mut app, _realtime) = app();
    let groove = |name: &str| GrooveId::named(name).unwrap();
    command(&mut app, "swing 66");
    assert_eq!(app.song().groove, groove("Swing 66"));
    command(&mut app, "swing 67");
    assert_eq!(
        app.song().groove,
        groove("Swing 66"),
        "only a swing groove's percent"
    );
    assert!(app.status.contains("52, 54"), "{}", app.status);
    command(&mut app, "groove mpc-58");
    assert_eq!(app.song().groove, groove("MPC 58"));
    command(&mut app, "tgroove pull");
    assert_eq!(app.song().tracks[0].groove, Some(groove("Pull")));
    assert_eq!(app.song().tracks[1].groove, None);
    command(&mut app, "tgroove off");
    assert_eq!(app.song().tracks[0].groove, None);
    press(&mut app, "uu");
    assert_eq!(app.song().groove, groove("MPC 58"));
    assert_eq!(app.song().tracks[0].groove, None);
    press(&mut app, "u");
    assert_eq!(app.song().groove, groove("Swing 66"));
    press(&mut app, "u");
    assert_eq!(app.song().groove, GrooveId::STRAIGHT);
}

/// A one-frame WAV, whose value comes from its name, so files of different names are different
/// files to the bank, which knows a file by its content.
fn wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let name = path.file_name().unwrap().to_string_lossy();
    let value = name.bytes().fold(1000_i16, |value, byte| {
        value.wrapping_mul(31).wrapping_add(i16::from(byte))
    });
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    writer.write_sample(value).unwrap();
    writer.finalize().unwrap();
}

#[test]
fn a_folder_opens_the_sample_explorer_and_choosing_loads_the_sample() {
    let (mut app, _realtime) = app();
    let folder = std::env::temp_dir().join(format!("vt-browse-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("kit/.hidden")).unwrap();
    wav(&folder.join("kit/kick.wav"));
    wav(&folder.join("kit/snare.WAV"));
    wav(&folder.join("kit/.hidden/secret.wav"));
    std::fs::write(folder.join("kit/notes.txt"), "not audio").unwrap();

    command(
        &mut app,
        &format!("sample {}", folder.join("kit").display()),
    );
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(
        explorer_rows(&app)[1..],
        ["    kick.wav", "    snare.WAV"],
        "only samples, and no hidden ones"
    );
    press(&mut app, "/sn");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    let chosen = sample_of(&app, 0).unwrap();
    assert!(chosen.ends_with("snare.WAV"), "{chosen}");
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn the_sample_explorer_lists_and_loads_aiff_and_flac() {
    let (mut app, _realtime) = app();
    let folder = std::env::temp_dir().join(format!("vt-browse-formats-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let made = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../vt-model/tests/samples");
    std::fs::copy(made.join("stereo-16.aif"), folder.join("hat.aif")).unwrap();
    std::fs::copy(made.join("stereo-16.aif"), folder.join("ride.aiff")).unwrap();
    std::fs::copy(made.join("stereo-16.flac"), folder.join("tom.flac")).unwrap();
    wav(&folder.join("kick.wav"));
    std::fs::write(folder.join("loop.mp3"), "not read").unwrap();

    command(&mut app, &format!("sample {}", folder.display()));
    assert_eq!(
        explorer_rows(&app)[1..],
        [
            "    hat.aif",
            "    kick.wav",
            "    ride.aiff",
            "    tom.flac"
        ]
    );
    press(&mut app, "/tom");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.status, "Track 01 plays 01 tom, loaded into the bank");
    let chosen = sample_of(&app, 0).unwrap();
    assert!(chosen.ends_with("tom.flac"), "{chosen}");
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn a_plain_write_keeps_live_mutes_and_solos() {
    let (mut app, _realtime) = app();
    let folder = std::env::temp_dir().join(format!("vt-mute-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    command(&mut app, &format!("w {}", folder.display()));
    tap(&mut app, " m");
    command(&mut app, "w");
    let saved = vt_model::project::load_song(&folder).unwrap();
    assert!(saved.tracks[0].mute);
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn cuj_5_reuse_and_vary_a_phrase() {
    let (mut app, _realtime) = app();
    // Add the same phrase twice to this chain, then open the second use.
    press(&mut app, " 2");
    assert_eq!(app.view, View::Chain);
    press(&mut app, "yyp");
    assert_eq!(app.chain_entries().len(), 2);
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "gd");
    assert_eq!(app.view, View::Phrase);
    assert!(app.status.contains("used 2 times"), "{}", app.status);
    // The clone stays here. An edit reaches only the clone.
    press(&mut app, "gc");
    assert_eq!(app.phrase_id(), Some(1));
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.song().tracks[0].phrases[&0].steps()[0].note,
        Note::Empty
    );
    assert_eq!(
        app.song().tracks[0].phrases[&1].steps()[0].note,
        Note::On(60)
    );
    assert_eq!(app.chain_entries()[0].phrase, 0);
    // Ctrl-O returns to where the jump started, and Ctrl-I goes forward again.
    let back = parse_sequence("C-o").unwrap()[0];
    app.key(back, Instant::now());
    assert_eq!(app.view, View::Chain);
    assert_eq!(app.cursor.slot, 1);
    app.key(parse_sequence("C-i").unwrap()[0], Instant::now());
    assert_eq!(app.view, View::Phrase);
    // Undo removes the clone's use again.
    press(&mut app, "uu");
    assert_eq!(app.chain_entries()[1].phrase, 0);
}

#[test]
fn gd_on_the_empty_row_adds_the_lowest_unused_phrase() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2G");
    assert_eq!(app.cursor.slot, 1);
    assert_eq!(app.phrase_id(), None);
    press(&mut app, "gd");
    assert_eq!(app.view, View::Phrase);
    assert_eq!(app.phrase_id(), Some(1));
    assert_eq!(app.chain_entries().len(), 2);
    // Leaving the chain view never leaves the cursor on the empty row.
    press(&mut app, " 2G 3");
    assert_eq!(app.cursor.slot, 1);
    assert_eq!(app.phrase_id(), Some(1));
    assert_eq!(notes(&app).len(), 16);
}

#[test]
fn the_chain_view_edits_rows_and_keeps_one() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2");
    press(&mut app, "dd");
    assert!(app.status.contains("at least one"), "{}", app.status);
    // Typing a phrase on the empty row adds it, and a new ID is created.
    press(&mut app, "ji1f");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.chain_entries()[1].phrase, 0x1F);
    assert!(app.song().tracks[0].phrases.contains_key(&0x1F));
    // Transpose by keys and by typing, and out of range is refused.
    press(&mut app, "l5=");
    assert_eq!(app.chain_entries()[1].transpose, 5);
    press(&mut app, "i-12");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.chain_entries()[1].transpose, -12);
    press(&mut app, "i60");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.chain_entries()[1].transpose, -12);
    assert!(app.status.contains("48"), "{}", app.status);
    press(&mut app, "99-");
    assert_eq!(app.chain_entries()[1].transpose, -48);
    // Removing the row copies it, so it can be pasted back.
    press(&mut app, "dd");
    assert_eq!(app.chain_entries().len(), 1);
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "kp");
    assert_eq!(app.chain_entries()[1].phrase, 0x1F);
}

#[test]
fn phrase_view_keys_move_through_the_chain_and_grr_lists_uses() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2yyp 3");
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "[P");
    assert_eq!(app.cursor.slot, 0);
    press(&mut app, "]p");
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "]p");
    assert_eq!(app.cursor.slot, 1, "motions stop at the edge");
    press(&mut app, "[p]P");
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "grr");
    assert_eq!(app.mode, Mode::Picker);
    let uses: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(listed)
        .collect();
    assert_eq!(uses, ["Section 01 slot 00", "Section 01 slot 01"]);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.cursor.slot, 0);
    assert_eq!(app.view, View::Phrase);
}

#[test]
fn a_shared_chain_warns_on_clone_and_gc_big_separates_it() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1gc 3");
    assert_eq!(app.chain_uses(), 2);
    press(&mut app, "gc");
    assert!(app.status.contains("shared"), "{}", app.status);
    press(&mut app, "gC");
    assert_eq!(app.chain_uses(), 1);
    assert_ne!(app.chain_id(), Some(0));
}

#[test]
fn undo_keeps_the_cursor_on_a_row_that_exists() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2yyp 3");
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "u");
    assert_eq!(app.cursor.slot, 0);
    assert_eq!(app.phrase_id(), Some(0));
}

#[test]
fn render_writes_what_is_in_memory_with_live_mutes() {
    let (mut app, _realtime) = app();
    let path = std::env::temp_dir().join(format!("vt-app-render-{}.wav", std::process::id()));
    let _ = std::fs::remove_file(&path);
    command(&mut app, &format!("render {} --rows 4", path.display()));
    assert!(app.status.contains("Rendered"), "{}", app.status);
    let reader = hound::WavReader::open(&path).unwrap();
    assert_eq!(reader.len(), 24_000 * 2);
    std::fs::remove_file(&path).unwrap();
    assert!(!app.dirty, "rendering isn't an edit");
    command(&mut app, "render /no/such/folder/x.wav");
    assert!(app.status.contains("Couldn't write"), "{}", app.status);
}

mod autosave {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use ratatui::crossterm::event::KeyCode;
    use vt_model::project;
    use vt_model::song::Song;
    use vt_model::step::Note;
    use vt_tui::input::mode::Mode;

    use super::{app, command, press, special};

    const AUTOSAVE: &str = "song.autosave.ron";

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vt-auto-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    fn later(start: Instant, seconds: u64) -> Instant {
        start + Duration::from_secs(seconds)
    }

    #[test]
    fn only_a_changed_project_is_written_and_only_when_due() {
        let path = folder("changed");
        let (mut app, _realtime) = app();
        command(&mut app, &format!("w {}", path.display()));
        let start = Instant::now();
        app.autosave_if_due(start);
        app.autosave_if_due(later(start, 61));
        assert!(!path.join(AUTOSAVE).exists(), "nothing has changed");

        press(&mut app, "ia");
        special(&mut app, KeyCode::Esc);
        app.autosave_if_due(later(start, 62));
        assert!(!path.join(AUTOSAVE).exists(), "not due yet");
        app.autosave_if_due(later(start, 125));
        assert!(path.join(AUTOSAVE).exists());
        let saved = project::load_song(&path).unwrap();
        assert_eq!(
            saved.tracks[0].phrases[&0].steps()[0].note,
            Note::Empty,
            "song.ron is untouched"
        );

        // No change since then means no new write.
        std::fs::remove_file(path.join(AUTOSAVE)).unwrap();
        app.autosave_if_due(later(start, 190));
        assert!(!path.join(AUTOSAVE).exists());

        // A real save clears it.
        press(&mut app, "is");
        special(&mut app, KeyCode::Esc);
        app.autosave_if_due(later(start, 260));
        assert!(path.join(AUTOSAVE).exists());
        command(&mut app, "w");
        assert!(!path.join(AUTOSAVE).exists());
        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn an_unsaved_project_with_no_folder_is_left_alone() {
        let (mut app, _realtime) = app();
        press(&mut app, "ia");
        special(&mut app, KeyCode::Esc);
        let start = Instant::now();
        app.autosave_if_due(start);
        app.autosave_if_due(later(start, 61));
        assert!(app.dirty);
        assert!(app.status.is_empty(), "{}", app.status);
    }

    /// A folder as a crash would leave it: a saved song, and newer work beside it.
    fn crashed(path: &Path) {
        project::save_song(path, &Song::new(2)).unwrap();
        let mut newer = Song::new(2);
        std::sync::Arc::make_mut(newer.tracks[0].phrases.get_mut(&0).unwrap())
            .step_mut(0)
            .unwrap()
            .note = Note::On(60);
        project::write_autosave(path, &newer).unwrap();
    }

    #[test]
    fn a_newer_autosave_is_offered_and_restores_as_unsaved() {
        let path = folder("restore");
        crashed(&path);
        let (mut app, _realtime) = app();
        command(&mut app, &format!("e {}", path.display()));
        assert_eq!(app.mode, Mode::Restore);
        assert_eq!(
            app.song().tracks[0].phrases[&0].steps()[0].note,
            Note::Empty
        );
        // Other keys don't answer.
        press(&mut app, "q");
        special(&mut app, KeyCode::Esc);
        assert_eq!(app.mode, Mode::Restore);
        press(&mut app, "y");
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(
            app.song().tracks[0].phrases[&0].steps()[0].note,
            Note::On(60)
        );
        assert!(app.dirty);
        command(&mut app, "w");
        let saved = project::load_song(&path).unwrap();
        assert_eq!(saved.tracks[0].phrases[&0].steps()[0].note, Note::On(60));
        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn declining_deletes_the_autosave_and_keeps_the_song() {
        let path = folder("decline");
        crashed(&path);
        let (mut app, _realtime) = app();
        command(&mut app, &format!("e {}", path.display()));
        press(&mut app, "n");
        assert_eq!(app.mode, Mode::Normal);
        assert!(!path.join(AUTOSAVE).exists());
        assert_eq!(
            app.song().tracks[0].phrases[&0].steps()[0].note,
            Note::Empty
        );
        assert!(!app.dirty);
        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn an_unreadable_autosave_is_kept_and_reported() {
        let path = folder("broken");
        project::save_song(&path, &Song::new(2)).unwrap();
        std::fs::write(path.join(AUTOSAVE), "(format_version: 99)").unwrap();
        let (mut app, _realtime) = app();
        command(&mut app, &format!("e {}", path.display()));
        press(&mut app, "y");
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status.contains("kept"), "{}", app.status);
        assert!(path.join(AUTOSAVE).exists());
        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn quitting_on_purpose_removes_the_autosave() {
        let path = folder("quit");
        let (mut app, _realtime) = app();
        command(&mut app, &format!("w {}", path.display()));
        press(&mut app, "ia");
        special(&mut app, KeyCode::Esc);
        let start = Instant::now();
        app.autosave_if_due(start);
        app.autosave_if_due(later(start, 61));
        assert!(path.join(AUTOSAVE).exists());
        command(&mut app, "q");
        assert!(!app.should_quit, "unsaved work needs :q!");
        assert!(path.join(AUTOSAVE).exists());
        command(&mut app, "q!");
        assert!(app.should_quit);
        assert!(!path.join(AUTOSAVE).exists());
        std::fs::remove_dir_all(&path).unwrap();
    }
}

fn screen(app: &App) -> ratatui::buffer::Buffer {
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::phrase::draw(frame, frame.area(), app))
        .unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn a_selection_keeps_its_text_readable_and_shows_the_cursor() {
    let (mut app, _realtime) = app();
    press(&mut app, "Vj");
    let buffer = screen(&app);
    // Past the step number and playhead gutter, on the row the selection started from.
    let selected = &buffer[(10, 2)];
    assert_eq!(selected.bg, vt_tui::theme::SURFACE1);
    assert_eq!(selected.fg, vt_tui::theme::TEXT);
    assert_ne!(selected.symbol(), " ");
    let cursor = &buffer[(9, 3)];
    assert_eq!(cursor.bg, vt_tui::theme::ROSEWATER);
    assert_eq!(cursor.fg, vt_tui::theme::BASE);
}

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
    assert_eq!(app.octave, 3);

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
fn each_row_shows_how_far_it_is_from_the_cursor() {
    let (mut app, _realtime) = app();
    press(&mut app, "5j");
    let buffer = screen(&app);
    let row = |step: u16| -> String { (0..9).map(|x| buffer[(x, step + 2)].symbol()).collect() };
    assert_eq!(row(0), " 5  00   ");
    assert_eq!(row(4), " 1  04   ");
    assert_eq!(row(5), "    05   ", "the cursor's row is blank");
    assert_eq!(row(6), " 1  06   ");
    // Decimal, to match the count typed, beside a step number in hex.
    assert_eq!(row(15), "10  0F   ");
    assert_eq!(buffer[(1, 2)].fg, vt_tui::theme::OVERLAY0);
    assert_eq!(buffer[(4, 2)].fg, vt_tui::theme::YELLOW, "the beat marker");
    assert_eq!(buffer[(4, 3)].fg, vt_tui::theme::SUBTEXT1, "other steps");
}

#[test]
fn the_overview_shows_every_track_and_marks_the_one_being_edited() {
    let (mut app, _realtime) = app();
    press(&mut app, "W");
    let buffer = screen(&app);
    let title: String = (67..80).map(|x| buffer[(x, 0)].symbol()).collect();
    assert_eq!(
        title.trim_end(),
        " 01  02",
        "the returns and master are left out"
    );
    assert_eq!(
        buffer[(72, 0)].bg,
        vt_tui::theme::GREEN,
        "track 02 is marked"
    );
    assert_ne!(buffer[(68, 0)].bg, vt_tui::theme::GREEN);
    let headings: String = (0..30).map(|x| buffer[(x, 1)].symbol()).collect();
    assert_eq!(headings.trim_end(), "         Note              FX");
    assert_eq!(
        buffer[(27, 2)].bg,
        vt_tui::theme::FX_BAND,
        "the FX sit on a band"
    );
    assert_ne!(buffer[(9, 2)].bg, vt_tui::theme::FX_BAND);
}

#[test]
fn other_tracks_light_the_step_they_are_playing() {
    let song = Song::new(3);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    let color = |app: &App, row: u16| {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::phrase::draw(frame, frame.area(), app))
            .unwrap();
        // The next track's column in the overview, on a step two rows below the title.
        let buffer = terminal.backend().buffer();
        buffer[(68, row + 2)].fg
    };
    on_the_row_being_edited(&mut app);
    app.set_playhead(2);
    assert_eq!(color(&app, 2), vt_tui::theme::OVERLAY2, "stopped");
    app.transport = Transport::Playing;
    assert_eq!(color(&app, 2), vt_tui::theme::GREEN);
    assert_eq!(color(&app, 3), vt_tui::theme::OVERLAY2);
    app.transport = Transport::Paused;
    assert_eq!(color(&app, 2), vt_tui::theme::GREEN);
}

#[test]
fn tvol_sets_this_tracks_level_and_can_be_undone() {
    let (mut app, _realtime) = app();
    assert_eq!(app.song().tracks[0].level, 0xE0);
    press(&mut app, "W");
    command(&mut app, "tvol C0");
    assert_eq!(app.song().tracks[0].level, 0xE0);
    assert_eq!(app.song().tracks[1].level, 0xC0);
    assert!(app.dirty);
    press(&mut app, "u");
    assert_eq!(app.song().tracks[1].level, 0xE0);
    command(&mut app, "tvol loud");
    assert_eq!(app.song().tracks[1].level, 0xE0);
    assert!(app.status.contains(":tvol"));
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
fn insert_turns_the_cursor_green_and_boxes_the_track_without_moving_anything() {
    let (mut app, _realtime) = app();
    let row = |buffer: &ratatui::buffer::Buffer, y: u16| -> String {
        (0..40).map(|x| buffer[(x, y)].symbol()).collect()
    };

    let normal = screen(&app);
    assert_eq!(normal[(9, 2)].bg, vt_tui::theme::ROSEWATER);
    assert_eq!(normal[(8, 1)].symbol(), " ");
    assert_eq!(normal[(8, 2)].symbol(), " ");

    press(&mut app, "i");
    let insert = screen(&app);
    assert_eq!(insert[(9, 2)].bg, vt_tui::theme::GREEN);
    assert_eq!(insert[(9, 2)].fg, vt_tui::theme::BASE);
    assert_eq!(insert[(8, 1)].symbol(), "┌");
    assert_eq!(insert[(8, 2)].symbol(), "│");
    assert_eq!(insert[(8, 18)].symbol(), "└");
    assert_eq!(insert[(8, 2)].fg, vt_tui::theme::GREEN);

    // Take away the box, and the steps are where they were.
    assert_eq!(
        row(&normal, 2).replace(' ', ""),
        row(&insert, 2).replace(['│', ' '], "")
    );
    let step_text_start = |b: &ratatui::buffer::Buffer| b[(12, 2)].symbol().to_owned();
    assert_eq!(step_text_start(&normal), step_text_start(&insert));

    special(&mut app, KeyCode::Esc);
    let back = screen(&app);
    assert_eq!(back[(8, 1)].symbol(), " ");
    assert_eq!(back[(9, 2)].bg, vt_tui::theme::ROSEWATER);
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

/// A kit file beside its sounds, returning the kit's path.
fn kit_file() -> PathBuf {
    let root = folder();
    let sounds = root.join("drums");
    std::fs::create_dir_all(&sounds).unwrap();
    wav(&sounds.join("kick.wav"));
    wav(&sounds.join("snare.wav"));
    wav(&sounds.join("hat.wav"));
    let kit = root.join("909.ron");
    std::fs::write(
        &kit,
        r#"(sounds: [
            (path: "drums/snare.wav"),
            (path: "drums/kick.wav"),
            (path: "drums/hat.wav"),
        ])"#,
    )
    .unwrap();
    kit
}

/// The file a track's sampler plays, through the bank.
fn file_of(song: &Song, track: usize) -> Option<String> {
    let number = song.tracks.get(track)?.sample();
    let entry = song.bank.get(number)?;
    Some(entry.source()?.path.clone())
}

fn sample_of(app: &App, track: usize) -> Option<String> {
    file_of(app.song(), track)
}

#[test]
fn a_kit_with_a_bad_sound_changes_nothing_and_names_it() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    let drums = kit.parent().unwrap().join("drums");
    std::fs::write(drums.join("broken.wav"), "not audio").unwrap();
    std::fs::write(
        &kit,
        r#"(sounds: [(path: "drums/kick.wav"), (path: "drums/broken.wav")])"#,
    )
    .unwrap();
    command(&mut app, &format!("kit {} all", kit.display()));
    assert_eq!(sample_of(&app, 0), None);
    assert!(!app.dirty);
    assert!(app.status.contains("broken"), "{}", app.status);

    std::fs::write(&kit, r#"(sounds: [(path: "drums/gone.wav")])"#).unwrap();
    command(&mut app, &format!("kit {} all", kit.display()));
    assert_eq!(sample_of(&app, 0), None);
    assert!(app.status.contains("gone"), "{}", app.status);

    std::fs::write(&kit, "not a kit").unwrap();
    command(&mut app, &format!("kit {} all", kit.display()));
    assert_eq!(sample_of(&app, 0), None);
    assert!(app.status.contains("909.ron"), "{}", app.status);
}

#[test]
fn a_kit_is_referred_to_and_saving_elsewhere_gathers_its_sounds() {
    let project = folder();
    let (mut app, _realtime) = app();
    command(&mut app, &format!("w {}", project.display()));
    let kit = kit_file();
    let snare = kit.parent().unwrap().join("drums/snare.wav");
    command(&mut app, &format!("kit {} all", kit.display()));
    assert!(std::path::Path::new(&sample_of(&app, 0).unwrap()).is_absolute());
    assert!(!project.join("samples").exists());

    command(&mut app, "w");
    let reopened = vt_model::project::load_song(&project).unwrap();
    assert_eq!(
        file_of(&reopened, 0).as_deref(),
        Some(snare.canonicalize().unwrap().to_str().unwrap())
    );

    let bundle = folder();
    command(&mut app, &format!("w {}", bundle.display()));
    assert_eq!(sample_of(&app, 0).unwrap(), "samples/snare.wav");
    assert!(bundle.join("samples/hat.wav").exists());
}

#[test]
fn a_kit_adds_nothing_until_a_sound_is_chosen_for_a_track() {
    let (mut app, _realtime) = app();
    command(&mut app, "track add 2");
    let kit = kit_file();
    let named = kit.parent().unwrap().join("named.ron");
    std::fs::write(
        &named,
        r#"(sounds: [
            (label: "SD", name: "Snare 909", path: "drums/snare.wav"),
            (path: "drums/hat.wav"),
        ])"#,
    )
    .unwrap();
    let tracks = app.song().tracks.len();
    let depth = app.history.undo_depth();
    command(&mut app, &format!("kit {}", named.display()));
    assert_eq!(app.song().tracks.len(), tracks);
    assert_eq!(sample_of(&app, 0), None);
    assert_eq!(app.history.undo_depth(), depth);
    assert!(app.status.contains("2 sounds"), "{}", app.status);

    press(&mut app, "WW 5");
    assert_eq!(app.mode, Mode::Picker);
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(listed)
        .collect();
    assert_eq!(
        labels[..3],
        ["Kit named", "SD  Snare 909", "hat"],
        "the kit first, by name or the file's"
    );
    special(&mut app, KeyCode::Enter);
    assert!(sample_of(&app, 2).unwrap().ends_with("snare.wav"));
    assert_eq!(app.song().tracks[2].name, "SD");
    assert_eq!(sample_of(&app, 0), None, "only this track");
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(sample_of(&app, 2), None);
    assert_eq!(app.song().tracks[2].name, "");

    press(&mut app, " 5");
    app.key(parse_sequence("C-n").unwrap()[0], Instant::now());
    special(&mut app, KeyCode::Enter);
    assert!(sample_of(&app, 2).unwrap().ends_with("hat.wav"));
    assert_eq!(app.song().tracks[2].name, "", "no label, no name");
}

#[test]
fn a_kit_with_all_fills_tracks_and_a_bad_one_keeps_the_last() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    command(&mut app, &format!("kit {} all", kit.display()));
    assert!(sample_of(&app, 2).unwrap().ends_with("hat.wav"));
    command(&mut app, &format!("kit {}", kit.display()));
    assert_eq!(app.kit.len(), 3);
    command(&mut app, "kit nothing-here");
    assert_eq!(app.kit.len(), 3, "still the last one");
}

/// The track page as lines of text.
fn track_text(app: &App) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..16)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect()
}

/// Choose a machine with `K` on the Sound page's Machine row, by typing its name.
fn choose_machine(app: &mut App, name: &str) {
    press(app, " 4gg");
    press(app, "K");
    press(app, name);
    special(app, KeyCode::Enter);
}

/// The value of the track's machine's setting at this place.
fn machine_value(app: &App, setting: usize) -> u8 {
    app.song().tracks[0].machine_value(setting)
}

#[test]
fn k_on_machine_chooses_a_voice_and_shows_its_settings() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    assert_eq!(column_name(&app), "Machine");
    assert!(
        track_text(&app)[3].contains("Sample"),
        "{:?}",
        track_text(&app)
    );
    let depth = app.history.undo_depth();
    press(&mut app, "K");
    assert_eq!(
        app.picker.as_ref().unwrap().shown().count(),
        Machine::ALL.len()
    );
    press(&mut app, "VA");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::VirtualAnalog)
    );
    assert_eq!(app.history.undo_depth(), depth + 1);
    let text = track_text(&app);
    assert!(
        text[2].contains("Machine") && text[2].contains("VA"),
        "{text:?}"
    );
    assert!(
        text[3].contains("Detune") && text[3].contains("80"),
        "{text:?}"
    );
    assert!(
        text[4].contains("Square") && text[4].contains("00"),
        "{text:?}"
    );
    assert!(
        text[5].contains("Saw") && text[5].contains("████"),
        "{text:?}"
    );
    assert!(
        text[6].contains("Send B") && !text[6].contains("Saw"),
        "a voice of three settings: {text:?}"
    );
    assert!(
        !text
            .iter()
            .any(|line| line.starts_with("Sample ") || line.contains("Voice"))
    );

    press(&mut app, "j");
    assert_eq!(column_name(&app), "Detune");
    press(&mut app, "=");
    assert_eq!(machine_value(&app, 0), 0x81);
    press(&mut app, "j");
    press(&mut app, "i60");
    assert_eq!(machine_value(&app, 1), 0x60);
    press(&mut app, "x");
    assert_eq!(machine_value(&app, 1), 0);
    // Past Square's fine digits to Pan.
    press(&mut app, "ll");
    assert_eq!(column_name(&app), "Pan");
    press(&mut app, "jj");
    assert_eq!(column_name(&app), "Send B", "the row with only its Send B");
    press(&mut app, "ggh-");
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Sampler,
        "VA comes after Sample"
    );
    assert!(track_text(&app)[2].contains("Sample"));
    press(&mut app, "=x");
    assert_eq!(app.song().tracks[0].machine, Machine::Sampler);
}

#[test]
fn each_voice_keeps_its_own_settings() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    let names = |app: &App| -> Vec<String> {
        let text = track_text(app);
        (3..6)
            .map(|row| text[row].split_whitespace().next().unwrap().to_owned())
            .collect()
    };
    choose_machine(&mut app, "FM");
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::Fm)
    );
    assert_eq!(names(&app), ["Ratio", "Index", "Feedback"]);
    press(&mut app, "jj");
    assert_eq!(column_name(&app), "Index");
    press(&mut app, "i40");
    assert_eq!(machine_value(&app, 1), 0x40);

    // Another voice starts from its own defaults. Fold is two along, past PD.
    press(&mut app, "gg==");
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::Waveshaping),
        "the next"
    );
    assert_eq!(names(&app), ["Shape", "Fold", "Slope"]);
    assert_eq!(machine_value(&app, 1), 0, "Fold's own");
    press(&mut app, "jji20");
    assert_eq!(machine_value(&app, 1), 0x20);

    // Back to FM finds it as it was, and the same through samples.
    press(&mut app, "gg--");
    assert_eq!(machine_value(&app, 1), 0x40);
    choose_machine(&mut app, "Sample");
    assert_eq!(app.song().tracks[0].machine, Machine::Sampler);
    choose_machine(&mut app, "Fold");
    assert_eq!(machine_value(&app, 1), 0x20);
}

#[test]
fn six_op_patch_counts_patches_and_k_lists_them_by_name() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    let patch = |app: &App| machine_value(app, 0);
    choose_machine(&mut app, "6-op");
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Patch");
    assert_eq!(patch(&app), 0x20, "E.PIANO 1 to start");
    assert!(
        track_text(&app)[3].contains("20   E.PIANO 1"),
        "{:?}",
        track_text(&app)
    );
    assert!(
        track_text(&app)[4].contains("Bright    80"),
        "as the patch was made"
    );
    press(&mut app, "-");
    assert_eq!(patch(&app), 0x1F);
    press(&mut app, "i00=");
    assert_eq!(patch(&app), 1, "one patch a step");
    assert!(track_text(&app)[3].contains("01   Mooger Low"));
    press(&mut app, "i5F=");
    assert_eq!(patch(&app), 0x5F, "it stops at the last");
    press(&mut app, "i90");
    assert_eq!(patch(&app), 0x5F);

    press(&mut app, "K");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 96);
    press(&mut app, "MARIMBA");
    special(&mut app, KeyCode::Enter);
    assert!(
        track_text(&app)[3].contains("MARIMBA"),
        "{:?}",
        track_text(&app)
    );
    assert_eq!(usize::from(patch(&app)), 49);
}

#[test]
fn the_sound_page_sets_a_synths_accent() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    let accent = |app: &App| machine_value(app, 3);
    choose_machine(&mut app, "Kick");
    press(&mut app, "jjjj");
    assert_eq!(column_name(&app), "Accent");
    assert_eq!(accent(&app), 0xCC, "Plaits' own strength to start");
    assert!(
        track_text(&app)[6].starts_with("Accent    CC"),
        "{:?}",
        track_text(&app)
    );
    press(&mut app, "=");
    assert_eq!(accent(&app), 0xCD);
    press(&mut app, "i40");
    assert_eq!(accent(&app), 0x40);
    press(&mut app, "x");
    assert_eq!(accent(&app), 0xCC, "back to its default");
}

#[test]
fn choosing_a_sample_goes_back_to_playing_samples() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    command(&mut app, &format!("kit {}", kit.display()));
    choose_machine(&mut app, "VA");
    press(&mut app, " 5");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].machine, Machine::Sampler);
    assert!(sample_of(&app, 0).unwrap().ends_with("snare.wav"));
}

#[test]
fn bars_show_where_a_value_is() {
    use vt_tui::views::widgets::bar::{centre_bar, level_bar};
    assert_eq!(level_bar(0x00, 9), "         ");
    assert_eq!(level_bar(0x01, 9), "▏        ");
    assert_eq!(level_bar(0x40, 9), "██▎      ");
    assert_eq!(level_bar(0x80, 9), "████▌    ");
    assert_eq!(level_bar(0xFE, 9), "████████▉");
    assert_eq!(level_bar(0xFF, 9), "█████████");
    assert_eq!(centre_bar(0x00, 9), "│        ");
    assert_eq!(centre_bar(0x80, 9), "    │    ");
    assert_eq!(centre_bar(0xFF, 9), "        │");
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    let text = track_text(&app).join("\n");
    assert!(text.contains("8000 ▕    │    ▏"), "{text}");
}

#[test]
fn a_moving_bar_follows_what_plays_and_marks_what_is_set() {
    use ratatui::style::Style;
    use vt_tui::theme;
    use vt_tui::views::widgets::bar::ValueBar;
    let bar_spans =
        |set, live, centred| ValueBar::new(set, 9).playing(live).centred(centred).spans();
    let text = |spans: &[ratatui::text::Span]| {
        spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    };
    let style_at = |spans: &[ratatui::text::Span], cell: usize| spans[cell].style;
    // Nothing moving: only the bar, in one piece.
    let still = bar_spans(0x80, 0x80, false);
    assert_eq!((still.len(), text(&still).as_str()), (1, "████▌    "));
    assert_eq!(text(&bar_spans(0x80, 0x80, true)), "    │    ");

    // Moved below the setting: the fill follows, and a dim mark stays where it's set.
    let below = bar_spans(0xC0, 0x40, false);
    assert_eq!(text(&below), "██▎   │  ");
    assert_eq!(style_at(&below, 6), theme::faint());
    assert_eq!(style_at(&below, 0).fg, Some(theme::MAUVE));

    // Moved above it: the mark is cut out of the fill, so it still shows.
    let above = bar_spans(0x40, 0xC0, false);
    assert_eq!(text(&above), "██│███▊  ");
    assert_eq!(
        style_at(&above, 2),
        Style::new().fg(theme::BASE).bg(theme::MAUVE)
    );

    // Around a centre: a bright mark for what plays and a dim one for what's set.
    let centred = bar_spans(0x80, 0xC0, true);
    assert_eq!(text(&centred), "    │ │  ");
    assert_eq!(style_at(&centred, 4), theme::faint());
    assert_eq!(style_at(&centred, 6).fg, Some(theme::MAUVE));
    // A small move that stays in the set value's cell keeps the bright mark.
    let near = bar_spans(0x80, 0x82, true);
    assert_eq!(text(&near), "    │    ");
    assert_eq!(style_at(&near, 4).fg, Some(theme::MAUVE));
}

#[test]
fn a_bar_moves_at_the_same_pace_through_the_mark_as_either_side_of_it() {
    use vt_tui::views::widgets::bar::{ValueBar, level_bar};
    let text = |set, live| -> String {
        ValueBar::new(set, 9)
            .playing(live)
            .spans()
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    };
    for set in 0..=u8::MAX {
        for live in 0..u8::MAX {
            // Each step the plain bar takes shows, wherever the mark is.
            if level_bar(live, 9) != level_bar(live + 1, 9) {
                assert_ne!(
                    text(set, live),
                    text(set, live + 1),
                    "set {set:02X}, from {live:02X}"
                );
            }
        }
    }
    // A slow rise across a mark at 80: the edge goes through its cell, with no mark to hide it.
    let frames =
        [0x70, 0x74, 0x78, 0x7B, 0x7E, 0x82, 0x85, 0x89, 0x8D, 0x92].map(|live| text(0x80, live));
    assert_eq!(
        frames,
        [
            "███▉│    ",
            "████│    ",
            "████▏    ",
            "████▎    ",
            "████▍    ",
            "████▌    ",
            "████▋    ",
            "████▊    ",
            "████▉    ",
            "████│▏   ",
        ]
    );
}

#[test]
fn the_page_draws_bars_from_live_values_and_leaves_the_numbers_alone() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    let cut = |app: &App| {
        track_page_text(app)
            .into_iter()
            .find(|line| line.starts_with("Cut"))
            .unwrap()
    };
    assert!(cut(&app).contains("FF00 ▕█████████▏"), "{}", cut(&app));
    app.live = Some(vt_engine::track_state::LiveValues {
        level: 0xE0,
        pan: 0x80,
        cutoff: 0x40,
        resonance: 0,
        send_a: 0,
        send_b: 0,
        machine: [0x80, 0, 0x80, 0xCC, 0, 0, 0, 0],
        inserts: Default::default(),
        reduction: Default::default(),
        bypassed: Default::default(),
    });
    assert!(cut(&app).contains("FF00 ▕██▎     │▏"), "{}", cut(&app));
}

#[test]
fn the_ends_of_bars_line_up_down_each_column() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    for mark in ['▕', '▏'] {
        let mut columns: Vec<usize> = track_text(&app)
            .iter()
            .flat_map(|line| {
                line.chars()
                    .enumerate()
                    .filter(|(_, c)| *c == mark)
                    .map(|(x, _)| x)
                    .collect::<Vec<_>>()
            })
            .collect();
        columns.sort_unstable();
        columns.dedup();
        assert_eq!(columns.len(), 2, "{mark}: {columns:?}");
    }
}

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
fn a_missing_kit_says_so_and_a_missing_kit_folder_isnt_listed() {
    let (mut app, _realtime) = app();
    app.settings = Settings {
        kits: vec!["/nowhere/kits".to_owned(), "/nowhere/else".to_owned()],
        samples: Vec::new(),
    };
    command(&mut app, "kit nothing-here");
    assert!(
        app.status.contains("/nowhere/kits/nothing-here.ron"),
        "{}",
        app.status
    );
    command(&mut app, "kit");
    assert_eq!(app.mode, Mode::Picker);
    let roots = explorer_roots(&app);
    assert!(
        roots.iter().all(|(_, meaning)| meaning != "kits"),
        "{roots:?}"
    );
    assert_eq!(
        roots.last().unwrap().1,
        "home",
        "the home folder is always there"
    );
}

#[test]
fn kits_come_from_every_folder_in_the_settings_and_the_first_wins() {
    let (mut app, _realtime) = app();
    let first = kit_file();
    let second = kit_file();
    std::fs::rename(
        second.with_file_name("909.ron"),
        second.with_file_name("808.ron"),
    )
    .unwrap();
    let second = second.with_file_name("808.ron");
    // A kit of the same name further down is passed over.
    let shadowed = folder();
    std::fs::create_dir_all(&shadowed).unwrap();
    std::fs::write(shadowed.join("909.ron"), "(sounds: [])").unwrap();
    let folders = [first.parent().unwrap(), &shadowed, second.parent().unwrap()];
    app.settings = Settings {
        kits: folders.iter().map(|f| f.display().to_string()).collect(),
        samples: Vec::new(),
    };

    command(&mut app, "kit 808");
    assert!(
        app.status.starts_with("Kit 808: 3 sounds"),
        "{}",
        app.status
    );
    command(&mut app, "kit 909");
    assert!(
        app.status.starts_with("Kit 909: 3 sounds"),
        "{}",
        app.status
    );
    // The explorer starts from each kit folder, in order, on the kit in use.
    command(&mut app, "kit");
    let roots = explorer_roots(&app);
    let kit_roots: Vec<&str> = roots
        .iter()
        .filter(|(_, meaning)| meaning == "kits")
        .map(|(label, _)| label.as_str())
        .collect();
    let named: Vec<String> = folders.iter().map(|f| f.display().to_string()).collect();
    assert_eq!(kit_roots, named);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().pick, Pick::Kit(first));
    assert_eq!(picker.marked(), picker.current_index());
}

/// A kit folder with a kit of three labelled sounds at its top, and one of Ableton's under its
/// category, returning the folder.
fn kit_folders() -> PathBuf {
    let kit = labelled_kit();
    let root = kit.parent().unwrap().to_path_buf();
    let deeper = root.join("Ableton/Drum Machines");
    std::fs::create_dir_all(&deeper).unwrap();
    std::fs::write(
        deeper.join("808 Core Kit.ron"),
        r#"(source: "Ableton Core Library", category: "Drum Machines", sounds: [
            (label: "BD", name: "Kick 808", path: "../../drums/kick.wav"),
            (label: "SD", name: "Snare 808", path: "../../drums/snare.wav"),
        ])"#,
    )
    .unwrap();
    std::fs::write(
        deeper.join("909 Core Kit.ron"),
        r#"(sounds: [(path: "../../drums/hat.wav")])"#,
    )
    .unwrap();
    root
}

#[test]
fn space_8_explores_the_kit_folders_with_each_kits_sounds_beside_them() {
    let (mut app, _realtime) = app();
    let root = kit_folders();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
    };
    press(&mut app, " 8");
    assert_eq!(app.mode, Mode::Picker);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.title, "Kits");
    assert!(picker.hint.unwrap().contains("L on tracks"));
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v kits", "  > Ableton", "  > drums", "    909"]
    );
    // The kit under the cursor is shown beside the list, each sound with its waveform.
    press(&mut app, "jj");
    let picker = app.picker.as_ref().unwrap();
    let shown = picker.preview.as_deref().unwrap();
    assert_eq!(shown.heading, "909");
    assert_eq!(shown.note, "3 sounds");
    let labels: Vec<&str> = shown.lines.iter().map(|line| line.label.as_str()).collect();
    assert_eq!(labels, ["BD", "CH", ""]);
    assert!(shown.lines.iter().all(|line| line.wave.is_some()));

    // Into the folders, and the preview follows.
    press(&mut app, "kkll");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "808 Core Kit");
    let shown = picker.preview.as_deref().unwrap();
    assert_eq!(shown.heading, "808 Core Kit");
    assert_eq!(shown.note, "2 sounds from Ableton Core Library");
    assert_eq!(shown.lines[1].name, "Snare 808");

    // Enter makes it the kit Space 5 lists first, and adds nothing.
    let depth = app.history.undo_depth();
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.kit.len(), 2);
    assert_eq!(app.history.undo_depth(), depth);
    assert!(
        app.status.starts_with("Kit 808 Core Kit: 2 sounds"),
        "{}",
        app.status
    );
    assert_eq!(
        app.recent_places.of(vt_tui::explorer::Purpose::Kit),
        [root.join("Ableton/Drum Machines")],
        "its folder is a recent place"
    );

    // It opens on the kit in use, with its folder at the top as a recent place, and L puts its
    // sounds on tracks, as one undo step.
    command(&mut app, "kit");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(
        picker.current().unwrap().label,
        "808 Core Kit",
        "opens on it"
    );
    assert_eq!(picker.marked(), picker.current_index());
    assert_eq!(explorer_roots(&app)[0].1, "recent");
    press(&mut app, "L");
    assert_eq!(app.mode, Mode::Normal);
    assert!(sample_of(&app, 0).unwrap().ends_with("kick.wav"));
    assert!(sample_of(&app, 1).unwrap().ends_with("snare.wav"));
    assert_eq!(app.song().tracks[1].name, "SD");
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(sample_of(&app, 0), None);
}

#[test]
fn a_kits_preview_draws_its_sounds_with_their_waveforms() {
    let (mut app, _realtime) = app();
    let root = kit_folders();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
    };
    press(&mut app, " 8jj");
    // The terminal is sent a waveform for each sound.
    let mut sent = Vec::new();
    let mut graphics = vt_tui::kitty::Graphics::new();
    graphics.sync(&mut app, &mut sent).unwrap();
    let mut picker = app.picker.clone().unwrap();
    assert_eq!(picker.preview_images.len(), 3);
    assert!(picker.preview_images.iter().all(Option::is_some));
    let backend = ratatui::backend::TestBackend::new(120, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::picker::draw(frame, frame.area(), &mut picker))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = (0..24)
        .map(|y| {
            (0..120)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect();
    assert!(text.contains("╭ Kits ─"), "{text}");
    assert!(
        text.contains("Enter use  L on tracks  H \u{f204}\u{a0}hidden  P \u{f205}\u{a0}preview"),
        "{text}"
    );
    assert!(text.contains("│ 909"), "{text}");
    let bd = text.lines().find(|line| line.contains(" BD  ")).unwrap();
    assert!(bd.contains('\u{10EEEE}'), "its waveform beside it: {bd}");
    // Moving to another kit sends that kit's, and lets the old ones go.
    press(&mut app, "kkll");
    sent.clear();
    graphics.sync(&mut app, &mut sent).unwrap();
    let sent = String::from_utf8_lossy(&sent);
    assert_eq!(sent.matches("a=T").count(), 2, "one for each of its sounds");
}

#[test]
fn space_5_explores_the_sample_folders_and_opens_on_the_tracks_own() {
    let (mut app, _realtime) = app();
    let library = folder();
    std::fs::create_dir_all(library.join("hats")).unwrap();
    std::fs::create_dir_all(library.join("kicks")).unwrap();
    wav(&library.join("hats/closed.wav"));
    wav(&library.join("hats/open.wav"));
    wav(&library.join("kicks/deep.wav"));
    wav(&library.join("kicks/short.wav"));
    app.settings = Settings {
        kits: Vec::new(),
        samples: vec![library.display().to_string()],
    };
    press(&mut app, " 5");
    assert_eq!(
        explorer_rows(&app)[..3],
        ["v samples", "  > hats", "  > kicks"],
        "the first place open, on the list"
    );
    press(&mut app, "jl/short");
    special(&mut app, KeyCode::Enter);
    assert!(sample_of(&app, 0).unwrap().ends_with("kicks/short.wav"));
    press(&mut app, " 5");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(
        picker.current().unwrap().label,
        "short.wav",
        "on the track's own"
    );
    assert_eq!(picker.marked(), picker.current_index());
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v recent", "    deep.wav", "    short.wav", "> samples"],
        "in its folder, at the top as the last place a sample came from"
    );
    std::fs::remove_dir_all(&library).unwrap();
}

#[test]
fn the_sample_explorer_starts_from_the_project_the_settings_here_and_home() {
    let (mut app, _realtime) = app();
    let library = folder();
    std::fs::create_dir_all(library.join("hats")).unwrap();
    wav(&library.join("hats/closed.wav"));
    let project = folder();
    command(&mut app, &format!("w {}", project.display()));
    app.settings = Settings {
        kits: Vec::new(),
        samples: vec![library.display().to_string(), "/nowhere/samples".to_owned()],
    };
    command(&mut app, "sample");
    assert_eq!(app.mode, Mode::Picker);
    let meanings: Vec<String> = explorer_roots(&app)
        .into_iter()
        .map(|(_, meaning)| meaning)
        .collect();
    assert_eq!(
        meanings,
        ["project", "samples", "here", "home"],
        "a folder that isn't there is passed over"
    );
    std::fs::remove_dir_all(&library).unwrap();
    std::fs::remove_dir_all(&project).unwrap();
}

#[test]
fn kits_in_folders_are_found_in_the_explorer_and_named_by_their_path() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    let root = kit.parent().unwrap();
    let deeper = root.join("Ableton/Drum Machines");
    std::fs::create_dir_all(&deeper).unwrap();
    let text = std::fs::read_to_string(&kit)
        .unwrap()
        .replace("drums/", "../../drums/");
    std::fs::write(deeper.join("808 Core Kit.ron"), text).unwrap();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
    };
    command(&mut app, "kit");
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v kits", "  > Ableton", "  > drums", "    909"],
        "the first kit folder open, its folders first"
    );
    press(&mut app, "ll");
    assert_eq!(
        explorer_rows(&app)[..5],
        [
            "v kits",
            "  v Ableton",
            "    v Drum Machines",
            "        808 Core Kit",
            "  > drums"
        ]
    );
    special(&mut app, KeyCode::Esc);
    command(&mut app, "kit Ableton/Drum Machines/808 Core Kit all");
    assert_eq!(app.status, "Kit 808 Core Kit: 3 tracks");
}

#[test]
fn the_sample_explorer_opens_a_kit_like_a_folder_and_plays_and_chooses_its_sounds() {
    let (mut app, _realtime) = app();
    let root = kit_folders();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
    };
    press(&mut app, " 5");
    assert_eq!(explorer_roots(&app)[0].1, "kits");
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v kits", "  > Ableton", "  > drums", "  > 909"],
        "a kit is a folder, after the real ones"
    );
    // Down to a kit, by its name, and into it.
    press(&mut app, "ll");
    assert_eq!(
        app.picker.as_ref().unwrap().current().unwrap().label,
        "808 Core Kit"
    );
    press(&mut app, "l");
    assert_eq!(
        explorer_rows(&app)[3..6],
        ["      v 808 Core Kit", "          BD", "          SD"]
    );
    let picker = app.picker.as_ref().unwrap();
    let bd = picker.current().unwrap();
    assert_eq!((bd.label.as_str(), bd.meaning.as_str()), ("BD", "Kick 808"));

    // It plays as the cursor moves onto it, with its waveform beside the list.
    app.finish_loading();
    assert!(
        app.file_playing
            .as_ref()
            .is_some_and(|path| path.ends_with("drums/kick.wav")),
        "{:?}",
        app.file_playing
    );
    let shown = app.picker.as_ref().unwrap().preview.clone().unwrap();
    assert_eq!(shown.heading, "kick.wav");
    assert!(shown.wave.is_some());

    // Hidden files shown keep the kit open and the cursor on its sound.
    press(&mut app, "jH");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "SD");
    press(&mut app, "H");

    // Choosing it is choosing from the kit in use: its label names the track.
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert!(sample_of(&app, 0).unwrap().ends_with("drums/snare.wav"));
    assert_eq!(app.song().tracks[0].name, "SD");
    assert_eq!(app.song().bank.get(1).unwrap().title(), "SD Snare 808");
}

#[test]
fn a_kit_sound_opens_to_its_variants_and_layers_and_a_missing_one_says_so() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    let root = kit.parent().unwrap().to_path_buf();
    std::fs::write(
        root.join("layered.ron"),
        r#"(name: "Layered", sounds: [
            (label: "BD", name: "Kick", path: "drums/kick.wav", variants: ["drums/snare.wav"],
             layers: ["drums/hat.wav", "drums/kick.wav"], gain: -3.0, choke: Some(2)),
            (label: "GH", name: "Ghost", path: "drums/gone.wav"),
        ])"#,
    )
    .unwrap();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
    };
    press(&mut app, " 5jjl");
    assert_eq!(
        explorer_rows(&app)[3..6],
        ["  v Layered", "    > BD", "      GH"],
        "a sound with variants or layers opens"
    );
    // l opens it: its own file, then its variants, then its layers from the softest.
    press(&mut app, "l");
    let picker = app.picker.as_ref().unwrap();
    let files: Vec<String> = picker
        .rows()
        .iter()
        .filter_map(|row| match row {
            vt_tui::input::picker::Row::Item(item) if picker.depth(*item) == 3 => {
                let item = picker.item(*item);
                Some(format!("{}  {}", item.label, item.meaning))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        files,
        [
            "kick.wav  Main",
            "snare.wav  Variant 1",
            "hat.wav  Layer 1 soft",
            "kick.wav  Layer 2 hard"
        ]
    );
    // A variant is chosen as the sound is, with its gain, label and choke group.
    press(&mut app, "j");
    special(&mut app, KeyCode::Enter);
    assert!(sample_of(&app, 0).unwrap().ends_with("drums/snare.wav"));
    assert_eq!(app.song().tracks[0].name, "BD");
    assert_eq!(app.song().tracks[0].choke, Some(2));
    assert_eq!(app.song().bank.get(1).unwrap().edit.play.gain_db, -3.0);
    press(&mut app, "u");

    // Enter on the sound itself chooses its own file, where l opens it.
    press(&mut app, " 5jjl");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "BD");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert!(sample_of(&app, 0).unwrap().ends_with("drums/kick.wav"));
    press(&mut app, "u");

    // A sound whose file has gone is listed, and says why as a missing file does.
    press(&mut app, " 5jjlj");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "GH");
    app.finish_loading();
    let shown = app.picker.as_ref().unwrap().preview.clone().unwrap();
    assert_eq!(shown.heading, "gone.wav");
    assert!(shown.note.contains("gone.wav"), "{}", shown.note);
    special(&mut app, KeyCode::Enter);
    assert_eq!(sample_of(&app, 0), None);
    assert!(app.status.contains("gone.wav"), "{}", app.status);
}

/// The icon of the first row shown with this name.
fn icon_of(app: &App, label: &str) -> vt_tui::explorer::Icon {
    use vt_tui::input::picker::Row;
    let picker = app.picker.as_ref().expect("the explorer is open");
    picker
        .rows()
        .iter()
        .find_map(|row| match row {
            Row::Item(item) if picker.item(*item).label == label => picker.icon(*item),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no row {label}"))
}

/// Where the explorer draws a glyph, and its colour.
fn glyph_colour(app: &App, glyph: char) -> Option<Color> {
    let backend = ratatui::backend::TestBackend::new(163, 42);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut picker = app.picker.clone().unwrap();
    terminal
        .draw(|frame| vt_tui::views::picker::draw(frame, frame.area(), &mut picker))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let at = buffer
        .content
        .iter()
        .position(|cell| cell.symbol() == glyph.to_string())?;
    let after = &buffer.content[at + 1];
    assert_eq!(
        after.symbol(),
        "\u{a0}",
        "a no-break space keeps it one cell"
    );
    Some(buffer.content[at].fg)
}

#[test]
fn the_explorer_shows_what_each_row_is_with_an_icon() {
    use vt_tui::explorer::Icon;
    use vt_tui::glyphs;
    let (mut app, _realtime) = app();
    let kit = kit_file();
    let root = kit.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(root.join("Loops")).unwrap();
    wav(&root.join("Loops/break.wav"));
    wav(&root.join("Fill 120 bpm.wav"));
    std::fs::write(root.join("broken.wav"), "not a sound").unwrap();
    std::fs::write(
        root.join("layered.ron"),
        r#"(name: "Layered", sounds: [
            (label: "BD", name: "Kick", path: "drums/kick.wav", layers: ["drums/hat.wav"]),
            (label: "SD", name: "Snare", path: "drums/snare.wav"),
            (label: "GH", name: "Ghost", path: "drums/gone.wav"),
        ])"#,
    )
    .unwrap();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
    };
    press(&mut app, " 5");
    assert_eq!(
        explorer_rows(&app)[..7],
        [
            "v kits",
            "  > drums",
            "  > Loops",
            "  > 909",
            "    broken.wav",
            "    Fill 120 bpm.wav",
            "  > Layered",
        ]
    );
    for (label, icon) in [
        ("drums", Icon::Folder),
        ("909", Icon::Kit),
        ("Layered", Icon::Kit),
        ("broken.wav", Icon::Sample),
        ("Fill 120 bpm.wav", Icon::Loop),
    ] {
        assert_eq!(icon_of(&app, label), icon, "{label}");
    }
    // A folder's icon is open or closed, in place of an arrow, and dim.
    assert_eq!(
        glyph_colour(&app, glyphs::FOLDER_OPEN),
        Some(vt_tui::theme::OVERLAY2)
    );
    assert_eq!(
        glyph_colour(&app, glyphs::FOLDER),
        Some(vt_tui::theme::OVERLAY2)
    );
    assert!(!picker_text(&app, 163, 42).contains('▸'), "no arrows");

    // A kit's sounds are pads, stacked where they open to more, and a warning where the file has
    // gone, in the error colour.
    press(&mut app, "5jl");
    assert_eq!(icon_of(&app, "BD"), Icon::KitSoundLayered);
    assert_eq!(icon_of(&app, "SD"), Icon::KitSound);
    assert_eq!(icon_of(&app, "GH"), Icon::Missing);
    assert_eq!(
        glyph_colour(&app, glyphs::MISSING),
        Some(vt_tui::theme::RED)
    );
    press(&mut app, "l");
    assert_eq!(
        icon_of(&app, "hat.wav"),
        Icon::Sample,
        "a layer is a sample"
    );
    press(&mut app, "hh");

    // A file that won't read is warned of once the explorer has tried it.
    press(&mut app, "kk");
    assert_eq!(
        app.picker.as_ref().unwrap().current().unwrap().label,
        "broken.wav"
    );
    app.finish_loading();
    assert_eq!(icon_of(&app, "broken.wav"), Icon::Missing);

    // The sample in use is marked by its icon in the colour of what's current.
    press(&mut app, "j");
    special(&mut app, KeyCode::Enter);
    assert!(sample_of(&app, 0).unwrap().ends_with("Fill 120 bpm.wav"));
    press(&mut app, " 5");
    assert_eq!(glyph_colour(&app, glyphs::LOOP), Some(vt_tui::theme::GREEN));
}

#[test]
fn the_sample_explorer_lists_the_kits_and_the_library_and_no_other_ron_files() {
    let (mut app, _realtime) = app();
    let root = kit_folders();
    let library = folder();
    std::fs::create_dir_all(library.join("Loops")).unwrap();
    wav(&library.join("Loops/break.wav"));
    let project = folder();
    command(&mut app, &format!("w {}", project.display()));
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
    };
    app.library = Some(library.clone());
    press(&mut app, " 5");
    let meanings: Vec<String> = explorer_roots(&app)
        .into_iter()
        .map(|(_, meaning)| meaning)
        .collect();
    assert_eq!(meanings, ["project", "kits", "library", "here", "home"]);
    // The project's own song isn't a kit, so it isn't listed.
    let rows = explorer_rows(&app);
    assert!(!rows.iter().any(|row| row.contains("song")), "{rows:?}");
    std::fs::remove_dir_all(&library).unwrap();
    std::fs::remove_dir_all(&project).unwrap();
}

#[test]
fn a_sample_is_referred_to_rather_than_copied() {
    let project = folder();
    let (mut app, _realtime) = app();
    command(&mut app, &format!("w {}", project.display()));
    let source = folder();
    std::fs::create_dir_all(&source).unwrap();
    let tone = source.join("tone.wav");
    wav(&tone);
    command(&mut app, &format!("sample {}", tone.display()));
    assert_eq!(
        sample_of(&app, 0).unwrap(),
        tone.canonicalize().unwrap().display().to_string()
    );
    assert!(!project.join("samples").exists());
    assert!(app.status.contains("loaded"), "{}", app.status);
}

#[test]
fn a_sample_inside_the_project_stays_relative_so_a_bundle_can_move() {
    let project = folder();
    let inside = project.join("samples");
    std::fs::create_dir_all(&inside).unwrap();
    wav(&inside.join("tone.wav"));
    let (mut app, _realtime) = app();
    command(&mut app, &format!("w {}", project.display()));
    command(&mut app, &format!("sample {}/tone.wav", inside.display()));
    assert_eq!(sample_of(&app, 0).unwrap(), "samples/tone.wav");

    std::fs::write(
        project.join("kit.ron"),
        r#"(sounds: [(path: "samples/tone.wav")])"#,
    )
    .unwrap();
    command(&mut app, &format!("kit {}/kit.ron", project.display()));
    assert_eq!(sample_of(&app, 0).unwrap(), "samples/tone.wav");
}

#[test]
fn a_bundle_keeps_working_after_its_sources_are_gone() {
    let source = folder();
    std::fs::create_dir_all(&source).unwrap();
    let tone = source.join("tone.wav");
    wav(&tone);
    let (mut app, _realtime) = app();
    command(&mut app, &format!("sample {}", tone.display()));
    let bundle = folder();
    command(&mut app, &format!("w {}", bundle.display()));
    std::fs::remove_dir_all(&source).unwrap();

    let song = vt_model::project::load_song(&bundle).unwrap();
    let mut pool = SamplePool::default();
    let errors = vt_model::project::load_samples(Some(&bundle), &song, 48_000, &mut pool);
    assert!(errors.is_empty());
    assert_eq!(file_of(&song, 0).as_deref(), Some("samples/tone.wav"));
}

/// Run the audio side long enough to act on what was sent, and return the loudest sample.
fn loudest(realtime: &mut Realtime, frames: usize) -> f32 {
    let mut out = vec![0.0_f32; frames * 2];
    realtime.process(&mut out);
    out.iter().fold(0.0, |a, b| a.max(b.abs()))
}

fn with_a_sound(app: &mut App) {
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tone.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for _ in 0..48_000 {
        writer.write_sample(16_000_i16).unwrap();
    }
    writer.finalize().unwrap();
    command(app, &format!("sample {}", path.display()));
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

    press(&mut app, "k;");
    assert!(
        loudest(&mut rt, 2000) > 0.01,
        "the phrase view auditions a row"
    );
    assert!(Arc::ptr_eq(&version, app.song()));
    assert_eq!(app.transport, Transport::Stopped);

    press(&mut app, "j;");
    let _ = loudest(&mut rt, 4800);
    assert_eq!(loudest(&mut rt, 2000), 0.0, "an empty row is silent");

    press(&mut app, " 2;");
    assert!(
        loudest(&mut rt, 2000) > 0.01,
        "the chain view auditions its row"
    );
}

/// Press a key now, and let it go after this many milliseconds.
fn hold_for(app: &mut App, c: char, milliseconds: u64) {
    let now = Instant::now();
    app.key(Key::char(c), now);
    app.key_released(Key::char(c), now + Duration::from_millis(milliseconds));
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

fn labelled_kit() -> PathBuf {
    let kit = kit_file();
    std::fs::write(
        &kit,
        r#"(sounds: [
            (label: "BD", path: "drums/kick.wav"),
            (label: "CH", path: "drums/hat.wav"),
            (path: "drums/snare.wav"),
        ])"#,
    )
    .unwrap();
    kit
}

#[test]
fn replace_shows_its_mode_and_a_red_cursor_with_no_box() {
    let (mut app, _realtime) = app();
    press(&mut app, "r");
    assert_eq!(app.mode, Mode::Replace);
    let buffer = screen(&app);
    assert_eq!(buffer[(9, 2)].bg, vt_tui::theme::RED);
    assert_eq!(buffer[(8, 2)].symbol(), " ");
    press(&mut app, "a");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(screen(&app)[(9, 2)].bg, vt_tui::theme::ROSEWATER);
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

/// A row of the phrase view as text, up to the end of its number.
fn row_number_at(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
    (3..6).map(|x| buffer[(x, y)].symbol()).collect()
}

#[test]
fn rows_count_from_the_loop_start_and_dim_outside_it() {
    let (mut app, _realtime) = app();
    command(&mut app, "len 4");
    press(&mut app, "2]l");
    // A slide keeps row 00 where it was on screen, so scroll up to see what is above it.
    app.top = 0;
    let buffer = screen(&app);
    assert_eq!(row_number_at(&buffer, 2), "-02");
    assert_eq!(row_number_at(&buffer, 3), "-01");
    assert_eq!(row_number_at(&buffer, 4), " 00");
    assert_eq!(row_number_at(&buffer, 7), " 03");
    assert_eq!(row_number_at(&buffer, 8), " 04");
    assert_eq!(buffer[(5, 4)].fg, vt_tui::theme::YELLOW, "the loop start");
    assert_eq!(buffer[(5, 5)].fg, vt_tui::theme::SUBTEXT1);
    assert_eq!(buffer[(5, 2)].fg, vt_tui::theme::OVERLAY0, "not played");
    assert_eq!(
        buffer[(5, 8)].fg,
        vt_tui::theme::OVERLAY2,
        "a beat, not played"
    );
    assert_eq!(
        buffer[(9, 8)].fg,
        vt_tui::theme::OVERLAY0,
        "the step's text"
    );
    assert_eq!(
        buffer[(9, 6)].fg,
        vt_tui::theme::OVERLAY0,
        "an empty cell is faint where it plays too"
    );
}

#[test]
fn other_tracks_are_blank_above_row_00_and_keep_going_past_this_loop() {
    let song = Song::new(2);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    command(&mut app, "len 4");
    press(&mut app, "]L");
    app.top = 0;
    let buffer = screen(&app);
    let neighbour = |y: u16| -> String { (72..75).map(|x| buffer[(x, y)].symbol()).collect() };
    assert_eq!(neighbour(2), "   ", "above the loop");
    assert_eq!(neighbour(6), "---", "row 00 has the other track's row 00");
    assert_eq!(
        neighbour(10),
        "---",
        "the other track is longer than this loop"
    );
}

#[test]
fn the_playhead_marks_where_the_loop_is_playing() {
    let (mut app, _realtime) = app();
    command(&mut app, "len 4");
    press(&mut app, "2]L");
    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    app.set_playhead(1);
    app.top = 0;
    let buffer = screen(&app);
    // The loop starts at stored step 8, so row 01 is on the screen's row 11.
    assert_eq!(row_number_at(&buffer, 11), " 01");
    assert_eq!(buffer[(7, 11)].symbol(), ">");
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

/// A selected 4-step track beside a 2-step track and an 8-step track, whose steps are numbered by
/// their notes.
fn tracks_of_different_lengths() -> (App, Realtime) {
    let mut song = Song::new(3);
    for (id, length) in [(0, 4), (1, 2), (2, 8)] {
        let mut phrase = Phrase::new(length);
        for step in 0..length {
            *phrase.step_mut(step).unwrap() = Step {
                note: Note::On(60 + step as u8),
                ..Step::default()
            };
        }
        song.tracks[id].phrases.insert(0, Arc::new(phrase));
    }
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

/// Another track's column on one row of steps: its marker and note, and its colour.
/// A column of the overview past the track being edited, which is the overview's first here.
fn beside(buffer: &ratatui::buffer::Buffer, column: u16, row: u16) -> (String, Color) {
    let x = 71 + column * 4;
    let text = (x..x + 4).map(|x| buffer[(x, row + 2)].symbol()).collect();
    (text, buffer[(x + 1, row + 2)].fg)
}

#[test]
fn other_tracks_show_their_own_length_and_playhead() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    let (dim, green) = (vt_tui::theme::OVERLAY2, vt_tui::theme::GREEN);
    let note = |text: &str, color| (text.to_owned(), color);

    // Stopped, nothing is marked. The 2-step track ends early, and the 8-step one has rows past
    // the selected track's 4.
    let buffer = screen(&app);
    assert_eq!(beside(&buffer, 0, 1), note(" C#4", dim));
    assert_eq!(beside(&buffer, 0, 2).0, "    ");
    assert_eq!(beside(&buffer, 1, 3), note(" D#4", dim));
    assert_eq!(beside(&buffer, 1, 7), note(" G-4", dim));

    on_the_row_being_edited(&mut app);
    // Each track marks where it is in its own loop.
    app.transport = Transport::Playing;
    app.set_playhead(5);
    let buffer = screen(&app);
    assert_eq!(beside(&buffer, 0, 1), note(">C#4", green));
    assert_eq!(beside(&buffer, 0, 3).0, "    ");
    assert_eq!(beside(&buffer, 1, 5), note(">F-4", green));
    assert_eq!(beside(&buffer, 1, 1), note(" C#4", dim));

    app.transport = Transport::Paused;
    assert_eq!(beside(&screen(&app), 1, 5), note(">F-4", green));
}

#[test]
fn a_column_shows_the_phrase_its_chain_is_on() {
    let mut song = Song::new(2);
    song.tracks[0].phrases.insert(0, Arc::new(Phrase::new(4)));
    for (id, midi) in [(1, 60), (2, 64)] {
        let mut phrase = Phrase::new(2);
        *phrase.step_mut(0).unwrap() = Step {
            note: Note::On(midi),
            ..Step::default()
        };
        song.tracks[1].phrases.insert(id, Arc::new(phrase));
    }
    let entry = |phrase| ChainEntry {
        phrase,
        transpose: 0,
    };
    let entries = vec![entry(1), entry(2)];
    song.tracks[1].chains.insert(
        1,
        Arc::new(Chain {
            entries,
            label: String::new(),
        }),
    );
    song.sections[0].chains[1] = Some(1);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    assert_eq!(beside(&screen(&app), 0, 0).0, " C-4");

    // Stopped, it follows the cursor, as that's where playing would start.
    app.cursor.step = 2;
    assert_eq!(beside(&screen(&app), 0, 0).0, " E-4");
    app.cursor.step = 0;

    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    app.set_playhead(2);
    assert_eq!(beside(&screen(&app), 0, 0).0, ">E-4");
}

#[test]
fn rows_past_a_short_phrase_keep_the_box_and_have_nothing_above_the_loop() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    let time = Instant::now();
    app.key(Key::char('i'), time);
    let buffer = screen(&app);
    // Row 04 is past the selected track's end: it closes the box beside the longer track.
    let text = |row: u16| -> String {
        (0..65)
            .map(|x| buffer[(x, row + 2)].symbol())
            .collect::<String>()
    };
    assert!(text(3).contains('│'), "{}", text(3));
    assert!(text(4).contains('└'), "{}", text(4));
    assert!(text(5).contains(" 05"), "{}", text(5));
    assert_eq!(beside(&buffer, 1, 4).0, " E-4");
    assert_eq!(beside(&buffer, 1, 5).0, " F-4");
    app.key(Key::new(KeyCode::Esc), time);

    // A row above the selected track's loop start is above every other track's too.
    press(&mut app, "]l");
    app.top = 0;
    let buffer = screen(&app);
    assert_eq!(beside(&buffer, 1, 0).0, "    ");
    assert_eq!(beside(&buffer, 1, 1).0, " C-4");
}

/// An app on the track page.
fn on_track_page() -> (App, Realtime) {
    let (mut app, realtime) = tracks_of_different_lengths();
    // On Cut, as the rows above it aren't all values.
    press(&mut app, " 4jjjjjjjj");
    assert_eq!(app.view, View::Track);
    (app, realtime)
}

/// Start typing a value, type it and enter it.
fn typed(app: &mut App, text: &str) {
    press(app, "i");
    press(app, text);
    special(app, KeyCode::Enter);
}

/// Everything the track page draws, a line at a time.
fn track_page_text(app: &App) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..16)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

#[test]
fn the_track_page_sets_base_values_and_undoes() {
    let (mut app, _realtime) = on_track_page();
    assert_eq!(app.song().tracks[0].cutoff, 0xFF);
    press(&mut app, "i40");
    assert_eq!(app.song().tracks[0].cutoff, 0x40);
    assert_eq!(app.mode, Mode::Normal);
    press(&mut app, "3-");
    assert_eq!(app.song().tracks[0].cutoff, 0x3D);
    press(&mut app, "u");
    assert_eq!(app.song().tracks[0].cutoff, 0x40);

    // The output row, on the next track.
    press(&mut app, "W");
    press(&mut app, "ggjjl");
    press(&mut app, "i20");
    assert_eq!(app.song().tracks[1].pan, 0x20);
    assert_eq!(app.song().tracks[0].pan, 0x80);
    press(&mut app, "ki7F");
    assert_eq!(app.song().tracks[1].level, 0x7F);
}

#[test]
fn levels_snap_to_the_top_of_the_scale_and_filter_values_do_not() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    // Cut isn't a level, and Res isn't either.
    press(&mut app, "i10iFF");
    assert_eq!(track(&app).cutoff, 0xFF);
    // Level, by typing and by nudging.
    press(&mut app, "ggjli");
    press(&mut app, "FF");
    assert_eq!(track(&app).level, 0xF8);
    press(&mut app, "20-");
    assert_eq!(track(&app).level, 0xE4);
    press(&mut app, "99=");
    assert_eq!(track(&app).level, 0xF8);
    command(&mut app, "tvol FF");
    assert_eq!(track(&app).level, 0xF8);
    // Pan runs to FF, and the sends stop at the top.
    press(&mut app, "jiFF");
    assert_eq!(track(&app).pan, 0xFF);
    press(&mut app, "jiFF");
    assert_eq!(track(&app).send_a, 0xF8);
    press(&mut app, "jiFF");
    assert_eq!(track(&app).send_b, 0xF8);
}

#[test]
fn the_track_page_types_lfo_settings() {
    let (mut app, _realtime) = on_track_page();
    let lfo = |app: &App| app.song().tracks[0].lfos[0];
    // Down to LFO 1, on Dest.
    press(&mut app, "Ljj0");
    press(&mut app, "=");
    assert_eq!(lfo(&app).destination, Some(SettingId::CUT));
    press(&mut app, "-");
    assert_eq!(lfo(&app).destination, None);
    press(&mut app, "-");
    assert_eq!(
        lfo(&app).destination,
        Some(SettingId::lfo(1, 1).unwrap()),
        "the last for LFO 1 on a sample track"
    );

    press(&mut app, "l=");
    assert_eq!(lfo(&app).shape, Shape::Saw);

    press(&mut app, "l");
    typed(&mut app, "1/8d");
    assert_eq!(lfo(&app).rate, Rate::Note(720));
    typed(&mut app, "7st");
    assert_eq!(lfo(&app).rate, Rate::Steps(7));
    typed(&mut app, "2.5hz");
    assert_eq!(lfo(&app).rate, Rate::Hertz(250));
    typed(&mut app, "fast");
    assert_eq!(lfo(&app).rate, Rate::Hertz(250));
    assert!(app.status.contains("1/8d"), "{}", app.status);

    press(&mut app, "l");
    typed(&mut app, "-40");
    assert_eq!(lfo(&app).depth, -0x40);
    typed(&mut app, "+7F");
    assert_eq!(lfo(&app).depth, 0x7F);
    typed(&mut app, "100");
    assert_eq!(lfo(&app).depth, 0x7F);

    press(&mut app, "l");
    typed(&mut app, "90");
    assert_eq!(lfo(&app).phase, 90);
    typed(&mut app, "360");
    assert_eq!(lfo(&app).phase, 90);
    press(&mut app, "3-");
    assert_eq!(lfo(&app).phase, 45);
    press(&mut app, "4-");
    assert_eq!(lfo(&app).phase, 345);

    press(&mut app, "l=");
    assert!(lfo(&app).retrig);
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker, "i lists a setting that is a list");
    special(&mut app, KeyCode::Esc);

    // Nothing else moved.
    assert_eq!(app.song().tracks[0].lfos[1], Lfo::default());
}

#[test]
fn k_chooses_an_lfos_destination_from_a_list() {
    let (mut app, _realtime) = on_track_page();
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Normal, "not on a Dest");
    assert!(app.status.contains("Dest"), "{}", app.status);

    // The second LFO's Dest, on the next track.
    press(&mut app, "W");
    press(&mut app, "Ljjj0K/");
    assert_eq!(app.mode, Mode::Picker);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(
        picker.items().len(),
        14,
        "what a sample track has, its Sample, Slice, Start and Length, and none"
    );
    assert_eq!(picker.selected, 0, "what it moves now");
    press(&mut app, "res");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.view, View::Track);
    assert_eq!(
        app.song().tracks[1].lfos[1].destination,
        Some(SettingId::RES)
    );
    assert_eq!(app.song().tracks[1].lfos[0], Lfo::default());
    assert_eq!(app.song().tracks[0].lfos[1], Lfo::default());

    // It opens on what's set, and leaves the other settings alone. Esc changes nothing.
    typed_rate_on_second_lfo(&mut app);
    press(&mut app, "0K");
    assert_eq!(
        listed(app.picker.as_ref().unwrap().current().unwrap()),
        "Res  Filter resonance"
    );
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.song().tracks[1].lfos[1].destination,
        Some(SettingId::RES)
    );
    press(&mut app, "K/pan");
    special(&mut app, KeyCode::Enter);
    let lfo = app.song().tracks[1].lfos[1];
    assert_eq!(lfo.destination, Some(SettingId::PAN));
    assert_eq!(lfo.rate, Rate::Steps(7));

    press(&mut app, "u");
    assert_eq!(
        app.song().tracks[1].lfos[1].destination,
        Some(SettingId::RES)
    );
}

fn typed_rate_on_second_lfo(app: &mut App) {
    press(app, "ll");
    typed(app, "7st");
}

#[test]
fn the_track_page_sets_the_filters_type_and_drive() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    // Down to Drive, then up to Type.
    press(&mut app, "jj");
    press(&mut app, "i40");
    assert_eq!(track(&app).drive, 0x40);
    press(&mut app, "+");
    assert_eq!(track(&app).drive, 0x50);
    press(&mut app, "iFF");
    assert_eq!(track(&app).drive, 0xFF, "drive isn't on the level scale");
    press(&mut app, "x");
    assert_eq!(track(&app).drive, 0);

    press(&mut app, "kkkk");
    assert_eq!(track(&app).filter, FilterType::LowPass);
    press(&mut app, "=");
    assert_eq!(track(&app).filter, FilterType::HighPass);
    press(&mut app, "-");
    assert_eq!(track(&app).filter, FilterType::LowPass);
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 9);
    press(&mut app, "/ladder");
    special(&mut app, KeyCode::Enter);
    assert_eq!(track(&app).filter, FilterType::Ladder);
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker, "i lists it too");
    assert_eq!(
        app.picker.as_ref().unwrap().selected,
        4,
        "opens on what's set"
    );
    special(&mut app, KeyCode::Esc);
    assert!(track_page_text(&app)[9].contains("Ladder"));
    press(&mut app, "u");
    assert_eq!(track(&app).filter, FilterType::LowPass);
    press(&mut app, "K");
    press(&mut app, "ms20 hp");
    special(&mut app, KeyCode::Enter);
    assert_eq!(track(&app).filter, FilterType::Ms20HighPass);
    assert!(track_page_text(&app)[9].contains("MS20 HP"));
    press(&mut app, "K");
    press(&mut app, "dirty");
    special(&mut app, KeyCode::Enter);
    assert_eq!(track(&app).filter, FilterType::Dirty);
    assert!(track_page_text(&app)[9].contains("Dirty"));
}

#[test]
fn the_track_page_sets_the_filters_mode_for_its_type() {
    use vt_model::song::FilterMode;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    press(&mut app, "k");
    assert_eq!(column_name(&app), "Mode");
    assert!(track_page_text(&app)[10].contains("12 dB"));
    press(&mut app, "=");
    assert_eq!(track(&app).filter_mode, FilterMode::Slope24);
    press(&mut app, "=");
    assert_eq!(
        track(&app).filter_mode,
        FilterMode::Slope12,
        "round the two"
    );

    // The ladder lists its six, and starts on its own first.
    press(&mut app, "kK");
    press(&mut app, "/ladder");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "j");
    assert!(track_page_text(&app)[10].contains("LP24"));
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 6);
    press(&mut app, "/hp24");
    special(&mut app, KeyCode::Enter);
    assert_eq!(track(&app).filter_mode, FilterMode::HighPass24);

    // Another type that offers it keeps it, and one that doesn't plays its first, until it's back.
    press(&mut app, "kK");
    press(&mut app, "dirty");
    special(&mut app, KeyCode::Enter);
    assert!(track_page_text(&app)[10].contains("HP24"));
    // Round from the last type to `LP`.
    press(&mut app, "=");
    assert_eq!(track(&app).filter, FilterType::LowPass);
    assert!(track_page_text(&app)[10].contains("12 dB"));
    assert_eq!(track(&app).filter_mode, FilterMode::HighPass24);
    press(&mut app, "K");
    press(&mut app, "ms20 lp");
    special(&mut app, KeyCode::Enter);
    assert!(track_page_text(&app)[10].contains("--"));
    press(&mut app, "j=");
    assert!(app.status.contains("no modes"), "{}", app.status);
    press(&mut app, "K");
    assert_ne!(app.mode, Mode::Picker);
    press(&mut app, "kK");
    press(&mut app, "/ladder");
    special(&mut app, KeyCode::Enter);
    assert!(track_page_text(&app)[10].contains("HP24"));

    // `x` puts it back to the first.
    press(&mut app, "jx");
    assert_eq!(track(&app).filter_mode, FilterMode::default());
    assert!(track_page_text(&app)[10].contains("LP24"));
}

#[test]
fn the_gap_above_the_filter_is_only_on_screen() {
    use vt_tui::track_page::Control;
    let (mut app, _realtime) = on_track_page();
    press(&mut app, "kkk");
    assert_eq!(
        app.track_control(),
        Control::Setting(4),
        "the sampler's Play"
    );
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::FilterType);

    // A synth has more above its filter, and the gap stays just above it.
    press(&mut app, "kkkkkkK");
    press(&mut app, "/kick");
    special(&mut app, KeyCode::Enter);
    let lines = track_page_text(&app);
    assert!(lines[5].starts_with("Decay"), "{}", lines[5]);
    assert!(lines[6].starts_with("Accent"), "{}", lines[6]);
    assert!(lines[7].trim().is_empty(), "{}", lines[7]);
    assert!(lines[8].starts_with("Filter"), "{}", lines[8]);
}

#[test]
fn the_track_page_sets_pitch_and_a_choke_group() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    let lines = track_page_text(&app);
    assert!(
        lines[9].contains("Pitch") && lines[9].contains("80"),
        "{}",
        lines[9]
    );
    assert!(
        lines[11].contains("Choke") && lines[11].contains("Off"),
        "{}",
        lines[11]
    );

    // Beside Filter.
    press(&mut app, "kk$");
    assert_eq!(app.track_control().label(), "Pitch");
    press(&mut app, "i8C");
    assert_eq!(track(&app).pitch, 0x8C);
    press(&mut app, "x");
    assert_eq!(track(&app).pitch, 0x80);

    // Down past Groove.
    press(&mut app, "jj");
    assert_eq!(app.track_control().label(), "Choke");
    press(&mut app, "=");
    assert_eq!(track(&app).choke, Some(1));
    press(&mut app, "3=");
    assert_eq!(track(&app).choke, Some(4));
    typed(&mut app, "16");
    assert_eq!(track(&app).choke, Some(16));
    press(&mut app, "=");
    assert_eq!(track(&app).choke, Some(16), "16 is the last");
    typed(&mut app, "17");
    assert_eq!(track(&app).choke, Some(16));
    assert!(app.status.contains("1 to 16"), "{}", app.status);
    assert!(track_page_text(&app)[11].contains("16"));
    typed(&mut app, "off");
    assert_eq!(track(&app).choke, None);
    press(&mut app, "u");
    assert_eq!(track(&app).choke, Some(16));
    press(&mut app, "x");
    assert_eq!(track(&app).choke, None);
    press(&mut app, "=-");
    assert_eq!(track(&app).choke, None, "down from 1 is none");
}

#[test]
fn the_track_page_sets_how_long_notes_last() {
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let gate = |app: &App| app.song().tracks[0].gate_length;
    let line = |app: &App| {
        track_page_text(app)
            .into_iter()
            .find(|line| line.contains("Gate"))
            .unwrap_or_default()
    };
    assert!(
        line(&app).contains("Gate") && line(&app).contains("100%"),
        "{}",
        line(&app)
    );

    // Beside Res.
    press(&mut app, "j$");
    assert_eq!(app.track_control().label(), "Gate");
    assert_eq!(value_shown(&app).as_deref(), Some("100 %"));
    press(&mut app, "-");
    assert_eq!(gate(&app), Some(252), "a percent down");
    press(&mut app, "_");
    assert_eq!(gate(&app), Some(227), "ten more");
    assert!(line(&app).contains("89%"), "{}", line(&app));
    typed(&mut app, "25");
    assert_eq!(gate(&app), Some(0x40));
    assert_eq!(value_shown(&app).as_deref(), Some("25 %"));
    typed(&mut app, "50%");
    assert_eq!(gate(&app), Some(0x80));
    typed(&mut app, "0");
    assert_eq!(gate(&app), Some(0x80));
    assert!(app.status.contains("1 to 100"), "{}", app.status);
    press(&mut app, "200-");
    assert_eq!(gate(&app), Some(gate_value(1)), "1% is the least");
    press(&mut app, "x");
    assert_eq!(gate(&app), None, "back to the machine's");
    assert!(line(&app).contains("100%"), "{}", line(&app));
    press(&mut app, "u");
    assert_eq!(gate(&app), Some(gate_value(1)));
    press(&mut app, "x");

    // `Acid`'s own is 50%, as a 303's is.
    press(&mut app, "kkkkkkkkkhK");
    press(&mut app, "/acid");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::Acid)
    );
    assert!(
        line(&app).contains("Gate") && line(&app).contains("50%"),
        "{}",
        line(&app)
    );
}

#[test]
fn a_step_locks_the_gate_by_its_label() {
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, "llllli");
    press(&mut app, "len");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "40");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("gate:40"), "{step}");
    assert_eq!(column_name(&app), "Machine: Gate");
    assert_eq!(value_shown(&app).as_deref(), Some("25 %"));
}

#[test]
fn a_kit_sound_is_a_bank_entry_with_its_gain_and_tune_and_its_track_takes_the_choke_group() {
    use vt_model::bank::PlayParams;
    let (mut app, _realtime) = app();
    let kit = kit_file();
    let hats = kit.parent().unwrap().join("hats.ron");
    std::fs::write(
        &hats,
        r#"(sounds: [
            (label: "CH", name: "Closed", path: "drums/hat.wav", choke: Some(1), gain: -6.0,
             tune: 3),
            (label: "OH", path: "drums/snare.wav", choke: Some(1)),
        ])"#,
    )
    .unwrap();
    let track = |app: &App, index: usize| {
        let track = &app.song().tracks[index];
        (track.sample(), track.level, track.pitch, track.choke)
    };
    command(&mut app, &format!("kit {} all", hats.display()));
    assert_eq!(
        track(&app, 0),
        (1, 0xE0, 0x80, Some(1)),
        "the track's own stay"
    );
    assert_eq!(track(&app, 1), (2, 0xE0, 0x80, Some(1)));
    let closed = app.song().bank.get(1).unwrap();
    assert_eq!(
        (closed.label.as_str(), closed.name.as_str()),
        ("CH", "Closed")
    );
    assert_eq!(
        closed.edit.play,
        PlayParams {
            gain_db: -6.0,
            tune: 3,
            ..PlayParams::default()
        }
    );
    assert_eq!(app.song().bank.get(2).unwrap().title(), "OH snare");

    // Choosing one for a track puts it in the bank with the track's choke group and name, as
    // one step to undo.
    press(&mut app, "u");
    assert!(app.song().bank.is_empty());
    assert_eq!(track(&app, 0), (0, 0xE0, 0x80, None));
    command(&mut app, &format!("kit {}", hats.display()));
    let depth = app.history.undo_depth();
    press(&mut app, " 5");
    special(&mut app, KeyCode::Enter);
    assert_eq!(track(&app, 0), (1, 0xE0, 0x80, Some(1)));
    assert_eq!(app.song().bank.get(1).unwrap().edit.play.tune, 3);
    assert_eq!(app.song().tracks[0].name, "CH");
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(track(&app, 0), (0, 0xE0, 0x80, None));
    assert_eq!(sample_of(&app, 0), None);
    assert!(app.song().bank.is_empty());
}

#[test]
fn a_kit_fills_the_bank_once_and_a_file_tuned_twice_is_two_entries() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    let toms = kit.parent().unwrap().join("toms.ron");
    std::fs::write(
        &toms,
        r#"(sounds: [
            (label: "LT", path: "drums/kick.wav", tune: -5),
            (label: "HT", path: "drums/kick.wav", tune: 5),
            (label: "BD", path: "drums/kick.wav", tune: 5),
        ])"#,
    )
    .unwrap();
    command(&mut app, &format!("kit {} all", toms.display()));
    let numbers: Vec<u8> = (0..3).map(|at| app.song().tracks[at].sample()).collect();
    assert_eq!(
        numbers,
        [1, 2, 2],
        "the same file played the same way is one entry"
    );
    assert_eq!(app.song().bank.len(), 2);
    assert_eq!(app.samples.len(), 1, "and the file is loaded once");

    // Loading it again adds nothing.
    command(&mut app, &format!("kit {} all", toms.display()));
    assert_eq!(app.song().bank.len(), 2);

    // Nor does choosing the file from the explorer, which finds the entry it's in.
    let kick = kit.parent().unwrap().join("drums/kick.wav");
    press(&mut app, "W");
    command(&mut app, &format!("sample {}", kick.display()));
    assert_eq!(app.song().bank.len(), 2);
    assert!(app.status.contains("from the bank"), "{}", app.status);
}

#[test]
fn the_sample_row_lists_the_bank_steps_through_it_and_clears() {
    use vt_tui::track_page::Control;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    let kit = kit_file();
    command(&mut app, &format!("kit {} all", kit.display()));
    press(&mut app, "gg");
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Sample);
    let lines = track_page_text(&app);
    assert!(
        lines[3].starts_with("Sample") && lines[3].contains("01 snare"),
        "{}",
        lines[3]
    );
    assert_eq!(value_shown(&app).as_deref(), Some("01 snare: snare.wav"));

    press(&mut app, "=");
    assert_eq!(track(&app).sample(), 2);
    press(&mut app, "2=");
    assert_eq!(track(&app).sample(), 3, "it stops at the last");
    press(&mut app, "x");
    assert_eq!(track(&app).sample(), 0);
    assert_eq!(value_shown(&app).as_deref(), Some("Off: no sample"));
    press(&mut app, "-");
    assert_eq!(track(&app).sample(), 0);

    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(|item| item.label.clone())
        .collect();
    assert_eq!(
        labels,
        ["00  Off", "01  snare", "02  kick", "03  hat", "Add a file"]
    );
    press(&mut app, "/hat");
    special(&mut app, KeyCode::Enter);
    assert_eq!(track(&app).sample(), 3);
}

#[test]
fn a_step_locks_a_sample_from_the_bank() {
    use vt_model::step::FxCommand;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = app();
    let kit = kit_file();
    command(&mut app, &format!("kit {} all", kit.display()));
    press(&mut app, " 3");
    let slot = |app: &App| app.phrase_here().unwrap().steps()[0].fx[0];
    press(&mut app, "lllllK");
    press(&mut app, "smp");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.mode,
        Mode::Picker,
        "an SMP with no value lists the bank"
    );
    press(&mut app, "/kick");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(slot(&app).command, Some(FxCommand::Set(SettingId::SAMPLE)));
    assert_eq!(slot(&app).coarse(), 2);
    assert_eq!(value_shown(&app).as_deref(), Some("02 kick: kick.wav"));
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("sampler.sample:02"), "{step}");

    // K on its value lists the bank again, on the one it locks.
    press(&mut app, "K");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "02  kick");
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).coarse(), 1);
    // And it's on the list to lock, and for an LFO or an envelope to move.
    let track = &app.song().tracks[0];
    let sample = FxCommand::Set(SettingId::SAMPLE);
    assert!(FxCommand::on_track(track).contains(&sample));
    assert!(SettingId::for_lfo(track, 0).contains(&SettingId::SAMPLE));
    assert!(SettingId::for_envelope(track).contains(&SettingId::SAMPLE));
    assert_eq!(FxCommand::typed("smp", track), Some(sample));
}

#[test]
fn cleaning_the_bank_removes_what_nothing_plays_and_keeps_the_numbers() {
    use vt_model::action::Action;
    let (mut app, _realtime) = app();
    let kit = kit_file();
    command(&mut app, &format!("kit {} all", kit.display()));
    assert_eq!(app.song().bank.len(), 3);
    // Track 2 plays nothing now, but a step on track 1 locks the hat.
    let phrase = app.song().tracks[0].chains[&0].entries[0].phrase;
    app.apply(&Action::SetTrackSample {
        track: 1,
        sample: 0,
    });
    app.apply(&Action::SetTrackSample {
        track: 2,
        sample: 0,
    });
    let step: Step = "C-4 -- sampler.sample:03 --- ---".parse().unwrap();
    app.apply(&Action::SetStep {
        track: 0,
        phrase,
        step: 4,
        value: step,
    });
    command(&mut app, "bank clean");
    let left: Vec<u8> = app.song().bank.iter().map(|(number, _)| number).collect();
    assert_eq!(left, [1, 3]);
    assert!(app.status.contains("Removed 1 sample"), "{}", app.status);
    command(&mut app, "bank clean");
    assert_eq!(app.status, "Every sample in the bank is played");
    press(&mut app, "u");
    assert_eq!(app.song().bank.len(), 3);
}

#[test]
fn the_track_page_sets_the_utility_filter() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    assert_eq!((track(&app).low_cut, track(&app).high_cut), (0, 0xFF));
    let lines = track_page_text(&app);
    assert!(lines[14].contains("Low cut") && lines[14].contains("00"));
    assert!(lines[15].contains("High cut") && lines[15].contains("FF"));

    press(&mut app, "jjj");
    press(&mut app, "i40");
    assert_eq!(track(&app).low_cut, 0x40);
    press(&mut app, "x");
    assert_eq!(track(&app).low_cut, 0);
    press(&mut app, "j");
    press(&mut app, "i80");
    assert_eq!(track(&app).high_cut, 0x80);
    press(&mut app, "=");
    assert_eq!(track(&app).high_cut, 0x81);
    press(&mut app, "+");
    assert_eq!(track(&app).high_cut, 0x91);
    press(&mut app, "x");
    assert_eq!(track(&app).high_cut, 0xFF);
    press(&mut app, "u");
    assert_eq!(track(&app).high_cut, 0x91);
    assert_eq!(app.track_control().label(), "High cut");
}

#[test]
fn cmd_with_the_movement_keys_changes_the_value_without_moving() {
    let (mut app, _realtime) = on_track_page();
    let attack = |app: &App| app.song().tracks[0].envelopes[0].attack_ms;
    press(&mut app, "L0l");
    typed(&mut app, "20");
    let label = app.track_control().label();
    with_cmd(&mut app, "D-l");
    assert_eq!(attack(&app), 25, "as =");
    with_cmd(&mut app, "D-k");
    assert_eq!(attack(&app), 75, "as +");
    with_cmd(&mut app, "D-h");
    assert_eq!(attack(&app), 70, "as -");
    with_cmd(&mut app, "D-j");
    assert_eq!(attack(&app), 20, "as _");
    assert_eq!(app.track_control().label(), label, "the cursor stays");
    // Option no longer adjusts.
    alt(&mut app, "M-l");
    assert_eq!(attack(&app), 20, "Option is unbound");
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
    press(&mut app, " 6");
    with_cmd(&mut app, "D-l");
    assert_eq!(app.song().tracks[0].level, 0xE1);
    with_cmd(&mut app, "D-j");
    assert_eq!(app.song().tracks[0].level, 0xD1);
}

#[test]
fn a_held_cmd_l_keeps_adjusting() {
    use ratatui::crossterm::event::KeyEventKind;
    let (mut app, _realtime) = on_track_page();
    let attack = |app: &App| app.song().tracks[0].envelopes[0].attack_ms;
    press(&mut app, "L0l");
    typed(&mut app, "20");
    let key = parse_sequence("D-l").unwrap()[0];
    let now = Instant::now();
    app.key_event(key, KeyEventKind::Press, now);
    app.key_event(key, KeyEventKind::Repeat, now);
    // kitty on macOS sends a held key's repeats as presses.
    app.key_event(key, KeyEventKind::Press, now);
    app.key_event(key, KeyEventKind::Release, now);
    assert_eq!(attack(&app), 35, "three steps of 5");
}

#[test]
fn the_track_page_sets_envelopes() {
    let (mut app, _realtime) = on_track_page();
    let env = |app: &App, slot: usize| app.song().tracks[0].envelopes[slot];
    // Env 1, on its Attack.
    press(&mut app, "L0l");
    typed(&mut app, "20");
    assert_eq!(env(&app, 0).attack_ms, 20);
    press(&mut app, "=");
    assert_eq!(env(&app, 0).attack_ms, 25);
    press(&mut app, "+");
    assert_eq!(env(&app, 0).attack_ms, 75);
    typed(&mut app, "9000");
    assert_eq!(env(&app, 0).attack_ms, 75);
    assert!(app.status.contains("8000"), "{}", app.status);
    typed(&mut app, "8000");
    assert_eq!(env(&app, 0).attack_ms, 8000);
    press(&mut app, "=");
    assert_eq!(env(&app, 0).attack_ms, 8000);
    press(&mut app, "x");
    assert_eq!(env(&app, 0).attack_ms, 1);

    press(&mut app, "l");
    typed(&mut app, "180");
    assert_eq!(env(&app, 0).decay_ms, 180);
    press(&mut app, "l");
    typed(&mut app, "40");
    assert_eq!(env(&app, 0).sustain, 40);
    typed(&mut app, "101");
    assert_eq!(env(&app, 0).sustain, 40);
    assert!(app.status.contains("percent"), "{}", app.status);
    press(&mut app, "+");
    assert_eq!(env(&app, 0).sustain, 50);
    press(&mut app, "l");
    typed(&mut app, "300");
    assert_eq!(env(&app, 0).release_ms, 300);

    // Env 2, its depth in hex and its destination from a list.
    press(&mut app, "j0K");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(
        listed(app.picker.as_ref().unwrap().current().unwrap()),
        "Cut  Filter cutoff",
        "opens on what's chosen"
    );
    press(&mut app, "/pitch");
    special(&mut app, KeyCode::Enter);
    assert_eq!(env(&app, 1).destination, Some(SettingId::PITCH));
    press(&mut app, "lllll");
    typed(&mut app, "+18");
    assert_eq!(env(&app, 1).depth, 0x18);
    press(&mut app, "lK");
    press(&mut app, "off");
    special(&mut app, KeyCode::Enter);
    assert!(!env(&app, 1).reset);
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker, "i lists a setting that is a list");
    special(&mut app, KeyCode::Esc);

    // Each is one undo, and the first envelope was left alone.
    press(&mut app, "u");
    assert!(env(&app, 1).reset);
    assert_eq!(env(&app, 0).release_ms, 300);
}

#[test]
fn the_track_page_draws_each_envelopes_shape() {
    use vt_tui::kitty::{Graphics, id_color};
    let (mut app, _realtime) = on_track_page();
    let mut graphics = Graphics::new();
    let page = |app: &mut App, graphics: &mut Graphics| {
        graphics.sync(app, &mut Vec::new()).unwrap();
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let text = |buffer: &ratatui::buffer::Buffer| {
        (0..24)
            .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
    };
    press(&mut app, "L");
    let drawn = page(&mut app, &mut graphics);
    let lines = text(&drawn);
    let heading = lines.iter().position(|l| l.contains("Env 1  Vol")).unwrap();
    assert!(lines[heading].contains("Env 2  Cut"));
    let first = app.envelope_images.unwrap()[0];
    assert_eq!(drawn[(0, heading as u16 + 1)].fg, id_color(first));

    // Changing a value sends a new image, which the placeholders point to.
    press(&mut app, "gg0lll");
    typed(&mut app, "0");
    let changed = page(&mut app, &mut graphics);
    assert_ne!(changed[(0, heading as u16 + 1)].fg, id_color(first));
    assert_eq!(app.song().tracks[0].envelopes[0].sustain, 0);

    // The first envelope on Vol has no depth to show, and off it, it does.
    assert!(text(&page(&mut app, &mut graphics))[3].contains("--"));
    press(&mut app, "0K/cut");
    special(&mut app, KeyCode::Enter);
    assert!(text(&page(&mut app, &mut graphics))[3].contains("00"));
}

#[test]
fn every_list_setting_on_the_track_page_has_a_picker() {
    let (mut app, _realtime) = on_track_page();
    let lfo = |app: &App| app.song().tracks[0].lfos[0];
    let choose = |app: &mut App, keys: &str| {
        press(app, "K");
        assert_eq!(app.mode, Mode::Picker);
        press(app, keys);
        special(app, KeyCode::Enter);
        assert_eq!(app.mode, Mode::Normal);
    };
    press(&mut app, "Ljj0");
    choose(&mut app, "/cut");
    assert_eq!(lfo(&app).destination, Some(SettingId::CUT));

    press(&mut app, "l");
    choose(&mut app, "square");
    assert_eq!(lfo(&app).shape, Shape::Square);

    press(&mut app, "l");
    assert_eq!(app.mode, Mode::Normal);
    choose(&mut app, "1/8d");
    assert_eq!(lfo(&app).rate, Rate::Note(720));

    // Retrig is three along, past Depth and Phase, which aren't lists.
    press(&mut app, "l");
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.contains("Dest"), "{}", app.status);
    press(&mut app, "ll");
    choose(&mut app, "on");
    assert!(lfo(&app).retrig);
    assert_eq!(lfo(&app).shape, Shape::Square, "the others are left alone");

    // The picker opens on what's set.
    press(&mut app, "0lK");
    assert_eq!(app.picker.as_ref().unwrap().selected, 4);
    special(&mut app, KeyCode::Esc);

    // On a list setting, i opens it too. On Rate, i types.
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "li");
    assert_eq!(app.mode, Mode::Insert);
    special(&mut app, KeyCode::Esc);
}

#[test]
fn large_steps_on_the_track_page_suit_each_setting() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    press(&mut app, "_");
    assert_eq!(track(&app).cutoff, 0xEF);
    press(&mut app, "ggjl");
    press(&mut app, "+");
    assert_eq!(track(&app).level, 0xF0);
    press(&mut app, "jjjjjj+");
    assert_eq!(
        track(&app).groove,
        GrooveId::from_index(1),
        "a groove moves one at a time"
    );
    press(&mut app, "Ljj0ll");
    press(&mut app, "+");
    assert_eq!(track(&app).lfos[0].rate, Rate::Note(480));
    press(&mut app, "_");
    assert_eq!(track(&app).lfos[0].rate, Rate::Note(960));
    press(&mut app, "l+");
    assert_eq!(track(&app).lfos[0].depth, 0x10);
    press(&mut app, "l+");
    assert_eq!(track(&app).lfos[0].phase, 90);
    press(&mut app, "_");
    assert_eq!(track(&app).lfos[0].phase, 0);
}

#[test]
fn k_chooses_a_chain_rows_phrase_and_large_steps_work_there() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2Ggd 2Ggd 2ggjdddd 2gg");
    assert_eq!(app.chain_entries().len(), 1);
    let row = |app: &App, slot: usize| app.chain_entries()[slot];
    let start = row(&app, 0).phrase;

    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    let picker = app.picker.as_ref().unwrap();
    let count = picker.shown().count();
    assert!(count >= 2);
    assert_eq!(picker.selected, start as usize);
    press(&mut app, "02");
    special(&mut app, KeyCode::Enter);
    assert_eq!(row(&app, 0).phrase, 2);

    // The empty row past the end adds a row.
    press(&mut app, "jK");
    press(&mut app, "01");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.chain_entries().len(), 2);
    assert_eq!(row(&app, 1).phrase, 1);

    // Transpose isn't a list, and moves by octaves.
    press(&mut app, "lK");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.contains("Phrase"), "{}", app.status);
    press(&mut app, "+");
    assert_eq!(row(&app, 1).transpose, 12);
    press(&mut app, "=");
    assert_eq!(row(&app, 1).transpose, 13);
    press(&mut app, "_");
    assert_eq!(row(&app, 1).transpose, 1);
}

#[test]
fn x_puts_track_page_settings_back_to_their_defaults() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    press(&mut app, "i40");
    press(&mut app, "x");
    assert_eq!(track(&app).cutoff, 0xFF);
    press(&mut app, "u");
    assert_eq!(track(&app).cutoff, 0x40, "one undo");
    press(&mut app, "x");

    press(&mut app, "ggl");
    press(&mut app, "i40x");
    assert_eq!(track(&app).level, 0xE0);
    press(&mut app, "ji40x");
    assert_eq!(track(&app).pan, 0x80);
    press(&mut app, "ji40x");
    assert_eq!(track(&app).send_a, 0);
    press(&mut app, "ji40x");
    assert_eq!(track(&app).send_b, 0);

    // Only the setting under the cursor goes back.
    press(&mut app, "Ljj0=");
    press(&mut app, "l=");
    press(&mut app, "l");
    typed(&mut app, "7st");
    press(&mut app, "l");
    typed(&mut app, "+40");
    press(&mut app, "l");
    typed(&mut app, "90");
    press(&mut app, "l=");
    let lfo = |app: &App| track(app).lfos[0];
    press(&mut app, "x");
    assert!(!lfo(&app).retrig);
    press(&mut app, "hx");
    assert_eq!(lfo(&app).phase, 0);
    press(&mut app, "hx");
    assert_eq!(lfo(&app).depth, 0);
    press(&mut app, "hx");
    assert_eq!(lfo(&app).rate, Lfo::default().rate);
    assert_eq!(lfo(&app).shape, Shape::Saw);
    press(&mut app, "hx");
    assert_eq!(lfo(&app).shape, Shape::Tri);
    assert_eq!(lfo(&app).destination, Some(SettingId::CUT));
    press(&mut app, "hx");
    assert_eq!(lfo(&app), Lfo::default());

    // Already there, it leaves no undo step.
    let depth = app.history.undo_depth();
    press(&mut app, "x");
    assert_eq!(app.history.undo_depth(), depth);
}

#[test]
fn a_track_following_the_project_shows_its_groove_dimmed_and_x_returns_to_that() {
    let (mut app, _realtime) = on_track_page();
    command(&mut app, "swing 60");
    let groove_cell = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(80, 16);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let at = (0..80)
            .position(|x| buffer[(x, 10)].symbol() == "G")
            .unwrap() as u16
            + 10;
        let text: String = (at..at + 8).map(|x| buffer[(x, 10)].symbol()).collect();
        (text, buffer[(at, 10)].fg)
    };
    // The cursor is elsewhere, so the colour is the setting's own.
    assert_eq!(app.song().tracks[0].groove, None);
    let dim = vt_tui::theme::OVERLAY2;
    assert_eq!(groove_cell(&app), ("Swing 60".to_owned(), dim));

    // It's the track's own once set, and a nudge starts from what it was playing.
    press(&mut app, "k$=");
    assert_eq!(app.song().tracks[0].groove, GrooveId::swing(62));
    press(&mut app, "k");
    assert_eq!(groove_cell(&app), ("Swing 62".to_owned(), Color::Reset));

    press(&mut app, "j$x");
    assert_eq!(app.song().tracks[0].groove, None);
    press(&mut app, "k");
    assert_eq!(groove_cell(&app), ("Swing 60".to_owned(), dim));
}

#[test]
fn plus_makes_a_rate_faster_of_any_kind() {
    let (mut app, _realtime) = on_track_page();
    press(&mut app, "Ljj0ll");
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].lfos[0].rate, Rate::Note(720));
    typed(&mut app, "7st");
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].lfos[0].rate, Rate::Steps(6));
    typed(&mut app, "2hz");
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].lfos[0].rate, Rate::Hertz(210));
}

#[test]
fn typing_a_value_and_long_words_keep_the_sound_pages_columns_in_line() {
    let (mut app, _realtime) = app();
    let line = |app: &App, name: &str| {
        track_page_text(app)
            .into_iter()
            .find(|line| line.starts_with(name))
            .unwrap_or_else(|| panic!("{name} in {:#?}", track_page_text(app)))
    };
    // The bar stays where it is while a value is typed, from its first key.
    press(&mut app, " 4jjji");
    assert!(
        line(&app, "Start").starts_with("Start     _ 00 ▕         ▏"),
        "{}",
        line(&app, "Start")
    );
    special(&mut app, KeyCode::Esc);
    // A choice's name starts where the bars do, and a word beside it keeps to its column.
    let play = line(&app, "Play");
    assert!(
        play.starts_with("Play      00   One-shot             Group     Off"),
        "{play}"
    );
}

#[test]
fn the_track_page_shows_its_settings_in_columns() {
    let (mut app, _realtime) = on_track_page();
    press(&mut app, "Ljj0=ll");
    typed(&mut app, "1/8d");
    press(&mut app, "l");
    typed(&mut app, "+40");
    press(&mut app, "l");
    typed(&mut app, "90");
    let lines = track_page_text(&app);
    assert!(lines[0].contains(" Track 01 "), "{}", lines[0]);
    assert!(lines[2].contains("Attack") && lines[2].contains("Reset"));
    assert!(lines[3].starts_with("Env 1") && lines[3].contains("Vol"));
    assert!(lines[4].starts_with("Env 2") && lines[4].contains("Cut"));
    assert!(lines[6].contains("Dest") && lines[6].contains("Retrig"));
    let lfo: Vec<&str> = lines[7].split_whitespace().collect();
    let triangle = vt_tui::glyphs::TRIANGLE_WAVE.to_string();
    assert_eq!(
        lfo,
        [
            "LFO", "1", "Cut", "Tri", &triangle, "1/8d", "+40", "90°", "off"
        ]
    );
    assert!(lines[8].starts_with("LFO 2"));
    press(&mut app, "H");
    let lines = track_page_text(&app);
    // The trim is at the top of the output, at 0 dB in the middle of its bar.
    assert!(lines[2].starts_with("Machine"), "{}", lines[2]);
    assert!(
        lines[2].contains("Trim      80   ▕    │    ▏"),
        "{}",
        lines[2]
    );
    assert!(lines[3].starts_with("Sample") && lines[3].contains("Level"));
    assert!(lines[4].starts_with("Slice     00   whole"), "{}", lines[4]);
    assert!(lines[4].contains("Pan"));
    assert!(
        lines[5].starts_with("Start     0000 ▕         ▏"),
        "{}",
        lines[5]
    );
    assert!(lines[5].contains("Send A"));
    assert!(
        lines[6].starts_with("Length    FF00 ▕█████████▏"),
        "{}",
        lines[6]
    );
    assert!(lines[6].contains("Send B"), "{}", lines[6]);
    assert!(lines[7].starts_with("Play") && lines[7].contains("One-shot"));
    assert!(lines[7].contains("Group     Off"), "{}", lines[7]);
    assert!(lines[8].trim().is_empty(), "{}", lines[8]);
    assert!(lines[9].starts_with("Filter") && lines[9].contains("Pitch"));
    assert!(lines[10].starts_with("Mode") && lines[10].contains("12 dB"));
    assert!(lines[10].contains("Groove"), "{}", lines[10]);
    assert!(lines[10].contains("Straight"), "{}", lines[10]);
    assert!(lines[11].starts_with("Cut") && lines[11].contains("FF00 ▕█████████▏"));
    assert!(lines[11].contains("Choke"), "{}", lines[11]);
    assert!(lines[12].starts_with("Res") && lines[12].contains("Gate"));
    assert!(lines[13].starts_with("Drive") && lines[13].contains("Speed"));
}

#[test]
fn a_tracks_groove_on_the_page_is_chosen_from_a_list() {
    use vt_tui::track_page::Control;
    let (mut app, _realtime) = on_track_page();
    let groove = |name: &str| GrooveId::named(name);
    press(&mut app, "gg$jjjjjjj");
    assert_eq!(app.track_control(), Control::Groove);
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "/pull");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].groove, groove("Pull"));
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].groove, groove("Custom 1"));
    press(&mut app, "x");
    assert_eq!(app.song().tracks[0].groove, None, "back to the project's");
    // `i` chooses from the list too, which starts with following the project's.
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].groove, None);
}

#[test]
fn the_shape_panel_draws_its_settings_beside_the_grid() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, "lvGS");
    let panel = app.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::shape::draw(frame, frame.area(), &app, &panel))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: Vec<String> = (0..16)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let all = text.join("\n");
    for expected in [
        "Shape",
        "Velocity: 4 of 4 rows",
        "From",
        "Curve",
        "Ramp",
        "Notes",
        "Enter keep",
    ] {
        assert!(all.contains(expected), "{expected} in {all}");
    }
    // The grid's own columns are left showing.
    assert!(text.iter().all(|line| line[..40].trim().is_empty()));
}

#[test]
fn a_list_fill_draws_its_values_in_place_of_the_ends() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, "lvGSK/list");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "ji704055");
    special(&mut app, KeyCode::Enter);
    let panel = app.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::shape::draw(frame, frame.area(), &app, &panel))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let all: String = (0..16)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in ["List", "Values  70 40 55", "Run", "~ reverse the list"] {
        assert!(all.contains(expected), "{expected} in {all}");
    }
    assert!(!all.contains("From"), "{all}");
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
fn a_chosen_command_shows_in_its_cell_while_its_value_is_typed() {
    let (mut app, _realtime) = app();
    let row = |app: &App| -> String {
        let buffer = screen(app);
        (0..80).map(|x| buffer[(x, 2)].symbol()).collect()
    };
    // From K on an empty cell, as from typing, the command waits for its value.
    press(&mut app, "lllllK");
    press(&mut app, "CUT");
    special(&mut app, KeyCode::Enter);
    assert!(row(&app).contains("CUT   00"), "{}", row(&app));
    assert_eq!(
        app.phrase_here().unwrap().steps()[0].fx[0].command,
        None,
        "not written yet"
    );
    press(&mut app, "4");
    assert!(row(&app).contains("CUT   4_"), "{}", row(&app));
    press(&mut app, "0");
    assert_eq!(
        app.phrase_here().unwrap().steps()[0].fx[0].command,
        Some(vt_model::step::FxCommand::CUT)
    );
    // Typing the name works the same way, over a value already there.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "khiRES");
    assert!(row(&app).contains("RES_"), "{}", row(&app));
    special(&mut app, KeyCode::Enter);
    assert!(row(&app).contains("RES   40"), "{}", row(&app));
    assert_eq!(
        app.phrase_here().unwrap().steps()[0].fx[0].command,
        Some(vt_model::step::FxCommand::CUT),
        "still the old one until the value"
    );
}

#[test]
fn a_value_shows_its_mark_and_events_and_empty_slots_show_none() {
    let mut song = Song::new(1);
    let step = |text: &str| text.parse::<Step>().unwrap();
    let phrase = Phrase::from_steps(vec![
        step("C-4 40 cut:40 sna:20h res:10g"),
        step("C-4 40 del:10 --- ---"),
    ]);
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let app = App::new(song, None, samples, handle, 48_000);
    let buffer = screen(&app);
    let row = |y: u16| -> String { (0..80).map(|x| buffer[(x, y)].symbol()).collect() };
    assert!(
        row(2).contains(&format!(
            "CUT   4000 {} SNA   2000 h RES   1000 g",
            vt_tui::glyphs::LOCK
        )),
        "{}",
        row(2)
    );
    assert!(
        row(3).contains("DEL   10     ---   00     ---   00"),
        "{}",
        row(3)
    );
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
fn the_status_line_names_the_column() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    assert_eq!(column_name(&app), "Note");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Velocity");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Nudge");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Gate");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Gate length, the track's");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "FX1 command");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "FX1 value");
    press(&mut app, " 2");
    assert_eq!(column_name(&app), "Phrase");
}

/// A sample track with a cutoff lock, a lock for String's Decay, which it doesn't play, and an
/// event.
fn app_with_a_lock_for_another_voice() -> (App, Realtime) {
    let mut song = Song::new(1);
    let decay = SettingId::voice(SynthVoice::String, 2).unwrap();
    assert_eq!(decay.name(), "Decay");
    let text = format!("C-4 40 cut:40 {}:20 del:10", decay.key());
    let phrase = Phrase::from_steps(vec![text.parse::<Step>().unwrap()]);
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

#[test]
fn the_status_line_names_the_setting_a_cell_locks() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app_with_a_lock_for_another_voice();
    press(&mut app, "lllll");
    assert_eq!(column_name(&app), "Filter: Cut");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Filter: Cut", "on its value too");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Filter: Cut", "and its fine digits");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "String: Decay, not on this track");
    press(&mut app, "lll");
    assert_eq!(column_name(&app), vt_model::step::FxCommand::Del.meaning());
}

#[test]
fn the_status_line_shows_a_value_in_its_units() {
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = app_with_a_lock_for_another_voice();
    press(&mut app, "l");
    assert_eq!(value_shown(&app), None, "a velocity has no units");
    press(&mut app, "l");
    assert_eq!(value_shown(&app).as_deref(), Some("on the grid"), "a nudge");
    press(&mut app, "l");
    assert_eq!(value_shown(&app), None, "a gate has no units");
    press(&mut app, "l");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("100 %"),
        "the track's Gate"
    );
    press(&mut app, "l");
    // `cut:40` is a quarter of the way up ten octaves from 20 Hz.
    assert_eq!(value_shown(&app).as_deref(), Some("113 Hz"));
    press(&mut app, "lll");
    assert_eq!(value_shown(&app), None, "not on this track");
    press(&mut app, "lll");
    assert_eq!(value_shown(&app), None, "an event has no units");
    press(&mut app, " 4");
    let mut rows = Vec::new();
    for _ in 0..12 {
        rows.extend(value_shown(&app));
        press(&mut app, "j");
    }
    assert!(
        rows.contains(&"20.0 kHz".to_owned()),
        "the open cutoff: {rows:?}"
    );
}

#[test]
fn each_kind_of_cell_has_its_own_colours() {
    use ratatui::style::Modifier;
    use vt_tui::theme;
    let (app, _realtime) = app_with_a_lock_for_another_voice();
    let buffer = screen(&app);
    let row: String = (0..80).map(|x| buffer[(x, 2)].symbol()).collect();
    // Counted in cells, as a value's lock mark is a glyph of more than one byte.
    let style_at = |text: &str, past: usize| {
        let at = row.find(text).unwrap_or_else(|| panic!("no {text}: {row}"));
        let at = row[..at].chars().count();
        buffer[((at + past) as u16, 2)].style()
    };
    // A lock: its coarse digits bold in the value colour, and its fine ones set back.
    let coarse = style_at("CUT   4000", 6);
    assert_eq!(coarse.fg, Some(theme::MAUVE), "{row}");
    assert!(coarse.add_modifier.contains(Modifier::BOLD));
    assert_eq!(
        style_at("CUT   4000", 8).fg,
        Some(theme::fine(theme::MAUVE))
    );
    // An event's value is in its own colour, with no fine digits.
    assert_eq!(style_at("DEL   10", 6).fg, Some(theme::PEACH));
    assert!(row.contains("DEL   10     "), "{row}");
    // A lock the track doesn't have is grey all over, brighter than an empty cell.
    assert_eq!(style_at("DCAY", 0).fg, theme::dim().fg);
    assert_eq!(style_at("DCAY  2000", 6).fg, theme::dim().fg);
    assert_eq!(theme::dim().fg, Some(vt_tui::theme::OVERLAY2));
}

#[test]
fn a_gate_cell_shows_its_own_length_dim_and_the_tracks_as_faint_dashes() {
    use vt_tui::theme;
    use vt_tui::views::status::{column_name, value_shown};
    let mut song = Song::new(1);
    let steps = [
        "C-4 40 SGL1 80 cut:40 --- ---",
        "C-4 40 HLD2 --- --- ---",
        "C-4 40 --- --- ---",
    ]
    .iter()
    .map(|text| text.parse().unwrap())
    .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    let buffer = screen(&app);
    let row = |y: u16| -> String { (9..40).map(|x| buffer[(x, y)].symbol()).collect() };
    assert!(
        row(2).starts_with("C-4 40    SGL1 80 CUT   4000"),
        "{}",
        row(2)
    );
    assert!(row(3).starts_with("C-4 40    HLD2 -- ---"), "{}", row(3));
    // The `HLD2` lasts two lines of the timeline.
    assert_eq!(row(4).trim_end(), " │", "{}", row(4));
    assert!(row(5).starts_with("C-4 40    ---- -- ---"), "{}", row(5));
    // The length is the cell's third part, after the gate and its pulses.
    let length = 9 + 15;
    assert_eq!(Some(buffer[(length, 2)].fg), theme::dim().fg);
    assert_ne!(
        Some(buffer[(length - 3, 2)].fg),
        theme::dim().fg,
        "the gate isn't dim"
    );
    assert_eq!(Some(buffer[(length, 3)].fg), theme::faint().fg);
    assert_eq!(Some(buffer[(length, 5)].fg), theme::faint().fg);

    press(&mut app, "llll");
    assert_eq!(column_name(&app), "Gate length");
    assert_eq!(value_shown(&app).as_deref(), Some("50 %"));
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Gate length, the track's");
    assert_eq!(value_shown(&app).as_deref(), Some("100 %"));
}

#[test]
fn a_nudge_shows_dim_by_the_velocity_only_on_a_row_that_has_one() {
    use vt_tui::theme;
    use vt_tui::views::status::{column_name, value_shown};
    let mut song = Song::new(1);
    let steps: Vec<Step> = [
        "C-4 40 -3 SGL1 80 cut:40 --- ---",
        "C-4 40 --- --- ---",
        "--- -- +B --- --- ---",
    ]
    .iter()
    .map(|text| text.parse().unwrap())
    .collect();
    let phrase = Phrase::from_steps(steps);
    song.tracks[0].phrases.insert(0, Arc::new(phrase.clone()));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    let buffer = screen(&app);
    let row = |y: u16| -> String { (9..40).map(|x| buffer[(x, y)].symbol()).collect() };
    assert!(
        row(2).starts_with("C-4 40 -3 SGL1 80 CUT   4000"),
        "{}",
        row(2)
    );
    assert!(row(3).starts_with("C-4 40    ---- -- ---"), "{}", row(3));
    assert!(row(4).starts_with("--- -- +B ---- -- ---"), "{}", row(4));
    let nudge = 9 + 7;
    assert_eq!(Some(buffer[(nudge, 2)].fg), theme::dim().fg);
    assert_eq!(Some(buffer[(nudge + 1, 4)].fg), theme::dim().fg);
    assert_ne!(
        Some(buffer[(nudge - 3, 2)].fg),
        theme::dim().fg,
        "the velocity isn't dim"
    );

    // It's a stop of its own, after the velocity.
    press(&mut app, "ll");
    assert_eq!(app.cursor.field, vt_tui::app::Field::Nudge);
    assert_eq!(column_name(&app), "Nudge");
    assert_eq!(value_shown(&app).as_deref(), Some("3/24 of a step early"));
    press(&mut app, "j");
    assert_eq!(value_shown(&app).as_deref(), Some("on the grid"));
    press(&mut app, "j");
    assert_eq!(value_shown(&app).as_deref(), Some("11/24 of a step late"));

    // A sign typed first shows in the cell until its digit comes.
    press(&mut app, "ki-");
    let buffer = screen(&app);
    let typed: String = (nudge..nudge + 2)
        .map(|x| buffer[(x, 3)].symbol())
        .collect();
    assert_eq!(typed, "-_");
    press(&mut app, "2");
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.phrase_here().unwrap().steps()[1].to_string(),
        "C-4 40 -2 --- --- ---"
    );

    // It's saved in the step's text, and a row without one is written as before.
    let saved = ron::to_string(&phrase).unwrap();
    assert!(saved.contains("C-4 40 -3 SGL1 80 cut:40"), "{saved}");
    assert!(saved.contains("\"C-4 40 --- --- ---\""), "{saved}");
    assert_eq!(ron::from_str::<Phrase>(&saved).unwrap(), phrase);
}

#[test]
fn k_lists_commands_by_category_and_opens_searching() {
    use vt_tui::input::picker::{Focus, Row};
    let (mut app, _realtime) = app();
    press(&mut app, "lllllK");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.focus, Focus::Search);
    assert!(picker.query.is_empty());
    let names: Vec<&str> = picker
        .rows()
        .iter()
        .filter_map(|row| match row {
            Row::Category(category) => Some(picker.category(*category).0),
            Row::Item(_) => None,
        })
        .collect();
    assert_eq!(names[..2], ["Filter", "Output"]);
    assert_eq!(names.last().copied(), Some("Events"));
    assert_eq!(picker.current().unwrap().label, "---", "on the empty slot");
    // Typing searches long names as well as labels.
    press(&mut app, "send b");
    let current = app.picker.as_ref().unwrap().current().unwrap();
    assert_eq!(
        (current.label.as_str(), current.meaning.as_str()),
        ("SNB", "Send B")
    );
}

#[test]
fn zero_and_dollar_pick_the_chain_columns() {
    use vt_tui::tree::ChainColumn;
    let (mut app, _realtime) = app();
    press(&mut app, " 2$");
    assert_eq!(app.chain_column, ChainColumn::Transpose);
    press(&mut app, "0");
    assert_eq!(app.chain_column, ChainColumn::Phrase);
}

#[test]
fn x_in_a_chain_only_resets_transpose() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2x");
    assert!(app.status.contains("dd"), "{}", app.status);
    assert_eq!(app.chain_entries().len(), 1);
    press(&mut app, "l+x");
    assert_eq!(app.chain_entries()[0].transpose, 0);
}

#[test]
fn a_kit_fills_tracks_from_the_first_as_one_undo_step() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    command(&mut app, &format!("kit {} all", kit.display()));
    assert!(sample_of(&app, 0).unwrap().ends_with("snare.wav"));
    assert!(sample_of(&app, 1).unwrap().ends_with("kick.wav"));
    assert_eq!(app.status, "Kit 909: 3 tracks");
    assert_eq!(app.song().sound_tracks(), 3, "it added the track it needed");
    assert_eq!(app.levels.len(), 3 + FIXED_TRACKS);
    assert!(app.samples.id_of(&sample_of(&app, 1).unwrap()).is_some());

    press(&mut app, "u");
    assert_eq!(sample_of(&app, 0), None);
    assert_eq!(app.song().sound_tracks(), 2);
}

#[test]
fn a_kit_names_tracks_keeps_their_settings_and_leaves_later_ones_alone() {
    let (mut app, _realtime) = app();
    command(&mut app, "track add 2");
    press(&mut app, "WWW");
    let other = folder();
    std::fs::create_dir_all(&other).unwrap();
    let later = other.join("later.wav");
    wav(&later);
    command(&mut app, &format!("sample {}", later.display()));
    let kit = labelled_kit();
    let before = app.song().tracks[0].cutoff;
    command(&mut app, &format!("kit {} all", kit.display()));
    assert_eq!(app.song().tracks[0].name, "BD");
    assert_eq!(app.song().tracks[1].name, "CH");
    assert_eq!(app.song().tracks[2].name, "");
    assert_eq!(app.song().tracks[0].cutoff, before);
    assert!(sample_of(&app, 3).unwrap().ends_with("later.wav"));
}

#[test]
fn a_kit_warns_only_when_it_changes_a_tracks_sample() {
    let (mut app, _realtime) = app();
    let kit = labelled_kit();
    command(&mut app, &format!("kit {} all", kit.display()));
    assert!(!app.status.contains("Replaced"), "{}", app.status);
    command(&mut app, &format!("kit {} all", kit.display()));
    assert!(
        !app.status.contains("Replaced"),
        "the same again: {}",
        app.status
    );
    std::fs::write(
        &kit,
        r#"(sounds: [(label: "BD", path: "drums/snare.wav")])"#,
    )
    .unwrap();
    command(&mut app, &format!("kit {} all", kit.display()));
    assert!(app.status.contains("Replaced 01"), "{}", app.status);
}

#[test]
fn a_kit_refuses_a_bad_name_and_changes_nothing() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    for (text, expected) in [
        (r#"(sounds: [(label: "1A", path: "drums/kick.wav")])"#, "1A"),
        (
            r#"(sounds: [(label: "BDX", path: "drums/kick.wav")])"#,
            "BDX",
        ),
    ] {
        std::fs::write(&kit, text).unwrap();
        command(&mut app, &format!("kit {} all", kit.display()));
        assert!(app.status.contains(expected), "{}", app.status);
        assert!(!app.dirty);
    }
}

#[test]
fn name_sets_and_clears_a_tracks_name_and_it_shows_in_the_phrase() {
    let (mut app, _realtime) = app();
    command(&mut app, "name bd");
    assert_eq!(app.song().tracks[0].name, "BD");
    let buffer = screen(&app);
    let top: String = (0..40).map(|x| buffer[(x, 0)].symbol()).collect();
    assert!(top.contains("BD"), "{top}");
    command(&mut app, "name 1A");
    assert_eq!(app.song().tracks[0].name, "BD", "{}", app.status);
    command(&mut app, "name");
    assert_eq!(app.song().tracks[0].name, "");
}

#[test]
fn space_5_chooses_a_sample_for_the_current_track() {
    let (mut app, _realtime) = app();
    // With nothing to list it asks for a path, and either way the view stays.
    press(&mut app, " 5");
    assert_ne!(app.mode, Mode::Normal);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.view, View::Phrase);
}

#[test]
fn notes_played_while_editing_preview_on_this_tracks_sound() {
    let (mut app, mut realtime) = app();
    press(&mut app, "W");
    with_a_sound(&mut app);
    block(&mut realtime, &mut app);
    press(&mut app, "ia");
    assert!(loudest(&mut realtime, 2000) > 0.01, "it can be heard");
}

#[test]
fn h_and_l_change_pages_and_every_setting_is_on_one_page() {
    use vt_tui::track_page::{MOD_ROWS, Page, SAMPLE_ROWS};
    let (mut app, _realtime) = on_track_page();
    assert_eq!(app.track_page, Page::Sound);
    press(&mut app, "H");
    assert_eq!(app.track_page, Page::Sound, "it stops at the first");
    press(&mut app, "j$");
    press(&mut app, "L");
    assert_eq!(app.track_page, Page::Mod);
    assert_eq!(app.track_row, 0);
    let row = MOD_ROWS[0];
    assert!(row.contains(&app.track_control()), "the cursor stays valid");
    press(&mut app, "GH");
    assert_eq!(app.track_page, Page::Sound);
    assert_eq!(app.track_row, 0);
    let all: Vec<_> = SAMPLE_ROWS
        .iter()
        .chain(&MOD_ROWS)
        .flat_map(|row| row.iter())
        .collect();
    for (at, control) in all.iter().enumerate() {
        assert!(!all[at + 1..].contains(control), "{control:?} is on twice");
    }
    // Only here: in the phrase view they keep their meaning.
    press(&mut app, " 3L");
    assert_eq!(app.view, View::Phrase);
}

#[test]
fn the_sample_setting_chooses_from_the_bank_and_x_clears_it() {
    let (mut app, _realtime) = on_track_page();
    press(&mut app, "kkkkkkk");
    assert_eq!(app.track_control(), vt_tui::track_page::Control::Sample);
    // With nothing in the bank, K opens the explorer to add a file.
    press(&mut app, "K");
    assert_ne!(app.mode, Mode::Normal);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].sample(), 0, "nothing to step to");
    command(&mut app, "name bd");
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let tone = dir.join("tone.wav");
    wav(&tone);
    command(&mut app, &format!("sample {}", tone.display()));
    let drawn = page_text(&app);
    assert!(drawn.contains("01 tone"), "{drawn}");
    assert!(drawn.contains("BD"), "{drawn}");
    assert!(!drawn.contains(dir.to_str().unwrap()), "{drawn}");
    press(&mut app, "x");
    assert_eq!(sample_of(&app, 0), None);
    press(&mut app, "u");
    assert!(sample_of(&app, 0).is_some());
}

fn page_text(app: &App) -> String {
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..24)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn render_stems_writes_a_named_file_per_track() {
    let (mut app, _realtime) = app();
    with_a_sound(&mut app);
    command(&mut app, "name bd");
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let out = folder().join("stems");
    command(&mut app, &format!("render {} --stems", out.display()));
    assert!(app.status.contains("2 stems"), "{}", app.status);
    assert!(out.join("01-BD.wav").exists());
    assert!(out.join("02.wav").exists());
    command(&mut app, "mute");
    let quieter = folder().join("stems");
    command(&mut app, &format!("render {} --stems", quieter.display()));
    assert!(app.status.contains("1 stem to"), "{}", app.status);
    assert!(!quieter.join("01-BD.wav").exists());
}

#[test]
fn a_selection_lights_up_the_neighbour_columns_it_reaches() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    let selected = vt_tui::theme::selected().bg;

    press(&mut app, "lVW");
    assert!(app.mode.is_visual());
    let buffer = screen(&app);
    assert_eq!(buffer[(42, 2)].bg, selected.unwrap());
    assert_ne!(
        buffer[(42, 3)].bg,
        selected.unwrap(),
        "only the rows selected"
    );
}

fn mixer_text(app: &App) -> Vec<String> {
    mixer_text_at(app, 80)
}

/// The mixer's text, drawn this many columns wide.
fn mixer_text_at(app: &App, width: u16) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(width, 17);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::mixer::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..17)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// The line of the mixer starting with a setting's name.
fn mixer_line(app: &App, name: &str) -> String {
    mixer_text(app)
        .into_iter()
        .find(|line| line.starts_with(name))
        .unwrap()
}

#[test]
fn the_mixer_edits_each_tracks_settings_as_the_track_page_does() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 6");
    assert_eq!(app.view, View::Mixer);
    let depth = app.history.undo_depth();
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].level, 0xE1);
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(app.song().tracks[0].level, 0xE0);

    press(&mut app, "lj");
    assert_eq!(app.cursor.track, 1);
    press(&mut app, "i40");
    assert_eq!(app.song().tracks[1].pan, 0x40);
    assert_eq!(app.song().tracks[0].pan, 0x80, "only this track");
    press(&mut app, "x");
    assert_eq!(app.song().tracks[1].pan, 0x80);
    press(&mut app, "j+");
    assert_eq!(app.song().tracks[1].send_a, 0x10);
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Send B");
    press(&mut app, "Gk0");
    assert_eq!(app.cursor.track, 0);
    assert_eq!(column_name(&app), "Mute");
    press(&mut app, "$");
    assert_eq!(app.cursor.track, 3 + MAX_GROUPS, "B, the last with a mute");
    press(&mut app, "gg");
    assert_eq!(
        column_name(&app),
        "Trim",
        "above the level, as on a console"
    );
}

#[test]
fn the_mixers_mute_and_solo_reach_the_engine() {
    let (mut app, mut realtime) = app();
    with_a_sound(&mut app);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    block(&mut realtime, &mut app);
    let _ = loudest(&mut realtime, 96_000);
    press(&mut app, " 6Gk=");
    assert!(app.levels[0].mute);
    assert!(
        mixer_line(&app, "Mute").contains('M'),
        "{}",
        mixer_line(&app, "Mute")
    );
    press(&mut app, "=");
    assert!(app.levels[0].mute, "= only switches on");
    press(&mut app, " 3k;");
    assert_eq!(loudest(&mut realtime, 2000), 0.0, "muted");
    press(&mut app, " 6-");
    assert!(!app.levels[0].mute);
    press(&mut app, "K");
    assert!(app.levels[0].mute);
    press(&mut app, "x");
    assert!(!app.levels[0].mute);
    press(&mut app, "j=");
    assert!(app.levels[0].solo);
    assert!(mixer_line(&app, "Solo").contains(vt_tui::glyphs::SOLO));
    press(&mut app, "i");
    assert!(!app.levels[0].solo);
    assert_eq!(app.mode, Mode::Normal, "it switches rather than typing");
}

#[test]
fn a_long_mixer_scrolls_to_keep_the_track_in_view() {
    let (mut app, _realtime) = app();
    command(&mut app, "track add 30");
    command(&mut app, "name bd");
    press(&mut app, " 6");
    assert!(
        mixer_text(&app)[2].contains("01 BD"),
        "{}",
        mixer_text(&app)[2]
    );
    press(&mut app, "$");
    assert_eq!(app.cursor.track, 31 + FIXED_TRACKS);
    let names = &mixer_text(&app)[2];
    assert!(names.contains("32"), "{names}");
    assert!(
        names.contains("\u{a0}A ") && names.contains("\u{a0}M"),
        "{names}"
    );
    assert!(!names.contains("01"), "{names}");
}

/// The colour a mixer's text is drawn in where some text starts on a row.
fn mixer_color(app: &App, row: usize, text: &str) -> ratatui::style::Color {
    let backend = ratatui::backend::TestBackend::new(160, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::mixer::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let line: String = (0..160).map(|x| buffer[(x, row as u16)].symbol()).collect();
    let column = line[..line.find(text).unwrap()].chars().count();
    buffer[(column as u16, row as u16)].fg
}

#[test]
fn a_groups_strip_names_it_and_its_tracks_and_its_tracks_are_marked() {
    use vt_tui::glyphs::{GROUP, ICON_SPACE};
    let (mut app, _realtime) = app();
    command(&mut app, "track add 4");
    command(&mut app, "group 1 1-3");
    command(&mut app, "group 2 5");
    press(&mut app, " 6");
    let text = mixer_text_at(&app, 160);
    let (names, routing) = (&text[2], &text[3]);
    // G1's strip has its icon and name, and under it the tracks in it.
    let g1 = format!("{GROUP}{ICON_SPACE}G1");
    assert!(names.contains(&g1), "{names}");
    let column = |line: &str, text: &str| line[..line.find(text).unwrap()].chars().count();
    let under = routing.chars().skip(column(names, &g1)).collect::<String>();
    assert!(under.starts_with("01-03"), "{routing}");
    // Each track in a group is marked with it, under its name, and one in none isn't.
    let marks: Vec<String> = routing.split_whitespace().map(String::from).collect();
    assert_eq!(marks, ["G1", "G1", "G1", "G2", "01-03", "05"], "{routing}");
    // A gap parts the sound tracks from the groups, and the groups from the returns.
    let strip = |name: &str| column(names, name);
    assert_eq!(strip(&g1) - strip("06"), 7 + 2, "{names}");
    // The empty groups are folded away, so the returns come after G2.
    let g3 = format!("{GROUP}{ICON_SPACE}G3");
    assert!(!names.contains(&g3), "{names}");
    assert_eq!(
        strip("\u{a0}A") - 1 - strip(&format!("{GROUP}{ICON_SPACE}G2")),
        7 + 2
    );
    // A group and its tracks share its colour, and an empty group is faint, once it's shown.
    let blue = vt_tui::theme::group(1);
    assert_eq!(mixer_color(&app, 2, &g1), blue);
    assert_eq!(mixer_color(&app, 3, "G1"), blue);
    assert_eq!(mixer_color(&app, 3, "G2"), vt_tui::theme::group(2));
    press(&mut app, "zR");
    assert_eq!(mixer_color(&app, 2, &g3), vt_tui::theme::OVERLAY0);
    let names = &mixer_text_at(&app, 160)[2];
    assert_eq!(
        column(names, "\u{a0}A") - 1 - column(names, &format!("{GROUP}{ICON_SPACE}G4")),
        7 + 2
    );
}

#[test]
fn a_tracks_mute_and_solo_from_its_group_show_apart_from_its_own() {
    use vt_tui::glyphs::{MUTE, SOLO};
    let (mut app, _realtime) = app();
    command(&mut app, "track add 2");
    command(&mut app, "group 1 1-2");
    let group = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    press(&mut app, " 6");
    app.levels[group].mute = true;
    app.levels[2].mute = true;
    let mutes = mixer_line(&app, "Mute");
    let shown: Vec<&str> = mutes.split_whitespace().skip(1).collect();
    let mute = MUTE.to_string();
    assert_eq!(
        shown[..5],
        [&mute, &mute, &mute, "-", &mute],
        "01 to 04, and G1: {mutes}"
    );
    // The track's own in the active colour, its group's in the group's.
    let (blue, green) = (vt_tui::theme::group(1), vt_tui::theme::GREEN);
    assert_eq!(mixer_colors(&app, "Mute", MUTE), [blue, blue, green, green]);
    app.levels[group].mute = false;
    app.levels[group].solo = true;
    assert_eq!(mixer_colors(&app, "Solo", SOLO), [blue, blue, green]);
    assert!(mixer_colors(&app, "Mute", MUTE) == [green], "only 03's own");
}

/// The colour of each of a glyph on the mixer's row of a setting, from the left.
fn mixer_colors(app: &App, name: &str, glyph: char) -> Vec<ratatui::style::Color> {
    let backend = ratatui::backend::TestBackend::new(160, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::mixer::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let row = (0..16)
        .find(|&y| buffer[(0, y)].symbol() == &name[..1] && buffer[(1, y)].symbol() == &name[1..2])
        .unwrap();
    (0..160)
        .filter(|&x| buffer[(x, row)].symbol() == glyph.to_string())
        .map(|x| buffer[(x, row)].fg)
        .collect()
}

#[test]
fn the_mixers_meters_show_only_the_track_that_is_playing() {
    use vt_tui::meter::FLOOR_DB;
    let (mut app, mut realtime) = app();
    with_a_sound(&mut app);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    block(&mut realtime, &mut app);
    let _ = loudest(&mut realtime, 96_000);
    press(&mut app, " 3k;");
    let _ = loudest(&mut realtime, 2000);
    app.tick();
    assert!(
        app.track_meters[0].top() > FLOOR_DB,
        "{}",
        app.track_meters[0].top()
    );
    assert_eq!(app.track_meters[1].top(), FLOOR_DB);
}

#[test]
fn the_mixer_draws_a_strip_per_track_with_a_meter_and_a_fader() {
    use vt_engine::mixer::Reading;
    let (mut app, _realtime) = app();
    let reading = Reading {
        before: [1.0, 0.0],
        after: [1.0, 0.0],
        power: [100.0, 0.0],
        frames: 100,
    };
    for _ in 0..50 {
        app.track_meters[0].update(Some(reading), 0.033);
    }
    command(&mut app, "track add 6");
    press(&mut app, " 6");
    let text = mixer_text(&app);
    assert!(text[2].starts_with("       01"), "{}", text[2]);
    assert!(text[2].contains("08"), "eight strips fit: {}", text[2]);
    // Under the names and the groups, the meter's left channel is full, and the right empty.
    let bars = &text[4..10];
    let left = |line: &String| line.chars().nth(7).unwrap();
    let right = |line: &String| line.chars().nth(8).unwrap();
    assert_eq!(left(&bars[bars.len() - 1]), '█', "{bars:?}");
    assert_eq!(right(&bars[bars.len() - 1]), ' ', "{bars:?}");
    // Every track has a fader, for a level of E0 here. The next track has only that.
    let fader = |line: &String, strip: usize| line.chars().nth(7 + strip * 7 + 3).unwrap();
    assert_ne!(fader(&bars[bars.len() - 1], 0), ' ');
    assert_ne!(fader(&bars[bars.len() - 1], 1), ' ');
    // Turning a fader down lowers only that strip's.
    let filled = |text: &[String], strip: usize| {
        text[4..10]
            .iter()
            .filter(|line| fader(line, strip) != ' ')
            .count()
    };
    let before = filled(&text, 0);
    press(&mut app, "x");
    press(&mut app, "i10");
    let text = mixer_text(&app);
    assert!(filled(&text, 0) < before, "{text:?}");
    assert_eq!(filled(&text, 1), before);
    assert!(mixer_line(&app, "Level").contains("10"));
}

fn song_text(app: &App) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::song::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..12)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect()
}

#[test]
fn the_song_view_adds_copies_and_removes_sections() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1");
    assert_eq!(app.view, View::Song);
    assert_eq!(app.song().sections.len(), 1);
    press(&mut app, "a");
    assert_eq!(app.song().sections.len(), 2);
    assert_eq!(app.cursor.section, 1);
    assert_eq!(
        app.song().sections[1].chains,
        vec![None; app.song().tracks.len()],
        "no chains"
    );
    assert_eq!(app.song().tracks[0].chains.len(), 1, "none made");
    press(&mut app, "gc");
    assert_eq!(app.song().sections.len(), 3);
    assert_eq!(app.cursor.section, 2);
    assert_eq!(
        app.song().sections[2].chains,
        app.song().sections[1].chains,
        "the same chains"
    );
    press(&mut app, "dd");
    assert_eq!(app.song().sections.len(), 2);
    assert_eq!(app.cursor.section, 1);
    press(&mut app, "u");
    assert_eq!(app.song().sections.len(), 3);
    press(&mut app, "ddddddd");
    assert_eq!(app.song().sections.len(), 1, "one always stays");
    assert!(app.status.contains("at least one"), "{}", app.status);
    assert_eq!(app.cursor.section, 0);
    let text = song_text(&app);
    assert!(text[2].contains("Length") && text[2].contains("Start"));
    assert!(text[3].contains("16") && text[3].contains("new"));
}

#[test]
fn the_song_view_edits_length_start_and_chains() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 1");
    assert_eq!(column_name(&app), "Length");
    press(&mut app, "=");
    assert_eq!(app.song().sections[0].length, Some(17));
    press(&mut app, "-_");
    assert_eq!(app.song().sections[0].length, Some(1));
    press(&mut app, "i32");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[0].length, Some(32));
    press(&mut app, "x");
    assert_eq!(app.song().sections[0].length, None);
    press(&mut app, "i0");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[0].length, None, "{}", app.status);

    press(&mut app, "l");
    assert_eq!(column_name(&app), "Rpt");
    assert!(song_text(&app)[2].contains("Rpt"));
    press(&mut app, "=");
    assert_eq!(app.song().sections[0].repeat.get(), 2);
    press(&mut app, "+");
    assert_eq!(app.song().sections[0].repeat.get(), 6);
    press(&mut app, "__");
    assert_eq!(app.song().sections[0].repeat.get(), 1, "never below once");
    press(&mut app, "i12");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[0].repeat.get(), 12);
    press(&mut app, "i0");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[0].repeat.get(), 12, "{}", app.status);
    assert!(app.status.contains("repeat"), "{}", app.status);
    press(&mut app, "gc");
    assert_eq!(app.song().sections[1].repeat.get(), 12, "a copy keeps it");
    press(&mut app, "u");
    press(&mut app, "x");
    assert_eq!(app.song().sections[0].repeat.get(), 1);

    press(&mut app, "l");
    assert_eq!(column_name(&app), "Start");
    press(&mut app, "K");
    assert_eq!(app.song().sections[0].start, SectionStart::Reset);
    assert!(song_text(&app)[3].contains("reset"));
    press(&mut app, "x");
    assert_eq!(app.song().sections[0].start, SectionStart::New);

    press(&mut app, "ll");
    assert_eq!(column_name(&app), "Track 02 chain");
    press(&mut app, "x");
    assert_eq!(app.song().sections[0].chains[1], None);
    assert!(song_text(&app)[3].contains("--"));
    press(&mut app, "i05");
    assert_eq!(app.song().sections[0].chains[1], Some(5));
    assert!(
        app.song().tracks[1].chains.contains_key(&5),
        "made as needed"
    );
    press(&mut app, "=");
    assert_eq!(app.song().sections[0].chains[1], Some(6));
    press(&mut app, "$");
    assert_eq!(column_name(&app), "Master chain");
    press(&mut app, "0");
    assert_eq!(column_name(&app), "Length");
}

#[test]
fn other_views_edit_the_section_under_the_cursor() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1alllgd 3");
    let two = app.song().sections[1].chains[0].unwrap();
    let phrase = app.song().tracks[0].chains[&two].entries[0].phrase;
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.song().tracks[0].phrases[&phrase].steps()[0].note,
        Note::On(60)
    );
    assert_eq!(
        app.song().tracks[0].phrases[&0].steps()[0].note,
        Note::Empty
    );
    command(&mut app, "reset 8");
    assert_eq!(app.song().sections[1].reset, Some(8));
    assert_eq!(app.song().sections[0].reset, None, "only this section");
    press(&mut app, " 1k");
    assert_eq!(app.cursor.section, 0);
    assert_eq!(app.phrase_id(), Some(0));
}

#[test]
fn each_track_numbers_its_own_chains_and_phrases() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "W");
    assert_eq!(app.phrase_id(), Some(0));
    assert_eq!(
        notes(&app)[0],
        Note::Empty,
        "not the first track's phrase 00"
    );
    press(&mut app, " 2gc");
    assert_eq!(app.chain_entries()[0].phrase, 1);
    assert_eq!(
        app.song().tracks[0].phrases.len(),
        1,
        "the first track has none new"
    );
    command(&mut app, "chain FF");
    assert_eq!(app.chain_id(), Some(0xFF));
    assert!(!app.song().tracks[0].chains.contains_key(&0xFF));
    press(&mut app, " 1lll");
    assert!(
        song_text(&app)[3].contains("FF"),
        "{:?}",
        song_text(&app)[3]
    );
}

#[test]
fn an_empty_section_can_be_visited_from_every_view_and_filled() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1a");
    assert!(song_text(&app)[4].contains("--"));
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            vt_tui::views::chain::draw(frame, frame.area(), &app);
            vt_tui::views::phrase::draw(frame, frame.area(), &app);
            vt_tui::views::track::draw(frame, frame.area(), &app);
            vt_tui::views::mixer::draw(frame, frame.area(), &app);
        })
        .unwrap();
    press(&mut app, " 3ia");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.song().sections[1].chains[0], None);
    press(&mut app, " 1lllgd");
    assert_eq!(app.view, View::Phrase);
    assert!(app.song().sections[1].chains[0].is_some());
    assert_eq!(app.song().sections[1].chains[1], None, "only this track");
}

#[test]
fn gd_in_the_song_view_opens_a_new_chains_phrase_as_one_undo() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1gd");
    assert_eq!(app.view, View::Song, "{}", app.status);
    assert!(app.status.contains("track"));
    press(&mut app, "llllx");
    let depth = app.history.undo_depth();
    press(&mut app, "gd");
    assert_eq!(app.view, View::Phrase);
    assert_eq!(app.cursor.track, 1);
    assert!(app.chain_id().is_some());
    assert_eq!(app.chain_entries().len(), 1);
    assert_eq!(app.history.undo_depth(), depth + 1);
}

#[test]
fn gd_in_the_song_view_goes_by_how_many_phrases_a_chain_has() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1lllgd");
    assert_eq!(app.view, View::Phrase, "one phrase");
    press(&mut app, " 2jgd");
    assert_eq!(app.chain_entries().len(), 2);
    press(&mut app, " 1gd");
    assert_eq!(app.view, View::Chain, "several");
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!(app.view, View::Song, "C-o comes back");
}

#[test]
fn launch_sets_when_launches_start_and_shows_it() {
    let (mut app, _realtime) = app();
    assert_eq!(app.song().launch, Launch::every(16).unwrap());
    command(&mut app, "launch");
    assert!(app.status.contains("1bar"), "{}", app.status);
    let depth = app.history.undo_depth();
    command(&mut app, "launch 1/4");
    assert_eq!(app.song().launch, Launch::every(4).unwrap());
    assert!(app.status.contains("1/4"), "{}", app.status);
    assert_eq!(app.history.undo_depth(), depth + 1);
    command(&mut app, "launch 12");
    assert!(app.status.contains("12-step"), "{}", app.status);
    command(&mut app, "launch chain");
    assert_eq!(app.song().launch, Launch::ChainEnd);
    assert!(app.status.contains("chain ends"), "{}", app.status);
    press(&mut app, "uu");
    assert_eq!(app.song().launch, Launch::every(4).unwrap());
    command(&mut app, "launch soon");
    assert_eq!(app.song().launch, Launch::every(4).unwrap());
}

#[test]
fn a_playhead_follows_the_chain_a_track_is_on_whichever_row_shows_it() {
    let (mut app, mut realtime) = jamming();
    play_on(&mut app, &mut realtime, 3);
    assert_eq!(app.playing_section, 0);
    assert_eq!(app.cursor.section, 1);
    assert_eq!(
        app.chain_playhead(0, 1),
        None,
        "not playing this row's chain"
    );
    assert!(app.chain_playhead(0, 0).is_some(), "but this one");
    press(&mut app, "0lll");
    special(&mut app, KeyCode::Enter);
    play_on(&mut app, &mut realtime, 16);
    assert_eq!(app.jam[0].playing, Some(Some(1)));
    assert_eq!(app.chain_playhead(0, 0), None, "replaced by the launch");
    let launched = app.chain_playhead(0, 1).unwrap();
    assert!(launched < 16, "from the launch's own start: {launched}");
    assert_eq!(app.chain_playhead(1, 0).map(|step| step < 16), Some(true));
}

#[test]
fn space_c_in_a_phrase_plays_from_that_step_of_its_chain() {
    let (mut app, mut realtime) = app();
    press(&mut app, "5j c");
    block(&mut realtime, &mut app);
    block(&mut realtime, &mut app);
    app.tick();
    assert_eq!(app.transport, Transport::Playing);
    assert_eq!(app.chain_playhead(0, 0), Some(5));
}

#[test]
fn enter_says_when_the_launch_will_start() {
    let (mut app, _realtime) = jamming();
    press(&mut app, "0lll");
    special(&mut app, KeyCode::Enter);
    assert!(app.status.contains("on the 1bar grid"), "{}", app.status);
    command(&mut app, "launch chain");
    press(&mut app, " 1");
    special(&mut app, KeyCode::Enter);
    assert!(app.status.contains("when its chain ends"), "{}", app.status);
}

#[test]
fn a_label_names_the_chain_under_the_cursor() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 1lll");
    assert_eq!(column_name(&app), "Track 01 chain");
    let depth = app.history.undo_depth();
    command(&mut app, "label verse bass");
    assert_eq!(app.song().tracks[0].chains[&0].label, "verse bass");
    assert_eq!(column_name(&app), "Track 01 chain: verse bass");
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "gc");
    assert_eq!(
        column_name(&app),
        "Track 01 chain: verse bass",
        "same chain"
    );
    press(&mut app, "u");
    command(&mut app, "label");
    assert_eq!(column_name(&app), "Track 01 chain");
    press(&mut app, " 2");
    command(&mut app, "label from chain view");
    assert_eq!(
        app.song().tracks[0].chains[&0].label,
        "from chain view",
        "{}",
        app.status
    );
}

#[test]
fn label_is_refused_without_a_chain_to_name() {
    let (mut app, _realtime) = app();
    command(&mut app, "label nope");
    assert!(app.status.contains("song and chain"), "{}", app.status);
    press(&mut app, " 1");
    command(&mut app, "label nope");
    assert!(app.status.contains("track's column"), "{}", app.status);
    press(&mut app, "lllx");
    command(&mut app, "label nope");
    assert!(app.status.contains("no chain"), "{}", app.status);
    assert!(app.song().tracks[0].chains[&0].label.is_empty());
}

#[test]
fn k_chooses_a_cells_chain_by_number_or_label() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1aaj");
    press(&mut app, "lll");
    press(&mut app, "=");
    assert_eq!(app.song().sections[2].chains[0], Some(1));
    command(&mut app, "label bass");
    press(&mut app, "kK");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.shown().count(), 3, "-- and both chains");
    press(&mut app, "bass");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 1);
    let depth = app.history.undo_depth();
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[1].chains[0], Some(1));
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "K");
    assert_eq!(
        app.picker
            .as_ref()
            .unwrap()
            .current()
            .map(listed)
            .as_deref(),
        Some("01  bass"),
        "the cell's own is highlighted"
    );
    special(&mut app, KeyCode::Esc);
    press(&mut app, "K");
    press(&mut app, "--");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[1].chains[0], None);
    press(&mut app, "0K");
    assert!(app.status.contains("track's column"), "{}", app.status);
    assert!(app.picker.is_none());
}

#[test]
fn g_upper_c_copies_the_chain_in_the_cursors_column_alone() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1llll");
    command(&mut app, "label bass");
    press(&mut app, "gc");
    assert_eq!(app.cursor.section, 1);
    let depth = app.history.undo_depth();
    let old = app.song().tracks[1].chains[&0].clone();
    press(&mut app, "gC");
    assert_eq!(app.song().sections[1].chains[1], Some(1), "this cell");
    assert_eq!(app.song().sections[0].chains[1], Some(0), "the other row");
    assert_eq!(app.song().sections[1].chains[0], Some(0), "the other track");
    assert_eq!(app.song().tracks[1].chains[&0], old);
    let copy = &app.song().tracks[1].chains[&1];
    assert_eq!(copy.label, "bass");
    assert_ne!(copy.entries[0].phrase, old.entries[0].phrase);
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(app.song().sections[1].chains[1], Some(0));

    press(&mut app, "0gC");
    assert!(app.status.contains("track's column"), "{}", app.status);
    press(&mut app, "llllx");
    press(&mut app, "gC");
    assert!(app.status.contains("no chain"), "{}", app.status);
}

#[test]
fn a_chain_used_by_another_section_counts_as_shared() {
    let (mut app, _realtime) = app();
    assert_eq!(app.chain_uses(), 1);
    press(&mut app, " 1gc");
    assert_eq!(app.chain_uses(), 2);
    assert_eq!(app.phrase_uses(app.phrase_id().unwrap()).len(), 2);
}

#[test]
fn loop_and_transport_keys_reach_the_section_being_edited() {
    let (mut app, mut realtime) = app();
    with_a_sound(&mut app);
    press(&mut app, " 1a");
    block(&mut realtime, &mut app);
    command(&mut app, "loop");
    assert_eq!(app.looping, Some(1));
    assert!(app.status.contains("section 02"), "{}", app.status);
    press(&mut app, " c");
    block(&mut realtime, &mut app);
    block(&mut realtime, &mut app);
    app.tick();
    assert_eq!(app.playing_section, 1);
    press(&mut app, " 1k");
    assert!(song_text(&app)[4].contains(">02"), "{:?}", song_text(&app));
    command(&mut app, "loop");
    assert_eq!(app.looping, Some(0));
    command(&mut app, "loop");
    assert_eq!(app.looping, None);
}

#[test]
fn the_status_line_counts_sections_when_there_are_several() {
    let (mut app, _realtime) = app();
    let status = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(120, 2);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::status::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..120)
            .map(|x| buffer[(x, 0)].symbol())
            .collect::<String>()
    };
    assert!(!status(&app).contains("Sec "));
    press(&mut app, " 1a");
    assert!(status(&app).contains("Sec 2/2"), "{}", status(&app));
}

/// Run enough audio for reports and a step or more to arrive.
/// Have each track report the chain of the row being edited, for setting a playhead by hand.
fn on_the_row_being_edited(app: &mut App) {
    let chains = app.section_here().unwrap().chains.clone();
    for (jam, chain) in app.jam.iter_mut().zip(chains) {
        jam.chain = chain;
    }
}

fn play_on(app: &mut App, realtime: &mut Realtime, steps: usize) {
    let mut out = vec![0.0; steps * 6000 * 2];
    realtime.process(&mut out);
    app.tick();
}

/// A song view on a second section whose chains differ from the first's.
fn jamming() -> (App, Realtime) {
    let (mut app, mut realtime) = app();
    press(&mut app, " 1alllgd 1lgd 1");
    assert_eq!(app.song().sections[1].chains[..2], [Some(1), Some(1)]);
    press(&mut app, "\t");
    special(&mut app, KeyCode::Tab);
    play_on(&mut app, &mut realtime, 1);
    assert_eq!(app.transport, Transport::Playing);
    (app, realtime)
}

#[test]
fn enter_cues_this_chain_on_one_track_and_holds_the_section() {
    let (mut app, mut realtime) = jamming();
    assert_eq!(app.cursor.section, 1);
    assert_eq!(app.playing_section, 0);
    press(&mut app, "0lll");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.looping, Some(0), "the jam stays here");
    play_on(&mut app, &mut realtime, 1);
    assert_eq!(app.jam[0].cued, Some(Some(1)));
    assert!(
        song_text(&app)[4].contains("01~"),
        "{:?}",
        song_text(&app)[4]
    );
    play_on(&mut app, &mut realtime, 16);
    assert_eq!(app.jam[0].playing, Some(Some(1)));
    assert_eq!(app.jam[0].cued, None);
    assert_eq!(app.jam[1].playing, None, "the other track is untouched");
    assert!(
        song_text(&app)[4].contains("01>"),
        "{:?}",
        song_text(&app)[4]
    );
    assert_eq!(
        app.song().sections[0].chains[0],
        Some(0),
        "the song is unchanged"
    );
}

#[test]
fn enter_and_c_need_the_song_to_be_playing_and_enter_a_track() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1");
    special(&mut app, KeyCode::Enter);
    assert!(app.status.contains("while playing"), "{}", app.status);
    press(&mut app, "C");
    assert!(app.status.contains("start the song"), "{}", app.status);
    assert_eq!(app.song().sections.len(), 1);
    let (mut app, _realtime) = jamming();
    press(&mut app, "0");
    special(&mut app, KeyCode::Enter);
    assert!(app.status.contains("track's column"), "{}", app.status);
}

#[test]
fn c_captures_what_each_track_is_playing_as_one_undo() {
    let (mut app, mut realtime) = jamming();
    let depth = app.history.undo_depth();
    press(&mut app, "0lll");
    special(&mut app, KeyCode::Enter);
    play_on(&mut app, &mut realtime, 17);
    assert_eq!(app.jam[0].playing, Some(Some(1)));
    press(&mut app, "C");
    assert_eq!(app.song().sections.len(), 3);
    assert_eq!(app.song().sections[1].chains[0], Some(1), "launched");
    assert_eq!(app.song().sections[1].chains[1], Some(0), "from its row");
    assert_eq!(
        app.song().sections[1].length,
        Some(16),
        "the row it came from"
    );
    assert_eq!(app.cursor.section, 1);
    assert_eq!(app.looping, Some(1));
    play_on(&mut app, &mut realtime, 1);
    assert_eq!(app.playing_section, 1, "it carries on in the new row");
    assert_eq!(app.jam[0].playing, Some(Some(1)), "without restarting");
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(app.song().sections.len(), 2);
}

#[test]
fn c_copies_the_settings_of_the_row_it_came_from() {
    let (mut app, mut realtime) = jamming();
    press(&mut app, "k0");
    command(&mut app, "reset 8");
    press(&mut app, "llK");
    assert_eq!(app.song().sections[0].start, SectionStart::Reset);
    play_on(&mut app, &mut realtime, 1);
    press(&mut app, "C");
    assert_eq!(app.song().sections[1].reset, Some(8));
    assert_eq!(app.song().sections[1].start, SectionStart::Reset);
}

#[test]
fn a_stopped_track_captures_as_empty_and_nothing_playing_is_refused() {
    let (mut app, mut realtime) = jamming();
    press(&mut app, "0lll");
    press(&mut app, "x");
    special(&mut app, KeyCode::Enter);
    play_on(&mut app, &mut realtime, 17);
    assert_eq!(app.jam[0].playing, Some(None));
    press(&mut app, "C");
    assert_eq!(app.song().sections[1].chains[0], None);
    assert!(app.song().sections[1].chains[1].is_some());

    let (mut app, mut realtime) = jamming();
    press(&mut app, "0lllx");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "lx");
    special(&mut app, KeyCode::Enter);
    play_on(&mut app, &mut realtime, 17);
    press(&mut app, "C");
    assert!(app.status.contains("Nothing is playing"), "{}", app.status);
    assert_eq!(app.song().sections.len(), 2);
}

#[test]
fn envelopes_drawn_as_images_leave_placeholders_with_their_positions() {
    let (mut app, _realtime) = app();
    app.envelope_images = Some([0x01_02_03, 0x04_05_06]);
    press(&mut app, " 4L");
    let buffer = {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), &app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let at = |text: &str| {
        (0..24).find(|y| {
            (0..80)
                .map(|x| buffer[(x, *y)].symbol())
                .collect::<String>()
                .contains(text)
        })
    };
    let label = at("Env 1  Vol").unwrap();
    let first = &buffer[(0, label + 1)];
    assert!(
        first.symbol().starts_with('\u{10EEEE}'),
        "{:?}",
        first.symbol()
    );
    assert_eq!(first.fg, ratatui::style::Color::Rgb(1, 2, 3));
    // The second envelope is one image width and a gap along, and holds its row and column.
    let second = &buffer[(30, label + 2)];
    assert_eq!(second.symbol(), "\u{10EEEE}\u{30D}\u{305}");
    assert_eq!(second.fg, ratatui::style::Color::Rgb(4, 5, 6));
    assert_eq!(buffer[(27, label + 1)].symbol(), "\u{10EEEE}\u{305}\u{36D}");
}

#[test]
fn a_changed_envelope_image_gets_a_new_id_and_moving_the_cursor_sends_nothing() {
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        (out.matches(",c=28,").count(), out)
    };
    // Nothing is sent until the page that shows them.
    assert_eq!(sync(&mut app, &mut graphics).0, 0);
    press(&mut app, " 4L");
    assert_eq!(sync(&mut app, &mut graphics).0, 2);
    let first = app.envelope_images.unwrap();
    assert_ne!(first[0], first[1]);
    assert_eq!(sync(&mut app, &mut graphics).0, 0, "nothing has changed");

    // Each envelope's other look is sent once, and after that the cursor only swaps between them.
    press(&mut app, "j");
    assert_eq!(sync(&mut app, &mut graphics).0, 2);
    let moved = app.envelope_images.unwrap();
    assert!(moved[0] != first[0] && moved[1] != first[1]);
    press(&mut app, "k");
    assert_eq!(sync(&mut app, &mut graphics).0, 0);
    assert_eq!(app.envelope_images.unwrap(), first);
    press(&mut app, "j");
    assert_eq!(sync(&mut app, &mut graphics).0, 0);
    assert_eq!(app.envelope_images.unwrap(), moved);

    // A change makes a new image, and the old one is forgotten only after the screen moves on.
    press(&mut app, "=");
    let (sent, out) = sync(&mut app, &mut graphics);
    assert_eq!(sent, 1);
    let changed = app.envelope_images.unwrap();
    assert_ne!(changed[1], moved[1]);
    assert_eq!(changed[0], moved[0]);
    let forget = format!("a=d,d=I,i={},q=2", moved[1]);
    assert!(!out.contains(&forget));
    assert!(sync(&mut app, &mut graphics).1.contains(&forget));
}

#[test]
fn an_lfos_shape_has_its_glyph_after_its_name_and_keeps_the_columns_in_line() {
    use vt_tui::glyphs;
    use vt_tui::theme;
    let draw = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let (mut app, _realtime) = app();
    press(&mut app, " 4L");
    assert_eq!(app.song().tracks[0].lfos[0].shape, Shape::Tri);
    let plain = draw(&app);
    let row = (0..24)
        .find(|y| {
            (0..80)
                .map(|x| plain[(x, *y)].symbol())
                .collect::<String>()
                .starts_with("LFO 1")
        })
        .unwrap();
    let text = |buffer: &ratatui::buffer::Buffer, x: u16| buffer[(x, row)].symbol().to_owned();
    // After the name's column, one cell, dim while the cursor is on another LFO.
    assert_eq!(text(&plain, 30), glyphs::TRIANGLE_WAVE.to_string());
    assert_eq!(plain[(30, row)].fg, theme::dim().fg.unwrap());
    assert_eq!(text(&plain, 31), " ");

    // In the active colour while the cursor is on its LFO.
    press(&mut app, "jj");
    assert_eq!(draw(&app)[(30, row)].fg, theme::active().fg.unwrap());

    // A falling saw has no glyph, and the columns after it stay where they were.
    press(&mut app, "l=");
    assert_eq!(app.song().tracks[0].lfos[0].shape, Shape::Saw);
    let saw = draw(&app);
    assert_eq!(text(&saw, 30), " ");
    for x in 31..80 {
        assert_eq!(text(&saw, x), text(&plain, x), "at {x}");
    }
    assert_eq!(glyphs::lfo_shape(Shape::Ramp), Some(glyphs::SAWTOOTH_WAVE));
    for shape in [Shape::Saw, Shape::Random, Shape::Smooth] {
        assert_eq!(glyphs::lfo_shape(shape), None);
    }
}

#[test]
fn the_filter_curve_follows_its_settings_and_the_cursor() {
    use vt_tui::kitty::Graphics;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out).unwrap().matches(",c=24,").count()
    };
    press(&mut app, " 4");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let plain = app.filter_image.unwrap();
    assert_eq!(sync(&mut app, &mut graphics), 0);

    // On the filter's rows it's picked out, once.
    press(&mut app, "jjjjjj");
    assert_eq!(column_name(&app), "Filter");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let picked = app.filter_image.unwrap();
    assert_ne!(picked, plain);
    press(&mut app, "k");
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.filter_image.unwrap(), plain);

    // A new setting draws it again.
    press(&mut app, "jjj");
    assert_eq!(column_name(&app), "Cut");
    press(&mut app, "-");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_ne!(app.filter_image.unwrap(), picked);
    assert_eq!(sync(&mut app, &mut graphics), 0);

    // And so does a new mode.
    press(&mut app, "k");
    assert_eq!(column_name(&app), "Mode");
    press(&mut app, "=");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_eq!(sync(&mut app, &mut graphics), 0);
}

#[test]
fn the_filter_curve_moves_with_what_plays() {
    use vt_engine::track_state::LiveValues;
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        (out.matches(",c=24,").count(), out.matches("a=d,").count())
    };
    press(&mut app, " 4");
    sync(&mut app, &mut graphics);
    let still = app.filter_image.unwrap();
    let track = app.song().tracks[0].clone();
    let live = |cutoff, resonance| LiveValues {
        level: track.level,
        pan: track.pan,
        cutoff,
        resonance,
        send_a: track.send_a,
        send_b: track.send_b,
        machine: std::array::from_fn(|setting| track.machine_value(setting)),
        inserts: Default::default(),
        reduction: Default::default(),
        bypassed: Default::default(),
    };

    // Each new value draws it once, and it holds while the value does.
    app.live = Some(live(0x40, track.resonance));
    assert_eq!(sync(&mut app, &mut graphics).0, 1);
    let moved = app.filter_image.unwrap();
    assert_ne!(moved, still);
    // Holding, it sends nothing new, and the image it replaced is forgotten.
    assert_eq!(sync(&mut app, &mut graphics), (0, 1));
    assert_eq!(sync(&mut app, &mut graphics), (0, 0));
    app.live = Some(live(0x40, 0xC0));
    assert_eq!(sync(&mut app, &mut graphics).0, 1);
    assert_ne!(app.filter_image.unwrap(), moved);

    // Back on the setting, it's drawn from the setting again.
    app.live = Some(live(track.cutoff, track.resonance));
    assert_eq!(sync(&mut app, &mut graphics).0, 1);
}

#[test]
fn the_filter_curve_sits_in_a_box_under_the_filters_rows() {
    let draw = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    let plain = draw(&app);
    let row_of = |text: &str| {
        (0..24)
            .find(|y| {
                (0..80)
                    .map(|x| plain[(x, *y)].symbol())
                    .collect::<String>()
                    .starts_with(text)
            })
            .unwrap()
    };
    // `High cut` is the page's last row.
    let last = row_of("High cut");
    // Without an image, there's no box.
    assert_eq!(plain[(0, last + 1)].symbol(), " ");

    app.filter_image = Some(0x01_02_03);
    let drawn = draw(&app);
    let top = last + 1;
    // As wide as a value and its bar, which ends at the line after its cells.
    let bar_end = (0..80)
        .find(|x| plain[(*x, row_of("Drive"))].symbol() == "▏")
        .unwrap();
    assert_eq!(drawn[(0, top)].symbol(), "┌");
    assert_eq!(drawn[(bar_end, top)].symbol(), "┐");
    assert_eq!(drawn[(bar_end + 1, top)].symbol(), " ");
    assert_eq!(drawn[(0, top + 1)].symbol(), "│");
    assert_eq!(drawn[(1, top + 1)].symbol(), "\u{10EEEE}\u{305}\u{305}");
    assert_eq!(drawn[(1, top + 1)].fg, ratatui::style::Color::Rgb(1, 2, 3));
    assert_eq!(drawn[(bar_end, top + 3)].symbol(), "│");
    assert_eq!(drawn[(0, top + 4)].symbol(), "└");
    assert_eq!(drawn[(bar_end, top + 4)].symbol(), "┘");
    // The rows above it are as they were.
    for x in 0..80 {
        assert_eq!(drawn[(x, last)].symbol(), plain[(x, last)].symbol());
    }
}

#[test]
fn brackets_move_between_tracks_from_every_view() {
    for view in [" 1", " 2", " 3", " 4", " 6"] {
        let (mut app, _realtime) = app();
        press(&mut app, view);
        press(&mut app, "]b");
        assert_eq!(app.cursor.track, 1, "from {view:?}");
        press(&mut app, "[b");
        assert_eq!(app.cursor.track, 0, "from {view:?}");
        // It stops at the first track, rather than going round.
        press(&mut app, "[b");
        assert_eq!(app.cursor.track, 0, "from {view:?}");
    }
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

fn jump_labels(app: &App) -> Vec<String> {
    let track = app.song().tracks[app.cursor.track].clone();
    let jump = app.field_jump.as_ref().unwrap();
    let mut labels: Vec<String> = app
        .track_page
        .rows(app.song().tracks[app.cursor.track].machine)
        .iter()
        .flat_map(|row| row.iter())
        .filter_map(|control| {
            let name = control.label_on(&track);
            jump.label(*control).map(|label| format!("{label} {name}"))
        })
        .collect();
    labels.sort();
    labels
}

#[test]
fn a_jump_labels_matching_settings_nearest_first_and_lands_on_the_value() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 4/");
    assert_eq!(app.mode, Mode::Jump);
    assert!(jump_labels(&app).is_empty(), "nothing until you type");
    // Machine takes `a` for the `c`, and goes when the search narrows, leaving the rest as they were.
    press(&mut app, "cu");
    assert_eq!(jump_labels(&app), ["f Cut", "j Low cut", "k High cut"]);

    press(&mut app, "j");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.field_jump.is_none());
    assert_eq!(column_name(&app), "Low cut");
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].low_cut, 1, "ready to change");

    // The nearest is now Low cut itself.
    press(&mut app, "/c");
    assert_eq!(jump_labels(&app)[0], "a Low cut");
}

#[test]
fn a_jumps_label_is_never_the_next_letter_and_stays_as_you_type() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4/c");
    let first = jump_labels(&app);
    assert!(first.iter().any(|label| label.ends_with("Machine")));
    assert!(
        !first.iter().any(|label| label.starts_with(['h', 'u'])),
        "Machine goes on to h and Cut to u: {first:?}"
    );
    let label_of = |labels: &[String], name: &str| {
        labels
            .iter()
            .find(|label| label.ends_with(name))
            .cloned()
            .unwrap()
    };
    let cut = label_of(&first, " Cut");
    press(&mut app, "u");
    assert_eq!(
        label_of(&jump_labels(&app), " Cut"),
        cut,
        "it keeps its label"
    );
    press(&mut app, "t");
    assert_eq!(app.field_jump.as_ref().unwrap().query, "cut");
    assert_eq!(jump_labels(&app).len(), 3);
    assert_eq!(label_of(&jump_labels(&app), " Cut"), cut);
}

#[test]
fn a_jump_finds_each_row_of_a_repeated_name_and_can_be_left() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 4L/dep");
    assert_eq!(jump_labels(&app).len(), 4, "{:?}", jump_labels(&app));

    // A slip keeps the search.
    press(&mut app, "Q");
    assert_eq!(app.field_jump.as_ref().unwrap().query, "dep");
    special(&mut app, KeyCode::Backspace);
    special(&mut app, KeyCode::Backspace);
    assert_eq!(app.field_jump.as_ref().unwrap().query, "d");
    special(&mut app, KeyCode::Backspace);
    assert!(jump_labels(&app).is_empty());
    special(&mut app, KeyCode::Backspace);
    assert_eq!(app.mode, Mode::Normal);

    press(&mut app, "/dep");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(column_name(&app), "Dest", "it hasn't moved");

    // Enter goes to the nearest.
    press(&mut app, "/dep");
    special(&mut app, KeyCode::Enter);
    assert_eq!(column_name(&app), "Depth");
    assert_eq!(app.track_row, 0);
}

#[test]
fn ctrl_o_comes_back_from_a_jump_to_the_setting_it_left() {
    use vt_tui::track_page::Page;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 4jj");
    let start = (app.track_row, app.track_field);
    press(&mut app, "/res");
    special(&mut app, KeyCode::Enter);
    assert_eq!(column_name(&app), "Res");
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!((app.track_row, app.track_field), start);
    assert_eq!(app.view, View::Track);

    // It goes back to the page it left, too.
    press(&mut app, "/res");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "L");
    assert_eq!(app.track_page, Page::Mod);
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!(app.track_page, Page::Sound);
    assert_eq!((app.track_row, app.track_field), start);
}

#[test]
fn a_jumps_labels_cover_the_values_and_the_search_shows_below() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4/res");
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), &app))
        .unwrap();
    let label = &jump_labels(&app)[0][..1];
    let text = track_text(&app).join("\n");
    assert!(
        text.contains(&format!("Res{label}      {label}000 ")),
        "{text}"
    );
    assert!(text.contains("Cut       FF00 "), "{text}");
    let buffer = terminal.backend().buffer().clone();
    let y = (0..24)
        .find(|y| (0..3).map(|x| buffer[(x, *y)].symbol()).collect::<String>() == "Res")
        .unwrap();
    assert_eq!(buffer[(10, y)].bg, vt_tui::theme::PEACH);

    let backend = ratatui::backend::TestBackend::new(80, 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::status::draw(frame, frame.area(), &app))
        .unwrap();
    let last: String = (0..4)
        .map(|x| terminal.backend().buffer()[(x, 1)].symbol().to_owned())
        .collect();
    assert_eq!(last, "/res");
}

#[test]
fn a_jumps_labels_also_follow_the_names_being_searched() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4/cu");
    let line = |app: &App, name: &str| {
        track_text(app)
            .into_iter()
            .find(|line| line.starts_with(name))
            .unwrap()
    };
    // Over the letter after what's typed, in the same columns as without a label.
    assert!(line(&app, "Cuf").starts_with("Cuf       fF00 ▕"));
    assert!(line(&app, "Low cuj").starts_with("Low cuj   j0   ▕"));
    assert!(line(&app, "High cuk").starts_with("High cuk  kF   ▕"));
    assert!(line(&app, "Res").starts_with("Res       0000 ▕"));
    // It moves along as you type, and past the end of a name it sits just after it.
    press(&mut app, "t");
    assert!(line(&app, "Cutf").starts_with("Cutf      fF00 ▕"));
    special(&mut app, KeyCode::Esc);

    // A name over two rows has both their labels, and the columns stay put.
    press(&mut app, "L/dep");
    let labels = jump_labels(&app);
    let text = track_text(&app);
    let heading = text.iter().find(|line| line.contains("Attack")).unwrap();
    assert!(
        labels.iter().all(|label| label.ends_with("Depth")),
        "{labels:?}"
    );
    let after = &heading[heading.find("Dep").unwrap() + 3..];
    assert!(
        !after.starts_with("th"),
        "two labels over the rest: {heading}"
    );
    assert!(heading.contains("Reset"), "{heading}");
    let at = |line: &str, word: &str| line.find(word).unwrap();
    special(&mut app, KeyCode::Esc);
    let plain = track_text(&app);
    let before = plain.iter().find(|line| line.contains("Attack")).unwrap();
    assert_eq!(at(heading, "Reset"), at(before, "Reset"));
}

#[test]
fn a_jump_works_on_the_mixer_and_stays_on_the_cursors_track() {
    use vt_tui::track_page::Control;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 6l");
    assert_eq!(app.view, View::Mixer);
    assert_eq!(app.cursor.track, 1);
    press(&mut app, "/s");
    assert_eq!(app.mode, Mode::Jump);
    let jump = app.field_jump.as_ref().unwrap();
    let solo = jump.label(Control::Solo).unwrap();
    assert!(jump.label(Control::Level).is_none());

    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::mixer::draw(frame, frame.area(), &app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let line = |y: u16| {
        (0..80)
            .map(|x| buffer[(x, y)].symbol().to_owned())
            .collect::<String>()
    };
    // The name keeps its place with the label in it, and only this track's value has one.
    let y = (0..24)
        .find(|y| line(*y).starts_with(&format!("S{solo}lo")))
        .unwrap();
    assert_eq!(buffer[(1, y)].bg, vt_tui::theme::PEACH);
    assert_eq!(buffer[(7, y)].symbol(), "-", "track 1");
    assert_eq!(buffer[(14, y)].symbol(), solo.to_string(), "track 2");
    assert_eq!(buffer[(14, y)].bg, vt_tui::theme::PEACH);

    press(&mut app, &solo.to_string());
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(column_name(&app), "Solo");
    assert_eq!(app.cursor.track, 1);
    assert_eq!(app.view, View::Mixer);

    // Ctrl-O goes back to the column it left.
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!(column_name(&app), "Level");
    assert_eq!(app.view, View::Mixer);
}

#[test]
fn slash_only_jumps_on_the_parameter_pages() {
    let (mut app, _realtime) = app();
    press(&mut app, " 3/");
    assert_ne!(app.mode, Mode::Jump);
    assert!(app.field_jump.is_none());
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
    press(&mut app, " 4");
    assert!(!draw(&app).contains("Grid"), "{}", draw(&app));
}

fn alt(app: &mut App, name: &str) {
    app.key(parse_sequence(name).unwrap()[0], Instant::now());
}

/// A key with Cmd, pressed and let go as the terminal sends it, through the check that drops
/// unbound Cmd keys.
fn with_cmd(app: &mut App, name: &str) {
    use ratatui::crossterm::event::KeyEventKind;
    let key = parse_sequence(name).unwrap()[0];
    let now = Instant::now();
    app.key_event(key, KeyEventKind::Press, now);
    app.key_event(key, KeyEventKind::Release, now);
}

fn written_notes(app: &App) -> Vec<Note> {
    app.phrase_here()
        .unwrap()
        .steps()
        .iter()
        .map(|step| step.note)
        .filter(|note| *note != Note::Empty)
        .collect()
}

#[test]
fn with_a_scale_the_home_row_goes_up_it_from_the_root() {
    let (mut app, _realtime) = app();
    command(&mut app, "set root=d");
    command(&mut app, "set scale=dorian");
    assert!(app.scale.on, "setting one turns it on");
    entering_steps_down(&mut app);
    tap(&mut app, "iasdfghjkl");
    assert_eq!(
        written_notes(&app),
        [62, 64, 65, 67, 69, 71, 72, 74, 76].map(Note::On)
    );
}

#[test]
fn with_a_scale_the_row_above_is_silent_and_note_off_and_octave_still_work() {
    let (mut app, _realtime) = app();
    command(&mut app, "set scale=major");
    entering_steps_down(&mut app);
    let start = app.cursor.step;
    press(&mut app, "iwetyuop");
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

/// The FX page's text for a slot: the line with its number, and the line below.
fn fx_line(app: &App, slot: usize) -> String {
    let lines = track_page_text(app);
    let at = lines
        .iter()
        .position(|line| line.starts_with(&format!("{}  ", slot + 1)))
        .unwrap();
    format!("{}\n{}", lines[at], lines[at + 1])
}

fn on_fx_page() -> (App, vt_engine::link::Realtime) {
    let (mut app, realtime) = app();
    press(&mut app, " 4LL");
    (app, realtime)
}

#[test]
fn l_reaches_the_fx_page_and_stops_there() {
    use vt_tui::track_page::Page;
    let (mut app, _realtime) = on_fx_page();
    assert_eq!(app.track_page, Page::Fx);
    press(&mut app, "L");
    assert_eq!(app.track_page, Page::Fx, "it stops at the last");
    press(&mut app, "H");
    assert_eq!(app.track_page, Page::Mod);
    press(&mut app, "L");
    assert!(fx_line(&app, 0).starts_with("1  ---  "));
    assert!(fx_line(&app, 2).starts_with("3  ---  "));
}

#[test]
fn a_slot_takes_an_effect_and_shows_its_settings_from_its_list() {
    use vt_model::insert::Insert;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    let depth = app.history.undo_depth();
    assert_eq!(column_name(&app), "Effect");

    // Nothing but the effect, until there's something in the slot.
    press(&mut app, "$");
    assert_eq!(column_name(&app), "Effect");
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].inserts[0], Insert::ALL[1]);
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    let line = fx_line(&app, 0);
    assert!(line.starts_with("1  Distort"), "{line}");
    assert!(line.contains("Type     Soft"), "{line}");
    assert!(line.contains("Amount   40 ▕"), "{line}");
    assert!(line.contains("Mix       FF ▕█████▏"), "{line}");
    assert!(line.contains("Gain      E0 ▕"), "{line}");

    press(&mut app, "l");
    assert_eq!(column_name(&app), "Type");
    press(&mut app, "=");
    assert!(fx_line(&app, 0).contains("Type     Hard"));
    press(&mut app, "-");
    press(&mut app, "-");
    let line = fx_line(&app, 0);
    assert!(
        line.contains("Type     Distortion"),
        "it goes round: {line}"
    );
    // A pedal has a tone control as well.
    assert!(line.contains("Tone      80 ▕"), "{line}");
    press(&mut app, "-");
    assert!(fx_line(&app, 0).contains("Type     Overdrive"));
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "fold");
    special(&mut app, KeyCode::Enter);
    assert!(fx_line(&app, 0).contains("Type     Fold"));
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker, "typing a choice is the list too");
    special(&mut app, KeyCode::Esc);

    press(&mut app, "j");
    assert_eq!(column_name(&app), "Amount");
    press(&mut app, "+");
    assert!(fx_line(&app, 0).contains("Amount   50"));
    press(&mut app, "ia7");
    assert!(fx_line(&app, 0).contains("Amount   A7"));
    press(&mut app, "x");
    assert!(fx_line(&app, 0).contains("Amount   40"));
    let depth = app.history.undo_depth();
    press(&mut app, "x");
    assert_eq!(
        app.history.undo_depth(),
        depth,
        "already there, so no undo step"
    );

    press(&mut app, "kl");
    assert_eq!(column_name(&app), "Mix");
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Gain");
    press(&mut app, "_");
    assert!(fx_line(&app, 0).contains("Gain      D0"));
    press(&mut app, "x");
    assert!(fx_line(&app, 0).contains("Gain      E0"));
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Gain", "it stops at the last it uses");

    // Only this slot, and only this track.
    assert_eq!(app.song().tracks[0].inserts[1], Insert::Empty);
    assert_eq!(app.song().tracks[1].inserts[0], Insert::Empty);
}

#[test]
fn emptying_a_slot_keeps_the_cursor_valid() {
    use vt_model::insert::Insert;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "=$");
    assert_eq!(column_name(&app), "Mix");
    press(&mut app, "0x");
    assert_eq!(app.song().tracks[0].inserts[0], Insert::Empty);
    // Undo puts the settings back, and taking them away again leaves the cursor on the effect.
    press(&mut app, "u$");
    assert_eq!(column_name(&app), "Mix");
    press(&mut app, "u");
    assert_eq!(app.song().tracks[0].inserts[0], Insert::Empty);
    assert_eq!(column_name(&app), "Effect");
    // Another track's slot being empty does the same.
    press(&mut app, "=$W");
    assert_eq!(column_name(&app), "Effect");
    press(&mut app, "B");
    assert_eq!(column_name(&app), "Mix");
}

#[test]
fn the_effect_list_chooses_one_and_keeps_settings_if_it_is_already_there() {
    use vt_model::insert::Insert;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "dist");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[0].inserts[0], Insert::ALL[1]);
    press(&mut app, "lj");
    press(&mut app, "i10");
    press(&mut app, "0K");
    press(&mut app, "dist");
    special(&mut app, KeyCode::Enter);
    assert!(
        fx_line(&app, 0).contains("Amount   10"),
        "{}",
        fx_line(&app, 0)
    );
}

#[test]
fn a_jump_finds_a_slots_settings_only_where_they_exist() {
    use vt_tui::track_page::Control;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "/amo");
    assert_eq!(
        app.field_jump.as_ref().unwrap().query,
        "",
        "nothing to find yet"
    );
    special(&mut app, KeyCode::Esc);
    press(&mut app, "=j=");
    press(&mut app, "/amo");
    let jump = app.field_jump.as_ref().unwrap();
    assert!(jump.label(Control::InsertParam(0, 1)).is_some());
    let second = jump.label(Control::InsertParam(1, 1)).unwrap();
    assert!(jump.label(Control::InsertParam(2, 1)).is_none());
    // The name and the value both carry it, in the columns they had.
    let line = fx_line(&app, 1);
    assert!(
        line.contains(&format!("Amo{second}nt   {second}0 ▕")),
        "{line}"
    );
    press(&mut app, &second.to_string());
    assert_eq!(column_name(&app), "Amount");
    assert_eq!(app.track_row, 3);
}

#[test]
fn an_insert_set_on_the_page_is_heard() {
    let (mut app, mut realtime) = app();
    with_a_sound(&mut app);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    block(&mut realtime, &mut app);
    let _ = loudest(&mut realtime, 96_000);
    press(&mut app, " 3k;");
    let plain = loudest(&mut realtime, 2000);
    assert!(plain > 0.0);
    press(&mut app, " 4LL=lj");
    press(&mut app, "iff");
    block(&mut realtime, &mut app);
    press(&mut app, " 3;");
    let driven = loudest(&mut realtime, 2000);
    assert!(driven > 0.0);
    assert_ne!(plain, driven, "{plain} and {driven}");
}

#[test]
fn settings_are_in_columns_of_two_and_keys_move_as_they_are_seen() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "=j=gg");
    let name = |app: &App| column_name(app);
    assert_eq!(name(&app), "Effect");
    // On the effect, j goes straight to the next slot's effect.
    press(&mut app, "j");
    assert_eq!((name(&app).as_str(), app.track_row), ("Effect", 2));
    press(&mut app, "j");
    assert_eq!(
        (name(&app).as_str(), app.track_row),
        ("Effect", 4),
        "an empty slot too"
    );
    press(&mut app, "j");
    assert_eq!(name(&app), "Effect", "it stops at the last");
    press(&mut app, "gg");

    // Across then down the columns: 1 over 2, 3 over 4.
    press(&mut app, "l");
    assert_eq!(name(&app), "Type");
    press(&mut app, "j");
    assert_eq!(name(&app), "Amount");
    press(&mut app, "l");
    assert_eq!(name(&app), "Gain");
    press(&mut app, "k");
    assert_eq!(name(&app), "Mix");
    press(&mut app, "h");
    assert_eq!(name(&app), "Type");

    // On through the next slot, and a count goes as far as it says.
    press(&mut app, "jj");
    assert_eq!((name(&app).as_str(), app.track_row), ("Type", 2));
    press(&mut app, "k");
    assert_eq!((name(&app).as_str(), app.track_row), ("Amount", 1));
    press(&mut app, "2j");
    assert_eq!((name(&app).as_str(), app.track_row), ("Amount", 3));
}

#[test]
fn braces_go_to_the_same_setting_in_the_next_slot() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "=}={"); // Slots 1 and 2 hold an effect.
    press(&mut app, "lj");
    assert_eq!((column_name(&app).as_str(), app.track_row), ("Amount", 1));
    press(&mut app, "}");
    assert_eq!((column_name(&app).as_str(), app.track_row), ("Amount", 3));
    // An empty slot has only its effect, and the setting comes back after it.
    press(&mut app, "}");
    assert_eq!(column_name(&app), "Effect");
    press(&mut app, "{");
    assert_eq!((column_name(&app).as_str(), app.track_row), ("Amount", 3));
    press(&mut app, "{{{");
    assert_eq!(app.track_row, 1, "it stops at the first");
    // A count, and nothing on the other pages but saying so.
    press(&mut app, "2}");
    assert_eq!(app.track_row, 5);
    press(&mut app, "H");
    let (row, field) = (app.track_row, app.track_field);
    press(&mut app, "}");
    assert_eq!((app.track_row, app.track_field), (row, field));
    assert!(app.status.contains("FX page"), "{}", app.status);
}

#[test]
fn a_jump_labels_a_slots_effect_once() {
    use vt_tui::track_page::Control;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "/effect");
    let jump = app.field_jump.as_ref().unwrap();
    let labels: Vec<char> = (0..3)
        .map(|slot| jump.label(Control::Insert(slot)).unwrap())
        .collect();
    assert_eq!(
        labels
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );
    press(&mut app, &labels[2].to_string());
    assert_eq!(app.track_row, 4);
}

#[test]
fn crush_shows_whole_bits_that_step_and_stop_at_each_end() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "=lK");
    press(&mut app, "crush");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Bits");
    let line = fx_line(&app, 0);
    assert!(line.contains("Bits     9"), "{line}");
    assert!(!line.contains("Amount"), "{line}");
    press(&mut app, "=");
    assert!(fx_line(&app, 0).contains("Bits     10"));
    press(&mut app, "=====");
    assert!(
        fx_line(&app, 0).contains("Bits     12"),
        "it stops at the top"
    );
    press(&mut app, "x");
    assert!(fx_line(&app, 0).contains("Bits     9"));
    for _ in 0..12 {
        press(&mut app, "-");
    }
    assert!(
        fx_line(&app, 0).contains("Bits     2"),
        "it stops at the bottom"
    );
    // It's a list to choose from, and not typed.
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "6");
    special(&mut app, KeyCode::Enter);
    assert!(
        fx_line(&app, 0).contains("Bits     6"),
        "{}",
        fx_line(&app, 0)
    );
    // Another mode has its own setting, as it was.
    press(&mut app, "k-");
    assert!(
        fx_line(&app, 0).contains("Type     Sine"),
        "{}",
        fx_line(&app, 0)
    );
    assert!(
        fx_line(&app, 0).contains("Amount   40"),
        "{}",
        fx_line(&app, 0)
    );
}

#[test]
fn a_slots_settings_are_on_the_dest_list_only_while_it_holds_something() {
    let (mut app, _realtime) = app();
    // LFO 1's Dest is on the Mod page, its third row.
    press(&mut app, " 4Ljj");
    let list = |app: &mut App| {
        press(app, "K/");
        let labels: Vec<String> = app
            .picker
            .as_ref()
            .unwrap()
            .items()
            .iter()
            .map(listed)
            .collect();
        // The first Esc leaves the search for the list, and the second closes it.
        special(app, KeyCode::Esc);
        special(app, KeyCode::Esc);
        labels
    };
    assert!(!list(&mut app).iter().any(|label| label.starts_with("1 ")));

    press(&mut app, "L="); // Distort in slot 1.
    press(&mut app, "Hjj");
    let labels = list(&mut app);
    assert!(
        labels
            .iter()
            .any(|label| label.starts_with("1 Amount  Distort in slot 1")),
        "{labels:?}"
    );

    // Choosing one saves it, and it reads back on the page.
    press(&mut app, "K/1 amount");
    special(&mut app, KeyCode::Enter);
    let target = SettingId::slot(0, vt_model::settings::Effect::Distort, 1).unwrap();
    assert_eq!(app.song().tracks[0].lfos[0].destination, Some(target));
    let lfo = |app: &App| {
        track_page_text(app)
            .into_iter()
            .find(|line| line.starts_with("LFO 1"))
            .unwrap()
    };
    assert!(lfo(&app).contains("1 Amount"), "{}", lfo(&app));

    // Emptying the slot leaves it chosen, doing nothing, and saying so.
    press(&mut app, "Lx");
    press(&mut app, "H");
    assert_eq!(app.song().tracks[0].lfos[0].destination, Some(target));
    assert!(lfo(&app).contains("1 ---"), "{}", lfo(&app));
}

#[test]
fn stepping_dest_goes_through_a_slots_settings() {
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "=H");
    press(&mut app, "jj");
    // Past the thirteen a sample track has, as a synth's settings aren't on it.
    for _ in 0..14 {
        press(&mut app, "=");
    }
    assert_eq!(
        app.song().tracks[0].lfos[0].destination,
        Some(SettingId::slot(0, vt_model::settings::Effect::Distort, 0).unwrap()),
    );
}

#[test]
fn a_slots_setting_is_chosen_for_a_step_by_its_label_and_saved_by_its_key() {
    use vt_model::settings::{Effect, SettingId};
    use vt_model::step::FxCommand;
    let (mut app, _realtime) = app();
    press(&mut app, " 4LL= 3");
    let command = |app: &App, step: usize| app.phrase_here().unwrap().steps()[step].fx[0].command;
    let setting = |at| FxCommand::Set(SettingId::slot(0, Effect::Distort, at).unwrap());

    // The list has what's on the track, by label and then by group and name.
    press(&mut app, "lllllK");
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(listed)
        .collect();
    assert!(
        labels.iter().any(|label| label.starts_with("1MIX  1 Mix")),
        "{labels:?}"
    );
    assert!(
        !labels.iter().any(|label| label.starts_with("2")),
        "{labels:?}"
    );
    press(&mut app, "1amnt");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "80");
    assert_eq!(command(&app, 0), Some(setting(1)));
    assert_eq!(app.phrase_here().unwrap().steps()[0].fx[0].value, 0x8000);
    let text = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(
        text.contains("1.distort.amount:80"),
        "a file holds the setting's key: {text}"
    );

    // Typed, a slot's label needs its digit, and `Enter` takes it to the value.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "ggj0llllli1mix");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "40");
    assert_eq!(command(&app, 1), Some(setting(4)));

    // A label that isn't there says so, and opens the list searching for it.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "ggjj0llllliamnt");
    special(&mut app, KeyCode::Enter);
    assert_eq!(command(&app, 2), None);
    assert!(app.status.contains("isn't a label"), "{}", app.status);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.query, "amnt");
    assert_eq!(
        picker.current().map(listed).as_deref(),
        Some("1AMNT  1 Amount")
    );
    special(&mut app, KeyCode::Enter);
    press(&mut app, "20");
    assert_eq!(command(&app, 2), Some(setting(1)));
}

#[test]
fn the_phrase_view_names_a_slots_setting_in_a_wide_column() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4LL= 3lllllK1amnt");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "80");
    special(&mut app, KeyCode::Esc);
    let backend = ratatui::backend::TestBackend::new(120, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::phrase::draw(frame, frame.area(), &app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = (0..12)
        .map(|y| {
            (0..120)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect();
    assert!(text.contains("1AMNT 80"), "{text}");
    assert!(text.contains("---   00"), "{text}");
}

#[test]
fn the_fx_pages_bars_follow_what_plays() {
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "=");
    let mut live = vt_engine::track_state::LiveValues {
        level: 0xE0,
        pan: 0x80,
        cutoff: 0xFF,
        resonance: 0,
        send_a: 0,
        send_b: 0,
        machine: [0x80, 0, 0x80, 0xCC, 0, 0, 0, 0],
        inserts: Default::default(),
        reduction: Default::default(),
        bypassed: Default::default(),
    };
    live.inserts[0][..4].copy_from_slice(&[0, 0xFF, 0xFF, 0xE0]);
    app.live = Some(live);
    let line = fx_line(&app, 0);
    // The number stays what's set, and the fill goes where it's playing, with a mark on the setting.
    assert!(line.contains("Amount   40 ▕█│███▏"), "{line}");
    assert!(line.contains("Mix       FF ▕█████▏"), "{line}");
}

#[test]
fn the_lists_only_hold_the_loaded_machines_settings_by_their_names() {
    use vt_model::song::{Machine, SynthVoice};
    let (mut app, _realtime) = app();
    let labels = |app: &mut App, keys: &str| {
        press(app, keys);
        let found: Vec<String> = app
            .picker
            .as_ref()
            .unwrap()
            .items()
            .iter()
            .map(listed)
            .collect();
        // The first Esc leaves the search for the list, and the second closes it.
        special(app, KeyCode::Esc);
        special(app, KeyCode::Esc);
        found
    };
    let on = |labels: &[String], name: &str| {
        labels
            .iter()
            .any(|label| label.starts_with(&format!("{name}  ")))
    };
    // A sample track has no synth settings, in Dest or for a step.
    let dest = labels(&mut app, " 4LjjK/");
    assert!(!on(&dest, "Har") && on(&dest, "Pitch"), "{dest:?}");
    let steps = labels(&mut app, " 3lllllK");
    assert!(!on(&steps, "HAR") && on(&steps, "CUT"), "{steps:?}");

    // On a synth they're there under their names on this voice.
    press(&mut app, " 4HKfm");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::Fm)
    );
    let dest = labels(&mut app, "LggjjK/");
    assert!(on(&dest, "Ratio") && on(&dest, "Feedback"), "{dest:?}");
    let steps = labels(&mut app, " 3K");
    assert!(on(&steps, "INDX"), "{steps:?}");

    // One chosen stays through a change of machine, and says what happened.
    press(&mut app, " 4K/ratio");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].lfos[0].destination,
        Some(SettingId::voice(SynthVoice::Fm, 0).unwrap())
    );
    press(&mut app, "HggK");
    press(&mut app, "sa");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].lfos[0].destination,
        Some(SettingId::voice(SynthVoice::Fm, 0).unwrap())
    );
    press(&mut app, "L");
    let line = track_page_text(&app)
        .into_iter()
        .find(|line| line.starts_with("LFO 1"))
        .unwrap();
    assert!(line.contains("Ratio ---"), "{line}");
}

#[test]
fn an_lfos_dest_list_only_reaches_the_lfos_after_it() {
    let (mut app, _realtime) = app();
    let labels = |app: &mut App, keys: &str| {
        press(app, keys);
        let found: Vec<String> = app
            .picker
            .as_ref()
            .unwrap()
            .items()
            .iter()
            .map(listed)
            .collect();
        // The first Esc leaves the search for the list, and the second closes it.
        special(app, KeyCode::Esc);
        special(app, KeyCode::Esc);
        found
    };
    let has = |labels: &[String], name: &str| {
        labels
            .iter()
            .any(|label| label.starts_with(&format!("{name}  ")))
    };
    let first = labels(&mut app, " 4LjjK/");
    assert!(
        has(&first, "L2 Depth") && has(&first, "L2 Phase"),
        "{first:?}"
    );
    assert!(!has(&first, "L1 Depth"), "{first:?}");
    let second = labels(&mut app, "jK/");
    assert!(
        !has(&second, "L1 Depth") && !has(&second, "L2 Depth"),
        "{second:?}"
    );
    let envelope = labels(&mut app, "ggK/");
    assert!(
        has(&envelope, "L1 Depth") && has(&envelope, "L2 Phase"),
        "{envelope:?}"
    );

    press(&mut app, "jjK/l2 depth");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].lfos[0].destination,
        Some(SettingId::lfo(1, 0).unwrap())
    );
    let line = track_page_text(&app)
        .into_iter()
        .find(|line| line.starts_with("LFO 1"))
        .unwrap();
    assert!(line.contains("L2 Depth"), "{line}");
}

#[test]
fn dest_is_a_tree_of_categories_that_fold_and_a_search_flattens() {
    use vt_tui::input::picker::{Focus, Row};
    let (mut app, _realtime) = app();
    // Distort in slot 1, then LFO 1's Dest.
    press(&mut app, " 4LL=HjjK");
    let current = |app: &App| {
        app.picker
            .as_ref()
            .unwrap()
            .current()
            .map(|item| item.label.clone())
            .unwrap_or_default()
    };
    let picker = |app: &App| app.picker.as_ref().unwrap().clone();
    assert_eq!(picker(&app).focus, Focus::List);
    assert_eq!(current(&app), "---", "on what's chosen: nothing");
    assert!(
        picker(&app).rows().iter().all(|row| match row {
            Row::Category(category) => !picker(&app).category(*category).2,
            Row::Item(item) => !picker(&app).is_nested(*item),
        }),
        "every category folded, as none holds what's chosen"
    );

    // Down to Filter's heading, l into it, and down its list.
    press(&mut app, "jl");
    assert_eq!(current(&app), "Cut");
    press(&mut app, "j");
    assert_eq!(current(&app), "Res");
    assert_eq!(app.mode, Mode::Picker, "moving doesn't choose");

    // h closes it and goes up to it.
    press(&mut app, "h");
    assert_eq!(current(&app), "", "on the heading");
    let heading = picker(&app).rows()[picker(&app).selected];
    assert!(matches!(heading, Row::Category(_)));

    // A letter that isn't a key starts a search, which reaches every category.
    press(&mut app, "1 mi");
    assert_eq!(picker(&app).focus, Focus::Search);
    assert_eq!(current(&app), "1 Mix");
    // Esc goes back to the list, keeping what it found, as Esc leaves Insert mode.
    special(&mut app, KeyCode::Esc);
    assert_eq!(picker(&app).focus, Focus::List);
    assert_eq!(picker(&app).query, "1 mi");
    assert_eq!(current(&app), "1 Mix");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].lfos[0].destination,
        Some(SettingId::slot(0, vt_model::settings::Effect::Distort, 4).unwrap())
    );

    // It opens on what's chosen, inside its category, and Esc leaves it.
    press(&mut app, "K");
    assert_eq!(current(&app), "1 Mix");
    press(&mut app, "hk");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(
        app.song().tracks[0].lfos[0].destination,
        Some(SettingId::slot(0, vt_model::settings::Effect::Distort, 4).unwrap()),
        "changing nothing"
    );

    // On a sample track, Machine holds Pitch and Gate, and l opens it on Pitch.
    press(&mut app, "Kgg");
    let at = picker(&app)
        .rows()
        .iter()
        .position(|row| matches!(row, Row::Category(category) if picker(&app).category(*category) == ("Machine", 2, false)))
        .unwrap();
    for _ in 0..at {
        press(&mut app, "j");
    }
    press(&mut app, "l");
    assert_eq!(current(&app), "Pitch");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].lfos[0].destination,
        Some(SettingId::PITCH)
    );
}

#[test]
fn dests_window_draws_categories_as_dim_headings_with_counts() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4LjjKjl");
    let mut picker = app.picker.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(100, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::picker::draw(frame, frame.area(), &mut picker))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = (0..24)
        .map(|y| {
            (0..100)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect();
    let total = picker.items().len();
    assert!(
        text.contains(&format!("LFO destination {total}/{total}")),
        "{text}"
    );
    assert!(text.contains("│• ---"), "the chosen one is marked: {text}");
    assert!(text.contains("│▾ Filter  3"), "{text}");
    assert!(
        text.lines()
            .any(|line| line.contains("│    Cut ") && line.contains(" Filter cutoff")),
        "in its category, with its meaning: {text}"
    );
    assert!(text.contains("▸ Output  "), "{text}");
    let dim = vt_tui::theme::dim().fg;
    let heading = text
        .lines()
        .position(|line| line.contains("▸ Output"))
        .unwrap();
    let column = text.lines().nth(heading).unwrap().find('▸').unwrap();
    let column = text.lines().nth(heading).unwrap()[..column].chars().count();
    assert_eq!(buffer[(column as u16, heading as u16)].fg, dim.unwrap());
}

#[test]
fn a_plain_list_opens_on_the_list_and_typing_searches() {
    // `K` then typing on Machine, and `/` goes to the search.
    let (mut app, _realtime) = app();
    press(&mut app, " 4Kpl");
    assert_eq!(app.picker.as_ref().unwrap().query, "pl");
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal, "Esc clears the search, then closes");
    press(&mut app, "K/j");
    assert_eq!(app.picker.as_ref().unwrap().query, "j");
}

#[test]
fn dd_and_p_move_an_effect_and_swap_two_as_one_undo_step() {
    use vt_model::insert::{DistortType, Insert};
    let (mut app, _realtime) = on_fx_page();
    let inserts = |app: &App| app.song().tracks[0].inserts;
    let soft = Insert::ALL[1];
    let fold = soft.with_value(0, DistortType::Fold as u8);
    // Soft in slot 1, and Fold in slot 2.
    press(&mut app, "=}=lK");
    press(&mut app, "fold");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "gg0");
    assert_eq!(inserts(&app), [soft, fold, Insert::Empty]);
    let depth = app.history.undo_depth();

    // Slot 1 into slot 3, which is empty.
    press(&mut app, "dd");
    assert_eq!(inserts(&app), [Insert::Empty, fold, Insert::Empty]);
    assert!(app.status.contains("Distort"), "{}", app.status);
    press(&mut app, "}}p");
    assert_eq!(inserts(&app), [Insert::Empty, fold, soft]);
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    press(&mut app, "u");
    assert_eq!(inserts(&app), [soft, fold, Insert::Empty]);

    // Slot 1 onto slot 2: they swap.
    press(&mut app, "gg");
    press(&mut app, "dd}p");
    assert_eq!(inserts(&app), [fold, soft, Insert::Empty]);
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(inserts(&app), [soft, fold, Insert::Empty]);

    // Back where it came from changes nothing.
    press(&mut app, "ggddp");
    assert_eq!(inserts(&app), [soft, fold, Insert::Empty]);
}

#[test]
fn dd_and_p_say_why_when_there_is_nothing_to_move() {
    use vt_model::insert::Insert;
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "dd");
    assert!(app.status.contains("nothing"), "{}", app.status);
    press(&mut app, "p");
    assert!(app.status.contains("dd"), "{}", app.status);
    // On the other pages they aren't for steps or anything else.
    press(&mut app, "=dd");
    press(&mut app, "H");
    press(&mut app, "p");
    assert!(app.status.contains("FX page"), "{}", app.status);
    assert_eq!(app.view, View::Track);
    assert_eq!(app.song().tracks[0].inserts[0], Insert::Empty);
    // It's still there to put, across tracks too.
    press(&mut app, "LW");
    press(&mut app, "p");
    assert_eq!(app.song().tracks[1].inserts[0], Insert::ALL[1]);
    assert_eq!(app.song().tracks[0].inserts[0], Insert::Empty);
}

#[test]
fn the_sound_follows_the_new_order() {
    use vt_engine::track_state::LiveValues;
    let (mut app, mut realtime) = app();
    with_a_sound(&mut app);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, " 4LL=dd}p");
    block(&mut realtime, &mut app);
    press(&mut app, " 3k;");
    let _ = loudest(&mut realtime, 4000);
    app.tick();
    let live: LiveValues = app.live.unwrap();
    assert_eq!(live.inserts[1][1], 0x40, "the second slot has it");
    assert_eq!(live.inserts[0][..4], [0; 4], "the first doesn't");
}

fn lane_of(app: &App, slot: usize) -> Option<vt_model::song::Lane> {
    app.song().tracks[0].phrases[&0].lane(slot)
}

#[test]
fn lane_makes_an_fx_column_a_lane_and_the_loop_keys_then_set_it() {
    use vt_model::song::Lane;
    let (mut app, _realtime) = app();
    press(&mut app, "llllljj");
    assert_eq!(app.cursor.field, vt_tui::app::Field::FxCommand(0));
    command(&mut app, "lane 3");
    assert_eq!(
        lane_of(&app, 0),
        Some(Lane {
            start: 2,
            length: 3
        })
    );
    assert!(app.status.contains("FX 1"), "{}", app.status);
    // Now it has a lane, the loop keys set it rather than the phrase.
    press(&mut app, "jjjjge");
    assert_eq!(
        lane_of(&app, 0),
        Some(Lane {
            start: 2,
            length: 5
        })
    );
    command(&mut app, "len 4");
    assert_eq!(
        lane_of(&app, 0),
        Some(Lane {
            start: 2,
            length: 4
        })
    );
    press(&mut app, "kgs");
    assert_eq!(
        lane_of(&app, 0),
        Some(Lane {
            start: 5,
            length: 4
        })
    );
    assert_eq!(
        app.song().tracks[0].phrases[&0].len(),
        16,
        "the phrase keeps its loop"
    );
    press(&mut app, "kkkkge");
    assert!(app.status.contains("above its start"), "{}", app.status);
    command(&mut app, "lane");
    assert_eq!(lane_of(&app, 0), None, "following the notes again");
    press(&mut app, "u");
    assert_eq!(
        lane_of(&app, 0),
        Some(Lane {
            start: 5,
            length: 4
        }),
        "one undo each"
    );
}

#[test]
fn the_loop_keys_on_an_fx_column_without_a_lane_set_the_phrases_loop() {
    let (mut app, _realtime) = app();
    press(&mut app, "lllll7jge");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 8);
    command(&mut app, "len 6");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 6);
    assert_eq!(lane_of(&app, 0), None, "the column still follows the notes");
    press(&mut app, "hhh");
    command(&mut app, "lane 3");
    assert!(app.status.contains("FX column"), "{}", app.status);
    assert_eq!(lane_of(&app, 0), None);
}

#[test]
fn a_lane_shows_its_length_and_where_it_is_playing() {
    let (mut app, _realtime) = app();
    press(&mut app, "llllljrSNA");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "20gg");
    command(&mut app, "lane 3");
    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    // Place 4 is the notes' row 4, and the lane's row 4 % 3 = 1.
    app.set_playhead(4);
    app.top = 0;
    let buffer = screen(&app);
    let line = |y: u16| -> String { (0..80).map(|x| buffer[(x, y)].symbol()).collect() };
    assert!(line(0).contains("FX1:3"), "{}", line(0));
    let all: Vec<String> = (0..24).map(line).collect();
    let lane_row = all
        .iter()
        .find(|l| l.contains(">SNA"))
        .unwrap_or_else(|| panic!("no lane playhead:\n{}", all.join("\n")));
    assert!(lane_row.contains(" 01 "), "{lane_row}");
    let note_row = (0..24)
        .map(line)
        .find(|l| l.contains(" 04 >"))
        .unwrap_or_default();
    assert!(!note_row.is_empty(), "the notes' own playhead is on row 4");
}

#[test]
fn gg_and_g_on_a_column_with_a_lane_go_by_the_lane() {
    let (mut app, _realtime) = app();
    // FX 1's lane is steps 4 to 6 of the phrase's 16.
    press(&mut app, "lllll4j");
    command(&mut app, "lane 3");
    press(&mut app, "G");
    assert_eq!(app.cursor.step, 6, "the lane's last step");
    press(&mut app, "gg");
    assert_eq!(app.cursor.step, 4, "the lane's first");
    press(&mut app, "2G");
    assert_eq!(app.cursor.step, 5, "a count goes by the lane too");
    press(&mut app, "hhG");
    assert_eq!(app.cursor.step, 15, "the notes go by the phrase's loop");
}

#[test]
fn k_on_the_gate_column_chooses_a_gate_and_keeps_its_pulses() {
    let (mut app, _realtime) = app();
    press(&mut app, "lllrMLT3ggK");
    assert_eq!(app.mode, Mode::Picker);
    // `h` is a key in the list, so `/` starts the search.
    press(&mut app, "/hld");
    special(&mut app, KeyCode::Enter);
    let gate = app.song().tracks[0].phrases[&0].steps()[0].gate.unwrap();
    assert_eq!(
        (gate.kind, gate.pulses),
        (vt_model::step::GateKind::Hold, 3)
    );
}

#[test]
fn a_beat_row_is_one_that_starts_on_a_beat_in_time() {
    let (mut app, _realtime) = app();
    // Rows 00 and 01 last 2 pulses each, so row 02 starts on the second beat.
    // Collapsed, a row a line, as the timeline would put rows 00 and 01 over two lines each.
    press(&mut app, "lllrSGL2jrSGL2ggza");
    app.top = 0;
    let buffer = screen(&app);
    let number_colour = |row: u16| buffer[(5, row + 2)].fg;
    let beat = number_colour(0);
    assert_ne!(number_colour(1), beat, "row 01 starts at step 2");
    assert_eq!(number_colour(2), beat, "row 02 starts at step 4");
    assert_ne!(number_colour(4), beat, "row 04 starts at step 6");
    assert_eq!(number_colour(6), beat, "row 06 starts at step 8");
}

/// The colour of the rule under a row of the phrase view's grid, at its first column, if it has one.
fn rule_under(buffer: &ratatui::buffer::Buffer, y: u16) -> Option<ratatui::style::Color> {
    let cell = &buffer[(9, y)];
    cell.modifier
        .contains(ratatui::style::Modifier::UNDERLINED)
        .then_some(cell.underline_color)
}

#[test]
fn a_rule_under_a_row_marks_where_the_next_beat_or_bar_begins() {
    let (mut app, _realtime) = app();
    command(&mut app, "len 20");
    app.top = 0;
    let buffer = screen(&app);
    let beat = Some(vt_tui::theme::SURFACE1);
    let bar = Some(vt_tui::theme::OVERLAY0);
    assert_eq!(rule_under(&buffer, 2), None, "row 01 is a step");
    assert_eq!(rule_under(&buffer, 5), beat, "row 04 starts a beat");
    assert_eq!(rule_under(&buffer, 13), beat, "row 0C starts a beat");
    assert_eq!(rule_under(&buffer, 17), bar, "row 10 starts a bar");
    assert_eq!(
        rule_under(&buffer, 21),
        None,
        "nothing follows the last row"
    );
    // The rule runs under the whole row, its FX too.
    let fx = &buffer[(40, 17)];
    assert!(fx.modifier.contains(ratatui::style::Modifier::UNDERLINED));
    assert_eq!(fx.underline_color, vt_tui::theme::OVERLAY0);
}

#[test]
fn the_rules_follow_rows_that_last_several_pulses() {
    let (mut app, _realtime) = app();
    // Rows 00 and 01 last 2 pulses each, so row 02 starts on the second beat.
    // Collapsed, a row a line, as the timeline would put rows 00 and 01 over two lines each.
    press(&mut app, "lllrSGL2jrSGL2ggza");
    app.top = 0;
    let buffer = screen(&app);
    assert_eq!(rule_under(&buffer, 2), None, "row 01 starts at step 2");
    assert_eq!(
        rule_under(&buffer, 3),
        Some(vt_tui::theme::SURFACE1),
        "row 02 starts at step 4"
    );
    assert_eq!(rule_under(&buffer, 4), None, "row 03 starts at step 5");
}

#[test]
fn the_loops_last_row_is_followed_by_a_bar() {
    let (mut app, _realtime) = app();
    command(&mut app, "len 4");
    press(&mut app, "2]l");
    app.top = 0;
    let buffer = screen(&app);
    // Rows -02 and -01 come before the loop, which plays rows 00 to 03 of those kept.
    assert_eq!(row_number_at(&buffer, 7), " 03");
    let bar = Some(vt_tui::theme::OVERLAY0);
    assert_eq!(
        rule_under(&buffer, 3),
        bar,
        "row -01 is followed by the loop"
    );
    assert_eq!(
        rule_under(&buffer, 7),
        bar,
        "the loop starts again after row 03"
    );
    assert_eq!(rule_under(&buffer, 4), None);
}

#[test]
fn an_lfos_depth_is_locked_by_its_label_and_reads_in_its_units() {
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, "llllli");
    press(&mut app, "dep1");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "9F");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("lfo1.depth:9F"), "{step}");
    assert_eq!(column_name(&app), "LFO 1: Depth");
    // Fine-tuned to a depth of exactly +40.
    press(&mut app, "l");
    for _ in 0..0xE0 {
        press(&mut app, "=");
    }
    assert_eq!(value_shown(&app).as_deref(), Some("+40"));
    press(&mut app, "l");
    press(&mut app, "i");
    press(&mut app, "ptch");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "87");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    assert_eq!(value_shown(&app).as_deref(), Some("+7 st"));
}

#[test]
fn the_sound_page_stops_on_a_values_fine_digits() {
    use vt_model::settings::SettingId;
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = on_track_page();
    assert_eq!(column_name(&app), "Cut");
    let cut = |app: &App| {
        let track = &app.song().tracks[0];
        (track.cutoff, track.fine(SettingId::CUT))
    };
    // Cut starts open, at the top, where there's no fine part, so come down a step first.
    press(&mut app, "-");
    assert_eq!(cut(&app), (0xFE, 0));
    press(&mut app, "l");
    assert!(app.track_fine);
    assert_eq!(column_name(&app), "Cut", "still Cut, on its fine digits");
    press(&mut app, "i80");
    assert_eq!(cut(&app), (0xFE, 0x80));
    assert!(
        track_page_text(&app)
            .iter()
            .any(|line| line.starts_with("Cut       FE80"))
    );
    // The value's own digits keep it, and the top has none.
    press(&mut app, "h-");
    assert_eq!(cut(&app), (0xFD, 0x80));
    press(&mut app, "l");
    for _ in 0..0x80 {
        press(&mut app, "=");
    }
    assert_eq!(cut(&app), (0xFE, 0), "a fine nudge carries");
    press(&mut app, "-");
    assert_eq!(cut(&app), (0xFD, 0xFF));
    press(&mut app, "x");
    assert_eq!(cut(&app), (0xFD, 0), "x sets it back to 00");
    press(&mut app, "16-");
    assert_eq!(cut(&app), (0xFC, 0xF0));
    assert!(value_shown(&app).is_some());
    press(&mut app, "h==");
    assert_eq!(cut(&app), (0xFE, 0xF0));
    press(&mut app, "=");
    assert_eq!(cut(&app), (0xFF, 0), "none at the top");
    // Drive is whole, so it has no fine digits to stop on.
    press(&mut app, "/dri");
    special(&mut app, KeyCode::Enter);
    assert_eq!(column_name(&app), "Drive");
    press(&mut app, "l");
    assert!(!app.track_fine);
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
    press(&mut app, " 4;a");
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
    press(&mut app, " 4;a");
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
    assert_eq!(voices(&mut realtime), 1, "C-5 twice is one note");
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
fn semicolon_on_the_track_page_plays_the_sound_from_the_keyboard() {
    use vt_tui::views::status::column_name;
    let (mut app, mut realtime) = app();
    choose_machine(&mut app, "VA");
    let before = app.song().clone();
    press(&mut app, ";");
    assert_eq!(app.mode, Mode::Keys);
    assert_eq!(loudest(&mut realtime, 480), 0.0, "nothing until a key");
    press(&mut app, "a");
    assert!(loudest(&mut realtime, 4800) > 0.01, "a plays a note");
    press(&mut app, ">");
    assert_eq!(app.octave, 5, "> goes up an octave");
    assert!(Arc::ptr_eq(&before, app.song()), "nothing is written");
    // The page's own keys still work, so a setting changes while it plays.
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Detune");
    press(&mut app, "=");
    assert_eq!(machine_value(&app, 0), 0x81);
    assert_eq!(app.mode, Mode::Keys);
    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), &app))
        .unwrap();
    let corner = &terminal.backend().buffer()[(0, 0)];
    assert_eq!((corner.symbol(), corner.fg), ("╭", vt_tui::theme::YELLOW));
    press(&mut app, ";");
    assert_eq!(app.mode, Mode::Normal);
    // Leaving the track page leaves the keyboard too.
    press(&mut app, "; 3");
    assert_eq!((app.view, app.mode), (View::Phrase, Mode::Normal));
}

/// An app on the audio page, playing through the first of two devices.
fn on_the_audio_page() -> (App, Realtime) {
    use vt_tui::audio::DeviceInfo;
    let (mut app, realtime) = app();
    app.audio.devices = vec![
        DeviceInfo {
            id: "coreaudio:speakers".to_owned(),
            name: "Speakers".to_owned(),
            rates: vec![44_100, 48_000],
            buffers: vec![64, 128, 256, 512],
            default: true,
        },
        DeviceInfo {
            id: "coreaudio:headset".to_owned(),
            name: "Headset".to_owned(),
            rates: vec![44_100],
            buffers: vec![512, 1024],
            default: false,
        },
    ];
    app.audio.device = "Speakers".to_owned();
    app.audio.buffer = Some(512);
    app.audio.running = true;
    press(&mut app, " 7");
    assert_eq!(app.view, View::Audio);
    assert!(app.audio.refresh, "it lists the devices afresh");
    (app, realtime)
}

#[test]
fn the_audio_page_chooses_a_buffer_a_rate_and_a_device() {
    use vt_tui::audio::AudioChoice;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_the_audio_page();
    assert_eq!(column_name(&app), "Device");
    // A smaller buffer, one step down from what's playing.
    press(&mut app, "jj-");
    assert_eq!(column_name(&app), "Buffer");
    assert_eq!(
        app.audio_request,
        Some(AudioChoice {
            buffer: Some(256),
            ..AudioChoice::default()
        })
    );
    // As if the main loop had opened it.
    app.audio.choice = app.audio_request.take().unwrap();
    app.audio.buffer = Some(256);
    press(&mut app, "k-");
    assert_eq!(app.audio_request.as_ref().unwrap().rate, Some(44_100));
    app.audio_request = None;
    // Another device keeps the buffer only if it takes it.
    press(&mut app, "k=");
    let request = app.audio_request.take().unwrap();
    assert_eq!(request.device.as_deref(), Some("coreaudio:headset"));
    assert_eq!(request.buffer, None, "the headset doesn't take 256");
    // x goes back to the system's own choice for the row, and K lists the choices.
    press(&mut app, "jjx");
    assert_eq!(app.audio_request.take().unwrap().buffer, None);
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(listed)
        .collect();
    assert_eq!(labels[0], "64 frames, 1.3 ms", "{labels:?}");
    assert_eq!(
        app.picker
            .as_ref()
            .unwrap()
            .current()
            .map(|item| item.label.as_str()),
        Some("256 frames, 5.3 ms"),
        "it opens on the one playing"
    );
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.audio_request, None, "which is already playing");
}

#[test]
fn the_audio_page_shows_what_plays_and_when_it_stops() {
    let (mut app, _realtime) = on_the_audio_page();
    app.audio.latency_us = 21_300;
    let text = |app: &App| -> Vec<String> {
        let backend = ratatui::backend::TestBackend::new(90, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::audio::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..12)
            .map(|y| (0..90).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect()
    };
    let lines = text(&app);
    assert!(lines[2].contains("Speakers"), "{lines:?}");
    assert!(lines[3].contains("48000 Hz"), "{lines:?}");
    assert!(
        lines[4].contains("512 frames, 10.7 ms, the device's own"),
        "{lines:?}"
    );
    assert!(lines[6].contains("21.3 ms"), "{lines:?}");
    app.audio.running = false;
    assert!(text(&app).iter().any(|line| line.contains("Stopped")));
}

#[test]
fn a_new_engine_keeps_the_song_its_undo_and_the_mutes() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, " m");
    let song = Arc::clone(app.song());
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::clone(&song), Arc::clone(&samples)));
    let (handle, _fresh) = connect(Engine::new(44_100, version));
    app.reconnect(handle, samples, 44_100);
    assert_eq!(app.rate, 44_100);
    assert!(Arc::ptr_eq(&song, app.song()));
    assert!(app.levels[0].mute, "still muted");
    assert_eq!(app.transport, Transport::Stopped);
    press(&mut app, "u");
    assert_eq!(
        notes(&app)[0],
        Note::Empty,
        "and undo still reaches before it"
    );
}

#[test]
fn b_bypasses_a_slot_and_brings_it_back_with_undo() {
    let (mut app, _realtime) = on_fx_page();
    let bypassed = |app: &App| app.song().tracks[0].bypassed;
    press(&mut app, "b");
    assert!(app.status.contains("nothing"), "{}", app.status);
    assert_eq!(bypassed(&app), [false; 3]);
    press(&mut app, "=b");
    assert_eq!(bypassed(&app), [true, false, false]);
    assert!(app.status.contains("Bypassed"), "{}", app.status);
    press(&mut app, "b");
    assert_eq!(bypassed(&app), [false; 3]);
    press(&mut app, "bu");
    assert_eq!(bypassed(&app), [false; 3]);
    press(&mut app, "b");
    // Emptying the slot brings it back in, for whatever goes there next.
    press(&mut app, "x");
    assert_eq!(bypassed(&app), [false; 3]);
}

#[test]
fn zero_and_dollar_on_the_fx_page_stay_on_the_line_the_cursor_is_on() {
    use vt_tui::track_page::Control;
    let (mut app, _rt) = app();
    press(&mut app, " 4LL");
    press(&mut app, "K");
    press(&mut app, "delay");
    special(&mut app, KeyCode::Enter);
    // Down onto the slot's bottom line, whose first setting is Feedback.
    press(&mut app, "lj");
    assert_eq!(app.track_control(), Control::InsertParam(0, 1));
    press(&mut app, "$");
    assert_eq!(app.track_control(), Control::InsertParam(0, 7));
    press(&mut app, "0");
    assert_eq!(
        app.track_control(),
        Control::InsertParam(0, 1),
        "not the effect, drawn on the line above"
    );
    press(&mut app, "h");
    assert_eq!(
        app.track_control(),
        Control::Insert(0),
        "h still reaches it"
    );
    // On the top line, the effect is the start.
    press(&mut app, "lk0");
    assert_eq!(app.track_control(), Control::Insert(0));
    press(&mut app, "$");
    assert_eq!(app.track_control(), Control::InsertParam(0, 8));
}

#[test]
fn j_and_k_come_back_to_the_column_a_shorter_row_moved_them_from() {
    use vt_tui::track_page::Control;
    let (mut app, _rt) = app();
    press(&mut app, " 4LL");
    press(&mut app, "K");
    press(&mut app, "delay");
    special(&mut app, KeyCode::Enter);
    // Gain is the last on the top line, which is longer than the bottom.
    press(&mut app, "$");
    assert_eq!(app.track_control(), Control::InsertParam(0, 8));
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::InsertParam(0, 7));
    press(&mut app, "k");
    assert_eq!(app.track_control(), Control::InsertParam(0, 8));
    // A move along the row forgets the column.
    press(&mut app, "jhk");
    assert_eq!(app.track_control(), Control::InsertParam(0, 4));

    // On the Sound page, down past the rows with nothing on the right and back.
    press(&mut app, "HHlgg");
    assert_eq!(app.track_control(), Control::Trim);
    press(&mut app, "jjjjj");
    assert_eq!(app.track_control().label(), "Group");
    press(&mut app, "jjj");
    press(&mut app, "kkk");
    assert_eq!(app.track_control().label(), "Group");
    press(&mut app, "Ggg");
    assert_eq!(app.track_control(), Control::Trim, "G and gg keep it too");
}

/// Two tracks: 16 empty rows on the first, and 4 on the second, where the cursor is.
fn recording_app(rows: Vec<Step>) -> (App, Realtime) {
    let mut song = Song::new(2);
    song.tracks[0].phrases.insert(0, Arc::new(Phrase::new(16)));
    song.tracks[1]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(rows)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    press(&mut app, "W");
    assert_eq!(app.cursor.track, 1);
    (app, realtime)
}

/// Frames to a step at the default 120 BPM.
const FRAMES_PER_STEP: f64 = 6_000.0;
/// Seconds to a step at the default 120 BPM.
const SECONDS_PER_STEP: f64 = 0.125;

/// The time on the recording tests' own clock this many steps from the start. The app is given
/// these times for its frames and its keys, so where a note lands and how long it lasts come
/// from the frames played, never from how fast the test happens to run (#292).
fn time_at(at: f64) -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now) + Duration::from_secs_f64(at.max(0.0) * SECONDS_PER_STEP)
}

/// Keys pressed, tapped or held at a time on the recording tests' clock.
fn press_at(app: &mut App, at: f64, keys: &str) {
    for c in keys.chars() {
        app.key(Key::char(c), time_at(at));
    }
}

fn tap_at(app: &mut App, at: f64, keys: &str) {
    for c in keys.chars() {
        app.key(Key::char(c), time_at(at));
        app.key_released(Key::char(c), time_at(at));
    }
}

fn special_at(app: &mut App, at: f64, code: KeyCode) {
    app.key(Key::new(code), time_at(at));
}

fn hold_at(app: &mut App, at: f64, c: char) {
    app.key(Key::char(c), time_at(at));
}

fn let_go_of_at(app: &mut App, at: f64, c: char) {
    app.key_released(Key::char(c), time_at(at));
}

/// Play from the start, as `Shift-Tab` does, and come back with the clock at 0.
fn play_from_the_top(app: &mut App, realtime: &mut Realtime) -> f64 {
    special_at(app, 0.0, KeyCode::BackTab);
    let mut out = vec![0.0; 2];
    realtime.process(&mut out);
    let at = 1.0 / FRAMES_PER_STEP;
    app.tick_at(time_at(at));
    at
}

/// Run the audio side on to this many steps from the start, and give the app the frame at the
/// time it's got to.
fn play_to(app: &mut App, realtime: &mut Realtime, at: &mut f64, to: f64) {
    let frames = ((to - *at) * FRAMES_PER_STEP).round() as usize;
    let mut out = vec![0.0; frames * 2];
    realtime.process(&mut out);
    *at += frames as f64 / FRAMES_PER_STEP;
    app.tick_at(time_at(*at));
}

fn rows_of(app: &App, track: usize) -> Vec<Step> {
    app.song().tracks[track].phrases[&0].steps().to_vec()
}

/// A recorded note's gate cell, as its pulses and its length.
fn length_of(step: &Step) -> Option<(u8, Option<u8>)> {
    step.gate.map(|gate| (gate.pulses, gate.length))
}

fn status_line(app: &App) -> String {
    let backend = ratatui::backend::TestBackend::new(160, 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::status::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..160).map(|x| buffer[(x, 0)].symbol()).collect()
}

/// The status line's transport while playing with a realign waiting.
fn realign_shown() -> String {
    use vt_tui::glyphs::{PLAY, WAITING};
    format!(" {PLAY} {WAITING} REALIGN  ")
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
fn dropouts_put_a_warning_at_the_right_of_the_status_line_only_once_there_are_any() {
    use vt_tui::glyphs::{SPINNER, WARNING};
    let (mut app, _rt) = app();
    let line = status_line(&app);
    assert!(!line.contains(WARNING));
    assert!(
        !line.contains(SPINNER),
        "nothing is going on in the background"
    );
    assert_eq!(app.background_work(), None);
    app.audio.late = 2;
    app.audio.xruns = 1;
    let line = status_line(&app);
    assert!(line.ends_with(&format!("{WARNING} 3  ")), "{line}");
}

#[test]
fn a_tracks_heading_has_its_machine_and_mute_or_solo_glyphs() {
    use vt_tui::glyphs::{DRUM, MUTE, SAMPLER, SOLO, SYNTH};
    let (mut app, _rt) = app();
    let heading = |app: &App| -> String {
        let buffer = screen(app);
        (0..80).map(|x| buffer[(x, 0)].symbol()).collect()
    };
    let machine = app.song().tracks[0].machine;
    let glyph = vt_tui::glyphs::machine(machine);
    assert!(heading(&app).starts_with(&format!("{glyph} Track 01 Phrase")));
    tap(&mut app, " m");
    assert!(heading(&app).starts_with(&format!("{glyph} Track 01 {MUTE} Phrase")));
    // Over its column in the overview too, in place of the number's last cell.
    assert!(heading(&app).contains(&format!(" 01{MUTE}")));
    tap(&mut app, " m");
    app.levels[0].solo = true;
    assert!(heading(&app).contains(&format!("Track 01 {SOLO} Phrase")));
    assert_eq!(vt_tui::glyphs::machine(Machine::Sampler), SAMPLER);
    assert_eq!(
        vt_tui::glyphs::machine(Machine::Plaits(SynthVoice::Kick)),
        DRUM
    );
    assert_eq!(
        vt_tui::glyphs::machine(Machine::Plaits(SynthVoice::Fm)),
        SYNTH
    );
}

#[test]
fn space_r_arms_recording_only_while_playing_and_stopping_or_leaving_disarms() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    press(&mut app, " r");
    assert!(!app.recording());
    assert_eq!(app.status, "Recording arms while playing");

    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    assert!(app.recording());
    let (play, record) = (vt_tui::glyphs::PLAY, vt_tui::glyphs::RECORD);
    assert!(
        status_line(&app).contains(&format!(" {play} {record}  ")),
        "{}",
        status_line(&app)
    );
    press_at(&mut app, at, " r");
    assert!(!app.recording(), "it toggles");
    assert!(!status_line(&app).contains(record));
    assert!(status_line(&app).contains(&format!(" {play}  ")));

    // Pausing stops it.
    let tab = Key::new(KeyCode::Tab);
    press_at(&mut app, at, " r");
    app.key(tab, time_at(at));
    app.key_released(tab, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 1.0);
    assert_eq!(app.transport, Transport::Paused);
    assert!(!app.recording());

    // So does leaving the phrase view.
    app.key(tab, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 2.0);
    press_at(&mut app, at, " r");
    assert!(app.recording());
    press_at(&mut app, at, " 2");
    assert!(!app.recording());
}

#[test]
fn a_recorded_note_goes_on_the_row_its_own_track_is_playing_as_long_as_its_held() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    // Step 9 of the song is row 1 of the 4-row phrase. A quarter into it goes on it, nudged late
    // by a quarter of a step, six 24ths.
    play_to(&mut app, &mut rt, &mut at, 9.25);
    let a = Key::char('a');
    app.key(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(rows[1].note, Note::On(60), "{rows:?}");
    assert_eq!(rows[1].velocity, Some(app.note_velocity));
    assert_eq!(rows[1].nudge, 6);
    assert_eq!(app.cursor.step, 0, "the cursor stays");
    // Until it's let go, it holds on the row's pulse until something ends it.
    assert_eq!(length_of(&rows[1]), Some((1, Some(0xFF))));

    // Held for two steps, past its row's end, so it rings on, and the row keeps its pulse.
    play_to(&mut app, &mut rt, &mut at, 11.25);
    app.key_released(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(length_of(&rows[1]), Some((1, Some(0xFF))), "{rows:?}");
    assert!(
        rows[1].to_string().starts_with("C-4 64 +6 SGL1 FF "),
        "{}",
        rows[1]
    );
    assert!(
        rows[1].fx.iter().all(|slot| slot.command.is_none()),
        "no OFF"
    );
    assert_eq!(
        rows.iter().map(Step::pulses).sum::<usize>(),
        4,
        "the loop keeps its length"
    );
    assert_eq!(
        rows.iter().filter(|step| step.note != Note::Empty).count(),
        1
    );
    assert!(
        rows_of(&app, 0).iter().all(|step| *step == Step::default()),
        "the longer track is left alone"
    );
}

#[test]
fn a_note_played_late_in_a_step_waits_for_the_next_row() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    play_to(&mut app, &mut rt, &mut at, 9.75);
    let s = Key::char('s');
    app.key(s, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 9.9);
    app.key_released(s, time_at(at));
    assert!(
        rows_of(&app, 1).iter().all(|step| step.note == Note::Empty),
        "not written before its row, which would play it twice"
    );
    play_to(&mut app, &mut rt, &mut at, 10.1);
    let rows = rows_of(&app, 1);
    assert_eq!(rows[2].note, Note::On(62), "{rows:?}");
    assert_eq!(rows[2].nudge, -6, "a quarter of a step early");
    // Held for 0.15 of a step.
    let (pulses, length) = length_of(&rows[2]).unwrap();
    assert_eq!(pulses, 1);
    assert_eq!(length, Some(0x26));
}

#[test]
fn a_note_held_until_its_row_comes_round_ends_there() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    play_to(&mut app, &mut rt, &mut at, 9.0);
    let a = Key::char('a');
    app.key(a, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 13.5);
    // Letting go later changes nothing.
    play_to(&mut app, &mut rt, &mut at, 14.5);
    app.key_released(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(rows[1].note, Note::On(60));
    assert_eq!(
        length_of(&rows[1]),
        Some((1, Some(0xFF))),
        "held past its row, it rings on"
    );
}

#[test]
fn a_recorded_note_on_a_long_row_keeps_its_pulses_and_lasts_into_the_last() {
    let rows: Vec<Step> = ["--- -- SGL2", "--- --", "--- --"]
        .iter()
        .map(|text| text.parse().unwrap())
        .collect();
    let (mut app, mut rt) = recording_app(rows);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    // Step 8 starts the 4-step loop's row 0, which lasts two.
    let a = Key::char('a');
    play_to(&mut app, &mut rt, &mut at, 8.0);
    app.key(a, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 9.5);
    app.key_released(a, time_at(at));
    let (pulses, length) = length_of(&rows_of(&app, 1)[0]).unwrap();
    assert_eq!(pulses, 2);
    assert_eq!(length, Some(0x80), "half of its second pulse");

    // A tap lasts into its last pulse as little as the cell says, and the row stays as long.
    play_to(&mut app, &mut rt, &mut at, 12.0);
    app.key(a, time_at(at));
    play_to(&mut app, &mut rt, &mut at, 12.25);
    app.key_released(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(length_of(&rows[0]), Some((2, Some(0x00))), "{rows:?}");
    assert_eq!(rows.iter().map(Step::pulses).sum::<usize>(), 4);
}

#[test]
fn recording_overwrites_only_the_note_velocity_and_gate_and_a_pass_is_one_undo() {
    let rows: Vec<Step> = ["E-4 7F", "C-3 10 RST1 cut:40", "--- --", "--- --"]
        .iter()
        .map(|text| text.parse().unwrap())
        .collect();
    let (mut app, mut rt) = recording_app(rows.clone());
    let depth = app.history.undo_depth();
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    for (from, key) in [(9.0, 'a'), (11.0, 'd')] {
        play_to(&mut app, &mut rt, &mut at, from);
        app.key(Key::char(key), time_at(at));
        play_to(&mut app, &mut rt, &mut at, from + 0.5);
        app.key_released(Key::char(key), time_at(at));
    }
    let written = rows_of(&app, 1);
    assert_eq!(written[0], rows[0], "a row not played into stays");
    assert_eq!(written[1].note, Note::On(60));
    assert_eq!(written[1].velocity, Some(app.note_velocity));
    assert_eq!(
        written[1].gate_kind(),
        vt_model::step::GateKind::Single,
        "a rest would silence it"
    );
    assert_eq!(
        written[1].pulses(),
        1,
        "held for half a step, the row lasts as long as it did"
    );
    assert_eq!(written[1].fx, rows[1].fx, "its FX stay");
    assert_eq!(written[1].length(), Some(0x80));
    assert_eq!(written[3].note, Note::On(64));
    assert_eq!(app.history.undo_depth(), depth + 1, "one step for the pass");

    special_at(&mut app, at, KeyCode::Esc);
    press_at(&mut app, at, " r");
    press_at(&mut app, at, "u");
    assert_eq!(rows_of(&app, 1), rows);
}

#[test]
fn a_held_note_when_recording_stops_keeps_its_length_so_far() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    play_to(&mut app, &mut rt, &mut at, 8.0);
    app.key(Key::char('a'), time_at(at));
    play_to(&mut app, &mut rt, &mut at, 8.5);
    special_at(&mut app, at, KeyCode::Esc);
    press_at(&mut app, at, " r");
    assert!(!app.recording());
    let (pulses, length) = length_of(&rows_of(&app, 1)[0]).unwrap();
    assert_eq!(pulses, 1);
    assert_eq!(length, Some(0x80), "half a step so far");
}

#[test]
fn the_scope_sends_a_picture_only_when_it_changes_and_draws_above_the_meter() {
    use vt_tui::kitty::Graphics;
    let mut song = Song::new(2);
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "C-3 64 --- --- ---".parse().unwrap();
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, mut realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .matches(",c=12,r=3,")
            .count()
    };
    // Silence is a flat line, sent once.
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let flat = app.scope_image.unwrap();
    block(&mut realtime, &mut app);
    assert_eq!(sync(&mut app, &mut graphics), 0, "still silent");
    assert_eq!(app.scope_image, Some(flat));

    special(&mut app, KeyCode::Tab);
    for _ in 0..8 {
        block(&mut realtime, &mut app);
    }
    // It's brought up to date every other frame, so one of the next two sends it.
    let sent = sync(&mut app, &mut graphics) + sync(&mut app, &mut graphics);
    assert_eq!(sent, 1, "the note is heard");
    assert_ne!(app.scope_image, Some(flat));

    // A picture after every frame, for longer than there are ids, never takes the one on screen.
    let mut ids = std::collections::HashSet::new();
    for _ in 0..600 {
        block(&mut realtime, &mut app);
        let before = app.scope_image;
        let mut out = Vec::new();
        graphics.sync(&mut app, &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        let shown = app.scope_image.unwrap();
        assert!(
            !out.contains(&format!("a=d,d=I,i={shown},")),
            "the shown picture is kept"
        );
        if app.scope_image != before {
            ids.insert(shown);
        }
    }
    assert!(ids.len() > 100, "{}", ids.len());

    // Drawn at the top of the meter's column, after its gap.
    let backend = ratatui::backend::TestBackend::new(120, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, &mut app, None, Instant::now()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let corner = &buffer[(108, 0)];
    assert_eq!(corner.symbol(), "\u{10EEEE}\u{305}\u{305}");
    assert_eq!(corner.fg, vt_tui::kitty::id_color(app.scope_image.unwrap()));
    assert!(app.scope_shown);

    // On a screen too short for it, it isn't drawn, and nothing more is sent.
    let backend = ratatui::backend::TestBackend::new(120, 10);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, &mut app, None, Instant::now()))
        .unwrap();
    assert!(!app.scope_shown);
    block(&mut realtime, &mut app);
    let sent = sync(&mut app, &mut graphics) + sync(&mut app, &mut graphics);
    assert_eq!(sent, 0);
}

/// An app whose first track plays one sample and whose second plays another, and a third with none.
fn app_with_samples() -> (App, Realtime) {
    use vt_model::sample::SampleData;
    let mut song = Song::new(3);
    let mut pool = SamplePool::default();
    for (track, (path, level)) in [("a.wav", 0.5), ("b.wav", -0.25)].iter().enumerate() {
        pool.insert(
            path,
            SampleData {
                frames: vec![[*level; 2]; 100],
            },
        );
        song.sample_on(track, path);
    }
    let samples = Arc::new(pool);
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

#[test]
fn a_samples_waveform_is_sent_once_and_follows_the_cursor_and_the_track() {
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app_with_samples();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .matches(",c=24,r=3,")
            .count()
    };
    press(&mut app, " 4");
    // The filter's curve and the waveform.
    assert_eq!(sync(&mut app, &mut graphics), 2);
    let plain = app.waveform_image.unwrap();
    assert_ne!(Some(plain), app.filter_image);
    assert_eq!(sync(&mut app, &mut graphics), 0, "nothing has changed");

    // On the file's row it's picked out, once.
    press(&mut app, "j");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let picked = app.waveform_image.unwrap();
    assert_ne!(picked, plain);
    press(&mut app, "k");
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.waveform_image, Some(plain));

    // Another track's sample is drawn for it, and one with no sample has none.
    app.cursor.track = 1;
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_ne!(app.waveform_image, Some(plain));
    app.cursor.track = 2;
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.waveform_image, None);
    // A synth has none either, whatever file it still names.
    app.cursor.track = 0;
    press(&mut app, "=");
    assert_ne!(app.song().tracks[0].machine, Machine::Sampler);
    sync(&mut app, &mut graphics);
    assert_eq!(app.waveform_image, None);
}

#[test]
fn a_samples_waveform_sits_beside_the_filters_curve_under_the_output() {
    let (mut app, _realtime) = app_with_samples();
    press(&mut app, " 4");
    app.filter_image = Some(0x01_02_03);
    app.waveform_image = Some(0x04_05_06);
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), &app))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let top = (0..24).find(|y| buffer[(0, *y)].symbol() == "┌").unwrap();
    assert_eq!(buffer[(36, top)].symbol(), "┌");
    assert_eq!(buffer[(37, top + 1)].symbol(), "\u{10EEEE}\u{305}\u{305}");
    assert_eq!(buffer[(37, top + 1)].fg, Color::Rgb(4, 5, 6));
    assert_eq!(buffer[(1, top + 1)].fg, Color::Rgb(1, 2, 3));
    assert_eq!(buffer[(61, top + 4)].symbol(), "┘");
}

#[test]
fn a_compressors_curve_is_sent_once_and_again_only_for_its_threshold_or_ratio() {
    use vt_model::insert::{Compressor, Insert};
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out).unwrap().matches(",c=4,r=2,").count()
    };
    press(&mut app, " 4LL");
    assert_eq!(sync(&mut app, &mut graphics), 0, "no slot has a drawing");
    assert_eq!(app.slot_images, [None; 3]);
    press(&mut app, "K");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    assert!(matches!(
        app.song().tracks[0].inserts[0],
        Insert::Compressor(_)
    ));
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let first = app.slot_images[0].unwrap();
    assert_eq!(sync(&mut app, &mut graphics), 0, "nothing has changed");

    // The attack isn't on the curve, so changing it sends nothing.
    press(&mut app, "lll=");
    assert!(matches!(
        app.song().tracks[0].inserts[0],
        Insert::Compressor(Compressor { attack: 0x56, .. })
    ));
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.slot_images[0], Some(first));
    // The threshold is.
    press(&mut app, "h=");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_ne!(app.slot_images[0], Some(first));
}

#[test]
fn a_compressors_type_shows_its_own_settings_and_console_and_glue_pick_their_switches_with_k() {
    use vt_model::insert::{CONSOLE_AUTO_RELEASE, Compressor, CompressorType, Insert, range_db};
    let screen = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(163, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..24)
            .map(|y| {
                (0..163)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let compressor = |app: &App| match app.song().tracks[0].inserts[0] {
        Insert::Compressor(compressor) => compressor,
        other => panic!("{other:?}"),
    };
    let (mut app, _realtime) = app();
    press(&mut app, " 4LLK");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).kind, CompressorType::Clean);
    let clean = screen(&app);
    assert!(clean.contains("Type") && clean.contains("Clean"), "{clean}");
    assert!(clean.contains("Auto"), "{clean}");

    // `=` on Type steps it to Console, which shows its switches, and Auto and Makeup as Clean
    // does.
    press(&mut app, "l=");
    assert_eq!(compressor(&app).kind, CompressorType::Console);
    let console = screen(&app);
    for shown in [
        "Console", "Ratio", "4:1", "Attack", "10 ms", "Release", "Auto", "Off", "Mix", "Makeup",
    ] {
        assert!(console.contains(shown), "{shown}: {console}");
    }
    assert!(!console.contains("Gain"), "{console}");
    assert_eq!(compressor(&app).console_release, CONSOLE_AUTO_RELEASE);

    // The ratio is picked from its switch's three with `K`, and the release from its five.
    press(&mut app, "jl");
    press(&mut app, "K");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 3);
    press(&mut app, "10:1");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).console_ratio, 2);
    press(&mut app, "lK");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 5);
    press(&mut app, "0.3 s");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).console_release, 1);

    // Back to Clean, its own settings are as they were, and the console's are kept.
    press(&mut app, "khh-");
    let back = compressor(&app);
    assert_eq!(back.kind, CompressorType::Clean);
    assert_eq!(back.ratio, Compressor::DEFAULT.ratio);
    assert_eq!((back.console_ratio, back.console_release), (2, 1));

    // Two steps on is Glue, with Live's switches where Live loads them, its Range over the
    // makeup, and its clip after them.
    press(&mut app, "==");
    assert_eq!(compressor(&app).kind, CompressorType::Glue);
    let glue = screen(&app);
    for shown in [
        "Glue",
        "Ratio",
        "4:1",
        "Attack",
        "1 ms",
        "Release",
        "0.6 s",
        "Auto",
        "Mix",
        "Range",
        "Makeup",
        "Peak Clip",
    ] {
        assert!(glue.contains(shown), "{shown}: {glue}");
    }
    // Its attack has seven places, from 0.01 ms, and its release seven, Auto last.
    press(&mut app, "llK");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 7);
    press(&mut app, "0.01 ms");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).glue_attack, 0);
    press(&mut app, "jK");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 7);
    press(&mut app, "Auto");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).glue_release, 6);
    // The range reads in dB, all 70 of them as it starts.
    press(&mut app, "kll");
    let range = compressor(&app).range;
    assert_eq!(range, 0xFF);
    let shown = format!("{:.1} dB", range_db(f32::from(range)));
    assert_eq!(shown, "70.0 dB");
    assert!(status_line(&app).contains(&shown), "{}", status_line(&app));
}

#[test]
fn a_compressors_makeup_is_its_last_setting_and_reads_in_db() {
    use vt_model::insert::{CompressorType, Insert};
    let makeup = |app: &App| match app.song().tracks[0].inserts[0] {
        Insert::Compressor(compressor) => (compressor.kind, compressor.makeup),
        other => panic!("{other:?}"),
    };
    let (mut app, _realtime) = app();
    press(&mut app, " 4LLK");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    // On Console, `$` is its makeup, which starts at none and goes up in dB.
    press(&mut app, "l=");
    press(&mut app, "$");
    assert!(
        status_line(&app).contains("+0.0 dB"),
        "{}",
        status_line(&app)
    );
    press(&mut app, "+");
    let (kind, value) = makeup(&app);
    assert_eq!(kind, CompressorType::Console);
    assert!(value > 0, "{value}");
    let shown = format!("{:+.1} dB", vt_model::insert::makeup_db(f32::from(value)));
    assert!(status_line(&app).contains(&shown), "{}", status_line(&app));
}

#[test]
fn a_compressor_and_a_limiter_show_how_far_theyre_turning_down() {
    use vt_engine::track_state::LiveValues;
    let draw = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(163, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let (mut app, _realtime) = app();
    press(&mut app, " 4LLK");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "jjjK");
    press(&mut app, "Limit");
    special(&mut app, KeyCode::Enter);
    app.slot_images = [Some(0x01_02_03), None, None];
    // 18 dB is three quarters of the meter: all of its top row and half of the one below.
    app.live = Some(LiveValues {
        reduction: [18.0, 0.0, 3.0],
        ..LiveValues::default()
    });
    let buffer = draw(&app);
    let row_of = |text: &str| {
        (0..24)
            .find(|y| {
                (0..163)
                    .map(|x| buffer[(x, *y)].symbol())
                    .collect::<String>()
                    .contains(text)
            })
            .unwrap()
    };
    let comp = row_of("Comp");
    // The curve's placeholders, after the sixth column of settings, then a space, then the meter.
    assert_eq!(buffer[(138, comp)].symbol(), "\u{10EEEE}\u{305}\u{305}");
    assert_eq!(buffer[(138, comp)].fg, Color::Rgb(1, 2, 3));
    assert_eq!(buffer[(141, comp + 1)].symbol(), "\u{10EEEE}\u{30D}\u{310}");
    assert_eq!(buffer[(143, comp)].symbol(), "█");
    // Half a cell filled from the top: the lower half drawn in the background's colour.
    assert_eq!(buffer[(143, comp + 1)].symbol(), "▄");
    assert_eq!(buffer[(143, comp + 1)].fg, vt_tui::theme::BASE);
    // A limiter has its meter alone, where the curve would start: 3 dB is the top quarter
    // of its top row.
    let limit = row_of("Limit");
    assert_eq!(buffer[(138, limit)].symbol(), "▆");
    assert_eq!(buffer[(138, limit + 1)].symbol(), " ");

    // Bypassed, it turns nothing down, and its meter is empty.
    press(&mut app, "b");
    assert_eq!(app.song().tracks[0].bypassed, [false, false, true]);
    assert_eq!(draw(&app)[(138, limit)].symbol(), "▔");
}

#[test]
fn an_eqs_curve_follows_its_settings_and_the_cursor() {
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .matches(",c=10,r=2,")
            .count()
    };
    press(&mut app, " 4LLK");
    press(&mut app, "EQ3");
    special(&mut app, KeyCode::Enter);
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let picked = app.slot_images[0].unwrap();
    assert_eq!(sync(&mut app, &mut graphics), 0, "nothing has changed");

    // Away from the slot it's dim, once, and coming back sends nothing.
    press(&mut app, "jj");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let plain = app.slot_images[0].unwrap();
    assert_ne!(plain, picked);
    press(&mut app, "kk");
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.slot_images[0], Some(picked));

    // Any of its settings draws it again.
    press(&mut app, "l=");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_ne!(app.slot_images[0], Some(picked));

    let buffer = {
        let backend = ratatui::backend::TestBackend::new(163, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), &app))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let row = (0..24)
        .find(|y| {
            (0..163)
                .map(|x| buffer[(x, *y)].symbol())
                .collect::<String>()
                .contains("EQ3")
        })
        .unwrap();
    assert_eq!(buffer[(138, row)].symbol(), "\u{10EEEE}\u{305}\u{305}");
    assert_eq!(buffer[(147, row + 1)].symbol(), "\u{10EEEE}\u{30D}\u{34A}");
}

#[test]
fn the_fx_page_fits_six_columns_of_settings_beside_the_main_meter_at_163_columns() {
    use vt_model::insert::{Compressor, CompressorType, Insert};
    use vt_tui::track_page::Control;
    let (mut app, _realtime) = app();
    press(&mut app, " 4LLK");
    press(&mut app, "Comp");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "l==");
    let compressor = |app: &App| match app.song().tracks[0].inserts[0] {
        Insert::Compressor(compressor) => compressor,
        other => panic!("{other:?}"),
    };
    assert_eq!(compressor(&app).kind, CompressorType::Glue);
    // An EQ in the last slot, whose curve is the widest drawing.
    press(&mut app, "hjjK");
    press(&mut app, "EQ3");
    special(&mut app, KeyCode::Enter);
    app.slot_images = [Some(0x01_02_03), None, Some(0x04_05_06)];
    let rows = |app: &mut App, keys: bool| {
        app.mode = if keys {
            vt_tui::input::mode::Mode::Keys
        } else {
            vt_tui::input::mode::Mode::Normal
        };
        let backend = ratatui::backend::TestBackend::new(163, 42);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::run::draw_screen(frame, app, None, Instant::now()))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..42)
            .map(|y| (0..163).map(|x| buffer[(x, y)].clone()).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    let text = |row: &[ratatui::buffer::Cell]| row.iter().map(|c| c.symbol()).collect::<String>();
    let screen = rows(&mut app, false);
    let comp = screen
        .iter()
        .position(|row| text(row).contains("Glue"))
        .unwrap();
    // Glue's eleventh setting, its clip, starts a sixth column, on the top row, after Makeup's.
    let top = text(&screen[comp]);
    let bottom = text(&screen[comp + 1]);
    assert!(top.contains("Peak Clip") && top.contains("Off"), "{top}");
    assert!(
        top.find("Range").unwrap() < top.find("Peak Clip").unwrap(),
        "{top}"
    );
    assert!(
        bottom.contains("Makeup") && !bottom.contains("Peak Clip"),
        "{bottom}"
    );
    // The drawings sit after it, where every slot's does, the EQ's curve ending two columns
    // before the main meter's, which starts at 150 with its gap.
    let placeholder =
        |row: &[ratatui::buffer::Cell], x: usize| row[x].symbol().starts_with('\u{10EEEE}');
    let eq = screen
        .iter()
        .position(|row| text(row).contains("EQ3"))
        .unwrap();
    assert!(placeholder(&screen[comp], 138), "{top}");
    assert!(placeholder(&screen[eq], 138) && placeholder(&screen[eq + 1], 147));
    // Nothing on the page reaches the two columns before the meter's.
    for row in &screen[comp..eq + 2] {
        assert_eq!(
            (row[148].symbol(), row[149].symbol()),
            (" ", " "),
            "{}",
            text(row)
        );
    }
    // In the keyboard's box, one column in on each side, it all still fits.
    let boxed = rows(&mut app, true);
    let eq = boxed
        .iter()
        .position(|row| text(row).contains("EQ3"))
        .unwrap();
    assert!(placeholder(&boxed[eq + 1], 148), "{}", text(&boxed[eq + 1]));
    assert_eq!(boxed[eq + 1][149].symbol(), "│", "{}", text(&boxed[eq + 1]));
    // Along the bottom row the cursor stops at Makeup, the last there, and up and along is Peak
    // Clip, picked from its two with `K`, which switches the clip in.
    app.mode = vt_tui::input::mode::Mode::Normal;
    press(&mut app, "kkllllll");
    assert_eq!(app.track_control(), Control::InsertParam(0, 9));
    press(&mut app, "kl");
    assert_eq!(app.track_control(), Control::InsertParam(0, 10));
    press(&mut app, "K");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 2);
    press(&mut app, "On");
    special(&mut app, KeyCode::Enter);
    assert_eq!(compressor(&app).peak_clip, 1);
    assert_eq!(Compressor::DEFAULT.peak_clip, 0, "off, as Live loads it");
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

/// A WAV of a steady level, this long, at 44.1 kHz in stereo.
fn long_wav(path: &std::path::Path, seconds: f32) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for _ in 0..(seconds * 44_100.0) as usize * 2 {
        writer.write_sample(8000_i16).unwrap();
    }
    writer.finalize().unwrap();
}

/// The picker as text, drawn on a screen this big.
fn picker_text(app: &App, width: u16, height: u16) -> String {
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut picker = app.picker.clone().unwrap();
    terminal
        .draw(|frame| vt_tui::views::picker::draw(frame, frame.area(), &mut picker))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect()
}

/// A library of one folder with a long sample and a short one in it, as the first sample folder.
fn sample_library(app: &mut App) -> PathBuf {
    let library = folder();
    std::fs::create_dir_all(library.join("loops")).unwrap();
    long_wav(&library.join("loops/break.wav"), 2.5);
    long_wav(&library.join("loops/fill.wav"), 0.5);
    app.settings = Settings {
        kits: Vec::new(),
        samples: vec![library.display().to_string()],
    };
    library
}

#[test]
fn a_samples_preview_is_read_in_the_background_with_its_waveform_length_and_rate() {
    let (mut app, _realtime) = app();
    let library = sample_library(&mut app);
    press(&mut app, " 5l");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "break.wav");
    let shown = picker.preview.as_deref().unwrap();
    assert_eq!(shown.heading, "break.wav");
    if app.loader.is_waiting() {
        assert_eq!(shown.note, "Loading", "a placeholder until it's read");
        assert_eq!(shown.wave, None);
    }
    app.finish_loading();
    let shown = app.picker.as_ref().unwrap().preview.clone().unwrap();
    assert_eq!(shown.note, "2.50 s  44.1 kHz  stereo");
    assert!(shown.wave.is_some());

    // Its waveform is sent as an image, and drawn under the note.
    let mut sent = Vec::new();
    let mut graphics = vt_tui::kitty::Graphics::new();
    graphics.sync(&mut app, &mut sent).unwrap();
    let sent = String::from_utf8_lossy(&sent);
    assert_eq!(sent.matches("c=28,r=4").count(), 1, "28 by 4");
    let picker = app.picker.as_ref().unwrap();
    assert!(picker.preview_wave.is_some());
    let text = picker_text(&app, 120, 26);
    assert!(text.contains("│ 2.50 s  44.1 kHz  stereo"), "{text}");
    let waves = text
        .lines()
        .filter(|line| line.contains('\u{10EEEE}'))
        .count();
    assert_eq!(waves, 4, "four rows of it: {text}");

    // Moving away and back finds it kept, with nothing to wait for.
    press(&mut app, "jk");
    assert_eq!(
        app.picker
            .as_ref()
            .unwrap()
            .preview
            .as_deref()
            .unwrap()
            .note,
        "2.50 s  44.1 kHz  stereo"
    );
    // P hides the preview, and shows it again.
    press(&mut app, "P");
    let picker = app.picker.as_ref().unwrap();
    assert!(!picker.has_previews());
    assert!(picker.preview.is_none());
    press(&mut app, "P");
    assert!(app.picker.as_ref().unwrap().preview.is_some());
    std::fs::remove_dir_all(&library).unwrap();
}

#[test]
fn the_sample_explorers_hint_shows_whether_h_p_and_a_are_on_with_switches() {
    let (mut app, _realtime) = app();
    let library = sample_library(&mut app);
    press(&mut app, " 5l");
    let on = vt_tui::glyphs::SWITCH_ON;
    let off = vt_tui::glyphs::SWITCH_OFF;
    let hint = |app: &App| {
        let text = picker_text(app, 163, 42);
        text.lines()
            .find(|line| line.contains("Enter choose"))
            .unwrap()
            .to_owned()
    };
    let line = hint(&app);
    assert!(
        line.contains(&format!(
            "╰ Enter choose  H {off}\u{a0}hidden  P {on}\u{a0}preview  A {on}\u{a0}audition ─"
        )),
        "both on, whole: {line}"
    );
    // Each switch is green while it's on, and faint while it's off.
    let switch_colour = |app: &App, glyph: char, after: &str| {
        let backend = ratatui::backend::TestBackend::new(163, 42);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut picker = app.picker.clone().unwrap();
        terminal
            .draw(|frame| vt_tui::views::picker::draw(frame, frame.area(), &mut picker))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let area = buffer.area;
        (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .find(|&(x, y)| {
                buffer[(x, y)].symbol() == glyph.to_string()
                    && (1..=after.len() as u16)
                        .map(|i| buffer[(x + 1 + i, y)].symbol())
                        .collect::<String>()
                        == after
            })
            .map(|cell| buffer[cell].fg)
            .unwrap()
    };
    assert_eq!(switch_colour(&app, on, "preview"), vt_tui::theme::GREEN);
    // With the preview off the box is at its narrowest, and the hint still fits.
    press(&mut app, "P");
    let line = hint(&app);
    assert!(
        line.contains(&format!(
            "╰ Enter choose  H {off}\u{a0}hidden  P {off}\u{a0}preview  A {on}\u{a0}audition ─╯"
        )),
        "{line}"
    );
    assert_eq!(switch_colour(&app, off, "preview"), vt_tui::theme::OVERLAY0);
    press(&mut app, "A");
    let line = hint(&app);
    assert!(
        line.contains(&format!("P {off}\u{a0}preview  A {off}\u{a0}audition")),
        "{line}"
    );
    assert_eq!(
        switch_colour(&app, off, "audition"),
        vt_tui::theme::OVERLAY0
    );
    press(&mut app, "PA");
    assert_eq!(switch_colour(&app, on, "audition"), vt_tui::theme::GREEN);
    assert_eq!(switch_colour(&app, off, "hidden"), vt_tui::theme::OVERLAY0);
    press(&mut app, "H");
    let line = hint(&app);
    assert!(
        line.contains(&format!(
            "H {on}\u{a0}hidden  P {on}\u{a0}preview  A {on}\u{a0}audition"
        )),
        "{line}"
    );
    assert_eq!(switch_colour(&app, on, "hidden"), vt_tui::theme::GREEN);
    std::fs::remove_dir_all(&library).unwrap();
}

#[test]
fn moving_onto_a_sample_plays_it_on_the_track_until_the_cursor_moves_off() {
    let (mut app, mut realtime) = app();
    let library = sample_library(&mut app);
    let heard = |realtime: &mut Realtime, app: &mut App| {
        let mut out = vec![0.0; 4096];
        realtime.process(&mut out);
        app.tick();
        out[out.len() - 2].abs()
    };
    press(&mut app, " 5");
    assert_eq!(app.file_wanted, None, "a folder plays nothing");
    press(&mut app, "l");
    app.finish_loading();
    assert_eq!(app.file_playing, Some(library.join("loops/break.wav")));
    assert!(heard(&mut realtime, &mut app) > 0.01, "it plays");
    assert_eq!(realtime.engine().previews_sounding(), 1);
    assert!(sample_of(&app, 0).is_none(), "and isn't put on the track");

    // The next one takes over.
    press(&mut app, "j");
    app.finish_loading();
    assert_eq!(app.file_playing, Some(library.join("loops/fill.wav")));
    heard(&mut realtime, &mut app);

    // A turns it off, and stops what's playing.
    press(&mut app, "A");
    assert_eq!(app.file_playing, None);
    for _ in 0..8 {
        heard(&mut realtime, &mut app);
    }
    assert_eq!(realtime.engine().previews_sounding(), 0, "stopped");
    press(&mut app, "k");
    assert_eq!(app.file_wanted, None, "nothing plays while it's off");
    press(&mut app, "A");
    app.finish_loading();
    assert!(app.file_playing.is_some(), "A turns it on again");

    // Leaving the explorer stops it.
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.file_playing, None);
    for _ in 0..8 {
        heard(&mut realtime, &mut app);
    }
    assert_eq!(realtime.engine().previews_sounding(), 0);
    // Turned off, it stays off the next time.
    press(&mut app, " 5lA");
    special(&mut app, KeyCode::Esc);
    press(&mut app, " 5l");
    assert!(!app.picker.as_ref().unwrap().tree().unwrap().audition);
    assert_eq!(app.file_wanted, None);
    std::fs::remove_dir_all(&library).unwrap();
}

/// A folder of two projects and a plain folder.
fn projects() -> PathBuf {
    let root = folder();
    for (name, bpm) in [("one", 120_000), ("two", 137_630)] {
        let mut song = Song::new(4);
        song.millibpm = bpm;
        vt_model::project::save_song(&root.join(name), &song).unwrap();
    }
    std::fs::create_dir_all(root.join("ideas")).unwrap();
    std::fs::write(root.join("notes.txt"), "").unwrap();
    root
}

#[test]
fn e_with_no_folder_opens_a_project_from_beside_this_one_with_its_preview() {
    let root = projects();
    let (mut app, _realtime) = app();
    command(&mut app, &format!("e {}", root.join("one").display()));
    assert_eq!(app.folder.as_deref(), Some(root.join("one").as_path()));
    command(&mut app, "e");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().title, "Open a project");
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v projects", "  > ideas", "    one", "    two"],
        "projects and folders only, on this one"
    );
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "one");
    assert_eq!(picker.marked(), picker.current_index());
    press(&mut app, "j");
    let shown = app.picker.as_ref().unwrap().preview.clone().unwrap();
    assert_eq!(shown.heading, "two");
    assert_eq!(shown.note, "137.63 BPM  4 tracks");
    assert_eq!(shown.details, ["Saved just now"]);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.folder.as_deref(), Some(root.join("two").as_path()));
    assert_eq!(app.song().millibpm, 137_630);
    assert_eq!(
        app.recent_places.of(vt_tui::explorer::Purpose::Project),
        std::slice::from_ref(&root)
    );

    // Unsaved changes stop it before anything opens.
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "e");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.contains("Unsaved"), "{}", app.status);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn saving_a_new_project_chooses_a_folder_then_names_it_on_the_command_line() {
    let root = projects();
    let (mut app, _realtime) = app();
    app.recent_places
        .add(vt_tui::explorer::Purpose::Save, &root)
        .unwrap();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "w");
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v recent", "  > ideas", "  > one", "  > two"],
        "folders only, the recent place first"
    );
    let picker = app.picker.as_ref().unwrap();
    assert!(!picker.has_previews(), "nothing to show");
    // l opens a folder, and Enter chooses one.
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Command);
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
    let shown = vt_tui::explorer::place_name(&root, Some(&home));
    // The project's random name is filled in after the folder, so Enter saves it.
    let name = app.project_name();
    assert_eq!(app.command_line, format!("w {shown}/{name}"));
    assert!(app.status.contains("name"), "{}", app.status);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.folder.as_deref(), Some(root.join(&name).as_path()));
    assert!(!app.dirty, "{}", app.status);
    assert_eq!(app.project_name(), name, "the folder's name now");

    // :new does the same, with a new name, which can be typed over.
    command(&mut app, "new");
    assert_eq!(app.picker.as_ref().unwrap().title, "Save in");
    special(&mut app, KeyCode::Enter);
    let line = app.command_line.clone();
    assert!(line.starts_with(&format!("new {shown}/")), "{line}");
    assert!(!line.ends_with(&format!("/{name}")), "{line} is a new name");
    for _ in line.rsplit('/').next().unwrap().chars() {
        special(&mut app, KeyCode::Backspace);
    }
    press(&mut app, "fresh");
    special(&mut app, KeyCode::Enter);
    assert!(root.join("fresh/song.ron").is_file(), "{}", app.status);
    std::fs::remove_dir_all(&root).unwrap();
}

/// Whether a name is two lower case words joined by a hyphen, as random names are.
fn random_name_like(name: &str) -> bool {
    name.split_once('-').is_some_and(|(first, second)| {
        [first, second]
            .iter()
            .all(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase()))
    })
}

#[test]
fn a_new_project_has_a_random_name_numbered_in_a_folder_that_has_it() {
    let root = projects();
    let (mut app, _realtime) = app();
    let name = app.project_name();
    assert!(random_name_like(&name), "{name}");
    let (other, _other_realtime) = self::app();
    assert_ne!(other.project_name(), name, "each starts with its own");
    // A folder with the name already in it gets it numbered, as the project keeps its name.
    std::fs::create_dir_all(root.join(&name)).unwrap();
    app.recent_places
        .add(vt_tui::explorer::Purpose::Save, &root)
        .unwrap();
    command(&mut app, "w");
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    let given = app.command_line.rsplit('/').next().unwrap().to_owned();
    assert_eq!(
        given,
        format!("{name}-2"),
        "{name} is in the folder already"
    );
    // :new is a new project, so it starts with a new random name.
    special(&mut app, KeyCode::Esc);
    command(&mut app, "new");
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    let fresh = app.command_line.rsplit('/').next().unwrap().to_owned();
    assert!(random_name_like(&fresh), "{fresh}");
    assert_ne!(fresh, name);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn ctrl_r_rolls_a_new_name_on_a_name_line() {
    let root = projects();
    let (mut app, _realtime) = app();
    let roll = parse_sequence("C-r").unwrap()[0];
    let typed = format!("w {}/typed", root.display());
    press(&mut app, ":");
    press(&mut app, &typed);
    app.key(roll, Instant::now());
    let first = app.command_line.clone();
    let folder = format!("w {}/", root.display());
    assert!(first.starts_with(&folder), "{first}");
    assert!(random_name_like(&first[folder.len()..]), "{first}");
    app.key(roll, Instant::now());
    assert_ne!(app.command_line, first, "each press rolls again");
    assert_eq!(app.mode, Mode::Command);
    special(&mut app, KeyCode::Enter);
    let saved = app.folder.clone().unwrap();
    assert_eq!(saved.parent(), Some(root.as_path()), "{}", app.status);

    // A render's extension and flags stay.
    command(&mut app, "render --tail");
    special(&mut app, KeyCode::Enter);
    let line = app.command_line.clone();
    let project = app.project_name();
    assert!(line.ends_with(&format!("/{project}.wav --tail")), "{line}");
    app.key(roll, Instant::now());
    assert!(
        app.command_line.ends_with(".wav --tail"),
        "{}",
        app.command_line
    );
    assert!(!app.command_line.contains(&format!("/{project}.wav")));

    // A line that names nothing is left alone.
    special(&mut app, KeyCode::Esc);
    press(&mut app, ":bpm 12");
    app.key(roll, Instant::now());
    assert_eq!(app.command_line, "bpm 12");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn render_with_no_path_chooses_a_folder_and_keeps_its_flags_after_the_name() {
    let root = projects();
    let (mut app, _realtime) = app();
    command(&mut app, &format!("e {}", root.join("one").display()));
    command(&mut app, "render --tail");
    assert_eq!(app.picker.as_ref().unwrap().title, "Render into");
    // Choosing a folder shows nothing beside it, so only `H` has a switch.
    let text = picker_text(&app, 163, 42);
    assert!(
        text.contains("╰ l open  Enter choose  H \u{f204}\u{a0}hidden ─"),
        "{text}"
    );
    assert_eq!(
        explorer_rows(&app)[0],
        "v project",
        "the project's folder first"
    );
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
    let shown = vt_tui::explorer::place_name(&root.join("one"), Some(&home));
    assert_eq!(app.command_line, format!("render {shown}/one.wav --tail"));
    special(&mut app, KeyCode::Enter);
    assert!(root.join("one/one.wav").is_file(), "{}", app.status);
    // Another render is numbered, rather than written over the first.
    command(&mut app, "render");
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.command_line, format!("render {shown}/one-2.wav"));
    special(&mut app, KeyCode::Esc);

    // A project with no folder yet renders under its random name.
    let (mut unsaved, _unsaved_realtime) = self::app();
    unsaved
        .recent_places
        .add(vt_tui::explorer::Purpose::Render, &root)
        .unwrap();
    command(&mut unsaved, "render");
    special(&mut unsaved, KeyCode::Enter);
    let name = unsaved.project_name();
    assert!(
        unsaved.command_line.ends_with(&format!("/{name}.wav")),
        "{}",
        unsaved.command_line
    );
    std::fs::remove_dir_all(&root).unwrap();
}

/// Open the project page, with the cursor on its first row.
fn on_the_project_page() -> (App, Realtime) {
    let (mut app, realtime) = app();
    tap(&mut app, " 9");
    assert_eq!(app.view, View::Project);
    assert_eq!(app.key_scope(), Mode::Project);
    (app, realtime)
}

/// Put the project page's cursor on a setting.
fn project_cursor_on(app: &mut App, control: vt_tui::project_page::ProjectControl) {
    use vt_tui::project_page::PROJECT_ROWS;
    let (row, field) = PROJECT_ROWS
        .iter()
        .enumerate()
        .find_map(|(row, controls)| {
            let field = controls.iter().position(|found| *found == control)?;
            Some((row, field))
        })
        .expect("the page has it");
    tap(app, "gg");
    tap(app, "0");
    for _ in 0..row {
        tap(app, "j");
    }
    for _ in 0..field {
        tap(app, "l");
    }
    // The tempo's fine part is a stop of its own.
    if field > 0 && app.project_fine {
        tap(app, "l");
    }
    assert_eq!(app.project_control(), control);
}

#[test]
fn the_project_page_sets_the_tempo_and_its_fine_part_as_bpm_does_with_undo() {
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    project_cursor_on(&mut app, ProjectControl::Bpm);
    tap(&mut app, "=");
    assert_eq!(app.song().millibpm, 121_000);
    tap(&mut app, "+");
    assert_eq!(app.song().millibpm, 131_000, "a large step is 10 BPM");
    tap(&mut app, "u");
    assert_eq!(app.song().millibpm, 121_000, "each change is an undo step");
    // l goes onto the fine part, which moves by hundredths and carries into the whole.
    tap(&mut app, "l");
    assert!(app.project_fine);
    assert_eq!(app.project_control(), ProjectControl::Bpm);
    tap(&mut app, "-");
    assert_eq!(app.song().millibpm, 120_990);
    tap(&mut app, "x");
    assert_eq!(app.song().millibpm, 120_000, "x on the fine part clears it");
    // Typing on the fine part is the digits after the point.
    tap(&mut app, "i63");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().millibpm, 120_630);
    // On the whole, it's what :bpm takes, and says what :bpm says.
    tap(&mut app, "h");
    tap(&mut app, "i137.635");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().millibpm, 137_635);
    tap(&mut app, "i9");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().millibpm, 137_635);
    assert!(app.status.contains("20 to 400"), "{}", app.status);
    tap(&mut app, "ifast");
    special(&mut app, KeyCode::Enter);
    assert!(app.status.contains(":bpm takes"), "{}", app.status);
    tap(&mut app, "x");
    assert_eq!(
        app.song().millibpm,
        120_000,
        "x goes back to a new project's"
    );
    // Cmd with the movement keys changes it too, as everywhere.
    app.key(parse_sequence("D-l").unwrap()[0], Instant::now());
    assert_eq!(app.song().millibpm, 121_000);
}

#[test]
fn the_project_page_edits_the_values_its_commands_set() {
    use vt_model::song::Launch;
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();

    project_cursor_on(&mut app, ProjectControl::Groove);
    tap(&mut app, "=");
    assert_eq!(app.song().groove, GrooveId::swing(52).unwrap());
    tap(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    tap(&mut app, "/mpc 66");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().groove, GrooveId::named("MPC 66").unwrap());
    tap(&mut app, "x");
    assert_eq!(app.song().groove, GrooveId::STRAIGHT);
    tap(&mut app, "u");
    assert_eq!(
        app.song().groove,
        GrooveId::named("MPC 66").unwrap(),
        "undo, as :groove has"
    );

    project_cursor_on(&mut app, ProjectControl::Launch);
    assert_eq!(app.song().launch, Launch::named("1bar").unwrap());
    tap(&mut app, "=");
    assert_eq!(app.song().launch, Launch::named("2bar").unwrap());
    tap(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    tap(&mut app, "/chain");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().launch, Launch::ChainEnd);
    tap(&mut app, "i12");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().launch, Launch::every(12).unwrap());
    tap(&mut app, "=");
    assert_eq!(
        app.song().launch,
        Launch::named("1bar").unwrap(),
        "a grid with no name goes to the named one above it"
    );
    tap(&mut app, "u");
    assert_eq!(app.song().launch, Launch::every(12).unwrap());

    project_cursor_on(&mut app, ProjectControl::Reset);
    tap(&mut app, "+");
    assert_eq!(app.song().sections[0].reset, Some(16));
    tap(&mut app, "i0x40");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[0].reset, Some(64), "as :reset reads it");
    tap(&mut app, "x");
    assert_eq!(app.song().sections[0].reset, None);
    tap(&mut app, "u");
    assert_eq!(app.song().sections[0].reset, Some(64));

    project_cursor_on(&mut app, ProjectControl::Seed);
    tap(&mut app, "i42");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().seed, 42);
    tap(&mut app, "+");
    assert_eq!(app.song().seed, 52);
    tap(&mut app, "u");
    assert_eq!(app.song().seed, 42);

    project_cursor_on(&mut app, ProjectControl::Limiter);
    assert!(app.song().limiter);
    tap(&mut app, "K");
    assert!(!app.song().limiter, "K switches it");
    tap(&mut app, "=");
    assert!(app.song().limiter, "= switches it on");
    tap(&mut app, "u");
    assert!(!app.song().limiter);

    project_cursor_on(&mut app, ProjectControl::Grid);
    assert_eq!(app.song().skip, 4);
    tap(&mut app, "=");
    assert_eq!(app.song().skip, 8);
    tap(&mut app, "i3");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().skip,
        3,
        "any number of steps, as :set skip takes"
    );
    tap(&mut app, "K");
    tap(&mut app, "/1/16");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().skip, 1);
    tap(&mut app, "u");
    assert_eq!(app.song().skip, 3);
}

#[test]
fn the_project_page_sets_the_scale_its_root_and_auto_jump() {
    use vt_model::scale::ScaleKind;
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    project_cursor_on(&mut app, ProjectControl::Scale);
    assert!(!app.scale.on);
    tap(&mut app, "=");
    assert!(app.scale.on);
    assert_eq!(app.scale.kind, ScaleKind::ALL[0]);
    tap(&mut app, "-");
    assert!(!app.scale.on, "off is the first choice");
    // i lists them, as K does.
    tap(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker);
    tap(&mut app, "/dorian");
    special(&mut app, KeyCode::Enter);
    assert!(app.scale.on);
    assert_eq!(app.scale.kind, ScaleKind::Dorian);

    // The root moves by a semitone, and keeps the scale as it is.
    tap(&mut app, "j");
    assert_eq!(app.project_control(), ProjectControl::Root);
    tap(&mut app, "=");
    assert_eq!(app.scale.root, 1);
    tap(&mut app, "if#");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.scale.root, 6);
    tap(&mut app, "x");
    assert_eq!(app.scale.root, 0);
    project_cursor_on(&mut app, ProjectControl::Scale);
    tap(&mut app, "x");
    assert!(!app.scale.on);
    project_cursor_on(&mut app, ProjectControl::Root);
    tap(&mut app, "=");
    assert!(!app.scale.on, "the root alone doesn't turn it on");
    assert_eq!(app.scale.root, 1);

    project_cursor_on(&mut app, ProjectControl::AutoJump);
    assert!(!app.auto_jump);
    tap(&mut app, "=");
    assert!(app.auto_jump);
    assert!(app.dirty, "it's kept with the song");
    tap(&mut app, "K");
    assert!(!app.auto_jump);
}

#[test]
fn the_project_page_names_the_project_and_the_window_title_says_it() {
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    let random = app.project_name();
    assert_eq!(app.song().name, random, "the song holds its name");
    assert_eq!(app.window_title(), format!("vimtracker: {random}"));
    assert_eq!(app.project_control(), ProjectControl::Name);
    // i starts with the name as it is.
    tap(&mut app, "i");
    assert_eq!(app.mode, Mode::Insert);
    assert_eq!(app.entry.text, random);
    app.key(parse_sequence("C-u").unwrap()[0], Instant::now());
    tap(&mut app, "Verse Ideas");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.project_name(), "Verse Ideas");
    assert_eq!(app.window_title(), "vimtracker: Verse Ideas");
    assert!(app.dirty);
    tap(&mut app, "u");
    assert_eq!(app.project_name(), random, "renaming is an undo step");
    // C-r while typing rolls a new name.
    tap(&mut app, "i");
    app.key(parse_sequence("C-r").unwrap()[0], Instant::now());
    let rolled = app.entry.text.clone();
    assert!(random_name_like(&rolled), "{rolled}");
    assert_ne!(rolled, random);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.project_name(), rolled);
    // A name that couldn't be a folder's is refused.
    tap(&mut app, "i");
    app.key(parse_sequence("C-u").unwrap()[0], Instant::now());
    tap(&mut app, "a/b");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.project_name(), rolled);
    assert!(app.status.contains("no /"), "{}", app.status);
}

#[test]
fn saving_a_project_somewhere_new_names_it_after_the_folder_and_keeps_the_name_after() {
    let root = projects();
    let (mut app, _realtime) = app();
    app.apply(&vt_model::action::Action::SetSongName("verse".to_owned()));
    // Saving starts with its name.
    app.recent_places
        .add(vt_tui::explorer::Purpose::Save, &root)
        .unwrap();
    command(&mut app, "w");
    tap(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    assert!(app.command_line.ends_with("/verse"), "{}", app.command_line);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.folder.as_deref(), Some(root.join("verse").as_path()));
    // Renaming keeps the folder, and Save as starts with the new name.
    app.apply(&vt_model::action::Action::SetSongName("chorus".to_owned()));
    command(&mut app, "w");
    assert_eq!(app.folder.as_deref(), Some(root.join("verse").as_path()));
    let (mut other, _other_realtime) = self::app();
    command(&mut other, &format!("e {}", root.join("verse").display()));
    assert_eq!(
        other.project_name(),
        "chorus",
        "the name is kept in the song"
    );
    // A copy of the same name in the same place is numbered.
    command(&mut other, "w");
    let (mut copy, _copy_realtime) = self::app();
    copy.apply(&vt_model::action::Action::SetSongName("one".to_owned()));
    copy.recent_places
        .add(vt_tui::explorer::Purpose::Save, &root)
        .unwrap();
    command(&mut copy, "w");
    tap(&mut copy, "k");
    special(&mut copy, KeyCode::Enter);
    assert!(
        copy.command_line.ends_with("/one-2"),
        "{}",
        copy.command_line
    );
    // A name typed in its place names the project after it.
    for _ in 0.."one-2".len() {
        special(&mut copy, KeyCode::Backspace);
    }
    tap(&mut copy, "bridge");
    special(&mut copy, KeyCode::Enter);
    assert_eq!(copy.project_name(), "bridge");
    // An old song with no name takes its folder's.
    let mut nameless = Song::new(1);
    nameless.name.clear();
    vt_model::project::save_song(&root.join("old"), &nameless).unwrap();
    command(&mut copy, &format!("e {}", root.join("old").display()));
    assert_eq!(copy.project_name(), "old");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_project_pages_actions_open_the_explorer_and_its_audio_row_the_audio_page() {
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    for (action, title) in [
        (ProjectControl::Save, "Save in"),
        (ProjectControl::SaveAs, "Save in"),
        (ProjectControl::Render, "Render into"),
        (ProjectControl::Open, "Open a project"),
        (ProjectControl::New, "Save in"),
    ] {
        project_cursor_on(&mut app, action);
        special(&mut app, KeyCode::Enter);
        assert_eq!(app.mode, Mode::Picker, "{action:?}");
        assert_eq!(app.picker.as_ref().unwrap().title, title, "{action:?}");
        special(&mut app, KeyCode::Esc);
        assert_eq!(app.view, View::Project, "{action:?} comes back to the page");
    }
    // A folder chosen goes on to the name line, as :w's does.
    project_cursor_on(&mut app, ProjectControl::SaveAs);
    tap(&mut app, "K");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Command);
    let name = app.project_name();
    assert!(
        app.command_line.starts_with("w ") && app.command_line.ends_with(&format!("/{name}")),
        "{}",
        app.command_line
    );
    special(&mut app, KeyCode::Esc);
    // With changes, open and new refuse, as :e and :new do.
    project_cursor_on(&mut app, ProjectControl::Seed);
    tap(&mut app, "=");
    project_cursor_on(&mut app, ProjectControl::Open);
    special(&mut app, KeyCode::Enter);
    assert_ne!(app.mode, Mode::Picker);
    assert!(app.status.contains("Unsaved"), "{}", app.status);

    project_cursor_on(&mut app, ProjectControl::Audio);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::Audio);
    assert!(app.audio.refresh, "it lists the devices afresh");
}

#[test]
fn the_project_page_keeps_the_column_through_j_and_k() {
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    project_cursor_on(&mut app, ProjectControl::Root);
    tap(&mut app, "jj");
    assert_eq!(app.project_control(), ProjectControl::AutoJump);
    tap(&mut app, "j");
    assert_eq!(app.project_control(), ProjectControl::Seed, "a row of one");
    tap(&mut app, "jj");
    assert_eq!(app.project_control(), ProjectControl::GrooveShown);
    tap(&mut app, "j");
    assert_eq!(
        app.project_control(),
        ProjectControl::Humanise,
        "back in the right column"
    );
    tap(&mut app, "kkkk");
    assert_eq!(app.project_control(), ProjectControl::AutoJump);
    // A count goes further, and gg and G go to the ends, keeping it too.
    tap(&mut app, "2k");
    assert_eq!(app.project_control(), ProjectControl::Root);
    tap(&mut app, "G");
    assert_eq!(app.project_control(), ProjectControl::New);
    tap(&mut app, "g");
    tap(&mut app, "g");
    assert_eq!(app.project_control(), ProjectControl::Name);
    project_cursor_on(&mut app, ProjectControl::Scale);
    tap(&mut app, "0");
    assert_eq!(app.project_control(), ProjectControl::Bpm);
    assert!(!app.project_fine);
    tap(&mut app, "$");
    assert_eq!(app.project_control(), ProjectControl::Scale);
    // h from the right column lands on the tempo's fine part, as the track page's does.
    tap(&mut app, "h");
    assert_eq!(app.project_control(), ProjectControl::Bpm);
    assert!(app.project_fine);
    // The status line names the row.
    assert_eq!(vt_tui::views::status::column_name(&app), "BPM");
}

/// The project page as text, a line a row.
fn project_page_lines(app: &App) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(120, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::project::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..40)
        .map(|y| {
            (0..120)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn the_project_page_draws_the_projects_groove_as_ticks_by_its_pushes() {
    let (mut app, _realtime) = on_the_project_page();
    command(&mut app, "swing 66");
    let lines = project_page_lines(&app);
    let at = |start: &str| {
        lines
            .iter()
            .position(|line| line.starts_with(start))
            .unwrap_or_else(|| panic!("{start}: {lines:#?}"))
    };
    let edit = at("Edit");
    assert!(lines[edit].contains("Swing 66"), "{}", lines[edit]);
    let steps = at("Steps");
    assert!(lines[steps].contains("Steps     2 "), "{}", lines[steps]);
    assert!(lines[steps].contains("Humanise  0 ticks"));
    let push = at("Push");
    assert!(lines[push].contains("    0  +77  ticks"), "{}", lines[push]);
    assert!(lines[at("Velocity")].contains("  100  100  %"));
    // A tick above the line over the late step, and none under it.
    assert!(
        lines[push - 3].trim_end().ends_with('▆'),
        "{}",
        lines[push - 3]
    );
    assert!(lines[push - 2].contains("──────────"));
    assert!(lines[push - 1].trim().is_empty());
}

#[test]
fn changing_a_groove_that_comes_with_it_makes_one_of_the_projects_own() {
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    command(&mut app, "swing 66");
    let custom = GrooveId::custom(0).unwrap();
    project_cursor_on(&mut app, ProjectControl::Push(1));
    tap(&mut app, "=");
    assert_eq!(app.song().groove, custom, "the project plays the copy");
    assert_eq!(app.song().grooves[0].pushes(), &[0, 78]);
    assert_eq!(app.groove_shown, Some(custom));
    assert!(app.status.contains("copied to Custom 1"), "{}", app.status);
    tap(&mut app, "u");
    assert_eq!(
        app.song().groove,
        GrooveId::swing(66).unwrap(),
        "one undo step"
    );
    assert!(app.song().grooves.is_empty());
    // Its own changes in place.
    command(&mut app, "groove custom-1");
    assert_eq!(app.project_control(), ProjectControl::Push(1));
    tap(&mut app, "i-12");
    special(&mut app, KeyCode::Enter);
    let own = |app: &App| app.song().groove(custom);
    assert_eq!(own(&app).push(1), -12);
    tap(&mut app, "+");
    assert_eq!(own(&app).push(1), -2);
    tap(&mut app, "x");
    assert_eq!(own(&app).push(1), 0, "back on the grid");
    tap(&mut app, "i200");
    special(&mut app, KeyCode::Enter);
    assert_eq!(own(&app).push(1), 0, "past half a step is refused");
    assert!(app.status.contains("-120 to 120"), "{}", app.status);
    // The first step stays on the grid.
    tap(&mut app, "0=");
    assert_eq!(own(&app).push(0), 0);
    assert!(app.status.contains("first step"), "{}", app.status);

    project_cursor_on(&mut app, ProjectControl::GrooveSteps);
    tap(&mut app, "i16");
    special(&mut app, KeyCode::Enter);
    assert_eq!(own(&app).steps(), 16);
    tap(&mut app, "-");
    assert_eq!(own(&app).steps(), 15);
    project_cursor_on(&mut app, ProjectControl::Humanise);
    tap(&mut app, "+");
    assert_eq!(own(&app).humanise(), 4);
    project_cursor_on(&mut app, ProjectControl::Velocity(3));
    tap(&mut app, "_");
    assert_eq!(own(&app).velocity(3), 90);
    tap(&mut app, "i150");
    special(&mut app, KeyCode::Enter);
    assert_eq!(own(&app).velocity(3), 150);

    // Edit chooses which is drawn, and x goes back to the project's.
    project_cursor_on(&mut app, ProjectControl::GrooveShown);
    tap(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    tap(&mut app, "/pull");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.shown_groove().0, GrooveId::named("Pull").unwrap());
    assert_eq!(app.song().groove, custom, "only what's drawn changes");
    tap(&mut app, "x");
    assert_eq!(app.shown_groove().0, custom);
}

#[test]
fn a_recorded_note_goes_on_the_row_nearest_where_the_groove_lands_it() {
    let (mut app, mut rt) = recording_app(vec![Step::default(); 4]);
    // Every other step lands 96 ticks late, 0.4 of a step.
    command(&mut app, "swing 70");
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    // Step 9 of the song is row 1, which lands at 9.4. At 9.65 it's nearer than row 2, at 10,
    // though on the grid row 2 would be.
    play_to(&mut app, &mut rt, &mut at, 9.65);
    let a = Key::char('a');
    app.key(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(rows[1].note, Note::On(60), "{rows:?}");
    assert_eq!(rows[2].note, Note::Empty);
    assert_eq!(
        rows[1].nudge, 6,
        "a quarter of a step after where the groove lands it"
    );
    // Let go at 9.8, which is a quarter of row 1's step as the groove makes it, 0.6 long.
    play_to(&mut app, &mut rt, &mut at, 9.8);
    app.key_released(a, time_at(at));
    let (pulses, length) = length_of(&rows_of(&app, 1)[1]).unwrap();
    assert_eq!(pulses, 1);
    assert_eq!(length, Some(0x40));

    // A note just before a late row's landing goes on it, and waits for it to land.
    play_to(&mut app, &mut rt, &mut at, 11.3);
    let s = Key::char('s');
    app.key(s, time_at(at));
    assert_eq!(rows_of(&app, 1)[3].note, Note::Empty, "not before it lands");
    play_to(&mut app, &mut rt, &mut at, 11.5);
    app.key_released(s, time_at(at));
    assert_eq!(rows_of(&app, 1)[3].note, Note::On(62));
    assert_eq!(
        rows_of(&app, 1)[3].nudge,
        -2,
        "a tenth of a step before it lands"
    );
}

#[test]
fn a_tracks_speed_on_the_page_is_chosen_from_a_list() {
    use vt_model::speed::Speed;
    use vt_tui::track_page::Control;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let speed = |app: &App| app.song().tracks[0].speed;
    let line = |app: &App| {
        track_page_text(app)
            .into_iter()
            .find(|line| line.contains("Speed"))
            .unwrap_or_default()
    };
    assert!(line(&app).contains("1/16"), "{}", line(&app));
    press(&mut app, "gg$jjjjjjjjjj");
    assert_eq!(app.track_control(), Control::Speed);
    assert_eq!(value_shown(&app).as_deref(), Some("1/16"));
    press(&mut app, "=");
    assert_eq!(speed(&app), Speed::EighthTriplet);
    press(&mut app, "==");
    assert_eq!(speed(&app), Speed::QuarterTriplet);
    press(&mut app, "====");
    assert_eq!(speed(&app), Speed::Quarter, "it stops at the end");
    assert!(line(&app).contains("1/4"), "{}", line(&app));
    press(&mut app, "x");
    assert_eq!(speed(&app), Speed::Sixteenth);
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "/1/8t");
    special(&mut app, KeyCode::Enter);
    assert_eq!(speed(&app), Speed::EighthTriplet);
    press(&mut app, "u");
    assert_eq!(speed(&app), Speed::Sixteenth);
}

#[test]
fn a_tracks_reset_on_the_page_is_typed_in_steps_or_bars() {
    use vt_tui::track_page::Control;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let reset = |app: &App| app.song().tracks[0].reset;
    let line = |app: &App| {
        track_page_text(app)
            .into_iter()
            .find(|line| line.contains("Reset"))
            .unwrap_or_default()
    };
    assert!(line(&app).contains("Off"), "{}", line(&app));
    press(&mut app, "gg$jjjjjjjjjjj");
    assert_eq!(app.track_control(), Control::Reset);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("Off: its chain plays on")
    );
    press(&mut app, "=");
    assert_eq!(reset(&app), Some(1));
    press(&mut app, "4=");
    assert_eq!(reset(&app), Some(5));
    assert!(line(&app).contains("5-step"), "{}", line(&app));
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("its chain starts again every 5 steps of the song")
    );
    // A large step goes to the next whole bar, and back to the one before.
    press(&mut app, "+");
    assert_eq!(reset(&app), Some(16));
    assert!(line(&app).contains("1bar"), "{}", line(&app));
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("its chain starts again every bar of the song")
    );
    press(&mut app, "+");
    assert_eq!(reset(&app), Some(32));
    press(&mut app, "-");
    assert_eq!(reset(&app), Some(31));
    press(&mut app, "_");
    assert_eq!(reset(&app), Some(16));
    press(&mut app, "_");
    assert_eq!(reset(&app), None, "below a bar is off");

    typed(&mut app, "12");
    assert_eq!(reset(&app), Some(12));
    typed(&mut app, "2bar");
    assert_eq!(reset(&app), Some(32));
    typed(&mut app, "0x40");
    assert_eq!(reset(&app), Some(64));
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("its chain starts again every 4 bars of the song")
    );
    typed(&mut app, "0");
    assert_eq!(reset(&app), Some(64), "0 isn't a reset");
    assert!(app.status.contains("1 to 1024"), "{}", app.status);
    typed(&mut app, "off");
    assert_eq!(reset(&app), None);
    press(&mut app, "u");
    assert_eq!(reset(&app), Some(64));
    press(&mut app, "x");
    assert_eq!(reset(&app), None);
    press(&mut app, "-");
    assert_eq!(reset(&app), None, "down from off stays off");
}

#[test]
fn a_tracks_reset_is_saved_only_when_set() {
    let mut song = Song::new(1);
    let plain = ron::to_string(&song.tracks[0]).unwrap();
    // An envelope's `reset` is another thing.
    assert!(!plain.contains("reset:None"), "{plain}");
    assert!(!plain.contains("reset:Some"), "{plain}");
    song.tracks[0].reset = Some(16);
    let saved = ron::to_string(&song.tracks[0]).unwrap();
    assert!(saved.contains("reset:Some(16)"), "{saved}");
    let back: vt_model::song::Track = ron::from_str(&saved).unwrap();
    assert_eq!(back.reset, Some(16));
}

#[test]
fn space_a_realigns_the_cursors_track_on_the_launch_grid_while_playing() {
    let (mut app, mut rt) = app();
    tap(&mut app, " a");
    assert_eq!(app.status, "Realign works while playing");

    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    tap(&mut app, " a");
    assert_eq!(
        app.status,
        "Track 01 realigns on the 1bar grid; Space a again takes it back"
    );
    block(&mut rt, &mut app);
    assert!(rt.engine().track_realign_waiting(0));
    assert!(!rt.engine().realign_waiting(), "only this track");
    assert!(app.realigning_tracks[0]);
    assert!(status_line(&app).contains(&realign_shown()));
    assert!(!app.dirty, "it moves playback, not the song");

    // Again, it's taken back.
    tap(&mut app, " a");
    assert_eq!(app.status, "Track 01's realign taken back");
    block(&mut rt, &mut app);
    assert!(!rt.engine().track_realign_waiting(0));
    assert!(!status_line(&app).contains("REALIGN"));

    // Asked for again, it lands on the bar and is done.
    tap(&mut app, " a");
    block(&mut rt, &mut app);
    assert!(app.realigning_tracks[0]);
    for _ in 0..400 {
        block(&mut rt, &mut app);
    }
    assert!(!app.realigning_tracks[0], "landed on the bar");
    press(&mut app, "u");
    assert!(!app.dirty);
}

#[test]
fn space_a_on_a_return_says_it_has_no_chain() {
    let (mut app, mut rt) = app();
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    let returns = app.song().sound_tracks() + MAX_GROUPS;
    app.cursor.track = returns;
    tap(&mut app, " a");
    assert_eq!(app.status, "Return A plays no chain to realign");
}

#[test]
fn space_a_works_in_every_view_and_space_k_lists_it_beside_space_capital_a() {
    let (mut app, mut rt) = app();
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    for view in ["1", "2", "3", "4", "6", "7", "9", "0"] {
        press(&mut app, &format!(" {view}"));
        tap(&mut app, " a");
        assert!(
            app.status.starts_with("Track 01 realigns"),
            "{view}: {}",
            app.status
        );
        block(&mut rt, &mut app);
        tap(&mut app, " a");
        assert_eq!(app.status, "Track 01's realign taken back", "{view}");
        block(&mut rt, &mut app);
    }
    let rows = app.help_rows();
    let listed = rows
        .iter()
        .filter(|row| row.trim_start().starts_with("Space a "))
        .count();
    assert_eq!(listed, 8, "{rows:?}");
}

#[test]
fn a_step_locks_the_speed_by_its_label() {
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, "llllli");
    press(&mut app, "rate");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "04");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("speed:04"), "{step}");
    assert_eq!(column_name(&app), "Machine: Speed");
    assert_eq!(value_shown(&app).as_deref(), Some("1/8"));
}

#[test]
fn a_track_at_an_eighth_has_its_own_playhead() {
    let mut song = Song::new(2);
    song.tracks[0].phrases.insert(0, Arc::new(Phrase::new(16)));
    song.tracks[1].phrases.insert(0, Arc::new(Phrase::new(4)));
    song.tracks[1].speed = vt_model::speed::Speed::Eighth;
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    app.set_playhead(6);
    assert_eq!(app.chain_playhead(0, 0), Some(6));
    assert_eq!(app.chain_playhead(1, 0), Some(3));
    app.set_playhead(9);
    assert_eq!(
        app.chain_playhead(1, 0),
        Some(0),
        "its fifth eighth is its first row again"
    );
}

#[test]
fn a_recorded_note_goes_on_the_row_of_its_tracks_own_speed() {
    let mut song = Song::new(2);
    song.tracks[0].phrases.insert(0, Arc::new(Phrase::new(16)));
    song.tracks[1]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(vec![Step::default(); 4])));
    song.tracks[1].speed = vt_model::speed::Speed::Eighth;
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, mut rt) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    press(&mut app, "W");
    let mut at = play_from_the_top(&mut app, &mut rt);
    press_at(&mut app, at, " r");
    tap_at(&mut app, at, "i");
    // Song step 9.5 is the eighth track's step 4.75, so row 1's start, at its step 5, is nearer
    // than row 0's, and the note is a quarter of an eighth early.
    play_to(&mut app, &mut rt, &mut at, 9.5);
    let a = Key::char('a');
    app.key(a, time_at(at));
    assert!(
        rows_of(&app, 1).iter().all(|step| step.note == Note::Empty),
        "not before its row lands"
    );
    // Let go half an eighth later, at its step 5.25, so it lasts half its row's step.
    play_to(&mut app, &mut rt, &mut at, 10.5);
    app.key_released(a, time_at(at));
    let rows = rows_of(&app, 1);
    assert_eq!(rows[1].note, Note::On(60), "{rows:?}");
    assert_eq!(rows[1].nudge, -6);
    let (pulses, length) = length_of(&rows[1]).unwrap();
    assert_eq!(pulses, 1);
    assert_eq!(length, Some(0x80));
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

#[test]
fn k_chooses_cnd_then_its_condition_from_a_list() {
    use vt_model::step::FxCommand;
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    let slot = |app: &App| app.phrase_here().unwrap().steps()[0].fx[0];
    press(&mut app, "lllllK");
    press(&mut app, "CND");
    special(&mut app, KeyCode::Enter);
    // Choosing `CND` lists the conditions, with Elektron's names.
    let picker = app.picker.as_ref().expect("the conditions are listed");
    assert_eq!(picker.title, "Conditions");
    assert_eq!(picker.focus, vt_tui::input::picker::Focus::Search);
    press(&mut app, "1:4");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "1:4");
    let depth = app.history.undo_depth();
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).command, Some(FxCommand::Cnd));
    assert_eq!(slot(&app).coarse(), 0x14);
    assert_eq!(app.history.undo_depth(), depth + 1, "one change");
    assert_eq!(
        column_name(&app),
        "Play the row only when condition xx holds"
    );
    assert_eq!(value_shown(&app).as_deref(), Some("1:4, pass 1 of every 4"));

    // `K` on its value lists them again, on its own.
    press(&mut app, "K");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.title, "Conditions");
    assert_eq!(picker.current().unwrap().label, "1:4");
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);

    // `=` steps through the list, and `+` to a longer cycle.
    press(&mut app, "=");
    assert_eq!(slot(&app).coarse(), 0x24);
    press(&mut app, "+");
    assert_eq!(slot(&app).coarse(), 0x25);
    press(&mut app, "--");
    assert_eq!(slot(&app).coarse(), 0x44, "back through 1:5 to 4:4");

    // A value that isn't a condition isn't taken.
    press(&mut app, "r09");
    assert_eq!(slot(&app).coarse(), 0x44);
    assert!(app.status.contains("isn't a condition"), "{}", app.status);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "r07");
    assert_eq!(slot(&app).coarse(), 0x07);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("1ST, the first time round only")
    );
}

#[test]
fn space_l_turns_fill_on_and_held_it_goes_back() {
    let (mut app, _realtime) = app();
    press(&mut app, " ");
    hold_for(&mut app, 'l', 100);
    assert!(app.fill, "tapped, it stays on");
    assert_eq!(app.status, "Fill on");
    let backend = ratatui::backend::TestBackend::new(120, 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::status::draw(frame, frame.area(), &app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let status: String = (0..120).map(|x| buffer[(x, 0)].symbol()).collect();
    let fill = format!("{} FILL", vt_tui::glyphs::STOP);
    assert!(status.contains(&fill), "{status}");

    press(&mut app, " ");
    hold_for(&mut app, 'l', 100);
    assert!(!app.fill, "tapped again, it's off");

    // Held, it's on until it's let go.
    press(&mut app, " ");
    let now = Instant::now();
    app.key(Key::char('l'), now);
    assert!(app.fill);
    app.key_repeated(Key::char('l'), now + Duration::from_millis(300));
    assert!(app.fill, "a repeat doesn't switch it back");
    app.key_released(Key::char('l'), now + Duration::from_millis(600));
    assert!(!app.fill, "held, it goes back");
    assert!(!app.dirty, "fill isn't part of the song");
}

#[test]
fn k_chooses_acu_then_its_amount_from_a_list() {
    use vt_model::step::FxCommand;
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    let slot = |app: &App| app.phrase_here().unwrap().steps()[0].fx[0];
    press(&mut app, "lllllK");
    press(&mut app, "ACU");
    special(&mut app, KeyCode::Enter);
    // Choosing `ACU` lists how far it can move the note, up and then down.
    let picker = app.picker.as_ref().expect("the amounts are listed");
    assert_eq!(picker.title, "Accumulate");
    assert_eq!(picker.current().unwrap().label, "+1", "it opens on one up");
    assert_eq!(
        listed(picker.current().unwrap()),
        "+1  01  Up 1 semitone",
        "the scale is off"
    );
    press(&mut app, "-2");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "-2");
    let depth = app.history.undo_depth();
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).command, Some(FxCommand::Acu));
    assert_eq!(slot(&app).coarse(), 0xFE);
    assert_eq!(app.history.undo_depth(), depth + 1, "one change");
    assert_eq!(
        column_name(&app),
        "Move the note on xx scale degrees each time it plays"
    );
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("down 2 semitones each time it plays, wrap on this row")
    );

    // `K` on its value lists them again, on its own.
    press(&mut app, "K");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "-2");
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);

    // `=` goes up through `00`, as the amount is signed, and `+` by seven.
    press(&mut app, "==");
    assert_eq!(slot(&app).coarse(), 0x00);
    assert_eq!(value_shown(&app).as_deref(), Some("doesn't move the note"));
    press(&mut app, "=+");
    assert_eq!(slot(&app).coarse(), 0x08);
    press(&mut app, "+++++");
    assert_eq!(slot(&app).coarse(), 0x1C, "it stops at 28");

    // A value past 28 isn't taken.
    press(&mut app, "r1D");
    assert_eq!(slot(&app).coarse(), 0x1C);
    assert!(app.status.contains("is past 28"), "{}", app.status);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "rE4");
    assert_eq!(slot(&app).coarse(), 0xE4);

    // With the scale on, it's degrees.
    command(&mut app, "set scale=on");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("down 28 scale degrees each time it plays, wrap on this row")
    );
}

#[test]
fn the_mod_page_sets_the_accumulator_and_says_what_each_setting_does() {
    use vt_model::accumulator::{Accumulation, AtLimit, Moves, Reset};
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, " 4LGk");
    let text = track_text(&app).join("\n");
    assert!(
        text.contains("Up     Down   Moves  Limit  Adds   Reset"),
        "{text}"
    );
    assert!(
        text.contains("Acc       7      7      Row    Wrap   Row    Chain"),
        "{text}"
    );
    assert_eq!(column_name(&app), "Accumulator up");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("7 semitones above the note as written")
    );
    let accumulation = |app: &App| app.song().tracks[0].accumulation;

    // The limits move by one, or seven with `+`, and stop at 1 and 28.
    press(&mut app, "=");
    assert_eq!(accumulation(&app).up, 8);
    press(&mut app, "+++++");
    assert_eq!(accumulation(&app).up, 28);
    press(&mut app, "x");
    assert_eq!(accumulation(&app), Accumulation::default());
    // Typed, it's a number of degrees.
    press(&mut app, "li3");
    special(&mut app, KeyCode::Enter);
    assert_eq!(accumulation(&app).down, 3);
    press(&mut app, "i40");
    special(&mut app, KeyCode::Enter);
    assert_eq!(accumulation(&app).down, 3);
    assert!(app.status.contains("from 1 to 28"), "{}", app.status);

    // The others are lists, which `K` shows with what each does.
    press(&mut app, "l");
    press(&mut app, "K");
    let picker = app.picker.as_ref().expect("the choices are listed");
    assert_eq!(picker.title, "Accumulator moves");
    let items: Vec<String> = picker.items().iter().map(listed).collect();
    assert_eq!(
        items,
        [
            "Row  Only the row with the ACU moves",
            "Track  Every row on the track moves with the ACUs"
        ]
    );
    press(&mut app, "j");
    special(&mut app, KeyCode::Enter);
    assert_eq!(accumulation(&app).moves, Moves::Track);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("every row on the track moves with the ACUs")
    );
    press(&mut app, "l=");
    assert_eq!(accumulation(&app).at_limit, AtLimit::Bounce);
    press(&mut app, "ll==");
    assert_eq!(accumulation(&app).reset, Reset::Loops(1));
    let text = track_text(&app).join("\n");
    assert!(text.contains("Track  Bounce Row    1 loop"), "{text}");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("each time round the chain")
    );
}

#[test]
fn a_return_has_no_accumulator() {
    let (mut app, _realtime) = app();
    let returns = app
        .song()
        .tracks
        .iter()
        .position(|track| !track.is_sound())
        .unwrap();
    for _ in 0..returns {
        press(&mut app, "W");
    }
    press(&mut app, " 4LG");
    let text = track_text(&app).join("\n");
    assert!(!text.contains("Acc "), "{text}");
}

#[test]
fn the_row_playing_shows_and_says_the_note_an_accumulator_moved_it_to() {
    use vt_tui::views::status::value_shown;
    let rows = vec!["C-4 -- acu:02".parse().unwrap(), "D-4 --".parse().unwrap()];
    let (mut app, mut rt) = recording_app(rows);
    let row_text = |app: &App, row: u16| {
        let buffer = screen(app);
        (0..40)
            .map(|x| buffer[(x, row + 2)].symbol())
            .collect::<String>()
    };
    let mut at = play_from_the_top(&mut app, &mut rt);
    // The first time round, it plays as written.
    play_to(&mut app, &mut rt, &mut at, 0.5);
    assert!(row_text(&app, 0).contains("C-4"), "{}", row_text(&app, 0));
    assert_eq!(value_shown(&app), None);
    // The second time, two semitones up, as the scale is off.
    play_to(&mut app, &mut rt, &mut at, 2.5);
    assert!(row_text(&app, 0).contains(">"), "{}", row_text(&app, 0));
    assert!(row_text(&app, 0).contains("D-4"), "{}", row_text(&app, 0));
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("the row playing is heard as D-4")
    );
    // The row after, which has none, plays as written.
    play_to(&mut app, &mut rt, &mut at, 3.5);
    assert!(row_text(&app, 1).contains("D-4"), "{}", row_text(&app, 1));
    assert_eq!(value_shown(&app), None);
}

#[test]
fn k_chooses_arp_and_its_value_is_typed_as_two_digits() {
    use vt_model::step::FxCommand;
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    let slot = |app: &App| app.phrase_here().unwrap().steps()[0].fx[0];
    press(&mut app, "lllllK");
    press(&mut app, "ARP");
    let picker = app.picker.as_ref().expect("the commands are listed");
    assert_eq!(
        listed(picker.current().unwrap()),
        "ARP  Arpeggio: the note, x up and y up"
    );
    special(&mut app, KeyCode::Enter);
    // As `RET`'s, its two digits are typed, and nothing goes in until they are.
    assert_eq!(app.mode, Mode::Replace);
    assert_eq!(slot(&app).command, None);
    let depth = app.history.undo_depth();
    press(&mut app, "47");
    assert_eq!(slot(&app).command, Some(FxCommand::Arp));
    assert_eq!(slot(&app).coarse(), 0x47);
    assert_eq!(app.history.undo_depth(), depth + 1, "one change");
    assert_eq!(column_name(&app), "Arpeggio: the note, x up and y up");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("the note, 4 and 7 semitones up, Up at Tick, 1 octave, legato")
    );

    // `=` and `-` move the second note and `+` and `_` the first, neither carrying into the other.
    press(&mut app, "===");
    assert_eq!(slot(&app).coarse(), 0x4A);
    press(&mut app, "______");
    assert_eq!(slot(&app).coarse(), 0x0A, "it stops at 0");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("the note and 10 semitones up, Up at Tick, 1 octave, legato")
    );
    press(&mut app, "r00");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("only the note's octaves, Up at Tick, 1 octave, legato")
    );

    // With the scale on, they're degrees.
    press(&mut app, "r24");
    command(&mut app, "set scale=on");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("the note, 2 and 4 scale degrees up, Up at Tick, 1 octave, legato")
    );
}

#[test]
fn the_mod_page_sets_the_arpeggio_and_says_what_each_setting_does() {
    use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio};
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, " 4LG");
    let text = track_text(&app).join("\n");
    assert!(text.contains("Rate   Mode        Oct    Retrig"), "{text}");
    assert!(
        text.contains("Arp       Tick   Up          1      off"),
        "{text}"
    );
    assert_eq!(column_name(&app), "Arp rate");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("six notes to a step, a tick, as DEL counts")
    );
    let arpeggio = |app: &App| app.song().tracks[0].arpeggio;

    // The rates go from the shortest note to the longest, and stop at either end.
    press(&mut app, "=");
    assert_eq!(arpeggio(&app).rate, ArpRate::SixtyFourth);
    press(&mut app, "---");
    assert_eq!(arpeggio(&app).rate, ArpRate::HundredTwentyEighth);
    press(&mut app, "x");
    assert_eq!(arpeggio(&app), Arpeggio::default());
    // `K` lists them with how many notes each plays to a step.
    press(&mut app, "K");
    let picker = app.picker.as_ref().expect("the rates are listed");
    assert_eq!(picker.title, "Arp rate");
    assert_eq!(picker.current().unwrap().label, "Tick");
    let items: Vec<String> = picker.items().iter().map(listed).collect();
    assert_eq!(items[4], "1/32  Two notes to a step");
    assert_eq!(items.len(), 9);
    press(&mut app, "jjj");
    special(&mut app, KeyCode::Enter);
    assert_eq!(arpeggio(&app).rate, ArpRate::ThirtySecond);

    // The modes go round, and `K` lists them.
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Arp mode");
    press(&mut app, "-");
    assert_eq!(arpeggio(&app).mode, ArpMode::Random);
    press(&mut app, "K");
    let items: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(listed)
        .collect();
    assert_eq!(items[2], "Up-down  Up and back down, the ends played once");
    assert_eq!(
        items[3],
        "Up-down-inc  Up and back down, the ends played twice"
    );
    press(&mut app, "kk");
    special(&mut app, KeyCode::Enter);
    assert_eq!(arpeggio(&app).mode, ArpMode::UpDown);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("up and back down, the ends played once")
    );

    // The octaves are a number from 1 to 4.
    press(&mut app, "l====");
    assert_eq!(arpeggio(&app).octaves, 4);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("4 octaves: the notes climb that many before it starts again")
    );
    press(&mut app, "i2");
    special(&mut app, KeyCode::Enter);
    assert_eq!(arpeggio(&app).octaves, 2);
    press(&mut app, "i5");
    special(&mut app, KeyCode::Enter);
    assert_eq!(arpeggio(&app).octaves, 2);
    assert!(app.status.contains("from 1 to 4"), "{}", app.status);

    // Retrig is on or off.
    press(&mut app, "l=");
    assert!(arpeggio(&app).retrig);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("each note starts again, with its envelopes")
    );
    let text = track_text(&app).join("\n");
    assert!(
        text.contains("Arp       1/32   Up-down     2      on"),
        "{text}"
    );
    press(&mut app, "x");
    assert!(!arpeggio(&app).retrig);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("legato: each note only changes the pitch, and Acid slides")
    );

    // The longest mode has room, and the columns after it stay in line.
    press(&mut app, "hh+");
    assert_eq!(arpeggio(&app).mode, ArpMode::UpDownInc);
    let text = track_text(&app).join("\n");
    assert!(
        text.contains("Arp       1/32   Up-down-inc 2      off"),
        "{text}"
    );
}

#[test]
fn a_return_has_no_arpeggio() {
    let (mut app, _realtime) = app();
    let returns = app
        .song()
        .tracks
        .iter()
        .position(|track| !track.is_sound())
        .unwrap();
    for _ in 0..returns {
        press(&mut app, "W");
    }
    press(&mut app, " 4LG");
    let text = track_text(&app).join("\n");
    assert!(!text.contains("Arp "), "{text}");
}

#[test]
fn order_plays_the_phrases_rows_in_an_order_from_a_word_or_a_list() {
    use vt_model::order::Order;
    use vt_tui::input::command::Line;
    let (mut app, _realtime) = app();
    let order = |app: &App| app.phrase_here().unwrap().order();
    let heading = |app: &App| -> String {
        let buffer = screen(app);
        (0..80).map(|x| buffer[(x, 0)].symbol()).collect()
    };
    assert!(!heading(&app).contains("Forward"), "forward isn't named");
    command(&mut app, "order up-down");
    assert_eq!(order(&app), Order::UpDown);
    assert!(app.status.contains("the ends once"), "{}", app.status);
    assert!(heading(&app).contains("Up-down"), "{}", heading(&app));
    command(&mut app, "order pendulum");
    assert_eq!(order(&app), Order::UpDown, "the old names are gone");
    assert!(app.status.contains(":order takes"), "{}", app.status);
    command(&mut app, "order sideways");
    assert_eq!(order(&app), Order::UpDown);
    assert!(app.status.contains(":order takes"), "{}", app.status);
    // With nothing after it, a list opens on the order it plays in.
    command(&mut app, "order");
    let picker = app.picker.as_ref().expect("the list is open");
    assert_eq!(
        picker.current().map(|item| item.pick.clone()),
        Some(Pick::Line(Line::Order(Some(Order::UpDown))))
    );
    assert_eq!(picker.shown().count(), Order::ALL.len());
    press(&mut app, "shuf");
    special(&mut app, KeyCode::Enter);
    assert_eq!(order(&app), Order::Shuffle);
    assert_eq!(
        app.song().tracks[1].phrases[&0].order(),
        Order::Forward,
        "only this phrase's"
    );
    press(&mut app, "u");
    assert_eq!(order(&app), Order::UpDown);
    press(&mut app, "u");
    assert_eq!(order(&app), Order::Forward);
}

#[test]
fn playing_from_a_row_starts_at_its_step_with_the_walk_where_it_is_then() {
    let (mut app, _realtime) = app();
    command(&mut app, "len 4");
    command(&mut app, "order reverse");
    // A row's step is where it's written, whatever the order, and the walk plays what it plays
    // there, as it would playing up to it: in reverse, the bottom row first.
    assert_eq!(app.section_step(), 0);
    press(&mut app, "j");
    assert_eq!(app.section_step(), 1);
}

#[test]
fn k_lists_a_slots_bypass_with_its_effect_and_a_step_locks_it() {
    use vt_model::step::FxCommand;
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, " 4LLK");
    press(&mut app, "delay");
    special(&mut app, KeyCode::Enter);
    press(&mut app, " 3");
    assert_eq!(app.view, View::Phrase);
    let slot = |app: &App| app.phrase_here().unwrap().steps()[0].fx[0];
    press(&mut app, "lllllK");
    press(&mut app, "1byp");
    let picker = app.picker.as_ref().expect("the commands are listed");
    let item = picker.current().unwrap();
    assert_eq!(listed(item), "1BYP  1 Bypass");
    assert_eq!(item.category, "FX 1 Delay", "with the slot's effect");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Replace);
    press(&mut app, "01");
    let bypass = SettingId::bypass(0).unwrap();
    assert_eq!(slot(&app).command, Some(FxCommand::Set(bypass)));
    assert_eq!(slot(&app).coarse(), 0x01);
    assert_eq!(column_name(&app), "FX 1: Bypass");
    assert_eq!(value_shown(&app).as_deref(), Some("Bypassed"));
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("1.bypass:01"), "{step}");
    // It stops at the last choice, and back down is the slot heard.
    press(&mut app, "=");
    assert_eq!(slot(&app).coarse(), 0x01);
    press(&mut app, "-");
    assert_eq!(value_shown(&app).as_deref(), Some("In"));
    // Typed by its label too, and an LFO or envelope can't move it.
    let track = &app.song().tracks[0];
    assert_eq!(
        FxCommand::typed("1byp", track),
        Some(FxCommand::Set(bypass))
    );
    assert!(!SettingId::for_lfo(track, 0).contains(&bypass));
    assert!(!SettingId::for_envelope(track).contains(&bypass));
    // With nothing in a slot, there's nothing to bypass.
    assert!(FxCommand::on_track(track).contains(&FxCommand::Set(bypass)));
    let empty = FxCommand::Set(SettingId::bypass(1).unwrap());
    assert!(!FxCommand::on_track(track).contains(&empty));
}

#[test]
fn the_fx_page_fades_a_slot_a_lock_bypasses_while_the_song_plays() {
    use vt_engine::track_state::LiveValues;
    let (mut app, _realtime) = app();
    press(&mut app, " 4LLK");
    press(&mut app, "delay");
    special(&mut app, KeyCode::Enter);
    // Off the slot, so the cursor doesn't keep its colour.
    press(&mut app, "jj");
    // Whether the slot's effect is drawn faint.
    let faint = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height).any(|y| {
            let row: Vec<&str> = (0..60).map(|x| buffer[(x, y)].symbol()).collect();
            let at = row.windows(5).position(|cells| cells.concat() == "Delay");
            at.is_some_and(|x| buffer[(x as u16, y)].fg == vt_tui::theme::faint().fg.unwrap())
        })
    };
    let heard = |bypassed: bool| LiveValues {
        bypassed: [bypassed, false, false],
        ..LiveValues::default()
    };
    assert!(!faint(&app));
    app.transport = Transport::Playing;
    app.live = Some(heard(true));
    assert!(faint(&app), "a lock switches it out as it plays");
    app.live = Some(heard(false));
    assert!(!faint(&app));
    // Stopped, it shows the slot's own, whatever was last heard.
    app.transport = Transport::Stopped;
    app.live = Some(heard(true));
    assert!(!faint(&app));
    press(&mut app, "kkb");
    press(&mut app, "jj");
    assert!(faint(&app));
}

#[test]
fn the_track_page_sets_how_long_a_tie_glides() {
    use vt_model::settings::glide_value;
    use vt_tui::track_page::Control;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let glide = |app: &App| app.song().tracks[0].glide;
    let line = |app: &App| {
        track_page_text(app)
            .into_iter()
            .find(|line| line.contains("Glide"))
            .unwrap_or_default()
    };
    assert!(line(&app).contains("Off"), "{}", line(&app));

    // Beside High cut.
    press(&mut app, "jjjj$");
    assert_eq!(app.track_control(), Control::Glide);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("Off: a tie jumps to the next note")
    );
    press(&mut app, "=");
    assert_eq!(glide(&app), 1);
    assert_eq!(value_shown(&app).as_deref(), Some("2 ms"));
    press(&mut app, "+");
    assert_eq!(glide(&app), 0x11, "sixteen more");
    typed(&mut app, "60");
    assert_eq!(glide(&app), glide_value(60.0));
    assert!(line(&app).contains("60 ms"), "{}", line(&app));
    typed(&mut app, "5000");
    assert_eq!(glide(&app), glide_value(60.0));
    assert!(app.status.contains("up to 2000"), "{}", app.status);
    typed(&mut app, "off");
    assert_eq!(glide(&app), 0);
    press(&mut app, "300=");
    assert_eq!(glide(&app), 0xFF, "it stops at the top");
    assert!(line(&app).contains("2.00 s"), "{}", line(&app));
    press(&mut app, "x");
    assert_eq!(glide(&app), 0, "back to off");
    press(&mut app, "u");
    assert_eq!(glide(&app), 0xFF);
    press(&mut app, "x");

    // Off on `Acid`, a tie slides as a 303's does, by the voice's own `Slide`.
    press(&mut app, "ggh");
    press(&mut app, "K");
    press(&mut app, "/acid");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::Acid)
    );
    assert!(line(&app).contains("Slide"), "{}", line(&app));
    press(&mut app, "gg$jjjjjjjjjjjj");
    assert_eq!(app.track_control(), Control::Glide);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("Off: a tie slides by Acid's own Slide")
    );
}

#[test]
fn a_step_locks_the_glide_by_its_label() {
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, "llllli");
    press(&mut app, "gld");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "80");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("glide:80"), "{step}");
    assert_eq!(column_name(&app), "Machine: Glide");
    assert_eq!(value_shown(&app).as_deref(), Some("63 ms"));
}

#[test]
fn space_a_realigns_every_track_on_the_launch_grid_while_playing() {
    let (mut app, mut rt) = app();
    tap(&mut app, " A");
    assert_eq!(app.status, "Realign works while playing");

    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    tap(&mut app, " A");
    assert_eq!(
        app.status,
        "Every track realigns on the 1bar grid; Space A again takes it back"
    );
    block(&mut rt, &mut app);
    assert!(rt.engine().realign_waiting());
    assert!(app.realigning);
    assert!(status_line(&app).contains(&realign_shown()));
    assert!(!app.dirty, "it moves playback, not the song");

    // Again, it's taken back.
    tap(&mut app, " A");
    assert_eq!(app.status, "Realign taken back");
    block(&mut rt, &mut app);
    assert!(!rt.engine().realign_waiting());
    assert!(!status_line(&app).contains("REALIGN"));

    // Asked for again, it lands on the bar and is done.
    tap(&mut app, " A");
    block(&mut rt, &mut app);
    assert!(app.realigning);
    for _ in 0..400 {
        block(&mut rt, &mut app);
    }
    assert!(!app.realigning, "landed on the bar");
    assert_eq!(app.transport, Transport::Playing);

    // Nothing to undo.
    press(&mut app, "u");
    assert!(!app.dirty);
}

#[test]
fn a_realign_says_it_waits_for_the_bar_when_launches_wait_for_a_chain() {
    let (mut app, mut rt) = app();
    command(&mut app, "launch chain");
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    tap(&mut app, " A");
    assert_eq!(
        app.status,
        "Every track realigns on the next bar; Space A again takes it back"
    );
}

#[test]
fn space_a_works_in_every_view_and_space_k_lists_it() {
    let (mut app, mut rt) = app();
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    for view in ["1", "2", "3", "4", "6", "7", "9", "0"] {
        press(&mut app, &format!(" {view}"));
        tap(&mut app, " A");
        assert!(
            app.status.starts_with("Every track realigns"),
            "{view}: {}",
            app.status
        );
        // The engine says it's waiting in its next report, which the second press reads.
        block(&mut rt, &mut app);
        tap(&mut app, " A");
        assert_eq!(app.status, "Realign taken back", "{view}");
        block(&mut rt, &mut app);
    }
    let rows = app.help_rows();
    let listed = rows
        .iter()
        .filter(|row| row.trim_start().starts_with("Space A "))
        .count();
    assert_eq!(listed, 8, "{rows:?}");
}

/// A second of silence at 44.1 kHz, as a file in a folder of its own.
fn file_at_44k() -> PathBuf {
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("break.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav, spec).unwrap();
    for _ in 0..44_100 {
        writer.write_sample(0_i16).unwrap();
    }
    writer.finalize().unwrap();
    wav
}

#[test]
fn sample_sets_where_the_tracks_sample_plays_in_its_files_own_frames() {
    use vt_model::bank::Loop;
    let (mut app, _realtime) = app();
    command(&mut app, "sample start 0.25");
    assert!(app.status.contains("plays no sample"), "{}", app.status);
    command(&mut app, &format!("sample {}", file_at_44k().display()));
    let entry = |app: &App| app.song().bank.get(1).unwrap().edit.clone();

    command(&mut app, "sample start 0.25");
    assert_eq!(entry(&app).points.start, 11_025, "at the file's 44.1 kHz");
    assert!(app.status.contains("starts at 0.250 s"), "{}", app.status);
    command(&mut app, "sample end 500ms");
    assert_eq!(entry(&app).points.end, Some(22_050));
    command(&mut app, "sample end 0.1");
    assert!(app.status.contains("before the start"), "{}", app.status);
    assert_eq!(entry(&app).points.end, Some(22_050), "unchanged");
    command(&mut app, "sample loop 0.3 0.4 10ms");
    assert_eq!(
        entry(&app).points.loop_,
        Some(Loop {
            start: 13_230,
            end: 17_640,
            crossfade: 441,
        })
    );
    command(&mut app, "sample reverse");
    assert!(entry(&app).play.reverse);
    command(&mut app, "sample fade 10ms 20ms");
    assert_eq!(
        (entry(&app).play.fade_in, entry(&app).play.fade_out),
        (441, 882)
    );
    command(&mut app, "sample gain -3");
    command(&mut app, "sample tune 2 -30");
    let play = entry(&app).play;
    assert_eq!((play.gain_db, play.tune, play.cents), (-3.0, 2, -30));
    command(&mut app, "sample end");
    assert_eq!(entry(&app).points.end, None, "the file's end");

    // Each is a step to undo.
    press(&mut app, "uuu");
    assert_eq!(entry(&app).play.fade_in, 441);
    assert_eq!(entry(&app).play.gain_db, 0.0);
    press(&mut app, "uu");
    assert!(!entry(&app).play.reverse);
    assert!(entry(&app).points.loop_.is_some());
}

#[test]
fn the_sound_page_sets_a_samplers_start_length_and_play() {
    use vt_model::settings::PLAY_CHOICES;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let setting = |app: &App, id| app.song().tracks[0].setting(id);
    press(&mut app, "kkkkk");
    assert_eq!(app.track_control().label_on(&app.song().tracks[0]), "Start");
    typed(&mut app, "40");
    assert_eq!(setting(&app, SettingId::SAMPLE_START), 0x40);
    assert_eq!(value_shown(&app).as_deref(), Some("25 %"));
    press(&mut app, "j_");
    assert_eq!(setting(&app, SettingId::SAMPLE_LENGTH), 0xEF);
    press(&mut app, "j=");
    assert_eq!(setting(&app, SettingId::PLAY), 1);
    assert!(track_page_text(&app)[7].contains(PLAY_CHOICES[1]));
    press(&mut app, "===");
    assert_eq!(setting(&app, SettingId::PLAY), 3, "Thru is the last");
    assert!(track_page_text(&app)[7].contains("Thru"));
    // Above Start, the slice, which plays the sample whole until it's sliced.
    press(&mut app, "kkk");
    assert_eq!(app.track_control().label_on(&app.song().tracks[0]), "Slice");
    assert!(track_page_text(&app)[4].contains("00   whole"));
    press(&mut app, "==");
    assert_eq!(setting(&app, SettingId::SLICE), 2);
    press(&mut app, "jjj");
    press(&mut app, "x");
    assert_eq!(setting(&app, SettingId::PLAY), 0);
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker, "K lists the ways to play");
    special(&mut app, KeyCode::Esc);
}

/// A first track whose rows last these pulses, each with a note, beside a second track of this
/// many rows at this speed, numbered by their notes from `C-4`: the shape of the Blade test
/// project, whose acid line is 7 rows over 16 pulses beside drums of 16 rows.
fn timeline_song(pulses: &[u8], rows: usize, speed: vt_model::speed::Speed) -> (App, Realtime) {
    use vt_model::step::{Gate, GateKind};
    let mut song = Song::new(2);
    let steps = pulses
        .iter()
        .map(|pulses| Step {
            note: Note::On(48),
            gate: Some(Gate {
                kind: GateKind::Single,
                pulses: *pulses,
                length: None,
            }),
            ..Step::default()
        })
        .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let mut phrase = Phrase::new(rows);
    for row in 0..rows {
        phrase.step_mut(row).unwrap().note = Note::On(60 + row as u8);
    }
    song.tracks[1].phrases.insert(0, Arc::new(phrase));
    song.tracks[1].speed = speed;
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

/// The Blade project's acid rows, 16 pulses in all.
const BLADE: [u8; 7] = [2, 1, 3, 4, 3, 1, 2];

/// The second track's note on a row, as its rows are numbered from `C-4`.
fn note_name(row: usize) -> String {
    Note::On(60 + row as u8).to_string()
}

#[test]
fn the_timeline_gives_a_row_a_line_for_each_pulse() {
    let (app, _realtime) = timeline_song(&BLADE, 16, vt_model::speed::Speed::Sixteenth);
    let buffer = screen(&app);
    // Each row's own line has its number, and its continuation lines none.
    let numbered: Vec<(u16, String)> = (0..16)
        .map(|line| (line, row_number_at(&buffer, line + 2)))
        .filter(|(_, number)| !number.trim().is_empty())
        .collect();
    let expected: Vec<(u16, String)> = [0, 2, 3, 6, 10, 13, 14]
        .iter()
        .enumerate()
        .map(|(row, line)| (*line, format!(" {row:02}")))
        .collect();
    assert_eq!(numbered, expected);
    // A continuation line is dim: a line under the note it holds, and nothing else.
    let text = |y: u16| -> String { (9..30).map(|x| buffer[(x, y)].symbol()).collect() };
    assert_eq!(text(3).trim_end(), " │");
    assert_eq!(Some(buffer[(10, 3)].fg), vt_tui::theme::faint().fg);
    // The FX band carries on through it.
    assert_eq!(buffer[(27, 3)].bg, vt_tui::theme::FX_BAND);
    // Beats are lines that start on a beat. Row 02 lasts steps 3 to 5, so the rule before the
    // beat at step 4 is under its own line, and row 03's second line is before step 8's.
    let beat = Some(vt_tui::theme::SURFACE1);
    assert_eq!(rule_under(&buffer, 5), beat);
    assert_eq!(rule_under(&buffer, 9), beat);
    assert_eq!(rule_under(&buffer, 2), None);
    // Only row 00 starts on a beat: the others fall between, row 04 on step 10.
    assert_eq!(
        buffer[(5, 2)].fg,
        vt_tui::theme::YELLOW,
        "row 00 is on a beat"
    );
    assert_ne!(buffer[(5, 12)].fg, vt_tui::theme::YELLOW, "row 04 isn't");
}

#[test]
fn other_tracks_line_up_by_time_in_the_timeline() {
    let (app, _realtime) = timeline_song(&BLADE, 16, vt_model::speed::Speed::Sixteenth);
    let buffer = screen(&app);
    // The drums' sixteenths go down beside the acid's lines, one a line, rather than one a row.
    for line in 0..16 {
        assert_eq!(
            beside(&buffer, 0, line).0,
            format!(" {}", note_name(line as usize))
        );
    }
}

#[test]
fn a_slower_track_leaves_lines_blank_and_a_faster_one_marks_them() {
    let (app, _realtime) = timeline_song(&[1; 16], 8, vt_model::speed::Speed::Eighth);
    let buffer = screen(&app);
    for line in 0..16 {
        let expected = if line % 2 == 0 {
            format!(" {}", note_name(line as usize / 2))
        } else {
            "    ".to_owned()
        };
        assert_eq!(beside(&buffer, 0, line).0, expected, "line {line}");
    }
    // At a 32nd, two of its rows start in each sixteenth: the first shows, marked.
    let (app, _realtime) = timeline_song(&[1; 16], 32, vt_model::speed::Speed::ThirtySecond);
    let buffer = screen(&app);
    for line in 0..16 {
        let expected = format!(
            "{}{}",
            vt_tui::views::phrase::MORE_STARTS,
            note_name(line as usize * 2)
        );
        assert_eq!(beside(&buffer, 0, line).0, expected, "line {line}");
    }
}

#[test]
fn j_and_k_skip_continuation_lines_and_counts_count_rows() {
    let (mut app, _realtime) = timeline_song(&BLADE, 16, vt_model::speed::Speed::Sixteenth);
    press(&mut app, "j");
    assert_eq!(app.cursor.step, 1);
    press(&mut app, "2j");
    assert_eq!(app.cursor.step, 3, "a count counts rows");
    let buffer = screen(&app);
    // Relative numbers count rows too, and continuation lines have none.
    let gutter = |y: u16| -> String { (0..6).map(|x| buffer[(x, y)].symbol()).collect() };
    assert_eq!(gutter(2), " 3  00");
    assert_eq!(gutter(3).trim(), "");
    assert_eq!(gutter(4), " 2  01");
    assert_eq!(gutter(5), " 1  02");
    assert_eq!(gutter(8), "    03");
    assert_eq!(
        Some(buffer[(9, 8)].bg),
        vt_tui::theme::cursor(app.mode).bg,
        "the cursor is on row 03's own line"
    );
    press(&mut app, "3k");
    assert_eq!(app.cursor.step, 0);
    // A selection covers rows, and their continuation lines with them.
    press(&mut app, "Vj");
    let buffer = screen(&app);
    assert_eq!(Some(buffer[(10, 3)].bg), vt_tui::theme::selected().bg);
}

#[test]
fn the_view_scrolls_by_the_lines_rows_take() {
    // Rows of 8 pulses, 128 lines, on a screen of 21.
    let (mut app, _realtime) = timeline_song(&[8; 16], 16, vt_model::speed::Speed::Sixteenth);
    app.follow_cursor(21);
    press(&mut app, "3j");
    app.follow_cursor(21);
    assert_eq!(
        app.top, 2,
        "rows 02 and 03 are the 16 lines that fit with row 03"
    );
    press(&mut app, "L");
    assert_eq!(
        app.cursor.step, 3,
        "L goes to the last row whose lines all fit"
    );
    press(&mut app, "H");
    assert_eq!(app.cursor.step, 2);
    press(&mut app, "G");
    app.follow_cursor(21);
    assert_eq!(app.top, 14, "the last two rows fill 16 of the lines");
    // Collapsed, a row a line again, so every row fits.
    press(&mut app, "za");
    app.follow_cursor(21);
    assert_eq!(app.top, 0);
}

#[test]
fn the_playheads_follow_the_timeline() {
    let (mut app, _realtime) = timeline_song(&BLADE, 16, vt_model::speed::Speed::Sixteenth);
    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    // Step 5 is row 02's third pulse, on line 5, and the drums' row 05.
    app.set_playhead(5);
    let buffer = screen(&app);
    assert_eq!(buffer[(7, 7)].symbol(), ">", "on row 02's last line");
    assert_eq!(buffer[(7, 5)].symbol(), " ", "not on row 02's own line");
    assert_eq!(beside(&buffer, 0, 5).0, format!(">{}", note_name(5)));
}

#[test]
fn z_a_collapses_the_timeline_to_a_line_a_row_and_opens_it_again() {
    let (mut app, _realtime) = timeline_song(&BLADE, 16, vt_model::speed::Speed::Sixteenth);
    assert!(app.timeline_on(), "the timeline is the default");
    press(&mut app, "za");
    assert!(!app.timeline_on());
    assert!(app.status.contains("z a"), "{}", app.status);
    let buffer = screen(&app);
    for row in 0..7 {
        assert_eq!(row_number_at(&buffer, row + 2), format!(" {row:02}"));
    }
    // Collapsed, the other tracks' columns go by row number, as before the timeline.
    assert_eq!(beside(&buffer, 0, 3).0, format!(" {}", note_name(3)));
    press(&mut app, "za");
    assert!(app.timeline_on());
    assert_eq!(row_number_at(&screen(&app), 5), " 02");
}

#[test]
fn a_view_with_nothing_to_lay_out_in_time_looks_as_it_did() {
    let (mut app, _realtime) = timeline_song(&[1; 16], 16, vt_model::speed::Speed::Sixteenth);
    let timeline = screen(&app);
    press(&mut app, "za");
    assert_eq!(screen(&app), timeline);
}

#[test]
fn group_puts_tracks_in_a_group_or_none_as_one_undo_step() {
    let (mut app, _realtime) = app();
    let groups = |app: &App| {
        app.song().tracks[..4]
            .iter()
            .map(|track| track.group)
            .collect::<Vec<_>>()
    };
    command(&mut app, "track add 2");
    command(&mut app, "group 1 1-3");
    assert_eq!(groups(&app), [Some(1), Some(1), Some(1), None]);
    assert!(
        app.status.contains("Tracks 01-03 in group 1"),
        "{}",
        app.status
    );
    press(&mut app, "u");
    assert_eq!(groups(&app), [None; 4], "one undo takes them all out");
    app.key(
        Key {
            ctrl: true,
            ..Key::char('r')
        },
        Instant::now(),
    );
    assert_eq!(groups(&app), [Some(1), Some(1), Some(1), None]);

    command(&mut app, "group 2");
    assert_eq!(groups(&app)[0], Some(2), "the cursor's track");
    command(&mut app, "group off 2-3");
    assert_eq!(groups(&app), [Some(2), None, None, None]);
    command(&mut app, "group off");
    assert_eq!(groups(&app), [None; 4]);
    assert!(
        app.status.contains("Track 01 in no group"),
        "{}",
        app.status
    );

    let sounds = app.song().sound_tracks();
    command(&mut app, &format!("group 1 1-{}", sounds + 1));
    assert_eq!(
        groups(&app),
        [None; 4],
        "past the last track, nothing changes"
    );
    assert!(
        app.status.contains(&format!("{sounds} tracks")),
        "{}",
        app.status
    );
    command(&mut app, "group 5 1");
    assert!(app.status.contains(":group takes"), "{}", app.status);

    // A group can't join a group. The track picker reaches it, empty and folded away.
    let group = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    press(&mut app, " fbG1");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.cursor.track, group);
    command(&mut app, "group 2");
    assert_eq!(app.song().tracks[group].group, None);
    assert!(app.status.contains("Only tracks"), "{}", app.status);
}

#[test]
fn the_track_page_puts_a_track_in_a_group_and_a_group_has_its_own_page() {
    let (mut app, _realtime) = on_track_page();
    let group = |app: &App| app.song().tracks[0].group;
    // Under Send B, where the track's output is.
    press(&mut app, "kkk$");
    assert_eq!(app.track_control().label(), "Group");
    assert!(track_page_text(&app)[7].contains("Group     Off"));
    press(&mut app, "=");
    assert_eq!(group(&app), Some(1));
    assert!(track_page_text(&app)[7].contains("G1"));
    press(&mut app, "9=");
    assert_eq!(group(&app), Some(4), "4 is the last");
    typed(&mut app, "g2");
    assert_eq!(group(&app), Some(2));
    typed(&mut app, "5");
    assert_eq!(group(&app), Some(2));
    assert!(app.status.contains("1 to 4"), "{}", app.status);
    typed(&mut app, "off");
    assert_eq!(group(&app), None);
    press(&mut app, "u");
    assert_eq!(group(&app), Some(2));
    press(&mut app, "x");
    assert_eq!(group(&app), None);

    // A group's own page has its trim, level, pan and sends, on the right where a track's output
    // is.
    // The empty groups are folded away, so this is with them shown.
    let at = app.song().fixed_track(TrackKind::Group(2)).unwrap();
    app.folds.set_all(false);
    to_track(&mut app, at);
    let lines = track_page_text(&app);
    assert!(lines[0].contains("Group 2"), "{}", lines[0]);
    let output = lines[2].find("Trim").unwrap();
    assert!(output > 30, "{}", lines[2]);
    let labels = [
        (2, "Trim"),
        (3, "Level"),
        (4, "Pan"),
        (5, "Send A"),
        (6, "Send B"),
    ];
    for (line, label) in labels {
        assert_eq!(lines[line].find(label), Some(output), "{}", lines[line]);
    }
    press(&mut app, "ggjjjj");
    assert_eq!(app.track_control().label(), "Send B");
    typed(&mut app, "60");
    assert_eq!(app.song().tracks[at].send_b, 0x60);
}

#[test]
fn a_groups_page_lists_its_tracks_with_their_mutes_and_solos() {
    let (mut app, _realtime) = on_track_page();
    command(&mut app, "track add 6");
    command(&mut app, "group 1 2-4");
    let at = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    press(&mut app, &"W".repeat(at));
    let lines = track_page_text(&app);
    assert!(lines[2].starts_with("Tracks    02"), "{}", lines[2]);
    assert!(lines[3].starts_with("          03"), "{}", lines[3]);
    assert!(lines[4].starts_with("          04"), "{}", lines[4]);
    assert!(!lines[5].contains("05"), "{}", lines[5]);
    // The output stays beside the list.
    assert!(lines[2].contains("Trim"), "{}", lines[2]);
    assert!(lines[3].contains("Level"), "{}", lines[3]);
    app.levels[2].mute = true;
    assert!(
        track_page_text(&app)[3].contains(vt_tui::glyphs::MUTE),
        "{}",
        track_page_text(&app)[3]
    );
    // An empty group says so. It's folded away, so `W` steps over it, and the picker reaches it.
    let empty = app.song().fixed_track(TrackKind::Group(2)).unwrap();
    press(&mut app, "W");
    assert_eq!(app.cursor.track, empty + MAX_GROUPS - 1, "on A");
    press(&mut app, " fbG2");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.cursor.track, empty);
    assert!(track_page_text(&app)[2].starts_with("Tracks    None"));
}

#[test]
fn a_groups_icon_and_its_tracks_group_are_in_its_colour() {
    let (mut app, _realtime) = on_track_page();
    command(&mut app, "group 2");
    let color = vt_tui::theme::group(2);
    let cell_color = |app: &App, text: &str| {
        let backend = ratatui::backend::TestBackend::new(120, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let lines: Vec<String> = (0..24)
            .map(|y| (0..120).map(|x| buffer[(x, y)].symbol()).collect())
            .collect();
        let (y, line) = lines
            .iter()
            .enumerate()
            .find(|(_, line)| line.contains(text))
            .unwrap();
        let column = line[..line.find(text).unwrap()].chars().count();
        buffer[(column as u16, y as u16)].fg
    };
    assert_eq!(cell_color(&app, "G2"), color, "the track's Group");
    let at = app.song().fixed_track(TrackKind::Group(2)).unwrap();
    to_track(&mut app, at);
    let icon = vt_tui::glyphs::GROUP.to_string();
    assert_eq!(cell_color(&app, &icon), color, "the group's icon");
}

/// A view drawn 160 columns wide and 24 high.
fn drawn(
    app: &App,
    draw: fn(&mut ratatui::Frame, ratatui::layout::Rect, &App),
) -> ratatui::buffer::Buffer {
    let backend = ratatui::backend::TestBackend::new(160, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| draw(frame, frame.area(), app))
        .unwrap();
    terminal.backend().buffer().clone()
}

/// What's drawn, a line at a time.
fn drawn_lines(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

/// The colour of the first cell of some text on a line, from a column on.
fn color_from(buffer: &ratatui::buffer::Buffer, row: usize, from: usize, text: &str) -> Color {
    let line: String = drawn_lines(buffer)[row].chars().skip(from).collect();
    let column = from + line[..line.find(text).unwrap()].chars().count();
    buffer[(column as u16, row as u16)].fg
}

/// Eight tracks, 01 to 03 in group 1 and 05 in group 2.
fn grouped() -> (App, Realtime) {
    let (mut app, realtime) = app();
    command(&mut app, "track add 6");
    command(&mut app, "group 1 1-3");
    command(&mut app, "group 2 5");
    (app, realtime)
}

/// Where the phrase view's overview starts on its title line: past the grid's heading.
fn overview_start(title: &str) -> usize {
    let heading = title.find("Phrase").unwrap();
    title[..heading].chars().count() + "Phrase 00 1/1".len()
}

#[test]
fn a_group_and_its_tracks_are_named_in_its_colour_where_tracks_are_listed() {
    let (mut app, _realtime) = grouped();
    let (blue, peach) = (vt_tui::theme::group(1), vt_tui::theme::group(2));
    // The song view's columns.
    press(&mut app, " 1");
    let song = drawn(&app, vt_tui::views::song::draw);
    assert_eq!(color_from(&song, 2, 0, "02"), blue);
    assert_eq!(color_from(&song, 2, 0, "05"), peach);
    assert_eq!(color_from(&song, 2, 0, "G1"), blue);
    assert_ne!(color_from(&song, 2, 0, "04"), blue);
    // The phrase view's overview, beside the grid.
    press(&mut app, " 3");
    let phrase = drawn(&app, vt_tui::views::phrase::draw);
    let title = drawn_lines(&phrase)[0].clone();
    let from = overview_start(&title);
    assert_eq!(color_from(&phrase, 0, from, "02"), blue);
    assert_eq!(color_from(&phrase, 0, from, "05"), peach);
    assert_ne!(color_from(&phrase, 0, from, "04"), blue);
    // The titles of a track in a group carry its mark.
    assert!(title.contains("Track 01 G1 "), "{title}");
    let chain = drawn_lines(&drawn(&app, vt_tui::views::chain::draw));
    assert!(
        chain[0].starts_with("Track 01 G1  Chain 00"),
        "{}",
        chain[0]
    );
    let page = drawn_lines(&drawn(&app, vt_tui::views::track::draw));
    assert!(page[0].contains("Track 01 G1 "), "{}", page[0]);
    // The track picker.
    press(&mut app, " fb");
    let items = app.picker.as_ref().unwrap().items();
    assert_eq!(items[1].style.fg, Some(blue));
    assert_eq!(items[3].style.fg, None);
    assert_eq!(items[4].style.fg, Some(peach));
}

#[test]
fn z_a_folds_a_group_into_its_column_in_the_song_view_and_the_cursor_steps_over_it() {
    use vt_tui::glyphs::{FOLDED, ICON_SPACE};
    let (mut app, _realtime) = grouped();
    press(&mut app, " 1");
    let heading = |app: &App| song_text(app)[2].clone();
    // The empty groups are folded away, the others open.
    assert!(heading(&app).contains("08  G1  G2  A"), "{}", heading(&app));
    // On a track in a group, `z a` folds the group, and the cursor stands on it.
    press(&mut app, "llll");
    assert_eq!(app.song_column, 3 + 1);
    press(&mut app, "za");
    let g1 = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    assert_eq!(app.song_column, 3 + g1);
    assert!(app.status.contains("G1 folded: 01-03"), "{}", app.status);
    let text = heading(&app);
    assert!(!text.contains("01") && !text.contains("03"), "{text}");
    assert!(
        text.contains(&format!("08  G1{FOLDED}{ICON_SPACE}G2")),
        "{text}"
    );
    // The cursor steps over the folded tracks and the empty groups.
    press(&mut app, "0lll");
    assert_eq!(app.song_column, 3 + 3, "on 04");
    press(&mut app, "h");
    assert_eq!(app.song_column, 2, "on Start");
    press(&mut app, "$");
    assert_eq!(app.song_column, 3 + app.song().tracks.len() - 1);
    // `z a` on the group opens it again.
    press(&mut app, "hhhh");
    assert_eq!(app.song_column, 3 + g1);
    press(&mut app, "za");
    assert!(heading(&app).contains("01  02  03"), "{}", heading(&app));
    // `zR` opens every group, the empty ones too, and `zM` folds them all.
    press(&mut app, "zR");
    assert!(
        heading(&app).contains("G1  G2  G3  G4  A"),
        "{}",
        heading(&app)
    );
    press(&mut app, "lll");
    let g4 = g1 + 3;
    assert_eq!(app.song_column, 3 + g4);
    press(&mut app, "zM");
    assert_eq!(app.song_column, 3 + g4 + 1, "off the empty group, onto A");
    let text = heading(&app);
    assert!(text.contains(&format!("08  G1{FOLDED}")), "{text}");
    assert!(!text.contains("G3") && !text.contains("05"), "{text}");
    // `z a` on a track in no group says so.
    press(&mut app, "0lll");
    press(&mut app, "za");
    assert!(app.status.contains("isn't in a group"), "{}", app.status);
}

#[test]
fn the_mixer_folds_a_groups_strips_into_it() {
    use vt_tui::glyphs::{FOLDED, GROUP, ICON_SPACE};
    let (mut app, _realtime) = grouped();
    press(&mut app, " 6");
    let names = |app: &App| mixer_text_at(app, 160)[2].clone();
    let g1 = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    // Folding from a track in it puts the cursor on its strip.
    press(&mut app, "lza");
    assert_eq!(app.cursor.track, g1);
    let text = names(&app);
    assert!(!text.contains("01 ") && !text.contains("03 "), "{text}");
    assert!(
        text.contains(&format!("{GROUP}{ICON_SPACE}G1{FOLDED}{ICON_SPACE}")),
        "{text}"
    );
    assert!(text.trim_start().starts_with("04"), "{text}");
    // `h`, `l`, `W`, `B`, `0` and `$` step over folded tracks.
    press(&mut app, "0");
    assert_eq!(app.cursor.track, 3);
    press(&mut app, "h");
    assert_eq!(app.cursor.track, 3);
    press(&mut app, "B");
    assert_eq!(app.cursor.track, 3);
    press(&mut app, "W");
    assert_eq!(app.cursor.track, 4);
    press(&mut app, "$");
    assert_eq!(app.cursor.track, app.song().tracks.len() - 1);
    press(&mut app, "0l");
    assert_eq!(app.cursor.track, 4);
    // A track put in a folded group goes into it, and the cursor stands on the group.
    command(&mut app, "group 1");
    assert_eq!(app.cursor.track, g1);
    assert!(!names(&app).contains("05 "), "{}", names(&app));
    // The track picker opens the fold of a track it goes to.
    press(&mut app, " fb02");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.cursor.track, 1);
    assert!(names(&app).contains("02 "), "{}", names(&app));
}

#[test]
fn the_phrase_views_overview_shows_a_folded_group_as_a_column_of_its_own() {
    use vt_tui::glyphs::{FOLDED, ICON_SPACE};
    let (mut app, _realtime) = grouped();
    press(&mut app, " 6lza0");
    assert_eq!(app.cursor.track, 3);
    press(&mut app, " 3");
    let title = drawn_lines(&drawn(&app, vt_tui::views::phrase::draw))[0].clone();
    let overview: String = title.chars().skip(overview_start(&title)).collect();
    assert!(
        overview.contains(&format!(" G1{FOLDED}{ICON_SPACE}04 ")),
        "{overview}"
    );
    assert!(!overview.contains("01"), "{overview}");
    // `W` steps over the folded tracks in every view.
    press(&mut app, "BW");
    assert_eq!(app.cursor.track, 4);
}

#[test]
fn folds_are_not_saved_and_a_project_opens_with_its_groups_open() {
    let (mut app, _realtime) = grouped();
    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    press(&mut app, " 6zM");
    assert!(app.folds.hides(app.song(), 0));
    assert!(!app.dirty, "a fold isn't a change to the song");
    command(&mut app, &format!("e {}", path.display()));
    assert!(!app.folds.hides(app.song(), 0));
    let g3 = app.song().fixed_track(TrackKind::Group(3)).unwrap();
    assert!(app.folds.hides(app.song(), g3), "an empty group still is");
    std::fs::remove_dir_all(&path).unwrap();
}

#[test]
fn the_trim_sits_above_the_level_and_reads_in_db() {
    use vt_tui::track_page::Control;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = on_track_page();
    let trim = |app: &App| app.song().tracks[0].trim;
    press(&mut app, "gg$");
    assert_eq!(app.track_control(), Control::Trim);
    assert_eq!(value_shown(&app).as_deref(), Some("+0.0 dB"));
    let depth = app.history.undo_depth();
    press(&mut app, "=");
    assert_eq!(trim(&app), 0x81);
    assert_eq!(value_shown(&app).as_deref(), Some("+0.25 dB"));
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "+");
    assert_eq!(trim(&app), 0x91, "a large step is 4 dB");
    // Unlike a level, it runs to FF.
    press(&mut app, "iFF");
    assert_eq!(trim(&app), 0xFF);
    assert_eq!(value_shown(&app).as_deref(), Some("+31.75 dB"));
    press(&mut app, "i00");
    assert_eq!(value_shown(&app).as_deref(), Some("-inf dB"));
    press(&mut app, "x");
    assert_eq!(trim(&app), 0x80, "back to 0 dB");
    press(&mut app, "u");
    assert_eq!(trim(&app), 0x00);
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::Level);
    assert_eq!(app.song().tracks[0].level, 0xE0, "the fader is left alone");
}

#[test]
fn the_mixer_has_a_trim_above_each_fader_and_opens_on_the_fader() {
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, " 6");
    assert_eq!(column_name(&app), "Level");
    press(&mut app, "k");
    assert_eq!(column_name(&app), "Trim");
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].trim, 0x81);
    assert_eq!(value_shown(&app).as_deref(), Some("+0.25 dB"));
    let text = mixer_text(&app);
    let trim = text
        .iter()
        .position(|line| line.starts_with("Trim"))
        .unwrap();
    assert!(text[trim + 1].starts_with("Level"), "{text:?}");
    assert!(text[trim].contains("81") && text[trim].contains("80"));
    // Every strip has one, the groups', the returns' and the master's too.
    press(&mut app, "$");
    assert_eq!(app.cursor.track, 4 + MAX_GROUPS, "the master");
    assert_eq!(column_name(&app), "Trim");
}

#[test]
fn the_sound_page_with_its_trim_fits_at_163_by_42() {
    let (mut app, _realtime) = app();
    choose_machine(&mut app, "acid");
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::Acid),
        "a synth with eight settings, the longest page"
    );
    app.filter_image = Some(0x01_02_03);
    // While the keyboard plays, the page is in a box, which takes a row above and below.
    app.mode = Mode::Keys;
    let backend = ratatui::backend::TestBackend::new(163, 42);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, &mut app, None, Instant::now()))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let lines: Vec<String> = (0..42)
        .map(|y| (0..163).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let row = |text: &str| {
        lines
            .iter()
            .position(|line| line.contains(text))
            .unwrap_or_else(|| panic!("{text} in {lines:#?}"))
    };
    let machine = row("Machine");
    assert!(lines[machine].contains("Trim"), "{}", lines[machine]);
    assert!(
        lines[machine + 1].contains("Level"),
        "{}",
        lines[machine + 1]
    );
    // The filter's curve, in its box of three rows and two edges, still fits under the last row,
    // above the status line.
    let high_cut = row("High cut");
    assert!(lines[high_cut + 1].contains('┌'), "{}", lines[high_cut + 1]);
    let bottom = high_cut + 5;
    assert!(bottom < 40, "{lines:#?}");
    assert!(lines[bottom].contains('└'), "{}", lines[bottom]);
}
