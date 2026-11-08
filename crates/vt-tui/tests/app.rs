use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;
use vt_engine::engine::{Engine, Transport};
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{
    Chain, ChainEntry, FilterType, Launch, Machine, Phrase, SectionStart, Song, SynthVoice,
};
use vt_model::step::{Note, Step};
use vt_tui::app::App;
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
fn the_transport_keys_play_pause_and_restart() {
    let (mut app, mut rt) = app();
    press(&mut app, "jjj");
    let mut now = Instant::now();
    let mut hit = |app: &mut App, rt: &mut Realtime, key: Key| {
        now += Duration::from_millis(1000);
        app.key(key, now);
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
    let start = Instant::now();
    for gap in 0..20 {
        app.key(
            Key::new(KeyCode::Tab),
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
fn quick_taps_of_tab_all_count() {
    let (mut app, mut rt) = app();
    let mut now = Instant::now();
    let mut expected = Transport::Playing;
    for _ in 0..6 {
        app.key(Key::new(KeyCode::Tab), now);
        block(&mut rt, &mut app);
        assert_eq!(app.transport, expected);
        expected = if expected == Transport::Playing {
            Transport::Paused
        } else {
            Transport::Playing
        };
        now += Duration::from_millis(150);
    }
}

#[test]
fn repeats_after_a_leader_transport_key_dont_reach_normal_keys() {
    let (mut app, mut rt) = app();
    let start = Instant::now();
    app.key(Key::char(' '), start);
    for gap in 0..10 {
        app.key(Key::char('c'), start + Duration::from_millis(gap * 40));
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
    assert_eq!(app.phrase_id(), Some(0), "its own first phrase");
    press(&mut app, "WWWW");
    assert_eq!(app.cursor.track, 4, "the master stops the cursor");
    press(&mut app, "BBBBB");
    assert_eq!(app.cursor.track, 0);
}

#[test]
fn added_tracks_save_load_and_play() {
    let (mut app, mut rt) = app();
    command(&mut app, "track add 3");
    assert_eq!(app.song().sound_tracks(), 5);
    assert_eq!(app.levels.len(), 8);
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
    assert_eq!(app.levels.len(), 5);
}

#[test]
fn the_fixed_tracks_stay_after_new_ones_and_cant_be_removed() {
    let (mut app, _rt) = app();
    press(&mut app, "WWWW");
    assert_eq!(app.cursor.track, 4);
    command(&mut app, "track add 2");
    assert_eq!(app.cursor.track, 6, "still on the master");
    assert_eq!(app.song().tracks[6].name, "M");
    assert_eq!(app.song().sound_tracks(), 4);
    command(&mut app, "track del");
    assert_eq!(app.cursor.track, 5, "still on the master");
    assert_eq!(app.song().tracks[5].name, "M");
}

#[test]
fn a_fixed_tracks_mute_stays_with_it_as_tracks_come_and_go() {
    let (mut app, _rt) = app();
    press(&mut app, "WW");
    assert_eq!(app.song().tracks[app.cursor.track].name, "A");
    press(&mut app, " m");
    assert!(app.levels[2].mute);
    command(&mut app, "track add");
    assert!(app.levels[3].mute, "return A is here now");
    assert!(!app.levels[2].mute, "and the new track isn't muted");
    command(&mut app, "track del");
    assert!(app.levels[2].mute);
}

#[test]
fn the_master_cant_be_muted_or_soloed() {
    let (mut app, _rt) = app();
    press(&mut app, "WWWW");
    press(&mut app, " m");
    assert!(!app.levels[4].mute);
    assert!(app.status.contains("master"), "{}", app.status);
    press(&mut app, " s");
    assert!(!app.levels[4].solo);
}

#[test]
fn a_fixed_tracks_pages_only_show_what_it_has() {
    let (mut app, _rt) = app();
    press(&mut app, "WW 4");
    let lines = track_page_text(&app);
    assert!(lines[0].starts_with("Return A"), "{}", lines[0]);
    assert!(lines[2].starts_with("Level"), "{}", lines[2]);
    assert!(lines[3].starts_with("Pan"), "{}", lines[3]);
    assert!(!lines.iter().any(|line| line.contains("Machine")));
    press(&mut app, "WW");
    let lines = track_page_text(&app);
    assert!(lines[0].starts_with("Master"), "{}", lines[0]);
    assert!(lines[2].starts_with("Level"), "{}", lines[2]);
    assert!(!lines.iter().any(|line| line.contains("Pan")));
}

#[test]
fn the_mixer_leaves_out_what_a_fixed_track_doesnt_have() {
    let (mut app, _rt) = app();
    press(&mut app, " 6");
    press(&mut app, "lljj");
    assert_eq!(app.cursor.track, 2);
    press(&mut app, "i40");
    assert_eq!(app.song().tracks[2].send_a, 0, "a return sends nowhere");
    press(&mut app, "ll");
    assert_eq!(app.cursor.track, 4);
    press(&mut app, "ki40");
    assert_eq!(app.song().tracks[4].pan, 0x80, "the master has no pan");
}

#[test]
fn the_mixers_cursor_stays_on_rows_a_track_has_and_comes_back() {
    use vt_tui::views::status::column_name;
    let (mut app, _rt) = app();
    press(&mut app, " 6jj");
    assert_eq!(column_name(&app), "Send A");
    press(&mut app, "ll");
    assert_eq!(app.cursor.track, 2);
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
    press(&mut app, "hhhh");
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
        (1, "Send A".to_owned())
    );
    press(&mut app, "0");
    assert_eq!(
        (app.cursor.track, column_name(&app)),
        (0, "Send A".to_owned())
    );
    press(&mut app, "k$");
    assert_eq!(
        (app.cursor.track, column_name(&app)),
        (3, "Pan".to_owned()),
        "B, as M has none"
    );
    press(&mut app, "k$");
    assert_eq!(
        (app.cursor.track, column_name(&app)),
        (4, "Level".to_owned())
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
    std::fs::create_dir_all(folder.join("target")).unwrap();
    wav(&folder.join("target/built.wav"));
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
    let chosen = app.song().tracks[0].sample.clone().unwrap();
    assert!(chosen.ends_with("snare.WAV"), "{chosen}");
    std::fs::remove_dir_all(&folder).unwrap();
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
    // Ctrl-O returns to where the jump started, and Ctrl-N goes forward again.
    let back = parse_sequence("C-o").unwrap()[0];
    app.key(back, Instant::now());
    assert_eq!(app.view, View::Chain);
    assert_eq!(app.cursor.slot, 1);
    app.key(parse_sequence("C-n").unwrap()[0], Instant::now());
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
        .shown()
        .map(|item| item.label.clone())
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

    let velocity = |app: &App| app.song().tracks[0].phrases[&0].steps()[0].velocity;
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
    let title: String = (61..80).map(|x| buffer[(x, 0)].symbol()).collect();
    assert_eq!(
        title.trim_end(),
        " 01  02",
        "the returns and master are left out"
    );
    assert_eq!(
        buffer[(66, 0)].bg,
        vt_tui::theme::GREEN,
        "track 02 is marked"
    );
    assert_ne!(buffer[(62, 0)].bg, vt_tui::theme::GREEN);
    let headings: String = (0..30).map(|x| buffer[(x, 1)].symbol()).collect();
    assert_eq!(headings.trim_end(), "         Note        FX");
    assert_eq!(
        buffer[(21, 2)].bg,
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
        buffer[(65, row + 2)].fg
    };
    on_the_row_being_edited(&mut app);
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

fn sample_of(app: &App, track: usize) -> Option<String> {
    app.song().tracks.get(track)?.sample.clone()
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
        reopened.tracks[0].sample.as_deref(),
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
        r#"(sounds: [(label: "SD", path: "drums/snare.wav"), (path: "drums/hat.wav")])"#,
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
        .shown()
        .map(|item| item.label.clone())
        .collect();
    assert_eq!(
        labels[..2],
        ["SD  snare.wav", "    hat.wav"],
        "the kit first"
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
        track_text(&app)[3].contains("File"),
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
        text[6].contains("Swing"),
        "a voice of three settings: {text:?}"
    );
    assert!(
        !text
            .iter()
            .any(|line| line.contains("File") || line.contains("Voice"))
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
    // Past Detune's fine digits to Pan.
    press(&mut app, "kll");
    assert_eq!(column_name(&app), "Pan");
    press(&mut app, "jjj");
    assert_eq!(column_name(&app), "Swing", "the row with only swing");
    press(&mut app, "ggh-");
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Sample,
        "VA comes after Sample"
    );
    assert!(track_text(&app)[2].contains("Sample"));
    press(&mut app, "=x");
    assert_eq!(app.song().tracks[0].machine, Machine::Sample);
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
    assert_eq!(app.song().tracks[0].machine, Machine::Sample);
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
        track_text(&app)[3].contains("20 E.PIANO 1"),
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
    assert!(track_text(&app)[3].contains("01 Mooger Low"));
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
    assert_eq!(app.song().tracks[0].machine, Machine::Sample);
    assert!(sample_of(&app, 0).unwrap().ends_with("snare.wav"));
}

#[test]
fn bars_show_where_a_value_is() {
    use vt_tui::views::track::{centre_bar, level_bar};
    assert_eq!(level_bar(0x00), "         ");
    assert_eq!(level_bar(0x01), "▏        ");
    assert_eq!(level_bar(0x40), "██▎      ");
    assert_eq!(level_bar(0x80), "████▌    ");
    assert_eq!(level_bar(0xFE), "████████▉");
    assert_eq!(level_bar(0xFF), "█████████");
    assert_eq!(centre_bar(0x00), "│        ");
    assert_eq!(centre_bar(0x80), "    │    ");
    assert_eq!(centre_bar(0xFF), "        │");
    let (mut app, _realtime) = app();
    press(&mut app, " 4");
    let text = track_text(&app).join("\n");
    assert!(text.contains("8000 ▕    │    ▏"), "{text}");
}

#[test]
fn a_moving_bar_follows_what_plays_and_marks_what_is_set() {
    use ratatui::style::Style;
    use vt_tui::theme;
    use vt_tui::views::track::bar_spans;
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
    use vt_tui::views::track::{bar_spans, level_bar};
    let text = |set, live| -> String {
        bar_spans(set, live, false)
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    };
    for set in 0..=u8::MAX {
        for live in 0..u8::MAX {
            // Each step the plain bar takes shows, wherever the mark is.
            if level_bar(live) != level_bar(live + 1) {
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
        machine: [0x80, 0, 0x80, 0xCC],
        inserts: Default::default(),
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
    assert_eq!(song.tracks[0].sample.as_deref(), Some("samples/tone.wav"));
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
    let step = &app.song().tracks[0].phrases[&0].steps()[1];
    assert_eq!(step.note, Note::Empty);
    let first = &app.song().tracks[0].phrases[&0].steps()[0];
    assert_eq!(first.note, Note::On(60), "it clears where the cursor was");
    special(&mut app, KeyCode::Backspace);
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
    let neighbour = |y: u16| -> String { (66..69).map(|x| buffer[(x, y)].symbol()).collect() };
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
    app.playhead = 1;
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
    let x = 65 + column * 4;
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
    app.playhead = 5;
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
    app.playhead = 2;
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
        (0..59)
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
    press(&mut app, " 4jjj");
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
    press(&mut app, "ggjl");
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
    press(&mut app, "ggli");
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
        picker.shown().count(),
        8,
        "what a sample track has, and none"
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
        app.picker.as_ref().unwrap().current().unwrap().label,
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

    press(&mut app, "kkk");
    assert_eq!(track(&app).filter, FilterType::LowPass);
    press(&mut app, "=");
    assert_eq!(track(&app).filter, FilterType::HighPass);
    press(&mut app, "-");
    assert_eq!(track(&app).filter, FilterType::LowPass);
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 5);
    press(&mut app, "ladder");
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
    assert!(track_page_text(&app)[5].contains("Ladder"));
    press(&mut app, "u");
    assert_eq!(track(&app).filter, FilterType::LowPass);
}

#[test]
fn the_gap_above_the_filter_is_only_on_screen() {
    use vt_tui::track_page::Control;
    let (mut app, _realtime) = on_track_page();
    press(&mut app, "kk");
    assert_eq!(app.track_control(), Control::Sample);
    press(&mut app, "j");
    assert_eq!(app.track_control(), Control::FilterType);

    // A synth has more above its filter, and the gap stays just above it.
    press(&mut app, "kkK");
    press(&mut app, "kick");
    special(&mut app, KeyCode::Enter);
    let lines = track_page_text(&app);
    assert!(lines[5].starts_with("Decay"), "{}", lines[5]);
    assert!(lines[6].starts_with("Accent"), "{}", lines[6]);
    assert!(lines[7].trim().is_empty(), "{}", lines[7]);
    assert!(lines[8].starts_with("Filter"), "{}", lines[8]);
}

#[test]
fn the_track_page_sets_the_utility_filter() {
    let (mut app, _realtime) = on_track_page();
    let track = |app: &App| app.song().tracks[0].clone();
    assert_eq!((track(&app).low_cut, track(&app).high_cut), (0, 0xFF));
    let lines = track_page_text(&app);
    assert!(lines[9].contains("Low cut") && lines[9].contains("00"));
    assert!(lines[10].contains("High cut") && lines[10].contains("FF"));

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
fn option_with_the_movement_keys_changes_the_value_without_moving() {
    let (mut app, _realtime) = on_track_page();
    let attack = |app: &App| app.song().tracks[0].envelopes[0].attack_ms;
    press(&mut app, "L0l");
    typed(&mut app, "20");
    let label = app.track_control().label();
    alt(&mut app, "M-l");
    assert_eq!(attack(&app), 25, "as =");
    alt(&mut app, "M-k");
    assert_eq!(attack(&app), 75, "as +");
    alt(&mut app, "M-h");
    assert_eq!(attack(&app), 70, "as -");
    alt(&mut app, "M-j");
    assert_eq!(attack(&app), 20, "as _");
    assert_eq!(app.track_control().label(), label, "the cursor stays");
}

#[test]
fn option_with_the_movement_keys_changes_notes_and_selections() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let Note::On(start) = written_notes(&app)[0] else {
        panic!("no note");
    };
    press(&mut app, "gg");
    alt(&mut app, "M-l");
    alt(&mut app, "M-k");
    assert_eq!(written_notes(&app), [Note::On(start + 13)]);
    press(&mut app, "V");
    alt(&mut app, "M-h");
    alt(&mut app, "M-j");
    assert_eq!(written_notes(&app), [Note::On(start)], "on a selection too");
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
        app.picker.as_ref().unwrap().current().unwrap().label,
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
    let (mut app, _realtime) = on_track_page();
    let page = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..24)
            .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
    };
    press(&mut app, "L");
    let lines = page(&app);
    let heading = lines.iter().position(|l| l.contains("Env 1  Vol")).unwrap();
    assert!(lines[heading].contains("Env 2  Cut"));
    let dots = |line: &String| {
        line.chars()
            .filter(|c| ('\u{2801}'..='\u{28FF}').contains(c))
            .count()
    };
    assert!(dots(&lines[heading + 1]) > 0);

    // Changing a value redraws it.
    press(&mut app, "gg0lll");
    typed(&mut app, "0");
    assert_ne!(page(&app)[heading + 1], lines[heading + 1]);
    assert_eq!(app.song().tracks[0].envelopes[0].sustain, 0);

    // The first envelope on Vol has no depth to show, and off it, it does.
    assert!(page(&app)[3].contains("--"));
    press(&mut app, "0K/cut");
    special(&mut app, KeyCode::Enter);
    assert!(page(&app)[3].contains("00"));
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
    press(&mut app, "ggl");
    press(&mut app, "+");
    assert_eq!(track(&app).level, 0xF0);
    press(&mut app, "jjjj");
    typed(&mut app, "60");
    press(&mut app, "+");
    assert_eq!(track(&app).swing, Some(650));
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
fn a_track_following_the_project_shows_its_swing_dimmed_and_x_returns_to_that() {
    let (mut app, _realtime) = on_track_page();
    command(&mut app, "swing 60");
    let swing_cell = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(80, 16);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let at = (0..80)
            .position(|x| buffer[(x, 7)].symbol() == "S")
            .unwrap() as u16
            + 10;
        let text: String = (at..at + 2).map(|x| buffer[(x, 7)].symbol()).collect();
        (text, buffer[(at, 7)].fg)
    };
    // The cursor is elsewhere, so the colour is the setting's own.
    assert_eq!(app.song().tracks[0].swing, None);
    assert_eq!(swing_cell(&app), ("60".to_owned(), vt_tui::theme::OVERLAY2));

    // It's the track's own once set, and a nudge starts from what it was playing.
    press(&mut app, "j$=");
    assert_eq!(app.song().tracks[0].swing, Some(610));
    press(&mut app, "k");
    assert_eq!(swing_cell(&app), ("61".to_owned(), Color::Reset));

    press(&mut app, "j$x");
    assert_eq!(app.song().tracks[0].swing, None);
    press(&mut app, "k");
    assert_eq!(swing_cell(&app), ("60".to_owned(), vt_tui::theme::OVERLAY2));
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
fn the_track_page_shows_its_settings_in_columns() {
    let (mut app, _realtime) = on_track_page();
    press(&mut app, "Ljj0=ll");
    typed(&mut app, "1/8d");
    press(&mut app, "l");
    typed(&mut app, "+40");
    press(&mut app, "l");
    typed(&mut app, "90");
    let lines = track_page_text(&app);
    assert!(lines[0].starts_with("Track 01"), "{}", lines[0]);
    assert!(lines[2].contains("Attack") && lines[2].contains("Reset"));
    assert!(lines[3].starts_with("Env 1") && lines[3].contains("Vol"));
    assert!(lines[4].starts_with("Env 2") && lines[4].contains("Cut"));
    assert!(lines[6].contains("Dest") && lines[6].contains("Retrig"));
    let lfo: Vec<&str> = lines[7].split_whitespace().collect();
    assert_eq!(lfo, ["LFO", "1", "Cut", "Tri", "1/8d", "+40", "90°", "off"]);
    assert!(lines[8].starts_with("LFO 2"));
    press(&mut app, "H");
    let lines = track_page_text(&app);
    assert!(lines[2].starts_with("Machine") && lines[2].contains("Level"));
    assert!(lines[3].starts_with("File") && lines[3].contains("Pan"));
    assert!(lines[4].trim().is_empty(), "{}", lines[4]);
    assert!(lines[5].starts_with("Filter") && lines[5].contains("Send A"));
    assert!(lines[6].starts_with("Cut") && lines[6].contains("FF00 ▕█████████▏"));
    assert!(lines[7].starts_with("Res") && lines[7].contains("Swing"));
}

#[test]
fn track_swing_on_the_page_matches_its_command() {
    let (mut app, _realtime) = on_track_page();
    press(&mut app, "j$");
    typed(&mut app, "60");
    assert_eq!(app.song().tracks[0].swing, Some(600));
    press(&mut app, "=");
    assert_eq!(app.song().tracks[0].swing, Some(610));
    typed(&mut app, "off");
    assert_eq!(app.song().tracks[0].swing, None);
    typed(&mut app, "90");
    assert_eq!(app.song().tracks[0].swing, None);
    assert!(app.status.contains("50 to 75"), "{}", app.status);
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
    press(&mut app, "lllK");
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
        row(2).contains("CUT   4000 . SNA   2000 h RES   1000 g"),
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
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 5);
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
    assert_eq!(column_name(&app), "Gate");
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
    press(&mut app, "lll");
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
    press(&mut app, "lll");
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
    let style_at = |text: &str, past: usize| {
        let at = row.find(text).unwrap_or_else(|| panic!("no {text}: {row}"));
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
    assert_eq!(theme::dim().fg, Some(theme::OVERLAY2));
}

#[test]
fn k_lists_commands_by_group_and_opens_searching() {
    use vt_tui::input::picker::Focus;
    let (mut app, _realtime) = app();
    press(&mut app, "lllK");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.focus, Focus::Search);
    assert!(picker.query.is_empty());
    let mut names = picker.group_names();
    names.dedup();
    assert_eq!(
        names[..3],
        ["---".to_owned(), "Filter".into(), "Output".into()]
    );
    assert_eq!(names.last().map(String::as_str), Some("Events"));
    // Typing searches long names as well as labels.
    press(&mut app, "send b");
    assert_eq!(
        app.picker
            .as_ref()
            .unwrap()
            .current()
            .map(|item| item.label.as_str()),
        Some("SNB   Send B")
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
    assert_eq!(app.levels.len(), 6);
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
fn the_file_setting_chooses_a_sample_and_x_clears_it() {
    let (mut app, _realtime) = on_track_page();
    press(&mut app, "kk");
    assert_eq!(app.track_control(), vt_tui::track_page::Control::Sample);
    press(&mut app, "K");
    assert_ne!(app.mode, Mode::Normal);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "=");
    assert!(app.status.contains("K chooses"), "{}", app.status);
    command(&mut app, "name bd");
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let tone = dir.join("tone.wav");
    wav(&tone);
    command(&mut app, &format!("sample {}", tone.display()));
    let drawn = page_text(&app);
    assert!(drawn.contains("tone.wav"), "{drawn}");
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
    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::mixer::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..16)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
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
    assert_eq!(app.cursor.track, 3, "B, the last with a mute");
    press(&mut app, "gg");
    assert_eq!(column_name(&app), "Level");
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
    assert!(mixer_line(&app, "Solo")[4..].contains('S'));
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
    assert_eq!(app.cursor.track, 34);
    let names = &mixer_text(&app)[2];
    assert!(names.contains("32"), "{names}");
    assert!(names.contains(" A ") && names.contains(" M"), "{names}");
    assert!(!names.contains("01"), "{names}");
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
    // The meter's left channel is full, and the right empty.
    let bars = &text[3..10];
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
        text[3..10]
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
            .map(|i| i.label.as_str()),
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
fn an_lfos_shape_has_an_icon_sent_once_and_shared() {
    use vt_tui::kitty::Graphics;
    let (mut app, _realtime) = app();
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out).unwrap().matches(",c=4,").count()
    };
    press(&mut app, " 4L");
    // Both LFOs start on the same shape, so one image serves both.
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let plain = app.lfo_images.unwrap();
    assert_eq!(plain[0], plain[1]);
    assert_eq!(sync(&mut app, &mut graphics), 0);

    // The cursor picks one out, once. After that, moving it sends nothing.
    press(&mut app, "jj");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let picked = app.lfo_images.unwrap();
    assert_ne!(picked[0], picked[1]);
    assert_eq!(picked[1], plain[1]);
    press(&mut app, "k");
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.lfo_images.unwrap(), plain);

    // A new shape has its own image, and coming back to the old one needs nothing sent.
    press(&mut app, "jl=");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_ne!(app.lfo_images.unwrap()[0], picked[0]);
    press(&mut app, "-");
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.lfo_images.unwrap(), picked);
}

#[test]
fn an_lfos_icon_sits_after_its_name_and_keeps_the_columns_in_line() {
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
    let plain = draw(&app);
    app.lfo_images = Some([0x01_02_03, 0x04_05_06]);
    let drawn = draw(&app);
    let row = (0..24)
        .find(|y| {
            (0..80)
                .map(|x| plain[(x, *y)].symbol())
                .collect::<String>()
                .starts_with("LFO 1")
        })
        .unwrap();
    let text = |buffer: &ratatui::buffer::Buffer, x: u16| buffer[(x, row)].symbol().to_owned();
    // Name column, then the icon, with the same text after it either way.
    assert_eq!(text(&drawn, 30), "\u{10EEEE}\u{305}\u{305}");
    assert_eq!(drawn[(30, row)].fg, ratatui::style::Color::Rgb(1, 2, 3));
    assert_eq!(text(&drawn, 33), "\u{10EEEE}\u{305}\u{310}");
    assert_eq!(text(&drawn, 34), " ");
    for x in 36..80 {
        assert_eq!(text(&drawn, x), text(&plain, x), "at {x}");
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
    press(&mut app, "jj");
    assert_eq!(column_name(&app), "Filter");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let picked = app.filter_image.unwrap();
    assert_ne!(picked, plain);
    press(&mut app, "k");
    assert_eq!(sync(&mut app, &mut graphics), 0);
    assert_eq!(app.filter_image.unwrap(), plain);

    // A new setting draws it again.
    press(&mut app, "jj");
    assert_eq!(column_name(&app), "Cut");
    press(&mut app, "-");
    assert_eq!(sync(&mut app, &mut graphics), 1);
    assert_ne!(app.filter_image.unwrap(), picked);
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
        machine: [0, 1, 2, 3].map(|setting| track.machine_value(setting)),
        inserts: Default::default(),
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
    let last = row_of("High cut");
    // Without an image, there's no box.
    assert_eq!(plain[(0, last + 1)].symbol(), " ");

    app.filter_image = Some(0x01_02_03);
    let drawn = draw(&app);
    let top = last + 1;
    // As wide as a value and its bar, which ends at the line after its cells.
    let bar_end = (0..80)
        .rfind(|x| plain[(*x, row_of("Drive"))].symbol() == "▏")
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
        .shown()
        .map(|item| item.label.clone())
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
    assert_eq!(jump_labels(&app), ["d Low cut", "f High cut", "s Cut"]);

    press(&mut app, "d");
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
    assert!(line(&app, "Cus").starts_with("Cus       sF00 ▕"));
    assert!(line(&app, "Low cud").starts_with("Low cud   d0   ▕"));
    assert!(line(&app, "High cuf").starts_with("High cuf  fF   ▕"));
    assert!(line(&app, "Res").starts_with("Res       0000 ▕"));
    // It moves along as you type, and past the end of a name it sits just after it.
    press(&mut app, "t");
    assert!(line(&app, "Cuts").starts_with("Cuts      sF00 ▕"));
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
    press(&mut app, "iasdfghjkl;");
    assert_eq!(
        written_notes(&app),
        [62, 64, 65, 67, 69, 71, 72, 74, 76, 77].map(Note::On)
    );
}

#[test]
fn with_a_scale_the_row_above_is_silent_and_note_off_and_octave_still_work() {
    let (mut app, _realtime) = app();
    command(&mut app, "set scale=major");
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
    command(&mut app, "set root=d");
    command(&mut app, "set scale=dorian");
    press(&mut app, "i");
    alt(&mut app, "M-s");
    assert!(!app.scale.on);
    assert_eq!(app.mode, Mode::Insert, "in Insert, it stays");
    press(&mut app, "s");
    assert_eq!(written_notes(&app), [Note::On(62)], "the piano again");
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
    press(&mut app, "iawe");
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
    press(&mut app, "u");
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
    press(&mut app, "iaw");
    special(&mut app, KeyCode::Esc);
    // The cursor isn't on these.
    press(&mut app, "G");
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
    assert!(fx_line(&app, 0).contains("Type     Down"), "it goes round");
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
            .shown()
            .map(|item| item.label.clone())
            .collect();
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
    // Past the seven a sample track has, as a synth's settings aren't on it.
    for _ in 0..8 {
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
    press(&mut app, "lllK");
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .shown()
        .map(|item| item.label.clone())
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
    press(&mut app, "ggj0llli1mix");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "40");
    assert_eq!(command(&app, 1), Some(setting(4)));

    // A label that isn't there says so, and opens the list searching for it.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "ggjj0llliamnt");
    special(&mut app, KeyCode::Enter);
    assert_eq!(command(&app, 2), None);
    assert!(app.status.contains("isn't a label"), "{}", app.status);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.query, "amnt");
    assert_eq!(
        picker.current().map(|item| item.label.as_str()),
        Some("1AMNT 1 Amount")
    );
    special(&mut app, KeyCode::Enter);
    press(&mut app, "20");
    assert_eq!(command(&app, 2), Some(setting(1)));
}

#[test]
fn the_phrase_view_names_a_slots_setting_in_a_wide_column() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4LL= 3lllK1amnt");
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
        machine: [0x80, 0, 0x80, 0xCC],
        inserts: Default::default(),
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
            .shown()
            .map(|item| item.label.clone())
            .collect();
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
    let steps = labels(&mut app, " 3lllK");
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
            .shown()
            .map(|item| item.label.clone())
            .collect();
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
fn dest_can_be_browsed_by_group_as_well_as_searched() {
    use vt_tui::input::picker::Focus;
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
    let focus = |app: &App| app.picker.as_ref().unwrap().focus;
    assert_eq!(focus(&app), Focus::List);
    assert!(current(&app).starts_with("---"), "{}", current(&app));

    // h to the groups, down to Filter, which shows its own.
    press(&mut app, "hj");
    assert_eq!(focus(&app), Focus::Groups);
    assert!(current(&app).starts_with("Cut"), "{}", current(&app));

    // l into it, and down the list.
    press(&mut app, "lj");
    assert_eq!(focus(&app), Focus::List);
    assert!(current(&app).starts_with("Res"), "{}", current(&app));
    assert_eq!(app.mode, Mode::Picker, "browsing doesn't type or choose");
    press(&mut app, "x");
    assert!(app.picker.as_ref().unwrap().query.is_empty());

    // Enter on a group goes in, and Enter in the list chooses.
    press(&mut app, "hj");
    special(&mut app, KeyCode::Enter);
    assert_eq!(focus(&app), Focus::List);
    assert!(current(&app).starts_with("Pan"), "{}", current(&app));

    // The search reaches everything, and / comes back.
    press(&mut app, "/1 mi");
    assert_eq!(focus(&app), Focus::Search);
    assert!(current(&app).starts_with("1 Mix"), "{}", current(&app));
    press(&mut app, "/");
    assert_eq!(focus(&app), Focus::List);
    assert!(current(&app).starts_with("1 Mix"), "{}", current(&app));
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].lfos[0].destination,
        Some(SettingId::slot(0, vt_model::settings::Effect::Distort, 4).unwrap())
    );

    // It opens on what's chosen, and Esc leaves it.
    press(&mut app, "K");
    assert!(current(&app).starts_with("1 Mix"), "{}", current(&app));
    press(&mut app, "hk");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(
        app.song().tracks[0].lfos[0].destination,
        Some(SettingId::slot(0, vt_model::settings::Effect::Distort, 4).unwrap()),
        "changing nothing"
    );

    // A lone item in the groups is chosen right there. On a sample track, Machine holds Pitch.
    press(&mut app, "Kh");
    let picker = app.picker.as_ref().unwrap();
    let at = picker
        .group_names()
        .iter()
        .position(|name| name == "Pitch")
        .unwrap();
    let by = at as isize - picker.group as isize;
    for _ in 0..by.unsigned_abs() {
        press(&mut app, if by > 0 { "j" } else { "k" });
    }
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].lfos[0].destination,
        Some(SettingId::PITCH)
    );
}

