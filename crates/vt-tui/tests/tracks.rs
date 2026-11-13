//! Tracks: adding and removing them, the fixed tracks, moving between them, mutes and solos, names
//! and levels.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_model::constants::{FIXED_TRACKS, MAX_GROUPS};
use vt_model::song::Song;
use vt_model::step::Note;
use vt_tui::input::keys::Key;

mod common;
use common::*;

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
