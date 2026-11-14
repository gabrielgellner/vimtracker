//! Presets (decision 310): a track's sound saved with `:preset save` and loaded on another track
//! or in another song, by name or from the browser, as one undo step that keeps the track's place
//! in the song; an effect saved from one slot and loaded into another; and `K`'s ways in.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::KeyCode;
use vt_model::action::Action;
use vt_model::insert::Insert;
use vt_model::settings::{Effect, SettingId};
use vt_model::song::{Machine, SynthVoice};
use vt_tui::app::App;
use vt_tui::config::Settings;
use vt_tui::input::mode::Mode;

mod common;
use common::*;

/// An app whose presets folder is a scratch folder of its own, not there yet.
fn presets_app() -> (App, vt_engine::link::Realtime, PathBuf) {
    let (mut app, realtime) = app();
    let presets = folder().join("presets");
    app.settings = Settings {
        presets: presets.display().to_string(),
        ..Settings::none()
    };
    (app, realtime, presets)
}

/// Make track 1 an acid with its cutoff down, a delay in its second slot and a named macro, and
/// its level, pan and name its own.
fn acid_on_first(app: &mut App) {
    app.apply(&Action::Batch(vec![
        Action::SetTrackMachine {
            track: 0,
            machine: Machine::Plaits(SynthVoice::Acid),
        },
        Action::SetInsert {
            track: 0,
            slot: 1,
            insert: Insert::RETURN_DELAY,
        },
        Action::SetTrackLevel {
            track: 0,
            level: 0x30,
        },
        Action::SetTrackName {
            track: 0,
            name: "AC".to_owned(),
        },
    ]));
    let mut song = vt_model::song::Song::clone(app.song());
    let track = &mut song.tracks[0];
    track.cutoff = 0x40;
    track.macros[0].name = Some("Bite".to_owned());
    track.set_fine(SettingId::slot(1, Effect::Delay, 1).unwrap(), 0x40);
    let track = track.clone();
    app.apply(&Action::LoadSound {
        track: 0,
        preset: Box::new(vt_model::preset::SoundPreset::of(&track, None)),
        sample: 0,
    });
}

#[test]
fn a_sound_saved_loads_on_another_track_as_one_undo_step_keeping_its_place() {
    let (mut app, _realtime, presets) = presets_app();
    acid_on_first(&mut app);
    command(&mut app, "preset save bass/Acid Bite");
    let file = presets.join("sounds/bass/Acid Bite.ron");
    assert!(file.is_file(), "{}", app.status);
    assert!(
        app.status
            .starts_with("Saved track 01's sound as bass/Acid Bite")
    );
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("machine: Plaits(Acid)"), "{text}");
    assert!(text.contains("\"Bite\""), "{text}");

    // A name that's taken is refused, and `!` writes over it.
    command(&mut app, "preset save bass/Acid Bite");
    assert!(app.status.contains(":preset! save"), "{}", app.status);
    command(&mut app, "preset! save bass/Acid Bite");
    assert!(app.status.starts_with("Saved"), "{}", app.status);

    // On track 2, with a level, pan and name of its own.
    app.apply(&Action::Batch(vec![
        Action::SetTrackLevel {
            track: 1,
            level: 0x99,
        },
        Action::SetTrackName {
            track: 1,
            name: "ZZ".to_owned(),
        },
    ]));
    app.cursor.track = 1;
    assert_eq!(app.cursor.track, 1);
    let phrases = app.song().tracks[1].phrases.clone();
    let depth = app.history.undo_depth();
    command(&mut app, "preset load bass/Acid Bite");
    let loaded = &app.song().tracks[1];
    assert_eq!(
        loaded.machine,
        Machine::Plaits(SynthVoice::Acid),
        "{}",
        app.status
    );
    assert_eq!(loaded.cutoff, 0x40);
    assert_eq!(loaded.inserts[1], Insert::RETURN_DELAY);
    assert_eq!(
        loaded.fine(SettingId::slot(1, Effect::Delay, 1).unwrap()),
        0x40
    );
    assert_eq!(loaded.macros[0].name.as_deref(), Some("Bite"));
    assert_eq!((loaded.level, loaded.name.as_str()), (0x99, "ZZ"));
    assert_eq!(loaded.phrases, phrases);
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    assert!(
        app.status.starts_with("Track 02 plays Acid Bite"),
        "{}",
        app.status
    );
    press(&mut app, "u");
    assert_eq!(app.song().tracks[1].machine, Machine::Sampler);

    // A name it doesn't have is said.
    command(&mut app, "preset load bass/Nothing");
    assert!(app.status.contains("no sound preset called bass/Nothing"));
    command(&mut app, "preset save ../up");
    assert!(
        app.status.starts_with("A preset's name is"),
        "{}",
        app.status
    );
}

