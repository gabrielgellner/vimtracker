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
use vt_tui::input::keys::Key;
use vt_tui::input::mode::Mode;

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
    press(&mut app, "zq1");
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
    press(&mut app, "i[z]]z");
    let written = notes(&app);
    assert_eq!(written[0], Note::On(48));
    assert_eq!(written[1], Note::On(72));
}

#[test]
fn undo_and_redo_keys_restore_notes() {
    let (mut app, _rt) = app();
    press(&mut app, "iz");
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
    press(&mut app, "iz");
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
    press(&mut app, "iz");
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
    press(&mut app, "iz");
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
    press(&mut app, "iz");
    special(&mut app, KeyCode::Esc);
    let path = folder();
    command(&mut app, &format!("new {}", path.display()));
    assert!(app.status.contains("Unsaved"));
    assert!(!path.exists());
}
