use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::{Engine, Transport};
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::Song;
use vt_model::step::Note;
use vt_tui::app::{App, Field};
use vt_tui::input::keys::{Key, parse_sequence};
use vt_tui::input::mode::{Mode, View};

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
    app.song().phrases[&id]
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

#[test]
fn insert_mode_writes_notes_and_moves_down() {
    let (mut app, _rt) = app();
    press(&mut app, "i");
    assert_eq!(app.mode, Mode::Insert);
    press(&mut app, "ak1");
    let written = notes(&app);
    assert_eq!(written[0], Note::On(60));
    assert_eq!(written[1], Note::On(72));
    assert_eq!(written[2], Note::Off);
    assert_eq!(app.cursor.step, 3);
    special(&mut app, KeyCode::Backspace);
    assert_eq!(app.cursor.step, 2);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn octave_keys_change_entered_notes() {
    let (mut app, _rt) = app();
    press(&mut app, "izaxxa");
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
            code: KeyCode::Char('r'),
            ctrl: true,
            alt: false,
            shift: false,
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
fn enter_plays_from_the_cursor_and_stops() {
    let (mut app, mut rt) = app();
    press(&mut app, "jjj");
    let start = Instant::now();
    app.key(Key::new(KeyCode::Enter), start);
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Playing);
    assert_eq!(app.playhead, 3);
    app.key(
        Key::new(KeyCode::Enter),
        start + Duration::from_millis(1000),
    );
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Stopped);
}

#[test]
fn a_held_transport_key_is_ignored_while_it_repeats() {
    let (mut app, mut rt) = app();
    let start = Instant::now();
    for gap in 0..20 {
        app.key(
            Key::new(KeyCode::Enter),
            start + Duration::from_millis(gap * 40),
        );
    }
    block(&mut rt, &mut app);
    assert_eq!(
        app.transport,
        Transport::Playing,
        "only the first press counts"
    );
}

#[test]
fn repeats_after_a_leader_transport_key_dont_reach_normal_keys() {
    let (mut app, mut rt) = app();
    let start = Instant::now();
    app.key(Key::char(' '), start);
    for gap in 0..10 {
        app.key(Key::char('x'), start + Duration::from_millis(gap * 40));
    }
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Playing);
    assert!(app.pending().is_empty());
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
    assert_eq!(notes(&app).len(), 4);
    assert_eq!(app.cursor.step, 3);
    command(&mut app, "inst 1A");
    assert_eq!(app.instrument, 0x1A);
}

#[test]
fn saving_needs_a_folder_then_round_trips() {
    let (mut app, _rt) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "w");
    assert!(app.dirty);
    assert!(app.status.contains(":w path"));
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
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    app.tick();
    press(&mut app, "k");
    let mut out = vec![0.0; 512];
    rt.process(&mut out);
    app.key(Key::new(KeyCode::Enter), Instant::now());
    rt.process(&mut out);
    assert!(out.iter().any(|s| *s != 0.0), "the note should sound");
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
    assert_eq!(app.phrase_id(), Some(1));
    press(&mut app, "W");
    assert_eq!(app.cursor.track, 1, "the last track stops the cursor");
    press(&mut app, "BB");
    assert_eq!(app.cursor.track, 0);
}

#[test]
fn added_tracks_save_load_and_play() {
    let (mut app, mut rt) = app();
    command(&mut app, "track add 3");
    assert_eq!(app.song().tracks.len(), 5);
    assert_eq!(app.levels.len(), 5);
    press(&mut app, "WWWW");
    assert_eq!(app.cursor.track, 4);
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    let (mut other, _rt2) = self::app();
    command(&mut other, &format!("e {}", path.display()));
    assert_eq!(other.song().tracks.len(), 5);
    app.tick();
    let mut out = vec![0.0; 512];
    rt.process(&mut out);
    assert_eq!(rt.engine().current().song.tracks.len(), 5);
}

#[test]
fn only_the_last_track_can_be_removed_and_one_stays() {
    let (mut app, _rt) = app();
    press(&mut app, "W");
    command(&mut app, "track del");
    assert_eq!(app.song().tracks.len(), 1);
    assert_eq!(app.cursor.track, 0);
    assert_eq!(app.phrase_id(), Some(0));
    command(&mut app, "track del");
    assert_eq!(app.song().tracks.len(), 1);
    assert!(app.status.contains("at least one"));
    press(&mut app, "uj");
    assert_eq!(app.song().tracks.len(), 2);
    assert_eq!(app.levels.len(), 2);
}

#[test]
fn track_count_is_limited_to_the_constant() {
    let (mut app, _rt) = app();
    command(&mut app, "track add 62");
    assert_eq!(app.song().tracks.len(), 64);
    command(&mut app, "track add");
    assert_eq!(app.song().tracks.len(), 64);
    assert!(app.status.contains("at most"));
}

