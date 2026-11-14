//! Kits: their files and folders, filling tracks from one, and the bank entries their sounds make.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_model::constants::FIXED_TRACKS;
use vt_tui::app::App;
use vt_tui::config::Settings;
use vt_tui::input::keys::parse_sequence;
use vt_tui::input::mode::Mode;
use vt_tui::input::picker::Pick;

mod common;
use common::*;

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

    press(&mut app, "WW");
    command(&mut app, "sample");
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

    command(&mut app, "sample");
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

#[test]
fn a_missing_kit_says_so_and_a_missing_kit_folder_isnt_listed() {
    let (mut app, _realtime) = app();
    app.settings = Settings {
        kits: vec!["/nowhere/kits".to_owned(), "/nowhere/else".to_owned()],
        samples: Vec::new(),
        presets: String::new(),
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
        presets: String::new(),
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
    command(&mut app, "sample");
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
