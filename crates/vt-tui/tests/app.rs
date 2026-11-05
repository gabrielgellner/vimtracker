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
use vt_tui::app::App;
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
    let selected = &buffer[(6, 1)];
    assert_eq!(selected.bg, vt_tui::theme::SURFACE1);
    assert_eq!(selected.fg, vt_tui::theme::TEXT);
    assert_ne!(selected.symbol(), " ");
    let cursor = &buffer[(5, 2)];
    assert_eq!(cursor.bg, vt_tui::theme::ROSEWATER);
    assert_eq!(cursor.fg, vt_tui::theme::BASE);
}

#[test]
fn the_status_line_names_the_column() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    assert_eq!(column_name(&app), "Note");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Volume");
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
    special(&mut app, KeyCode::Up);
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
    let volumes: Vec<_> = app.song().phrases[&0]
        .steps()
        .iter()
        .map(|s| s.volume)
        .collect();
    assert_eq!(volumes[0], None);
    assert_eq!(volumes[1], Some(0x40));
}

#[test]
fn the_ableton_layout_plays_and_c_and_v_set_the_volume_written() {
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

    let volume = |app: &App| app.song().phrases[&0].steps()[0].volume;
    assert_eq!(volume(&app), None);
    press(&mut app, "a");
    assert_eq!(volume(&app), None);
    press(&mut app, "c");
    assert_eq!(app.note_volume, Some(0xE0));
    press(&mut app, "ca");
    assert_eq!(volume(&app), Some(0xC0));
    assert_eq!(notes(&app)[0], Note::On(48));

    // The whole note is one undo step.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "u");
    assert_eq!(volume(&app), None);

    press(&mut app, "i");
    press(&mut app, "vv");
    assert_eq!(app.note_volume, None);
    press(&mut app, "v");
    assert_eq!(app.note_volume, None);
    for _ in 0..12 {
        press(&mut app, "c");
    }
    assert_eq!(app.note_volume, Some(0x20));
}