#[test]
fn mute_isnt_in_undo_history_but_is_saved() {
    let (mut app, _rt) = app();
    press(&mut app, " m");
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
fn swing_commands_set_the_project_and_the_cursors_track_and_undo() {
    let (mut app, _realtime) = app();
    command(&mut app, "swing 66.7");
    assert_eq!(app.song().swing, 667);
    command(&mut app, "swing 80");
    assert_eq!(app.song().swing, 667, "out of range is refused");
    assert!(app.status.contains("50 to 75"), "{}", app.status);
    command(&mut app, "tswing 60");
    assert_eq!(app.song().tracks[0].swing, Some(600));
    assert_eq!(app.song().tracks[1].swing, None);
    command(&mut app, "tswing off");
    assert_eq!(app.song().tracks[0].swing, None);
    press(&mut app, "uu");
    assert_eq!(app.song().swing, 667);
    assert_eq!(app.song().tracks[0].swing, None);
    press(&mut app, "u");
    assert_eq!(app.song().swing, 500);
}

#[test]
fn the_instrument_view_sets_volume_attack_and_release() {
    let (mut app, _realtime) = app();
    press(&mut app, " 5");
    assert_eq!(app.view, View::Instrument);
    // Down from the sample row is volume. Two hex digits commit it.
    press(&mut app, "jia0");
    assert_eq!(app.song().instruments[&0].volume, 0xA0);
    assert_eq!(app.mode, Mode::Normal);
    // Attack is typed in milliseconds and needs Enter.
    press(&mut app, "ji25");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().instruments[&0].attack_ms, 25);
    // Each adjustment is one undo step.
    press(&mut app, "j+");
    assert_eq!(app.song().instruments[&0].release_ms, 55);
    press(&mut app, "u");
    assert_eq!(app.song().instruments[&0].release_ms, 50);
    // Step sizes shrink for short times, and values stop at the ends.
    press(&mut app, "k");
    press(&mut app, "3-");
    assert_eq!(app.song().instruments[&0].attack_ms, 10);
    press(&mut app, "99-");
    assert_eq!(app.song().instruments[&0].attack_ms, 0);
}

#[test]
fn the_instrument_view_refuses_values_out_of_range_and_keeps_notes_safe() {
    let (mut app, _realtime) = app();
    press(&mut app, " 5jj");
    press(&mut app, "i99999");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().instruments[&0].attack_ms, 1);
    assert!(app.status.contains("5000"), "{}", app.status);
    // These keys mean something else in the phrase view, and must not touch a step here.
    let before = notes(&app);
    press(&mut app, "kk");
    press(&mut app, "+-");
    assert_eq!(notes(&app), before);
    assert_eq!(app.cursor.step, 0);
}

#[test]
fn moving_between_instruments_creates_one_only_when_set() {
    let (mut app, _realtime) = app();
    press(&mut app, " 5ll");
    assert_eq!(app.instrument, 2);
    assert!(!app.song().instruments.contains_key(&2));
    press(&mut app, "j-");
    assert_eq!(app.song().instruments[&2].volume, 0xFE);
    press(&mut app, "h");
    assert_eq!(app.instrument, 1);
    press(&mut app, " 3");
    assert_eq!(app.view, View::Phrase);
    assert_eq!(app.mode, Mode::Normal);
}

fn wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    writer.write_sample(1000_i16).unwrap();
    writer.finalize().unwrap();
}

#[test]
fn a_folder_opens_the_sample_browser_and_choosing_loads_the_sample() {
    let (mut app, _realtime) = app();
    let folder = std::env::temp_dir().join(format!("vt-browse-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("kit/.hidden")).unwrap();
    wav(&folder.join("kit/kick.wav"));
    wav(&folder.join("kit/snare.WAV"));
    wav(&folder.join("kit/.hidden/secret.wav"));
    std::fs::write(folder.join("kit/notes.txt"), "not audio").unwrap();

    command(&mut app, &format!("sample {}", folder.display()));
    assert_eq!(app.mode, Mode::Picker);
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .shown()
        .map(|item| item.label.clone())
        .collect();
    assert_eq!(labels, ["kit/kick.wav", "kit/snare.WAV"]);
    press(&mut app, "sn");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    let chosen = app.song().instruments[&0].sample.clone().unwrap();
    assert!(chosen.ends_with("snare.WAV"), "{chosen}");
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn the_view_picker_flips_back_and_escape_changes_nothing() {
    let (mut app, _realtime) = app();
    press(&mut app, " 5");
    assert_eq!(app.view, View::Instrument);
    press(&mut app, "  ");
    assert_eq!(app.mode, Mode::Picker);
    // Keys that would edit in other modes only search here.
    press(&mut app, "x");
    assert_eq!(app.view, View::Instrument);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.view, View::Instrument);
    assert!(app.picker.is_none());
    // The list starts on the view before this one.
    press(&mut app, " ff");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::Phrase);
    press(&mut app, "  ");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::Instrument);
}

#[test]
fn the_view_picker_searches_and_moves_with_ctrl_keys() {
    let (mut app, _realtime) = app();
    press(&mut app, "  ins");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::Instrument);
    press(&mut app, "  zz");
    special(&mut app, KeyCode::Backspace);
    special(&mut app, KeyCode::Backspace);
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 3);
    app.key(parse_sequence("C-p").unwrap()[0], Instant::now());
    assert_eq!(app.picker.as_ref().unwrap().selected, 0);
    app.key(parse_sequence("C-n").unwrap()[0], Instant::now());
    assert_eq!(app.picker.as_ref().unwrap().selected, 1);
    special(&mut app, KeyCode::Esc);
}