#[test]
fn dests_window_draws_groups_beside_the_list() {
    let (mut app, _realtime) = app();
    press(&mut app, " 4LjjKhj");
    let picker = app.picker.as_ref().unwrap();
    let backend = ratatui::backend::TestBackend::new(100, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::picker::draw(frame, frame.area(), picker))
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
    assert!(text.contains("  ---       Cut  Filter cutoff"), "{text}");
    assert!(text.contains("> Filter    Res  Filter resonance"), "{text}");
    assert!(text.contains("  Output"), "{text}");
    assert!(text.contains("/ search"), "{text}");
}

#[test]
fn a_plain_list_still_opens_on_its_search() {
    // `K` then typing on Machine, and / is just a letter.
    let (mut app, _realtime) = app();
    press(&mut app, " 4Kpl");
    assert_eq!(app.picker.as_ref().unwrap().query, "pl");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "K/");
    assert_eq!(app.picker.as_ref().unwrap().query, "/");
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
    press(&mut app, "llljj");
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
    press(&mut app, "lll7jge");
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
    press(&mut app, "llljrSNA");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "20gg");
    command(&mut app, "lane 3");
    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    // Place 4 is the notes' row 4, and the lane's row 4 % 3 = 1.
    app.playhead = 4;
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
    press(&mut app, "lll4j");
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
    press(&mut app, "llrMLT3ggK");
    assert_eq!(app.mode, Mode::Picker);
    press(&mut app, "hld");
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
    press(&mut app, "llrSGL2jrSGL2gg");
    app.top = 0;
    let buffer = screen(&app);
    let number_colour = |row: u16| buffer[(5, row + 2)].fg;
    let beat = number_colour(0);
    assert_ne!(number_colour(1), beat, "row 01 starts at step 2");
    assert_eq!(number_colour(2), beat, "row 02 starts at step 4");
    assert_ne!(number_colour(4), beat, "row 04 starts at step 6");
    assert_eq!(number_colour(6), beat, "row 06 starts at step 8");
}

#[test]
fn an_lfos_depth_is_locked_by_its_label_and_reads_in_its_units() {
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, "llli");
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