#[test]
fn a_sampled_sound_plays_the_same_file_in_another_song() {
    let (mut app, _realtime, presets) = presets_app();
    with_a_sound(&mut app);
    let file = file_of(app.song(), 0).unwrap();
    command(&mut app, "preset save Tone");
    assert!(presets.join("sounds/Tone.ron").is_file(), "{}", app.status);

    let (mut other, _realtime) = vt_tui_app_with(&presets);
    other.cursor.track = 1;
    command(&mut other, "preset load Tone");
    let played = file_of(other.song(), 1).unwrap_or_else(|| panic!("{}", other.status));
    assert_eq!(
        Path::new(&played).canonicalize().unwrap(),
        Path::new(&file).canonicalize().unwrap()
    );
    assert_eq!(other.song().bank.iter().count(), 1);
    // Loaded again, it's the same entry in the bank.
    other.cursor.track = 0;
    command(&mut other, "preset load Tone");
    assert_eq!(other.song().bank.iter().count(), 1);
    assert_eq!(
        other.song().tracks[0].sample(),
        other.song().tracks[1].sample()
    );

    // With its file gone, it says which and loads nothing.
    std::fs::remove_file(&file).unwrap();
    let (mut third, _realtime) = vt_tui_app_with(&presets);
    let depth = third.history.undo_depth();
    command(&mut third, "preset load Tone");
    assert!(
        third.status.starts_with("Tone can't load") && third.status.contains("tone.wav"),
        "{}",
        third.status
    );
    assert_eq!(third.history.undo_depth(), depth);
}

/// Another app, a song of its own, with the same presets folder.
fn vt_tui_app_with(presets: &Path) -> (App, vt_engine::link::Realtime) {
    let (mut app, realtime) = app();
    app.settings = Settings {
        presets: presets.display().to_string(),
        ..Settings::none()
    };
    (app, realtime)
}

#[test]
fn an_effect_saved_from_one_slot_loads_into_another() {
    let (mut app, _realtime, presets) = presets_app();
    acid_on_first(&mut app);
    press(&mut app, " 7");
    // Down to the second slot's effect.
    press(&mut app, "}");
    command(&mut app, "preset save Dub Delay");
    assert!(
        presets.join("effects/Dub Delay.ron").is_file(),
        "{}",
        app.status
    );
    assert!(
        app.status.starts_with("Saved track 01's FX 2 as Dub Delay"),
        "{}",
        app.status
    );

    // Into the last slot, from the browser.
    press(&mut app, "}}");
    command(&mut app, "preset");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().title, "Effect presets");
    let rows = explorer_rows(&app);
    assert!(rows.iter().any(|row| row.contains("Dub Delay")), "{rows:?}");
    let depth = app.history.undo_depth();
    let at = rows
        .iter()
        .position(|row| row.contains("Dub Delay"))
        .unwrap();
    let here = rows
        .iter()
        .position(|row| row.starts_with("v "))
        .unwrap_or(0)
        + 1;
    for _ in here..at {
        press(&mut app, "j");
    }
    special(&mut app, KeyCode::Enter);
    let track = &app.song().tracks[0];
    assert_eq!(track.inserts[3], Insert::RETURN_DELAY, "{}", app.status);
    assert_eq!(
        track.fine(SettingId::slot(3, Effect::Delay, 1).unwrap()),
        0x40
    );
    assert_eq!(app.history.undo_depth(), depth + 1);

    // An empty slot has nothing to save.
    press(&mut app, "{");
    command(&mut app, "preset save Nothing");
    assert!(app.status.contains("is empty"), "{}", app.status);
}

#[test]
fn k_on_the_machine_lists_the_presets_and_the_browser_loads_one() {
    let (mut app, _realtime, presets) = presets_app();
    acid_on_first(&mut app);
    command(&mut app, "preset save Acid Bite");
    app.cursor.track = 1;
    press(&mut app, " 5gg");
    press(&mut app, "K");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.title, "Machine");
    let labels: Vec<String> = picker.items().iter().map(listed).collect();
    assert!(
        labels
            .iter()
            .any(|label| label.contains("Load a sound preset  :preset")),
        "{labels:?}"
    );
    // Typing finds it, and Enter opens the browser on the presets folder's sounds.
    press(&mut app, "/load");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Picker, "{}", app.status);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.title, "Sound presets");
    let rows = explorer_rows(&app);
    assert_eq!(
        rows,
        ["v recent", "    Acid Bite"],
        "the sounds folder, saved in last"
    );
    assert_eq!(picker.current().unwrap().label, "Acid Bite");
    assert_eq!(picker.current().unwrap().meaning, "Acid · Delay · 1 macro");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[1].machine,
        Machine::Plaits(SynthVoice::Acid)
    );
    assert!(presets.join("sounds").is_dir());

    // Save is there too, typing the command for the name.
    press(&mut app, "K");
    press(&mut app, "/save");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Command);
    assert_eq!(app.command_line, "preset save ");
}

#[test]
fn with_no_presets_yet_or_no_folder_it_says_so() {
    let (mut app, _realtime, _presets) = presets_app();
    command(&mut app, "preset");
    assert!(
        app.status
            .starts_with("No sound presets yet: :preset save name"),
        "{}",
        app.status
    );
    app.settings = Settings::none();
    command(&mut app, "preset save X");
    assert!(app.status.contains("no presets folder"), "{}", app.status);
}

#[test]
fn a_fixed_track_has_no_sound_preset() {
    let (mut app, _realtime, _presets) = presets_app();
    let master = app.song().tracks.len() - 1;
    app.cursor.track = master;
    assert_eq!(app.cursor.track, master);
    command(&mut app, "preset save X");
    assert!(
        app.status.starts_with("Only a track that plays sounds"),
        "{}",
        app.status
    );
}