#[test]
fn a_plain_write_keeps_live_mutes_and_solos() {
    let (mut app, _realtime) = app();
    let folder = std::env::temp_dir().join(format!("vt-mute-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    command(&mut app, &format!("w {}", folder.display()));
    press(&mut app, " m");
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
    assert_eq!(app.phrase_id(), Some(2));
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.song().phrases[&0].steps()[0].note, Note::Empty);
    assert_eq!(app.song().phrases[&2].steps()[0].note, Note::On(60));
    assert_eq!(app.chain_entries()[0].phrase, 0);
    // Ctrl-O returns to where the jump started, and Tab goes forward again.
    let back = parse_sequence("C-o").unwrap()[0];
    app.key(back, Instant::now());
    assert_eq!(app.view, View::Chain);
    assert_eq!(app.cursor.slot, 1);
    special(&mut app, KeyCode::Tab);
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
    assert_eq!(app.phrase_id(), Some(2));
    assert_eq!(app.chain_entries().len(), 2);
    // Leaving the chain view never leaves the cursor on the empty row.
    press(&mut app, " 2G 3");
    assert_eq!(app.cursor.slot, 1);
    assert_eq!(app.phrase_id(), Some(2));
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
    assert!(app.song().phrases.contains_key(&0x1F));
    // Transpose by keys and by typing, and out of range is refused.
    press(&mut app, "l5+");
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
        .shown()
        .map(|item| item.label.clone())
        .collect();
    assert_eq!(uses, ["Track 01 slot 00", "Track 01 slot 01"]);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.cursor.slot, 0);
    assert_eq!(app.view, View::Phrase);
}

#[test]
fn gd_on_an_instrument_field_opens_or_creates_it() {
    let (mut app, _realtime) = app();
    // Note, volume, then instrument.
    press(&mut app, "llgd");
    assert_eq!(app.view, View::Instrument);
    assert_eq!(app.instrument, 1, "00 exists, so a new one is 01");
    assert!(app.song().instruments.contains_key(&1));
    let back = parse_sequence("C-o").unwrap()[0];
    app.key(back, Instant::now());
    assert_eq!(app.view, View::Phrase);
    assert_eq!(app.song().phrases[&0].steps()[0].instrument, Some(1));
    assert_eq!(
        app.instrument, 0,
        "jumping back restores the old instrument"
    );
    press(&mut app, "gd");
    assert_eq!(app.instrument, 1, "this time it already has one");
}

#[test]
fn a_shared_chain_warns_on_clone_and_gc_big_separates_it() {
    let (mut app, _realtime) = app();
    press(&mut app, "W");
    command(&mut app, "chain 0");
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
            saved.phrases[&0].steps()[0].note,
            Note::Empty,
            "song.ron is untouched"
        );

        // No change since then means no new write.
        std::fs::remove_file(path.join(AUTOSAVE)).unwrap();
        app.autosave_if_due(later(start, 190));
        assert!(!path.join(AUTOSAVE).exists());

        // A real save clears it.
        press(&mut app, "ia");
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
        std::sync::Arc::make_mut(newer.phrases.get_mut(&0).unwrap())
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
        assert_eq!(app.song().phrases[&0].steps()[0].note, Note::Empty);
        // Other keys don't answer.
        press(&mut app, "q");
        special(&mut app, KeyCode::Esc);
        assert_eq!(app.mode, Mode::Restore);
        press(&mut app, "y");
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.song().phrases[&0].steps()[0].note, Note::On(60));
        assert!(app.dirty);
        command(&mut app, "w");
        let saved = project::load_song(&path).unwrap();
        assert_eq!(saved.phrases[&0].steps()[0].note, Note::On(60));
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
        assert_eq!(app.song().phrases[&0].steps()[0].note, Note::Empty);
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
    let selected = &buffer[(6, 2)];
    assert_eq!(selected.bg, vt_tui::theme::SURFACE1);
    assert_eq!(selected.fg, vt_tui::theme::TEXT);
    assert_ne!(selected.symbol(), " ");
    let cursor = &buffer[(5, 3)];
    assert_eq!(cursor.bg, vt_tui::theme::ROSEWATER);
    assert_eq!(cursor.fg, vt_tui::theme::BASE);
}

#[test]
fn the_status_line_names_the_column() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    assert_eq!(column_name(&app), "Note");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Velocity");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Instrument");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "FX1 command");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "FX1 value");
    press(&mut app, " 2");
    assert_eq!(column_name(&app), "Phrase");
    press(&mut app, " 5");
    assert_eq!(column_name(&app), "Sample");
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
    let last = app.guide.unwrap().lines().len() - 10;
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
    assert_eq!(app.cursor.step, 2);
    special(&mut app, KeyCode::Esc);

    press(&mut app, "gg");
    command(&mut app, "set step=4");
    press(&mut app, "iaaaa");
    special(&mut app, KeyCode::Esc);
    let written: Vec<usize> = notes(&app)
        .iter()
        .enumerate()
        .filter(|(_, note)| matches!(note, Note::On(_)))
        .map(|(step, _)| step)
        .collect();
    assert_eq!(written, [0, 4, 8, 12]);

    command(&mut app, "set step=0");
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
    let velocities: Vec<_> = app.song().phrases[&0]
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
    command(&mut app, "set step=0");
    press(&mut app, "ia");
    assert_eq!(notes(&app)[0], Note::On(60));
    press(&mut app, "w");
    assert_eq!(notes(&app)[0], Note::On(61));
    press(&mut app, "k");
    assert_eq!(notes(&app)[0], Note::On(72));
    press(&mut app, "x;");
    assert_eq!(notes(&app)[0], Note::On(88));
    press(&mut app, "zz");
    assert_eq!(app.octave, 3);

    let velocity = |app: &App| app.song().phrases[&0].steps()[0].velocity;
    press(&mut app, "a");
    assert_eq!(velocity(&app), Some(0x64), "filled in from the default");
    press(&mut app, "c");
    assert_eq!(app.note_velocity, 0x54, "only the top digit moves");
    press(&mut app, "a");
    assert_eq!(velocity(&app), Some(0x54));
    assert_eq!(notes(&app)[0], Note::On(48));

    // The whole note is one undo step.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "u");
    assert_eq!(velocity(&app), Some(0x64));

    press(&mut app, "i");
    press(&mut app, "v");
    assert_eq!(app.note_velocity, 0x64, "back on the default");
    press(&mut app, "v");
    assert_eq!(app.note_velocity, 0x74);
    press(&mut app, "v");
    assert_eq!(app.note_velocity, 0x7F);
    press(&mut app, "v");
    assert_eq!(app.note_velocity, 0x7F);
    press(&mut app, "c");
    assert_eq!(app.note_velocity, 0x74, "steps back on from the top");
    press(&mut app, "c");
    assert_eq!(app.note_velocity, 0x64);
    for _ in 0..12 {
        press(&mut app, "c");
    }
    assert_eq!(app.note_velocity, 0x04);
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
        // The neighbour column after the focused track's text, on a step two rows below the title.
        let buffer = terminal.backend().buffer();
        buffer[(34, row + 2)].fg
    };
    app.playhead = 2;
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
    assert_eq!(app.song().tracks[0].level, 0xFF);
    press(&mut app, "W");
    command(&mut app, "tvol C0");
    assert_eq!(app.song().tracks[0].level, 0xFF);
    assert_eq!(app.song().tracks[1].level, 0xC0);
    assert!(app.dirty);
    press(&mut app, "u");
    assert_eq!(app.song().tracks[1].level, 0xFF);
    command(&mut app, "tvol loud");
    assert_eq!(app.song().tracks[1].level, 0xFF);
    assert!(app.status.contains(":tvol"));
}

#[test]
fn a_note_then_right_lands_on_the_next_beat() {
    let (mut app, _realtime) = app();
    press(&mut app, "i");
    for _ in 0..4 {
        press(&mut app, "a");
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
    press(&mut app, "jji");
    for _ in 0..4 {
        press(&mut app, "a");
        special(&mut app, KeyCode::Right);
    }
    assert_eq!(written_steps(&app), [2, 6, 10, 14]);
}

#[test]
fn right_counts_from_the_note_with_any_skip_or_step() {
    let (mut app, _realtime) = app();
    command(&mut app, "set skip=8");
    press(&mut app, "ia");
    special(&mut app, KeyCode::Right);
    press(&mut app, "a");
    assert_eq!(written_steps(&app), [0, 8]);

    let (mut app, _realtime) = self::app();
    command(&mut app, "set step=2");
    press(&mut app, "ia");
    assert_eq!(app.cursor.step, 2);
    special(&mut app, KeyCode::Right);
    assert_eq!(app.cursor.step, 4);
}

#[test]
fn left_counts_from_the_note_too_and_any_other_move_forgets_it() {
    let (mut app, _realtime) = app();
    press(&mut app, "8Gi");
    press(&mut app, "a");
    assert_eq!(app.cursor.step, 8);
    special(&mut app, KeyCode::Left);
    assert_eq!(app.cursor.step, 3);

    press(&mut app, "a");
    assert_eq!(app.cursor.step, 4);
    special(&mut app, KeyCode::Up);
    special(&mut app, KeyCode::Down);
    special(&mut app, KeyCode::Right);
    assert_eq!(app.cursor.step, 8);

    press(&mut app, "a");
    special(&mut app, KeyCode::Backspace);
    assert_eq!(app.cursor.step, 8);
    special(&mut app, KeyCode::Right);
    assert_eq!(app.cursor.step, 12);
}

#[test]
fn shift_arrows_snap_to_the_grid() {
    let (mut app, _realtime) = app();
    press(&mut app, "jji");
    press(&mut app, "a");
    assert_eq!(app.cursor.step, 3);
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
    assert_eq!(normal[(5, 2)].bg, vt_tui::theme::ROSEWATER);
    assert_eq!(normal[(4, 1)].symbol(), " ");
    assert_eq!(normal[(4, 2)].symbol(), " ");

    press(&mut app, "i");
    let insert = screen(&app);
    assert_eq!(insert[(5, 2)].bg, vt_tui::theme::GREEN);
    assert_eq!(insert[(5, 2)].fg, vt_tui::theme::BASE);
    assert_eq!(insert[(4, 1)].symbol(), "┌");
    assert_eq!(insert[(4, 2)].symbol(), "│");
    assert_eq!(insert[(4, 18)].symbol(), "└");
    assert_eq!(insert[(4, 2)].fg, vt_tui::theme::GREEN);

    // Take away the box, and the steps are where they were.
    assert_eq!(
        row(&normal, 2).replace(' ', ""),
        row(&insert, 2).replace(['│', ' '], "")
    );
    let step_text_start = |b: &ratatui::buffer::Buffer| b[(5, 2)].symbol().to_owned();
    assert_eq!(step_text_start(&normal), step_text_start(&insert));

    special(&mut app, KeyCode::Esc);
    let back = screen(&app);
    assert_eq!(back[(4, 1)].symbol(), " ");
    assert_eq!(back[(5, 2)].bg, vt_tui::theme::ROSEWATER);
}

#[test]
fn enter_plays_and_stops_from_insert_and_keeps_writing() {
    let (mut app, mut rt) = app();
    press(&mut app, "i");
    let start = Instant::now();
    app.key(Key::new(KeyCode::Enter), start);
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Playing);
    assert_eq!(app.mode, Mode::Insert);
    press(&mut app, "a");
    assert_eq!(notes(&app)[0], Note::On(60));
    app.key(
        Key::new(KeyCode::Enter),
        start + Duration::from_millis(1000),
    );
    block(&mut rt, &mut app);
    assert_eq!(app.transport, Transport::Stopped);
    assert_eq!(app.mode, Mode::Insert);
}

#[test]
fn enter_still_finishes_one_typed_digit_of_an_instrument_in_insert() {
    let (mut app, mut rt) = app();
    press(&mut app, "lli5");
    special(&mut app, KeyCode::Enter);
    block(&mut rt, &mut app);
    assert_eq!(app.song().phrases[&0].steps()[0].instrument, Some(5));
    assert_eq!(app.transport, Transport::Stopped);
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

fn sample_of(app: &App, id: u32) -> Option<String> {
    app.song().instruments.get(&id)?.sample.clone()
}

#[test]
fn a_kit_fills_slots_in_file_order_as_one_undo_step() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    command(&mut app, "inst 5");
    command(&mut app, &format!("kit {}", kit.display()));
    assert!(sample_of(&app, 0).unwrap().ends_with("snare.wav"));
    assert!(sample_of(&app, 1).unwrap().ends_with("kick.wav"));
    assert!(sample_of(&app, 2).unwrap().ends_with("hat.wav"));
    assert_eq!(sample_of(&app, 3), None);
    assert_eq!(app.instrument, 0);
    assert_eq!(app.status, "Kit 909: 00 01 02");
    assert!(app.samples.id_of(&sample_of(&app, 1).unwrap()).is_some());

    press(&mut app, "u");
    assert_eq!(sample_of(&app, 0), None);
    assert_eq!(sample_of(&app, 1), None);
    assert_eq!(sample_of(&app, 2), None);
}

#[test]
fn a_kit_replaces_samples_but_keeps_settings_and_later_slots() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    let other = folder();
    std::fs::create_dir_all(&other).unwrap();
    let later = other.join("later.wav");
    wav(&later);
    command(&mut app, "inst 3");
    command(&mut app, &format!("sample {}", later.display()));
    command(&mut app, "inst 0");
    press(&mut app, " 5jia0");
    assert_eq!(app.song().instruments[&0].volume, 0xA0);
    command(&mut app, &format!("kit {}", kit.display()));
    assert!(sample_of(&app, 3).unwrap().ends_with("later.wav"));
    assert_eq!(app.song().instruments[&0].volume, 0xA0);
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
    command(&mut app, &format!("kit {}", kit.display()));
    assert_eq!(sample_of(&app, 0), None);
    assert!(!app.dirty);
    assert!(app.status.contains("broken"), "{}", app.status);

    std::fs::write(&kit, r#"(sounds: [(path: "drums/gone.wav")])"#).unwrap();
    command(&mut app, &format!("kit {}", kit.display()));
    assert_eq!(sample_of(&app, 0), None);
    assert!(app.status.contains("gone"), "{}", app.status);

    std::fs::write(&kit, "not a kit").unwrap();
    command(&mut app, &format!("kit {}", kit.display()));
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
    command(&mut app, &format!("kit {}", kit.display()));
    assert!(std::path::Path::new(&sample_of(&app, 0).unwrap()).is_absolute());
    assert!(!project.join("samples").exists());

    command(&mut app, "w");
    let reopened = vt_model::project::load_song(&project).unwrap();
    assert_eq!(
        reopened.instruments[&0].sample.as_deref(),
        Some(snare.canonicalize().unwrap().to_str().unwrap())
    );

    let bundle = folder();
    command(&mut app, &format!("w {}", bundle.display()));
    assert_eq!(sample_of(&app, 0).unwrap(), "samples/snare.wav");
    assert!(bundle.join("samples/hat.wav").exists());
}

#[test]
fn a_missing_kit_or_none_at_all_says_so() {
    let (mut app, _realtime) = app();
    command(&mut app, "kit nothing-here");
    assert!(app.status.contains("nothing-here"), "{}", app.status);
    command(&mut app, "kit");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.contains("samples/kits"), "{}", app.status);
}

#[test]
fn only_ron_files_count_as_kits() {
    use vt_tui::picking::kits_in;
    let kit = kit_file();
    let root = kit.parent().unwrap();
    std::fs::write(root.join("stray.wav"), "").unwrap();
    std::fs::write(root.join(".hidden.ron"), "").unwrap();
    assert_eq!(kits_in(root), vec![kit.clone()]);
}

fn step_instrument(app: &App, step: usize) -> Option<u32> {
    app.song().phrases[&0].steps()[step].instrument
}

#[test]
fn editing_the_instrument_in_the_phrase_is_remembered_for_the_next_note() {
    let (mut app, _realtime) = app();
    command(&mut app, "inst 4");
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "kll+");
    assert_eq!(step_instrument(&app, 0), Some(5));
    assert_eq!(app.instrument, 5);
    press(&mut app, "-");
    assert_eq!(app.instrument, 4);
    press(&mut app, "3+");
    assert_eq!(step_instrument(&app, 0), Some(7));
    assert_eq!(app.instrument, 7);

    press(&mut app, "hhji");
    press(&mut app, "a");
    assert_eq!(step_instrument(&app, 1), Some(7));
}

#[test]
fn typing_an_instrument_is_remembered_too() {
    let (mut app, _realtime) = app();
    press(&mut app, "lli1a");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.instrument, 0x1A);
    press(&mut app, "hhi");
    press(&mut app, "a");
    assert_eq!(step_instrument(&app, 1), Some(0x1A));
}

#[test]
fn plus_on_an_empty_instrument_cell_writes_the_remembered_one() {
    let (mut app, _realtime) = app();
    command(&mut app, "inst 9");
    press(&mut app, "ll+");
    assert_eq!(step_instrument(&app, 0), Some(9));
    assert_eq!(app.instrument, 9);
    press(&mut app, "u");
    assert_eq!(step_instrument(&app, 0), None);
    press(&mut app, "-");
    assert_eq!(step_instrument(&app, 0), Some(9));
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
    command(&mut app, "inst 0");
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
    assert_eq!(
        song.instruments[&0].sample.as_deref(),
        Some("samples/tone.wav")
    );
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
    let _ = loudest(&mut rt, 96_000);
    press(&mut app, " 5;");
    assert!(
        loudest(&mut rt, 2000) > 0.01,
        "the instrument view auditions"
    );
}

#[test]
fn p_no_longer_plays_in_the_instrument_view() {
    let (mut app, mut rt) = app();
    with_a_sound(&mut app);
    press(&mut app, " 5p");
    assert_eq!(loudest(&mut rt, 2000), 0.0);
    press(&mut app, "r");
    assert_eq!(app.mode, Mode::Normal, "r is not bound in this view");
}

#[test]
fn semicolon_while_playing_says_why_it_did_nothing() {
    let (mut app, mut rt) = app();
    special(&mut app, KeyCode::Enter);
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

fn slot(name: &str) -> u32 {
    vt_model::label::slot_of(name).unwrap()
}

#[test]
fn a_label_is_its_slot_and_a_kit_puts_sounds_there() {
    let (mut app, _realtime) = app();
    command(&mut app, &format!("kit {}", labelled_kit().display()));
    assert_eq!(app.status, "Kit 909: BD CH 00");
    assert!(sample_of(&app, 0xBD).unwrap().ends_with("kick.wav"));
    assert!(sample_of(&app, slot("CH")).unwrap().ends_with("hat.wav"));
    assert!(sample_of(&app, 0).unwrap().ends_with("snare.wav"));
    assert_eq!(app.instrument, 0xBD);
    assert!(app.song().is_labelled(0xBD));
    assert!(app.song().is_labelled(slot("CH")));
    assert!(!app.song().is_labelled(0));

    press(&mut app, "u");
    assert_eq!(sample_of(&app, 0xBD), None);
    assert!(!app.song().is_labelled(0xBD));
}

#[test]
fn a_kit_warns_only_when_it_changes_a_slots_sample() {
    let (mut app, _realtime) = app();
    let kit = labelled_kit();
    command(&mut app, &format!("kit {}", kit.display()));
    command(&mut app, &format!("kit {}", kit.display()));
    assert_eq!(app.status, "Kit 909: BD CH 00", "the same sounds again");

    std::fs::write(
        &kit,
        r#"(sounds: [
            (label: "BD", path: "drums/snare.wav"),
            (label: "CH", path: "drums/hat.wav"),
            (label: "OH", path: "drums/hat.wav"),
        ])"#,
    )
    .unwrap();
    command(&mut app, &format!("kit {}", kit.display()));
    assert_eq!(app.status, "Kit 909: BD CH OH. Replaced BD: u to undo");
    press(&mut app, "u");
    assert!(sample_of(&app, 0xBD).unwrap().ends_with("kick.wav"));
}

#[test]
fn a_kit_refuses_bad_or_repeated_labels_and_changes_nothing() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    for (text, expected) in [
        (r#"(sounds: [(label: "1A", path: "drums/kick.wav")])"#, "1A"),
        (
            r#"(sounds: [(label: "BDX", path: "drums/kick.wav")])"#,
            "BDX",
        ),
        (
            r#"(sounds: [(label: "BD", path: "drums/kick.wav"), (label: "bd", path: "drums/hat.wav")])"#,
            "twice",
        ),
    ] {
        std::fs::write(&kit, text).unwrap();
        command(&mut app, &format!("kit {}", kit.display()));
        assert!(app.status.contains(expected), "{}", app.status);
        assert!(!app.dirty);
    }
}

#[test]
fn typing_two_characters_writes_a_number_or_a_label_at_once() {
    let (mut app, _realtime) = app();
    press(&mut app, "ll");
    press(&mut app, "ibd");
    assert_eq!(step_instrument(&app, 0), Some(0xBD));
    press(&mut app, "ch");
    assert_eq!(step_instrument(&app, 1), Some(slot("CH")));
    assert_eq!(app.instrument, slot("CH"));
    press(&mut app, "04");
    assert_eq!(step_instrument(&app, 2), Some(4));
    press(&mut app, "1h");
    assert_eq!(step_instrument(&app, 3), None);
    assert_eq!(app.cursor.step, 3);
    assert!(app.status.contains("1H"), "{}", app.status);
    assert!(app.entry.text.is_empty());
}

#[test]
fn inst_takes_labels_and_notes_use_them_and_the_song_keeps_them() {
    let project = folder();
    let (mut app, _realtime) = app();
    command(&mut app, &format!("w {}", project.display()));
    command(&mut app, "inst ch");
    assert_eq!(app.instrument, slot("CH"));
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    assert_eq!(step_instrument(&app, 0), Some(slot("CH")));
    command(&mut app, "w");
    let reopened = vt_model::project::load_song(&project).unwrap();
    assert_eq!(reopened.phrases[&0].steps()[0].instrument, Some(slot("CH")));
}

#[test]
fn label_marks_a_hex_named_slot_and_off_clears_it() {
    let (mut app, _realtime) = app();
    command(&mut app, "label BD");
    assert_eq!(app.instrument, 0xBD);
    assert!(app.song().is_labelled(0xBD));
    command(&mut app, "label off");
    assert!(!app.song().is_labelled(0xBD));
    command(&mut app, "inst ch");
    command(&mut app, "label off");
    assert!(app.song().is_labelled(slot("CH")));
    assert!(app.status.contains("CH"), "{}", app.status);
}

#[test]
fn k_picks_an_instrument_for_this_step_with_labels_first() {
    let (mut app, _realtime) = app();
    command(&mut app, &format!("kit {}", labelled_kit().display()));
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Normal, "not on the Instrument column");
    assert!(app.status.contains("Instrument column"), "{}", app.status);

    press(&mut app, "llK");
    assert_eq!(app.mode, Mode::Picker);
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .shown()
        .map(|item| item.label.clone())
        .collect();
    assert_eq!(labels, ["BD  kick.wav", "CH  hat.wav", "00  snare.wav"]);
    press(&mut app, "hat");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(step_instrument(&app, 0), Some(slot("CH")));
    assert_eq!(app.instrument, slot("CH"));
    assert_eq!(app.cursor.step, 0);

    press(&mut app, "K");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "CH  hat.wav");
    special(&mut app, KeyCode::Esc);
    assert_eq!(step_instrument(&app, 0), Some(slot("CH")));
}

#[test]
fn space_i_k_chooses_the_current_instrument_from_anywhere_and_writes_nothing() {
    let (mut app, _realtime) = app();
    command(&mut app, &format!("kit {}", labelled_kit().display()));
    let before = Arc::clone(app.song());
    let dirty = app.dirty;

    // On the Note column, where plain K refuses.
    press(&mut app, " iK");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "hat");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.instrument, slot("CH"));
    assert_eq!(app.cursor.field, Field::Note, "the cursor stays put");
    assert_eq!(app.cursor.step, 0);
    assert_eq!(*app.song(), before, "nothing is written");
    assert_eq!(app.dirty, dirty);

    // The next note uses it.
    press(&mut app, "ia");
    assert_eq!(step_instrument(&app, 0), Some(slot("CH")));
    special(&mut app, KeyCode::Esc);

    // It opens on the current one, and Esc changes nothing.
    press(&mut app, " iK");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "CH  hat.wav");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.instrument, slot("CH"));
}

#[test]
fn space_i_k_works_in_the_chain_and_instrument_views() {
    for (view_keys, view) in [(" 2", View::Chain), (" 5", View::Instrument)] {
        let (mut app, _realtime) = app();
        command(&mut app, &format!("kit {}", labelled_kit().display()));
        press(&mut app, view_keys);
        assert_eq!(app.view, view);
        let scope = app.key_scope();
        let before = Arc::clone(app.song());
        press(&mut app, " iK");
        assert_eq!(app.mode, Mode::Picker, "{view:?}");
        press(&mut app, "kick");
        special(&mut app, KeyCode::Enter);
        assert_eq!(app.instrument, slot("BD"), "{view:?}");
        assert_eq!(app.view, view, "stays in this view");
        assert_eq!(app.key_scope(), scope, "the view's keys still work");
        assert_eq!(*app.song(), before);
    }
}

#[test]
fn plus_and_minus_walk_named_slots_through_those_that_exist() {
    let (mut app, _realtime) = app();
    command(&mut app, &format!("kit {}", labelled_kit().display()));
    command(&mut app, "inst oh");
    command(
        &mut app,
        &format!("sample {}", {
            let dir = folder();
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("open.wav");
            wav(&path);
            path.display().to_string()
        }),
    );
    command(&mut app, "inst ch");
    press(&mut app, "ll+");
    assert_eq!(step_instrument(&app, 0), Some(slot("CH")));
    press(&mut app, "+");
    assert_eq!(step_instrument(&app, 0), Some(slot("OH")));
    press(&mut app, "+");
    assert_eq!(step_instrument(&app, 0), Some(slot("OH")));
    press(&mut app, "-");
    assert_eq!(step_instrument(&app, 0), Some(slot("CH")));
    press(&mut app, "-");
    assert_eq!(step_instrument(&app, 0), Some(0xBD));
    assert_eq!(app.instrument, 0xBD);
}

#[test]
fn labels_read_differently_in_the_phrase_even_when_they_are_hex() {
    let (mut app, _realtime) = app();
    command(&mut app, &format!("kit {}", labelled_kit().display()));
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "inst ch");
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "inst bc");
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let buffer = screen(&app);
    let cell = |x: u16, y: u16| (buffer[(x, y)].symbol().to_owned(), buffer[(x, y)].fg);
    let peach = vt_tui::theme::PEACH;
    // Steps start at row 2, and the instrument is 7 characters into a step's text.
    assert_eq!(cell(12, 2), ("B".to_owned(), peach));
    assert_eq!(cell(13, 2), ("D".to_owned(), peach));
    assert_eq!(cell(12, 3), ("C".to_owned(), peach));
    assert_eq!(cell(13, 3), ("H".to_owned(), peach));
    // BC is the same kind of name, but nothing labelled that slot.
    assert_eq!(buffer[(12, 4)].symbol(), "B");
    assert_ne!(buffer[(12, 4)].fg, peach);
}

#[test]
fn zero_and_dollar_pick_the_chain_columns_and_stay_out_of_the_instrument_view() {
    use vt_tui::tree::ChainColumn;
    let (mut app, _realtime) = app();
    press(&mut app, " 2$");
    assert_eq!(app.chain_column, ChainColumn::Transpose);
    press(&mut app, "0");
    assert_eq!(app.chain_column, ChainColumn::Phrase);
    press(&mut app, " 5l");
    let instrument = app.instrument;
    press(&mut app, "0$");
    assert_eq!(app.instrument, instrument);
}

#[test]
fn replace_shows_its_mode_and_a_red_cursor_with_no_box() {
    let (mut app, _realtime) = app();
    press(&mut app, "r");
    assert_eq!(app.mode, Mode::Replace);
    let buffer = screen(&app);
    assert_eq!(buffer[(5, 2)].bg, vt_tui::theme::RED);
    assert_eq!(buffer[(4, 2)].symbol(), " ");
    press(&mut app, "a");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(screen(&app)[(5, 2)].bg, vt_tui::theme::ROSEWATER);
}

#[test]
fn backspace_clears_a_note_with_what_it_was_written_with_but_keeps_fx() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Backspace);
    let step = &app.song().phrases[&0].steps()[1];
    assert_eq!(step.note, Note::Empty);
    let first = &app.song().phrases[&0].steps()[0];
    assert_eq!(first.note, Note::On(60), "it clears where the cursor was");
    special(&mut app, KeyCode::Backspace);
    let first = &app.song().phrases[&0].steps()[0];
    assert_eq!(first.note, Note::Empty);
    assert_eq!(first.velocity, None);
    assert_eq!(first.instrument, None);
}

#[test]
fn the_note_heard_while_typing_uses_the_velocity() {
    let (mut app, mut rt) = app();
    with_a_sound(&mut app);
    block(&mut rt, &mut app);
    let heard = |app: &mut App, rt: &mut Realtime, keys: &str| {
        press(app, keys);
        let level = loudest(rt, 2000);
        let _ = loudest(rt, 96_000);
        level
    };
    let quiet = heard(&mut app, &mut rt, "icccccc");
    assert_eq!(quiet, 0.0, "changing velocity plays nothing");
    let soft = heard(&mut app, &mut rt, "a");
    press(&mut app, "vvvvvvvv");
    let full = heard(&mut app, &mut rt, "a");
    assert!(soft * 2.0 < full, "{soft} vs {full}");
}
